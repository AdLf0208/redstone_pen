# Validation

Validated on Linux with Rust 1.95.0 on 2026-11-04, using the exact revisions in
`compatibility.json` (Horizon `c9d474d…`, horizon-mixins `d9c83d7…`) and the
real bundle committed here. This is the slice-4 record: two-or-three-arm tracks
(T junctions) and the explicit sneak-erase gesture. The detailed evidence,
behaviour table and deviations live in [`notes.md`](notes.md).

| Check | Result |
|---|---|
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Pass, 0 warnings |
| `cargo test --locked --offline` | **38 passed / 0 failed** |
| `python3 tools/horizon.py stage --horizon ../../horizon` | Pass, `stage_changed: true` (~62.3 s) |
| `python3 tools/horizon.py check --horizon ../../horizon` | Pass, `cargo_exit: 0`, 0 warnings (43.7 s) |
| `python3 tools/horizon.py test --horizon ../../horizon` | Pass, `cargo_exit: 0`, **38 passed / 0 failed / 531 filtered**, 0 warnings (80.3 s) |
| `python3 tools/horizon.py build --horizon ../../horizon` | Pass, `cargo_exit: 0`, 0 warnings (5m10s), 81,654,440-byte fast binary |
| `cargo horizon-mod build` (content folder, pinned tool) | Pass, 68 files |
| `cargo horizon-mod check content/redstonepen.hmod` | Pass, `ok:true`, 25,551 bytes, SHA-256 `6d1402c8bc187339db7db327b2867b40f400e026b15fc530fdf3e4fd6db14ff4`, unsigned, content only |
| `python3 tools/check_binding.py /tmp/opencode/redstonepen-model-states.json` | Pass, track **1584 states / 35 variants / 0 unbound**; relays 12/12, 12/12, 24/24, 24/24; every state matches exactly one variant |
| `tools/e2e.sh` (The End, creative, headless, `--dev-mod content/`) | **Pass, exit 0** — all signal, corner, relay, pen, T and sneak-erase phases |
| `SELFTEST_T=1 tools/e2e.sh` | **exit 1** at the corrupted T-lamp expectation (the T assertion can fail) |
| `SELFTEST_ERASE=1 tools/e2e.sh` | **exit 1** at the corrupted erased-lamp expectation (the erase assertion can fail) |
| `ports/tools/clean-targets.sh` | run after the job; 76 G free before/after, removed the incremental caches and `target/mixins` staged copies (all regenerable); sources, content and `.hmod` kept |

The T power table observed in game: source → west straight `15`, T `14`, east
straight `13`, south branch `13`, both lamps lit; cutting either arm leaves the
other side lit; a T missing the fed arm, a T with the wrong third arm and a
straight track at the junction are dark on the right side. A plain pen side
click on a two-wired-arm track widens it into a T; a sneak click drops the
clicked arm of a T or erases the wired cell, while the neighbour lamp stays
lit; a sneak click on a support draws nothing.

The TDD RED runs are recorded in `notes.md`: slice 4's five new tests failed
first with 102 compile errors against the missing `Shape::tee`/`ClickMode`
API, then 38/38 passed. No game was launched for the compile checks; the e2e
used a scratch `HORIZON_HOME`/`XDG_*` home and the prepared 26.3 resources, and
no Minecraft file is shipped. The upstream Java repository was read only for
behaviour (`RedstoneTrack.java` net/add/remove lines cited in `notes.md`); the
port is a clean-room Rust rewrite.

## 2026-11-09 — Horizon `1d63e0a`, both artifacts, e2e green

Validated against Horizon `1d63e0a5a37a7a708eb0b32b49bf0dfeb28ed8a9` +
horizon-mixins `5a64af938ec86b00d53d16a408aabc0038fb3e57`, every step through
`ports/tools/build-slot.sh` with `CARGO_INCREMENTAL=0`.

| Check | Result |
|---|---|
| `cargo fmt` + `--check`, `cargo clippy --all-targets -- -D warnings` | exit 0, 0 warnings |
| `cargo test --locked --offline` | 38 passed |
| `horizon.py stage` / `check` / `test` / `build` | exit 0; check 0 warnings; embedded 38 passed, 1240 filtered |
| `tools/package.sh` | `.hmod` 26,996 B `e35a0b9d…c918938c` (68 entries), `.hnmod` 23,558 B `639fd9f4…a78009` (11); reproducible |
| `tools/e2e.sh` | `PASS` + `EXIT:0` |
| `SELFTEST=1 tools/e2e.sh` | `FAIL` + `EXIT:1` (fail-capability) |
| `check-e2e-logs.py redstonepen --strict` | 2/2 ok |

Evidence: `ports/_shared/evidence/redstonepen/`.
`upstream_ready.py redstonepen` still blocks on the one-file-per-project
registry rule (content + code split).

