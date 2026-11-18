# Redstone Pen — a Horizon port

**Redstone Pen** is a Minecraft mod. Minecraft mod adding a pen to draw Redstone tracks in all directions, a PLC for Redstone, and relays. This repository is a **port of it to [Horizon](https://github.com/horizon-rust/horizon)**, the Rust engine for Minecraft Java 26.3. It carries the complete source of the port plus the content files it ships, and installs as a single `.hmod` package.

Upstream: [stfwi/redstonepen](https://github.com/stfwi/redstonepen) — licence **MIT**.

| | |
|---|---|
| Upstream mod | Redstone Pen ([source](https://github.com/stfwi/redstonepen)) |
| Ported to | Horizon 0b14f8a, Minecraft Java 26.3, mod API `0.2` |
| Package | `redstonepen.hmod` (install it with the Horizon launcher) |
| Licence | **MIT** — the upstream mod's licence; see [Licence and credit](#licence-and-credit) |

## What this port implements

- a block state `axis` (x, y or z), `bend` (`none` for a straight track, one
- two **opposite** arms are a straight track, two **perpendicular** arms are a
- the pen draws a straight track on the clicked face, extends the clicked arm,
- slice 2 adds the **inverted, pulse and bistable relays** (separate blocks,
- **The signal read is the engine's `getSignal`.** 0.1.0 rebuilt "what does this block give
- **The sneak is the acting player's own flag.** 0.1.0 read the local key state
- **The pen's click is the item's moment, not the block's.** 0.1.0's global `use_item_on` saw
- **No logger.** The 0.1.0 embedded-content helper was the only `log::warn!`; it is gone with

## Not yet ported

- **One track per block, two or three arms** (the brief's documented
- **Corner/T collision box:** the state's `axis` follows the first arm and the
- **Pen gestures:** an arm click extends; a side click bends a track with
- **Relay scope:** plain, inverted, pulse and bistable relays are carried;
- **Pen economy:** the pen does not consume redstone or wear (upstream's
- **Support rule:** a track does not require or lose its supporting face; the
- **No custom renderer/particles/overlay:** vanilla redstone-block textures as
- **Content recipes:** the pen recipe is simplified from the custom extended

## Installing

1. Install the [Horizon launcher](https://github.com/horizon-rust/horizon/releases) (0.2.70 or newer).
2. Add `redstonepen.hmod` to the instance's mods.
3. The package is pinned to a specific Horizon build; the launcher checks that pin.

## Licence and credit

Redstone Pen was created by its upstream authors; the code, models and assets in the original mod
remain theirs and are used here under **MIT**. The Rust in this repository is a clean-room
rewrite for Horizon, not a translation of the original source.

- `LICENSE` — the upstream licence text.
- `LICENSE-NOTICE.md` — what this port carries and where it came from.
- The upstream project: https://github.com/stfwi/redstonepen

## Technical details

```
horizon-mod.toml   package manifest (id, version, api, [native] pin)
Cargo.toml         the crate; depends on `horizon-mod-api` from the fetched SDK
src/               lib.rs (the one `impl Hooks`), runtime.rs (pure logic + tests), glue
assets/ data/      the content the package ships
tools/             content generators and the e2e scenario
```

- Build: `cargo horizon-mod build` (packages; it does not compile against the game).
- Test: `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- In-game: `ports/tools/launcher-game.sh run redstonepen` in the campaign tree, or install the
  `.hmod` and play; the harness scenario is `tools/e2e.sh`.

This repository is generated from the campaign's port at `ports/redstonepen/`; the port's own
`notes.md` is the full record (decisions, traps, evidence).
