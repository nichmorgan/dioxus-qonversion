[![Discord Server](https://img.shields.io/discord/899851952891002890.svg?logo=discord&style=flat-square)](https://discord.gg/sKJSVNSCDJ)

# dioxus-qonversion

Unofficial [Dioxus](https://dioxuslabs.com/) bridge for [Qonversion](https://qonversion.io) Subscription Management Mode.

**Mobile only for now** (iOS and Android). Web and desktop are out of scope. Qonversion’s own [Web SDK](https://documentation.qonversion.io/docs/web-sdk) covers browser checkout (Stripe/Paddle); this crate does not wrap it.

This crate is **not affiliated** with Qonversion.

## Status

Early — the public API is not stable yet. See the [roadmap](https://github.com/nichmorgan/dioxus-qonversion/issues/1).

## Ownership

| Layer | Owns |
| -- | -- |
| **Library** | Qonversion primitives (init, identify, paywall, entitlements, restore, …) |
| **App** | Project key, Remote Config field names, gating UI, fail-open vs fail-closed policy |

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option.
