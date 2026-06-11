//! The mockable transport seam: a raw request/response pair and the [`Transport`] trait,
//! plus the default reqwest-based implementation.

use crate::error::TransportError;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use tossinvest_model::AccountSeq;
use tossinvest_rate::RateLimitGroup;
use url::Url;

/// Whether a request must carry an `Authorization: Bearer` header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthRequirement {
    /// No authorization header (e.g. the token endpoint itself).
    None,
    /// A bearer access token is required.
    Bearer,
}

/// A pre-serialized request body with its content type.
#[derive(Clone, Debug)]
pub struct RawBody {
    /// The `Content-Type` header value.
    pub content_type: &'static str,
    /// The serialized bytes.
    pub bytes: Vec<u8>,
}

/// A protocol-level request, independent of the HTTP client. Cloneable so it can be retried.
#[derive(Clone, Debug)]
pub struct RawRequest {
    /// HTTP method.
    pub method: Method,
    /// Path relative to the base URL (e.g. `/api/v1/orders`).
    pub path: String,
    /// Query parameters.
    pub query: Vec<(String, String)>,
    /// Extra headers (auth/account are injected by the client).
    pub headers: HeaderMap,
    /// Optional request body.
    pub body: Option<RawBody>,
    /// The rate-limit group this request belongs to.
    pub group: RateLimitGroup,
    /// Authorization requirement.
    pub auth: AuthRequirement,
    /// The account to scope to (sets `X-Tossinvest-Account`), if any.
    pub account: Option<AccountSeq>,
    /// Whether this request is safe to retry on transient failure.
    pub retryable: bool,
}

impl RawRequest {
    /// A `GET` request in the given rate-limit group, requiring a bearer token. Retryable.
    pub fn get(path: impl Into<String>, group: RateLimitGroup) -> Self {
        Self {
            method: Method::GET,
            path: path.into(),
            query: Vec::new(),
            headers: HeaderMap::new(),
            body: None,
            group,
            auth: AuthRequirement::Bearer,
            account: None,
            retryable: true,
        }
    }

    /// A `POST` request in the given rate-limit group, requiring a bearer token. Not
    /// retryable by default (mutations); call [`RawRequest::retryable`] to opt in.
    pub fn post(path: impl Into<String>, group: RateLimitGroup) -> Self {
        Self {
            method: Method::POST,
            path: path.into(),
            query: Vec::new(),
            headers: HeaderMap::new(),
            body: None,
            group,
            auth: AuthRequirement::Bearer,
            account: None,
            retryable: false,
        }
    }

    /// Add a query parameter.
    pub fn query(mut self, key: &str, value: impl Into<String>) -> Self {
        self.query.push((key.to_owned(), value.into()));
        self
    }

    /// Add an optional query parameter (skipped if `None`).
    pub fn query_opt(mut self, key: &str, value: Option<String>) -> Self {
        if let Some(v) = value {
            self.query.push((key.to_owned(), v));
        }
        self
    }

    /// Set the authorization requirement.
    pub fn auth(mut self, auth: AuthRequirement) -> Self {
        self.auth = auth;
        self
    }

    /// Scope the request to an account (sets the `X-Tossinvest-Account` header).
    pub fn account(mut self, seq: AccountSeq) -> Self {
        self.account = Some(seq);
        self
    }

    /// Mark whether the request is safe to retry.
    pub fn set_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// Attach a JSON body (serializing the value).
    pub fn json_body<T: serde::Serialize>(mut self, value: &T) -> Result<Self, TransportError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| TransportError::Other(format!("serialize body: {e}")))?;
        self.body = Some(RawBody {
            content_type: "application/json",
            bytes,
        });
        Ok(self)
    }

    /// Attach a `application/x-www-form-urlencoded` body.
    pub fn form_body(mut self, pairs: &[(&str, &str)]) -> Result<Self, TransportError> {
        let bytes = serde_urlencoded::to_string(pairs)
            .map_err(|e| TransportError::Other(format!("serialize form: {e}")))?
            .into_bytes();
        self.body = Some(RawBody {
            content_type: "application/x-www-form-urlencoded",
            bytes,
        });
        Ok(self)
    }
}

/// A protocol-level response.
#[derive(Clone, Debug)]
pub struct RawResponse {
    /// HTTP status code.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Raw body bytes.
    pub body: Vec<u8>,
}

impl RawResponse {
    /// Look up a header as a string.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

/// The mockable transport boundary: turns a [`RawRequest`] into a [`RawResponse`]. The
/// reqwest implementation is the only thing that touches a socket; tests swap in a mock.
#[async_trait::async_trait]
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// Execute a request and return the raw response.
    async fn execute(&self, req: RawRequest) -> Result<RawResponse, TransportError>;
}

/// The default reqwest-backed transport.
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    base_url: Url,
}

impl ReqwestTransport {
    /// Build a transport against the given base URL.
    pub fn new(base_url: Url) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }

    /// Build a transport from an existing reqwest client.
    pub fn with_client(client: reqwest::Client, base_url: Url) -> Self {
        Self { client, base_url }
    }
}

fn map_reqwest_error(e: reqwest::Error) -> TransportError {
    if e.is_timeout() {
        TransportError::Timeout
    } else if e.is_connect() {
        TransportError::Connect(e.to_string())
    } else {
        TransportError::Other(e.to_string())
    }
}

#[async_trait::async_trait]
impl Transport for ReqwestTransport {
    async fn execute(&self, req: RawRequest) -> Result<RawResponse, TransportError> {
        let url = self
            .base_url
            .join(&req.path)
            .map_err(|e| TransportError::Url(format!("{}: {e}", req.path)))?;

        let mut builder = self.client.request(req.method, url);
        if !req.query.is_empty() {
            builder = builder.query(&req.query);
        }
        builder = builder.headers(req.headers);
        if let Some(body) = req.body {
            builder = builder
                .header(CONTENT_TYPE, HeaderValue::from_static(body.content_type))
                .body(body.bytes);
        }

        let resp = builder.send().await.map_err(map_reqwest_error)?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.bytes().await.map_err(map_reqwest_error)?.to_vec();
        Ok(RawResponse {
            status,
            headers,
            body,
        })
    }
}
