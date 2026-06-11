//! `tossinvest` — async Rust client for the [Toss Securities Open API](https://developers.tossinvest.com/docs).
//!
//! REST client with OAuth2 client-credentials auth, a mockable [`Transport`] seam, an
//! auth/account/rate-limit/retry request pipeline, and the full typed endpoint surface
//! (market data, stock info, market calendars, accounts, holdings, orders). The pure data
//! model is re-exported from [`tossinvest_model`]; per-group rate limiting is shared via
//! [`tossinvest_rate`].
//!
//! ```no_run
//! use tossinvest::{Credentials, TossClient};
//! # async fn run() -> tossinvest::Result<()> {
//! let client = TossClient::new(Credentials::from_env()?)?;
//! let prices = client.prices(&["005930", "AAPL"]).await?;
//! let accounts = client.accounts().await?;
//! let acct = client.account(accounts[0].account_seq);
//! let holdings = acct.holdings(None).await?;
//! # Ok(()) }
//! ```
//!
//! For the stateful, observable layer (a reconciler that holds live state so UIs subscribe
//! instead of polling), see the `tossinvest-state` crate.

#[doc(inline)]
pub use tossinvest_model as model;
#[doc(inline)]
pub use tossinvest_rate as rate;

pub mod api;
pub mod auth;
pub mod client;
pub mod config;
pub mod error;
pub mod transport;

pub use api::CandleInterval;
pub use auth::{Credentials, InMemoryTokenStore, StoredToken, TokenManager, TokenStore};
pub use client::{AccountClient, TossClient, TossClientBuilder};
pub use config::{ClientConfig, RetryPolicy};
pub use error::{ApiErrorKind, Error, Result, TransportError};
pub use transport::{
    AuthRequirement, RawBody, RawRequest, RawResponse, ReqwestTransport, Transport,
};

// Re-export the most-used model types at the crate root for ergonomics.
pub use tossinvest_model::{
    AccountSeq, ClientOrderId, Currency, OrderCreateRequest, OrderId, OrderListFilter,
    OrderModifyRequest, OrderStatus, Side, Symbol,
};
pub use tossinvest_rate::RateLimitGroup;
