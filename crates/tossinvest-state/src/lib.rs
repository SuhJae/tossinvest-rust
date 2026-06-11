//! `tossinvest-state` — stateful, observable layer for the [`tossinvest`] client.
//!
//! A single background reconciler centralizes all polling and holds live state (orders,
//! holdings, market data) so that UIs **subscribe** instead of polling. State is exposed
//! two ways: a wait-free `arc-swap` snapshot (for immediate-mode GUIs) and a sequenced
//! [`Delta`](core::Delta) stream (for event-driven UIs). The order finite-state machine is
//! reconciled as a pure, replay-testable fold ([`core::ProjectedState`]); the stateless
//! [`tossinvest`] client remains the substrate and stays fully usable on its own.
//!
//! The [`core`] module is runtime-agnostic (no tokio). The tokio reconciler and
//! [`StateHandle`] live behind the default `tokio` feature.
//!
//! See `DESIGN.md` (§4–§5) at the repository root.

pub mod core;

#[cfg(feature = "tokio")]
mod runtime;

#[cfg(feature = "tokio")]
pub use runtime::{PriceLease, RefreshTarget, SchedulerConfig, StateHandle, SubmitOutcome};

#[doc(inline)]
pub use tossinvest as client;
