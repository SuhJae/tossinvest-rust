# tossinvest

Async Rust client for the [Toss Securities Open API](https://developers.tossinvest.com/docs) (토스증권 Open API).

REST client with OAuth2 client-credentials auth, a mockable transport middleware spine, the full
typed endpoint surface (market data, stock info, calendars, accounts, holdings, orders), and
per-group rate limiting. The pure model is re-exported from `tossinvest-model`; the stateful,
observable layer lives in `tossinvest-state`.

> **Status: early scaffolding / name reservation.** See the
> [design document](https://github.com/SuhJae/tossinvest-rust/blob/main/DESIGN.md).

Licensed under Apache-2.0.
