//! Client configuration and the builder.

use std::time::Duration;

/// Retry behavior for transient failures (429, 5xx, connection errors).
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Maximum number of attempts (including the first).
    pub max_attempts: u32,
    /// Base backoff delay; doubled each attempt.
    pub base_delay: Duration,
    /// Cap on any single backoff delay.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            base_delay: Duration::from_millis(250),
            max_delay: Duration::from_secs(8),
        }
    }
}

impl RetryPolicy {
    /// Disable retries (a single attempt).
    pub fn disabled() -> Self {
        Self {
            max_attempts: 1,
            ..Self::default()
        }
    }
}

/// Client configuration.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    /// Base server URL.
    pub base_url: String,
    /// Retry policy.
    pub retry: RetryPolicy,
    /// How long before expiry a token is refreshed.
    pub token_skew: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://openapi.tossinvest.com".to_owned(),
            retry: RetryPolicy::default(),
            token_skew: Duration::from_secs(60),
        }
    }
}
