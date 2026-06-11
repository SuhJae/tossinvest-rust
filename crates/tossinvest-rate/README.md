# tossinvest-rate

Per-endpoint-group rate-limiting policy for the [Toss Securities Open API](https://developers.tossinvest.com/docs):
the `RateLimitGroup` set, the documented TPS table, the 09:00–10:00 KST peak window, and a shared
[`governor`](https://crates.io/crates/governor)-based limiter registry.

Part of the [`tossinvest`](https://crates.io/crates/tossinvest) crate family.

> **Status: early scaffolding / name reservation.** See the
> [design document](https://github.com/SuhJae/tossinvest-rust/blob/main/DESIGN.md).

Licensed under Apache-2.0.
