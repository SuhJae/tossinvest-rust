//! The HTTP client: request pipeline (auth → account → rate-limit → retry), status-split
//! decoding, and the `TossClient` / `AccountClient` handles.

use crate::auth::{Credentials, InMemoryTokenStore, TokenManager, TokenStore};
use crate::config::{ClientConfig, RetryPolicy};
use crate::error::{ApiErrorKind, Error, Result, TransportError};
use crate::transport::{AuthRequirement, RawRequest, RawResponse, ReqwestTransport, Transport};
use reqwest::header::{AUTHORIZATION, HeaderValue};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;
use tossinvest_model::{AccountSeq, ApiResponse, ErrorResponse};
use tossinvest_rate::{Feedback, RateLimitHeaders, RateLimiterRegistry};
use url::Url;

/// The top-level client. Cheap to clone (everything is behind `Arc`).
#[derive(Clone, Debug)]
pub struct TossClient {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    transport: Arc<dyn Transport>,
    tokens: TokenManager,
    limiter: RateLimiterRegistry,
    retry: RetryPolicy,
}

impl TossClient {
    /// Start building a client with the given OAuth2 credentials.
    pub fn builder(credentials: Credentials) -> TossClientBuilder {
        TossClientBuilder::new(credentials)
    }

    /// Build a client with default configuration from the given credentials.
    pub fn new(credentials: Credentials) -> Result<Self> {
        Self::builder(credentials).build()
    }

    /// Bind an account; account-scoped endpoints live on the returned handle.
    pub fn account(&self, seq: AccountSeq) -> AccountClient {
        AccountClient {
            client: self.clone(),
            seq,
        }
    }

    /// The shared rate-limiter registry (also used by the stateful layer).
    pub fn rate_limiters(&self) -> &RateLimiterRegistry {
        &self.inner.limiter
    }

    /// Execute a request through the full pipeline and decode the success payload.
    pub(crate) async fn call<T: DeserializeOwned>(&self, mut req: RawRequest) -> Result<T> {
        // 1. Authorization.
        if req.auth == AuthRequirement::Bearer {
            let token = self.inner.tokens.token().await?;
            let value = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e| {
                Error::InvalidArg {
                    field: "authorization",
                    reason: e.to_string(),
                }
            })?;
            req.headers.insert(AUTHORIZATION, value);
        }
        // 2. Account scoping.
        if let Some(seq) = req.account {
            let value = HeaderValue::from_str(&seq.to_string()).map_err(|e| Error::InvalidArg {
                field: "x-tossinvest-account",
                reason: e.to_string(),
            })?;
            req.headers.insert("X-Tossinvest-Account", value);
        }

        // 3. Rate-limit, execute, retry.
        let resp = self.send_with_retry(req).await?;

        // 4. Status-split decode.
        decode::<T>(resp)
    }

    async fn send_with_retry(&self, req: RawRequest) -> Result<RawResponse> {
        let max = self.inner.retry.max_attempts.max(1);
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            // Honor a `Retry-After` park for this group (set by a prior 429).
            if let Some(until) = self.inner.limiter.parked_until(req.group) {
                let now = std::time::Instant::now();
                if until > now {
                    tokio::time::sleep(until - now).await;
                }
            }
            // Proactive shaping at the group's current (possibly throttled-down) rate.
            self.inner.limiter.until_ready(req.group).await;
            let result = self.inner.transport.execute(req.clone()).await;

            match result {
                Ok(resp) => {
                    // Feed the outcome to the congestion controller for FUTURE requests.
                    self.inner
                        .limiter
                        .record_feedback(req.group, feedback_for(&resp));

                    if resp.status.is_success() {
                        return Ok(resp);
                    }
                    // A 401 may mean an expired token — invalidate so the next attempt refreshes.
                    if resp.status.as_u16() == 401 && req.auth == AuthRequirement::Bearer {
                        self.inner.tokens.invalidate();
                    }
                    let retry_after = parse_retry_after(&resp);
                    let should_retry =
                        req.retryable && attempt < max && is_retryable_status(resp.status.as_u16());
                    if should_retry {
                        sleep_backoff(&self.inner.retry, attempt, retry_after).await;
                        continue;
                    }
                    return Ok(resp); // decode() will turn it into Error::Api
                }
                Err(e) => {
                    // A timeout is a soft congestion signal.
                    if matches!(e, TransportError::Timeout) {
                        self.inner
                            .limiter
                            .record_feedback(req.group, Feedback::Timeout);
                    }
                    if req.retryable && attempt < max && e.is_transient() {
                        sleep_backoff(&self.inner.retry, attempt, None).await;
                        continue;
                    }
                    return Err(Error::Transport(e));
                }
            }
        }
    }
}

