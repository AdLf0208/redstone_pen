# Redstone Pen for Horizon

A native Horizon port of **Redstone Pen 1.11.42** (`stfwi/redstonepen`, MIT): a pen that
draws powered tracks, a track block with one level of decay per step, and plain, inverted,
pulse and bistable relays. This is the documented **one track per block** slice: a track has
two or three arms — opposite arms are a straight track (an `axis`), perpendicular arms a
**corner** (`bend`), three arms a **T** — and one `power` state, because upstream's 24
segments in one block entity cannot be expressed with block states. A sneak click with the
pen is the explicit erase gesture. See [`notes.md`](notes.md) for the full scope, semantics,
the gaps and the migration record.

- Package: Horizon **0.2.0**, one flat SDK `.hmod` (`api = "0.2"`) built against SDK
  `0b14f8acfd97fa2c750e42795249dcc981178ca4` / mixins
  `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`, game 26.3.
- Code: `src/lib.rs` is the one `impl mod_api::Hooks`; `src/support.rs` is the game glue
  (the scoped Parts, the pen click, the bounded level-tick solve); `src/runtime.rs` is the
  port's pure logic with its 39 unit tests, unchanged since 0.1.0.
- Content: `assets/` and `data/` at the package root — the pen, the track (straight, 12
  corners and 20 Ts), the four relays, their models, loot, recipes, tag, creative tab and lang.
- Licence: MIT, rank 0. **Local-only** until the maintainer approves publishing
  (`ARCHITECTURE.md` §8).

## Validate

The SDK ladder runs in the port; nothing is staged.

```sh
cargo test --offline                       # 49 tests: 39 runtime, 10 glue
cargo clippy --all-targets --offline -- -D warnings
cargo fmt --check                          # NOT --all: that checks the fetched SDK

rm -f Cargo.lock                           # a packed lock changes the artifact
cargo horizon-mod build                    # -> target/horizon-mod/redstonepen.hmod
cargo horizon-mod check redstonepen.hmod
cargo horizon-mod fits redstonepen.hmod --build nightly
```

The in-game scenario is [`tools/e2e.sh`](tools/e2e.sh); it needs a game built by
`horizon-launcher` from this package (`GAME=` points at it), which is not installed in this
environment — the 0.1.0 runs it records are historical. `tools/check_binding.py` and
`tools/gen_track_shapes.py` still audit the content (state binding, shape tables).

The retired `.hnmod` tree (mixin plan, per-hook helpers, the old tools and the 0.1.0
artifacts) is kept as evidence in
`../_shared/evidence/redstonepen/retired-hnmod-0.1.0/`.
