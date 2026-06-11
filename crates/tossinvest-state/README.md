# tossinvest-state

Stateful, observable layer for the [`tossinvest`](https://crates.io/crates/tossinvest) client.

A single background reconciler centralizes all polling and holds live state (orders, holdings,
market data) so UIs **subscribe** instead of polling — via a wait-free snapshot *and* a sequenced
event-delta stream. The order FSM is reconciled as a pure, replay-testable fold. The stateless
client stays fully usable on its own.

> **Status: early scaffolding / name reservation.** See the
> [design document](https://github.com/SuhJae/tossinvest-rust/blob/main/DESIGN.md) (§4–§5).

Licensed under Apache-2.0.