/// Build congestion-controller feedback from a response (status + rate-limit headers).
fn feedback_for(resp: &RawResponse) -> Feedback {
    let headers = RateLimitHeaders {
        limit: resp
            .header("x-ratelimit-limit")
            .and_then(|v| v.parse().ok()),
        remaining: resp
            .header("x-ratelimit-remaining")
            .and_then(|v| v.parse().ok()),
        reset: resp
            .header("x-ratelimit-reset")
            .and_then(|v| v.parse().ok()),
    };
    match resp.status.as_u16() {
        429 => Feedback::RateLimited {
            retry_after: parse_retry_after(resp),
            headers,
        },
        500..=599 => Feedback::ServerError,
        _ => Feedback::Success { headers },
    }
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn parse_retry_after(resp: &RawResponse) -> Option<Duration> {
    resp.header("retry-after")
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
}

async fn sleep_backoff(policy: &RetryPolicy, attempt: u32, retry_after: Option<Duration>) {
    let delay = retry_after.unwrap_or_else(|| {
        let exp = policy
            .base_delay
            .saturating_mul(1u32 << (attempt - 1).min(16));
        exp.min(policy.max_delay)
    });
    tokio::time::sleep(delay).await;
}

/// Decode a response: success → `ApiResponse<T>.result`; failure → typed `Error::Api`.
fn decode<T: DeserializeOwned>(resp: RawResponse) -> Result<T> {
    if resp.status.is_success() {
        let parsed: ApiResponse<T> = serde_json::from_slice(&resp.body)
            .map_err(|e| Error::Decode(format!("{e} (status {})", resp.status.as_u16())))?;
        Ok(parsed.result)
    } else {
        let status = resp.status.as_u16();
        let retry_after = parse_retry_after(&resp);
        match serde_json::from_slice::<ErrorResponse>(&resp.body) {
            Ok(env) => {
                let kind = ApiErrorKind::classify(status, &env.error.code, retry_after);
                Err(Error::Api {
                    status,
                    code: env.error.code,
                    kind,
                    message: env.error.message,
                    request_id: env.error.request_id,
                })
            }
            Err(e) => Err(Error::Decode(format!(
                "error envelope (status {status}): {e}"
            ))),
        }
    }
}

/// An account-scoped handle. Account/order endpoints live here so the
/// `X-Tossinvest-Account` header can never be forgotten.
#[derive(Clone, Debug)]
pub struct AccountClient {
    pub(crate) client: TossClient,
    pub(crate) seq: AccountSeq,
}

impl AccountClient {
    /// The bound account sequence.
    pub fn account_seq(&self) -> AccountSeq {
        self.seq
    }

    /// The underlying (account-agnostic) client.
    pub fn client(&self) -> &TossClient {
        &self.client
    }

    pub(crate) async fn call<T: DeserializeOwned>(&self, req: RawRequest) -> Result<T> {
        self.client.call(req.account(self.seq)).await
    }
}

/// Builder for [`TossClient`].
#[derive(Debug)]
pub struct TossClientBuilder {
    credentials: Credentials,
    config: ClientConfig,
    store: Option<Arc<dyn TokenStore>>,
    transport: Option<Arc<dyn Transport>>,
}

impl TossClientBuilder {
    fn new(credentials: Credentials) -> Self {
        Self {
            credentials,
            config: ClientConfig::default(),
            store: None,
            transport: None,
        }
    }

    /// Override the base URL.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = url.into();
        self
    }

    /// Override the full configuration.
    pub fn config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Override the retry policy.
    pub fn retry(mut self, retry: RetryPolicy) -> Self {
        self.config.retry = retry;
        self
    }

    /// Provide a custom token store (for persistence across restarts).
    pub fn token_store(mut self, store: Arc<dyn TokenStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Provide a custom transport (e.g. a mock for tests).
    pub fn transport(mut self, transport: Arc<dyn Transport>) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<TossClient> {
        let base = Url::parse(&self.config.base_url).map_err(|e| Error::InvalidArg {
            field: "base_url",
            reason: e.to_string(),
        })?;
        let transport: Arc<dyn Transport> = self
            .transport
            .unwrap_or_else(|| Arc::new(ReqwestTransport::new(base)));
        let store: Arc<dyn TokenStore> = self
            .store
            .unwrap_or_else(|| Arc::new(InMemoryTokenStore::default()));
        let limiter = RateLimiterRegistry::with_base_limits();
        let tokens = TokenManager::new(
            self.credentials,
            store,
            transport.clone(),
            limiter.clone(),
            self.config.token_skew,
        );
        Ok(TossClient {
            inner: Arc::new(Inner {
                transport,
                tokens,
                limiter,
                retry: self.config.retry,
            }),
        })
    }
}
