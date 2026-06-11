//! `tossinvest-model` — pure, runtime-free data types for the Toss Securities Open API.
//!
//! This crate has no I/O, no async runtime, and no HTTP dependency. It contains the
//! serde-(de)serializable request/response models, the domain newtypes
//! (`Symbol`, `AccountSeq`, `OrderId`, `Dec`, …), open enums that tolerate unknown
//! values, and the order-lifecycle types.
//!
//! Status: **scaffolding**. See `DESIGN.md` at the repository root for the full data
//! model, the order finite-state machine, and the crate-family architecture.
