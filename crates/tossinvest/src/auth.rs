//! OAuth2 client-credentials authentication: credentials, a pluggable token store, and a
//! single-flight token manager with proactive refresh.

use crate::error::Error;
use crate::transport::{AuthRequirement, RawRequest, Transport};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tossinvest_rate::{RateLimitGroup, RateLimiterRegistry};

/// OAuth2 client credentials.
#[derive(Clone)]
pub struct Credentials {
    /// The client id.
    pub client_id: String,
    /// The client secret (kept out of logs).
    pub client_secret: SecretString,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

impl Credentials {
    /// Build credentials from an id and secret.
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: SecretString::from(client_secret.into()),
        }
    }

    /// Read credentials from `TOSSINVEST_CLIENT_ID` / `TOSSINVEST_CLIENT_SECRET`.
    pub fn from_env() -> Result<Self, Error> {
        let id = std::env::var("TOSSINVEST_CLIENT_ID").map_err(|_| Error::InvalidArg {
            field: "TOSSINVEST_CLIENT_ID",
            reason: "environment variable not set".to_owned(),
        })?;
        let secret = std::env::var("TOSSINVEST_CLIENT_SECRET").map_err(|_| Error::InvalidArg {
            field: "TOSSINVEST_CLIENT_SECRET",
            reason: "environment variable not set".to_owned(),
        })?;
        Ok(Self::new(id, secret))
    }
}

/// A cached access token plus its absolute expiry (Unix seconds), suitable for persistence.
#[derive(Clone, Debug)]
pub struct StoredToken {
    /// The access token string.
    pub access_token: String,
    /// Expiry as seconds since the Unix epoch.
    pub expires_at_unix: i64,
}

/// A pluggable token cache. The default is in-memory; implement this to persist tokens
/// across restarts (keyring, redis, file, …).
pub trait TokenStore: Send + Sync + std::fmt::Debug {
    /// Load the cached token, if any.
    fn load(&self) -> Option<StoredToken>;
    /// Save a token.
    fn save(&self, token: StoredToken);
    /// Clear the cache.
    fn clear(&self);
}

/// The default in-memory token store.
#[derive(Debug, Default)]
pub struct InMemoryTokenStore {
    inner: std::sync::RwLock<Option<StoredToken>>,
}

impl TokenStore for InMemoryTokenStore {
    fn load(&self) -> Option<StoredToken> {
        self.inner.read().unwrap().clone()
    }
    fn save(&self, token: StoredToken) {
        *self.inner.write().unwrap() = Some(token);
    }
    fn clear(&self) {
        *self.inner.write().unwrap() = None;
    }
}

#[derive(Debug, Deserialize)]
struct OAuth2TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct OAuth2ErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Manages OAuth2 access tokens: proactive refresh (with a skew margin) and single-flight
/// concurrency (a burst of callers triggers exactly one token request).
#[derive(Debug)]
pub struct TokenManager {
    creds: Credentials,
    store: Arc<dyn TokenStore>,
    transport: Arc<dyn Transport>,
    limiter: RateLimiterRegistry,
    skew: Duration,
    refresh_lock: tokio::sync::Mutex<()>,
}

impl TokenManager {
    /// Build a token manager. `skew` is how long before expiry a token is considered stale.
    pub fn new(
        creds: Credentials,
        store: Arc<dyn TokenStore>,
        transport: Arc<dyn Transport>,
        limiter: RateLimiterRegistry,
        skew: Duration,
    ) -> Self {
        Self {
            creds,
            store,
            transport,
            limiter,
            skew,
            refresh_lock: tokio::sync::Mutex::new(()),
        }
    }

    fn fresh_token(&self) -> Option<String> {
        let t = self.store.load()?;
        let cutoff = now_unix() + self.skew.as_secs() as i64;
        (t.expires_at_unix > cutoff).then_some(t.access_token)
    }

    /// Return a valid access token, refreshing if necessary (single-flight).
    pub async fn token(&self) -> Result<String, Error> {
        if let Some(t) = self.fresh_token() {
            return Ok(t);
        }
        let _guard = self.refresh_lock.lock().await;
        // Re-check: another task may have refreshed while we waited for the lock.
        if let Some(t) = self.fresh_token() {
            return Ok(t);
        }
        self.refresh().await
    }

    /// Invalidate the cached token (e.g. after a 401).
    pub fn invalidate(&self) {
        self.store.clear();
    }

    async fn refresh(&self) -> Result<String, Error> {
        let req = RawRequest::post("/oauth2/token", RateLimitGroup::Auth)
            .auth(AuthRequirement::None)
            .form_body(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.creds.client_id.as_str()),
                ("client_secret", self.creds.client_secret.expose_secret()),
            ])?;

        self.limiter.until_ready(RateLimitGroup::Auth).await;
        let resp = self.transport.execute(req).await?;

        if resp.status.is_success() {
            let body: OAuth2TokenResponse = serde_json::from_slice(&resp.body)
                .map_err(|e| Error::Decode(format!("oauth2 token response: {e}")))?;
            let stored = StoredToken {
                access_token: body.access_token.clone(),
                expires_at_unix: now_unix() + body.expires_in,
            };
            self.store.save(stored);
            Ok(body.access_token)
        } else {
            // The token endpoint uses the OAuth2 error format, not the API envelope.
            match serde_json::from_slice::<OAuth2ErrorResponse>(&resp.body) {
                Ok(e) => Err(Error::OAuth2 {
                    error: e.error,
                    description: e.error_description,
                }),
                Err(_) => Err(Error::OAuth2 {
                    error: format!("http {}", resp.status.as_u16()),
                    description: None,
                }),
            }
        }
    }
}
