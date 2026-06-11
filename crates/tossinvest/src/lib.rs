//! `tossinvest` — async Rust client for the [Toss Securities Open API](https://developers.tossinvest.com/docs).
//!
//! REST client with OAuth2 client-credentials auth, a mockable transport middleware spine
//! (observe → retry → rate-limit → auth → account → reqwest), the full typed endpoint surface
//! (market data, stock info, market calendars, accounts, holdings, orders), and per-group rate
//! limiting shared via [`tossinvest_rate`].
//!
//! The pure data model is re-exported from [`tossinvest_model`]; for the stateful, observable
//! layer (a reconciler that holds live state so UIs subscribe instead of polling), see the
//! `tossinvest-state` crate.
//!
//! Status: **scaffolding**. See `DESIGN.md` at the repository root.

#[doc(inline)]
pub use tossinvest_model as model;
#[doc(inline)]
pub use tossinvest_rate as rate;
