# tossinvest-rust

Async Rust client for the **[Toss Securities Open API](https://developers.tossinvest.com/docs)** (토스증권 Open API) — KR + US equities: market data, accounts, holdings, and order management, plus an optional stateful layer that holds live state so UIs subscribe instead of polling.

> **Status: early development.** The architecture is fully specified in **[`DESIGN.md`](./DESIGN.md)**; crates are being implemented bottom-up. APIs will change until `0.1.0`.

## Crate family

| Crate | Layer | What it is |
|-------|-------|------------|
| [`tossinvest-model`](./crates/tossinvest-model) | L0 | Pure, runtime-free data types: serde models, domain newtypes, open enums, order-lifecycle types. No I/O. |
| [`tossinvest-rate`](./crates/tossinvest-rate) | L0.5 | Per-group rate-limit policy (TPS table + KST peak window) over a shared [`governor`] registry. |
| [`tossinvest`](./crates/tossinvest) | L1–3 | The async REST client: OAuth2 auth, mockable transport middleware spine, full typed endpoint surface. |
| [`tossinvest-state`](./crates/tossinvest-state) | L4 | Stateful, observable layer: one reconciler, snapshot + delta stream, replay-tested order FSM. Additive — the stateless client stays usable on its own. |

## Design highlights

- **REST-only, no push** → polling is *centralized* in the wrapper (one rate-limit-aware reconciler), not scattered across the UI.
- **Order FSM** reconciled as a pure, replay-testable fold (handles the new-id-on-modify/cancel, `*_REJECTED` sibling reverts, `REPLACED` relinking, terminal capture, and partial-fill-orthogonal-to-status quirks).
- **Two read primitives** — a wait-free `arc-swap` snapshot (immediate-mode GUIs) and a sequenced `DomainEvent` delta stream (event-driven UIs).
- **Adaptive rate limiting** — documented TPS as a ceiling, AIMD down on observed 429s, server `X-RateLimit-*`/`Retry-After` headers honored as authoritative.
- **Decimals are never floats** — every monetary/quantity value is exact (`rust_decimal`).

See **[`DESIGN.md`](./DESIGN.md)** for the full data model, the order finite-state machine, the stateful/observable layer, and the control plane.

## License

Licensed under the [Apache License, Version 2.0](./LICENSE).

[`governor`]: https://crates.io/crates/governor
