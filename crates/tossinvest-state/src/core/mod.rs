//! The runtime-agnostic core: identity keys, delta events, the UI-facing order view, and
//! the pure projection fold. No tokio, no I/O — fully replay-testable.

pub mod event;
pub mod state;
pub mod view;

pub use event::{Delta, OpKind, OrderKey};
pub use state::{OptimisticOrder, ProjectedState};
pub use view::{ClosedReason, Lifecycle, OrderLinks, OrderView};
