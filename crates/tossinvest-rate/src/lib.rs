//! `tossinvest-rate` — per-endpoint-group rate-limiting policy for the Toss Securities Open API.
//!
//! Defines the `RateLimitGroup` set, the documented TPS table, the 09:00–10:00 KST peak-window
//! schedule, and a process-wide limiter registry built on [`governor`]. The HTTP client's
//! rate-limit middleware and the stateful layer's reconciler both draw from the *same* registry,
//! so all callers share one TPS budget per group.
//!
//! Status: **scaffolding**. See `DESIGN.md` at the repository root.
