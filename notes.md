# Redstone Pen — Horizon port notes

Ported from **Redstone Pen 1.11.42** (`stfwi/redstonepen`, MIT), Minecraft Java
1.21.1 NeoForge, onto Horizon pin `c9d474d` / horizon-mixins `d9c83d7`.
**Local-only** until the MIT licence's derivative permission *and* the
maintainer's approval (ARCHITECTURE.md §8).

## Scope and the documented simplification

Upstream is a pen that draws thin redstone tracks on block faces. The track
block's block entity holds **24 independent wire segments** (four in-plane
directions on each of six faces), **6 face connectors** and **6 × 4 bits of
per-face power** (`RedstoneTrack.java:71-78`, `:117-158`). A Minecraft block
state cannot express that, so this slice ships **one track per block with two
or three arms**:

- a block state `axis` (x, y or z), `bend` (`none` for a straight track, one
  of 12 corner arm pairs, or one of 20 T arm triples) and `power` ∈ 0..15 →
  3 × 33 × 16 = **1584 states**, 35 blockstate variants;
- two **opposite** arms are a straight track, two **perpendicular** arms are a
  corner, and **three distinct arms are a T** (a net of three arms at one
  power, upstream's `updateConnections`); there are still no multiple tracks,
  centre connectors or per-face/attachment multiplicity, and no fourth arm;
- the pen draws a straight track on the clicked face, extends the clicked arm,
  bends a one-wired-arm track on a side click, **widens a two-wired-arm track
  into a T**, and erases: a plain click across an unwired track, or a **sneak
  click on any wired track** (the explicit erase modifier);
- slice 2 adds the **inverted, pulse and bistable relays** (separate blocks,
  like upstream); the plain relay remains the slice-1 re-strengthener. The
  bridge relay is still out of scope (see gaps).

The signal rules themselves stay upstream's: a track answers its power on its
arms, a track-to-track step costs one, a vanilla redstone wire step costs one,
any other source seeds its full strength, and a relay re-emits 15 from one
output face.

## Upstream behaviour (file:line)

| Behaviour | Upstream |
|---|---|
| Track registration (`noCollission().instabreak().dynamicShape().randomTicks()`) | `ModContent.java:39-45` |
| Pen/quill registration (stack 1; pen durability 256) | `ModContent.java:127-132` |
| Relay registration | `ModContent.java:59-66` |
| 24 wire bits, 6 connectors, 6 face powers | `RedstoneTrack.java:71-78` |
| Segment ↔ face/direction map, internal edges | `RedstoneTrack.java:117-158` |
| Internal-edge segments are one net with one power on both internal sides | `RedstoneTrack.java:1146-1312` (edges `:1165-1180`, net `:1219-1312`) |
| A net's power copies to every internal side; a vanilla-wire answer is −1 | `RedstoneTrack.java:1103-1106`, `:780-793` |
| Same-face segments (a flat corner) are one net | `RedstoneTrack.java:1219-1312` |
| Track shape and empty collision | `RedstoneTrack.java:236-260`, `:394-408` |
| `canBePlacedOnFace`, `dropList` | `RedstoneTrack.java:346-355`, `:371-378` |
| `getSignal` / `getDirectSignal` / `isSignalSource` | `RedstoneTrack.java:430-440` |
| Track power on a net, vanilla wire −1 | `RedstoneTrack.java:780-793` |
| Net solve: track/wire −1, bridge relay/other source unchanged | `RedstoneTrack.java:1062-1134` |
| Placement + first segment; pen click | `RedstonePenItem.java:107-151` |
| A drawn segment auto-joins the adjacent ones that point back (the corner gesture) | `RedstoneTrack.java:838-857`, `:886-944` |
| Add/remove segments, material use, last-wire removal | `RedstoneTrack.java:551-587`, `:886-944` |
| Empty-hand remove, left-click remove | `RedstoneTrack.java:473-477`, `RedstonePenItem.java:227-245` |
| Relay powered from all non-output sides; 15 on output; 2-tick fall | `CircuitComponents.java:458-505` |
| Inverted relay: 15 while unpowered; 2-tick delay turning the output off | `CircuitComponents.java:512-549` |
| Bistable relay: toggles `STATE` on a rising input edge, holds on falling | `CircuitComponents.java:552-584` |
| Pulse relay: `STATE` high for 2 ticks after a rising edge, then clears | `CircuitComponents.java:587-628` |
| Bridge relay input −2, left-to-right pass | `CircuitComponents.java:631-739` |

The full extraction is in
[`../_shared/upstream/redstonepen/FINDINGS.md`](../_shared/upstream/redstonepen/FINDINGS.md).
Slice 2 re-read `CircuitComponents.java:456-628` at `f8a37d5…` for the four
relay classes; the clone was deleted again after the line-numbered extraction
above.

## Port structure

| Path | Role |
|---|---|
| `mixins/redstonepen/runtime.rs` | pure model: `Dir`, `Axis`, `Shape` (two or three arms), `ClickMode`, `ClickAction`, `Supply`, `RelayKind`, `RelayPhase`, `placement_axis`, `click_action`, `connected`, `track_power`, `track_signal`, `relay_output`, `relay_variant_signal`, `relay_edge`, `relay_delayed_off`, `relay_notifies`, `relay_*`; 38 unit tests |
| `mixins/redstonepen/support.rs` | game glue: pen click (draw, extend, bend, widen, sneak erase), signal reads, dirty sets, bounded level-tick solve |
| `mixins/redstonepen/{signal,source,neighbor,changed,state_for_placement,use_item_on,tick_post}.rs` | one hook adapter each |
| `mixins/redstonepen/horizon-mixin.toml` | two `[[module]]`s + seven `[[injection]]`s |
| `content/` | content mod (`horizon-mod.toml`, assets, data) for the pen, the track block (straight, 12 corners and 20 Ts) and all four relays |
| `content/redstonepen.hmod` | built deliverable (see the verification table) |
| `tools/` | stage/check/test/build, e2e, binding check, packaging |

### Hook map (pin `c9d474d`)

| Injection | Hook | What it does |
|---|---|---|
| `block-signal` | `block_signal` | a track answers `power` on all its arms; a relay 15 on its output; direct and weak both (upstream `getDirectSignal` = `getSignal`) |
| `signal-source` | `is_signal_source` | tracks and relays |
| `neighbor-changed` | `neighbor_changed` | queue the track/relay for the next level tick |
| `block-changed` | `block_changed` | first evaluation + re-evaluate the neighbours, with the `writing` sentinel so `native_set_block`'s self-report is not a placement |
| `state-for-placement` | `state_for_placement` | a placed track is a straight track on the look's axis; a relay outputs away from the support |
| `use-item-on` | `use_item_on` | pen: draw on the clicked face, extend an arm, bend a one-wired-arm track, widen a two-wired-arm track into a T, sneak-erase |
| `level-tick-post` | `level_tick_post` | bounded relay/track solve and the neighbour notification |

### Signal model (runtime.rs)

- `Supply::{None,Track,Wire,Source}` + `supply_power` (`runtime.rs:447`):
  `Track(p)`/`Wire(p)` → `p-1`, `Source(p)` → `p`, `None` → 0.
- `track_power` (`:457`): the strongest arm of the net, over however many arms
  it has (2 or 3).
- `track_signal` (`:466`): answer on any of the track's arms (a corner on both,
  a straight track on its two axis faces, a T on all three).
- `Shape` (`:145-260`): two or three canonically ordered arms at one power
  (`straight`, `corner`, `tee`); `arms`, `arity`, `has_arm`,
  `bend_name`/`from_bend` (the state's `bend` value), `axis` (the state's
  `axis`). `CORNER_NAMES` (`:270`) and `TEE_NAMES` (`:286`) are the 12 + 20
  property names.
- `relay_output`/`relay_variant_signal` (`:542`, `:554`): the per-variant output
  face truth table.
- `relay_input`/`relay_powered` (`:475`): max of the five input sides.
- `placement_axis` (`:622`): project the look onto the clicked face's plane,
  dominant in-plane axis; a look along the normal falls back to the horizontal
  axis (x on N/S walls, z on E/W walls and floors).
- `click_action` (`:373`): an arm click extends; a side click bends a track
  with exactly one wired arm (keeping it and refusing its back), adds a third
  arm when both arms are wired (a T), refuses a fourth arm, and erases an
  unwired track. `ClickMode::Erase` (the sneak modifier, `:339`) drops the
  clicked arm of a T (`Narrow`, `erase_action`/`drop_arm` at `:413`/`:421`) or
  removes the cell.
- `connected` (`:681`): `other` is on one of this shape's arms **and** the
  other shape holds the arm that points back.
- `relay_facing` (`:690`): output away from the support.

### Corner and T model (slices 3-4)

Upstream does not have a "corner block": its block entity holds one wire
segment per (face, in-plane direction), and segments are joined by
`INTERNAL_EDGE_CONNECTION_MAPPING` (perpendicular faces meeting at a block
edge, `RedstoneTrack.java:145-160`) or by sharing a face. `updateConnections`
groups those segments into nets (`:1146-1316`), and every net has one power
copied to all its internal sides (`:1103-1106`; `net.power` is the strongest
internal side at `:1312`). A net of two segments is therefore exactly **two
arms at one power**, and a net of three is **three arms at one power** — the
model this slice uses:

- `Shape::straight(axis)` is two opposite arms, `Shape::corner(a, b)` two
  perpendicular arms, and `Shape::tee(a, b, c)` three distinct arms;
  3 + 12 + C(6,3)=20 = **35 shapes**. That is the largest net a block state
  can hold here: a fourth arm needs 15 more bend values and 2304 states, over
  the ~2000 budget.
- A corner and a T answer and feed on all arms with the same power (in on one
  arm, out on the others), and a track step costs one once per block, so a T
  fed on its west arm reads 15 (west straight), **14** (the T), 13 (east
  straight) and 13 (south branch) — both derived arms are equal, and each arm
  is independent: cutting one leaves the other lit (see the e2e).
- The content state is the template's `axis` plus the declared `bend` (33
  values: `none` + 12 pairs + 20 triples) plus `power` → **1584 states**, 35
  blockstate variants, all bound. `axis` is redundant for a corner or T, but
  `shapes` in a content file must be keyed on a **template** property, and
  `minecraft:basalt` (a behaviour-free `RotatedPillarBlock` with `axis`) is the
  only property a thin collision shape can follow; the corner's/T's collision
  bar follows its first arm while the model shows all arms exactly.
- The pen's corner gesture mirrors upstream's `modifySegments` add
  (`RedstoneTrack.java:826-857`, `:934-944`): clicking a track on a face that
  is not an arm adds that arm (upstream adds the clicked in-plane segment and
  the adjacent segments that point back). This slice keeps the arm that is
  already wired and drops the unwired one; with **both** arms wired the click
  adds the clicked face as a third arm, which is the T draw gesture; a fourth
  arm is refused. Clicking an arm extends a straight track into the next cell.
- The erase gesture mirrors upstream's separate remove path
  (`useWithoutItem` `:473-477`, the pen's `attack` `RedstonePenItem.java:227-245`,
  both `modifySegments(..., no_add=true, no_remove=false)`): a **sneak click**
  with the pen drops the clicked arm of a T, or removes the whole cell.
  Upstream removes the clicked segment; this port's cell has at most three
  arms, so removing one arm of a pair would leave a one-arm stub the state
  model does not carry, and the cell is removed instead. A sneak click on a
  support draws nothing. The sneak state is read from the local input
  (`support.rs`'s `sneaking`, `game.keys` + `KeyCode::ShiftLeft`, the same test
  the pin's placement code uses at `game.rs:6492`) because the pin's
  `NativeServerPlayer` has no sneaking field.

### Relay variants (slice 2)

Upstream registers five directed components (`ModContent.java:59-90`); slice 2
ships the four that do not need per-side wire reads. Each is a separate block
with the plain relay's `facing` (this slice's **output** face) plus:

| Variant | State | Output on the output face | Edge / timing | Upstream |
|---|---|---|---|---|
| plain `relay` | `powered` | `powered` | on immediately; off 2 ticks after the input falls | `:474-505` |
| `inverted_relay` | `powered` | `!powered` | off immediately when the input falls (output on); on 2 ticks after the input rises (output off) | `:518-549` |
| `pulse_relay` | `powered`, `state` | `state` | `state=1` on a rising input edge, cleared 2 ticks later; `powered` follows the input | `:593-628` |
| `bistable_relay` | `powered`, `state` | `state` | `state` toggles on a rising input edge and holds when the input falls; `powered` follows the input | `:558-584` |

The 2-tick deadlines live in `support.rs` (`Pending`/`PendingWrite`, advanced at
`level_tick_post`), so the pure runtime stays time-free:
`RELAY_OFF_DELAY_TICKS = PULSE_TICKS = 2` (`runtime.rs`). The output face is the
same `facing` the plain relay already uses; upstream derives it from
`FACING`+`ROTATION` (`CircuitComponents.java:377-381`), which this slice does
not carry.

Truth-table helpers: `relay_output` (plain = input, inverted = `!input`, pulse
and bistable = `state`), `relay_edge` (bistable toggles on rising, pulse arms,
plain/inverted track the input), `relay_delayed_off`, `relay_notifies`.

### Solve (support.rs)

- `signal_toward` (`support.rs:313`) rebuilds the per-direction read the pin
  lacks: our track/relay from state, a vanilla wire from `power`, everything
  else from `Behaviours::facts(state).weak` (another mod's `block_signal` is
  invisible — the same recorded deviation as the other redstone ports).
- `advance_pending` (`:666`) applies scheduled relay writes at the start of the
  level tick; `solve_relays` (`:724`) reads the five non-output sides and routes
  each variant through `relay_edge`/`relay_delayed_off`/`relay_notifies`.
  `track_supply` (`:349`) reads every arm (a connected track, a wire, or a
  source) and `wired_arms` (`:381`) tells the pen's bend/widen gesture which
  arms are wired; `sneaking` (`:410`) is the erase modifier.
  `solve_tracks` (`:821`) is a bounded worklist (`MAX_STEPS = 16_384`,
  `MAX_VISITS = 8`) that re-enqueues every connected track on an arm
  and any relay reading it. `write` (`:885`) uses `native_set_block(...,
  UPDATE_ALL)`, the `writing` sentinel, and (when `notify`)
  `native_update_signal_neighbors`.
- Pending writes (`Pending`/`PendingWrite`) model upstream's `scheduleTick`:
  plain falling output and inverted rising output wait `RELAY_OFF_DELAY_TICKS`,
  the pulse's `state` clears after `PULSE_TICKS`; `or_insert` keeps the earliest
  deadline, like `hasScheduledTick`.
- Relays run before tracks in a level tick, so a relay change reaches the first
  track past it in the same tick.

## Content

| Id | Kind | State | Template / model |
|---|---|---|---|
| `redstonepen:track` | block | `axis` x/y/z (template) × `bend` 33 (none + 12 corner pairs + 20 T triples) × `power` 0..15 = 1584 | `minecraft:basalt`; a 2-px bar per axis, two half-bars per corner, three per T |
| `redstonepen:relay` | block | `facing` (template) × `powered` = 12 | `minecraft:end_rod`; a 2-px plate per face |
| `redstonepen:inverted_relay` | block | `facing` × `powered` = 12 | same plate model |
| `redstonepen:pulse_relay` | block | `facing` × `powered` × `state` 0/1 = 24 | same plate model |
| `redstonepen:bistable_relay` | block | `facing` × `powered` × `state` 0/1 = 24 | same plate model |
| `redstonepen:pen` | item | — | `minecraft:stick` (stack 1) |

- Blockstates: track binds 35 variants (3 straight by `bend=none,axis`, 12
  corners and 20 Ts by `bend`); relay 12; inverted 12; pulse 24; bistable 24.
  Offline binding over the client's `model-states.json`: **track 1584/35/0**,
  **relay 12/12/0**, **inverted 12/12/0**, **pulse 24/24/0**,
  **bistable 24/24/0** (states/variants/unbound, every state matching exactly
  one variant).
- The 12 corner models are two 2-px half-bars from the cell's centre to each
  arm's face (`track_<arm>_<arm>.json`), and the 20 T models are three
  (`track_<arm>_<arm>_<arm>.json`); the collision/interaction shape still
  follows the `axis` bar, because a content `shapes` table can only be keyed on
  a template property. The visual model is exact; the collision bar covers the
  first arm.
- Loot: track drops one `minecraft:redstone`; each relay drops itself. All are
  in `minecraft:mineable/pickaxe` (the utilitarian finding: a mod block needs a
  mineable tag to drop).
- Recipes: the pen is a simplified shapeless recipe (redstone block + feather;
  upstream's custom extended shapeless recipe is not a Horizon type); the relay
  keeps upstream's `CCT`/`CCD` shaped recipe; the variants use upstream's
  one-item shapeless chain (`relay → inverted → bistable → pulse`) as plain
  `minecraft:crafting_shapeless` (the extended type is not available).
- Creative tab `itemGroup.tabredstonepen` with the pen and all four relays.
- **Visual deviation:** the upstream custom track/relay renderers are not
  carried. The models are vanilla-texture placeholders (`minecraft:block/
  redstone_block` bars and plates), so a track is a red bar rather than the
  thin dust overlay, and the relays do not change appearance with their state.
  No Minecraft asset is invented or shipped; only vanilla texture references
  are used.

## Verification

| Check | Command | Result |
|---|---|---|
| fmt | `cargo fmt --all --check` | exit 0 |
| runtime tests | `cargo test --locked --offline` | exit 0, **38 passed / 0 failed** |
| clippy | `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | exit 0, **0 warnings** |
| stage | `HORIZON_MIXINS=… python3 tools/horizon.py stage --horizon …` | exit 0, `stage_changed: true` (~62.3 s) |
| staged check | `… check` | exit 0, **0 warnings** (`cargo_exit: 0`, 43.7 s, `target/horizon-check.log`) |
| staged test | `… test` | exit 0, **38 passed / 0 failed / 531 filtered**, 0 warnings (`cargo_exit: 0`, 80.3 s, `target/horizon-test.log`) |
| staged build | `… build` | exit 0, **0 warnings** (5m10s), `target/horizon-cargo/fast/horizon-client` (81,654,440 B) |
| `.hmod` build | `cargo-horizon-mod build` (content folder, pinned tool) | exit 0, **68 files** (48 + the 20 T models) |
| `.hmod` check | `cargo-horizon-mod check content/redstonepen.hmod` | exit 0, `ok:true`, **25,551 bytes**, SHA-256 `6d1402c8bc187339db7db327b2867b40f400e026b15fc530fdf3e4fd6db14ff4`, 68 entries, unsigned, content only |
| content warnings | `…/horizon-mod-content-warnings.txt` | **0 bytes** |
| state binding | `python3 tools/check_binding.py /tmp/opencode/redstonepen-model-states.json` | **PASS**, track **1584/35/0**, relay 12/12/0, inverted 12/12/0, pulse 24/24/0, bistable 24/24/0 (states/variants/unbound; every state matches exactly one variant) |
| e2e | `tools/e2e.sh` | **PASS**, exit 0 (`/tmp/opencode/redstonepen-slice4-e2e-default.log`) |
| e2e T self-test | `SELFTEST_T=1 tools/e2e.sh` | **exit 1**, `FAIL: expected lamp at 6,64,-8 lit=false, got …lit=true` (`…-selftest-t.log`) — the T-lamp check can fail |
| e2e erase self-test | `SELFTEST_ERASE=1 tools/e2e.sh` | **exit 1**, `FAIL: expected lamp at 14,64,-16 lit=true, got …lit=false` (`…-selftest-erase.log`) — the sneak-erase check can fail |

The slice-3 counterfactual (plain `minecraft:stick` draws no track) and the
slice-3 `SELFTEST`/`SELFTEST_RELAY`/`SELFTEST_CORNER` runs remain as recorded
earlier; the pen item gate and those assertions are unchanged in slice 4.

The slice-4 TDD RED run is recorded too: the five new tests (`Shape::tee`
names/arms, the 1584-state bound, three-arm signal/power, T connectivity, and
the widen/narrow/erase click table) were written first against a `Shape` that
had no `tee`/`ClickMode`/slice `track_power`; `cargo test` failed with **102
compile errors**, and the implementation made **38/38** pass. Slice 3's RED run
(`Shape::corner` returning `None`, `track_signal` 0, `connected`/`click_action`
stubbed) was **11 failed / 21 passed**.

### E2E evidence (The End, creative, headless, `--dev-mod content/`)

| Scenario | Observed |
|---|---|
| 5 tracks from a redstone block | lamp `lit=true`; tracks `15,14,13,12,11` |
| cut track 3 | lamp `lit=false`; track 5 `power=0`; restore → `lit=true`, `11` |
| 20 tracks | far lamp `lit=false`; track 5 `11`, track 15 `1`, track 16 `0`, track 20 `0` |
| relay after 10 decayed tracks | relay `powered=true`; track 10 `6`, track 12 `15`, track 20 `7`, lamp `lit=true` |
| relay replaced by a plain track | track 20 `0`, lamp `lit=false` (counterfactual) |
| **corner L** (source → straight → corner `bend=north_west` → straight → lamp) | track `15`, corner `14`, far straight `13`, lamp `lit=true`; the corner state is `axis=z,bend=north_west,power=14` |
| **cross-axis control** (a straight track at the turn) | far straight `0`, lamp `lit=false` |
| **wrong bend control** (`bend=north_east`) | far straight `0`, lamp `lit=false` |
| **cut the corner** | lamp `lit=false`, far straight `0`; restore → lamp `lit=true`, corner `14`, far straight `13` |
| **stone at the turn** | lamp `lit=false` |
| **straight control** (two straight tracks) | track `15`, track `14`, lamp `lit=true` — the same one-step-per-block decay without a corner |
| **T junction** (source → west straight → T `bend=south_west_east` → east straight + south branch) | west `15`, T `14`, east `13`, south branch `13`, **both** lamps `lit=true`; the T state is `axis=z,bend=south_west_east,power=14` |
| **cut the T's east arm** | east lamp `lit=false`, south lamp `lit=true`, branch still `13`; restore → both lit |
| **cut the T's south arm** | south lamp `lit=false`, east lamp `lit=true`; restore → both lit |
| **T missing the fed arm** (`bend=north_south_east`) | T `0`, both lamps `lit=false` |
| **T with the wrong third arm** (`bend=north_west_east`) | T `14`, east lamp `lit=true`, south lamp `lit=false`, branch `0` |
| **straight at the T junction** (`bend=none`) | east lamp `lit=true`, south lamp `lit=false`, branch `0`; restoring the T lights both again |
| **pen widen → T** (two-wired-arm side click) | straight `axis=x` → `axis=z,bend=north_west_east` at `14`; the new north branch is `13` and both lamps light |
| **pen sneak-narrow** (T's south arm wired to a vanilla wire) | `bend=south_west_east,power=14` → `axis=x,bend=none,power=14`; the east lamp stays `lit=true` (neighbours untouched) |
| **pen sneak-erase** (wired T cell) | the T becomes `minecraft:air`; both its lamps go `lit=false` and the branch drains to `0` |
| **pen sneak on a support** | no track is drawn (`16,64,-16` stays `minecraft:air`) |
| stone between source and lamp | lamp `lit=false` |
| unpowered track line | lamp `lit=false`, track 5 `0` |
| inverted relay, input powered | `powered=true`, lamp `lit=false` |
| inverted relay, input removed | `powered=false`, lamp `lit=true` |
| plain relay control, input removed | `powered=false`, lamp `lit=false` |
| pulse relay after a rising edge | `powered=true,state=0` while the input is still powered; the bistable captured the pulse (`state=1`) and its lamp `lit=true` |
| pulse input removed / re-applied | bistable holds `state=1,lit=true`, then toggles to `state=0,lit=false` on the next pulse |
| bistable relay | rising edge → `state=1,lit=true`; input falls → `powered=false,state=1,lit=true` (holds); next rising edge → `state=0,lit=false` |
| pen click | straight track `axis=z,bend=none,power=0` at the target; end click extends to the next block; top click erases an unwired track |
| **pen-drawn L** | click a support (look west, down) → `axis=x`; east end click → the next straight; north side click → `bend=north_west`; north click again → a `z` straight; redstone block + lamp → tracks `15,14,13`, lamp `lit=true`; cutting the corner → lamp `lit=false` |
| states | 48 straight + 192 corner + **320 T** + 72 relay states accepted; `[axis=w]`, `[bend=sideways]`, `[bend=north_west_east_up]` (four arms), `[bend=north_south]` (opposite pair), `[channel=3]`, `pulse_relay[state=2]` and `inverted_relay[state=1]` rejected |
| items | `give` resolves for the pen and all four relays |

The signal rigs are **differentials** (stone vs track; relay vs plain track;
5-track vs 20-track; inverted vs plain; pulse vs the plain control; a corner vs
a straight track at the turn; a corner vs the wrong bend; a cut corner; a
straight two-track line vs the corner L; a T vs each cut arm, a T missing the
fed arm, a T with the wrong third arm and a straight track at the junction),
and `SELFTEST`/`SELFTEST_CORNER`/`SELFTEST_T`/`SELFTEST_ERASE` corrupt an
expectation on purpose (the new two were run in slice 4; the first two remain
slice-3 evidence). `COUNTERFACTUAL=1` with a plain stick is the pen's
differential: the support click draws nothing while the same setblock rigs
still pass.

## Gaps and deviations

1. **One track per block, two or three arms** (the brief's documented
   simplification): no multiple tracks/connectors in one cell, no per-face
   attachment, and no fourth arm (a cross/4-way junction is refused). Upstream's
   24-segment block entity is not expressible in states; a corner is the
   two-arm net that fits, a T the three-arm net (1584 states).
2. **Corner/T collision box:** the state's `axis` follows the first arm and the
   content `shapes` bar covers that arm only; the model shows all arms. Content
   `shapes` can only be keyed on a template property, so a per-shape collision
   box would need a 35-valued template property (no behaviour-free vanilla block
   has one).
3. **Pen gestures:** an arm click extends; a side click bends a track with
   exactly one wired arm (dropping the unwired arm), widens a two-wired-arm
   track into a T, and refuses a fourth arm or a U-turn; an unwired track's
   side click erases it. A **sneak click** is the explicit erase modifier: it
   drops the clicked arm of a T (leaving the other two as a corner or straight)
   or removes the cell, so a wired track no longer needs its neighbours cut
   first. Upstream removes just the clicked segment; this port removes the cell
   for a two-arm track because a one-arm stub is not in the state model (a
   one-arm net would add 6 bend values → 1872 states, still bounded, but it is
   not carried). The sneak flag is read from the local input state, not the
   acting player (the pin's `NativeServerPlayer` has no sneaking field), so a
   dedicated-server guest's sneak is invisible; the gesture is local/creative
   only, like the rest of the pen.
4. **Relay scope:** plain, inverted, pulse and bistable relays are carried;
   **bridge relay is blocked** (needs a `rotation`/left-right state and the
   per-side direct read `getDirectSignal(wire, side) − 2` plus
   `RedstoneTrack.getRedstonePower(side, true) − 2`, `CircuitComponents.java:
   631-739`; the pin exposes no per-side direct read, so only the four plain
   variants are implemented). `facing` is the output direction (upstream's is
   the mounting face plus a rotation), and the 2-tick deadlines are modelled as
   level-tick pending writes rather than vanilla block ticks.
5. **Pen economy:** the pen does not consume redstone or wear (upstream's
   durability-256 pen and inventory quill accounting are not carried).
6. **Support rule:** a track does not require or lose its supporting face; the
   track is free-standing. Upstream removes the face elements whose support is
   gone.
7. **No custom renderer/particles/overlay:** vanilla redstone-block textures as
   placeholder bars/plates; no lit tint on any relay and no distinct variant
   models; no pen HUD overlay or sneak signal readout.
8. **Content recipes:** the pen recipe is simplified from the custom extended
   shapeless recipe; the variant recipes use plain shapeless conversion instead
   of `redstonepen:crafting_extended_shapeless`; no repair recipes.
9. **Another mod's `block_signal` is invisible** to the solve (the pin has no
   per-direction read for mods; rebuilt from `facts.weak`, the same deviation
   recorded by morered/utilitarian). Comparator output is not special-cased.
10. **Out of scope:** control box, gauge, lever, buttons, remote, RCA bridge,
    coloured tracks, track bundles, the quill item.

## Upstream analysis (slices 3-4)

Re-cloned `stfwi/redstonepen` at `f8a37d50969131d22a7f51a7ae0a54d34690e945`
(`neoforge-1.21`, shallow, under the git-ignored
`ports/_shared/upstream/redstonepen/`). Slice 3 read `RedstoneTrack.java`
`:71-158` (segment/connector bit maps and the internal edge pairs), `:780-793`
(`getRedstonePower`), `:830-944` (`modifySegments`/
`isAdjacentWireSegmentAddable`), `:1062-1134` (`handleNetNeighborChanged`) and
`:1146-1312` (`updateConnections` net list), plus
`RedstonePenItem.java:107-151`. Slice 4 re-read the net/pen/add-remove paths:

- `STATE_FLAG_WIR_COUNT = 24`, `CON_COUNT = 6`, the 24 `(face, direction)`
  wire bits and the 12 `INTERNAL_EDGE_CONNECTION_MAPPING` pairs
  (`RedstoneTrack.java:71-160`);
- `updateConnections` (`:1146`): internal-edge groups are unioned
  (`:1165-1180`), each group plus its external routes becomes one net
  (`:1208-1316`), and the net's starting power is the strongest of its
  internal sides (`:1312`). Same-face wires share the face's route group, so a
  flat corner or T is one net; perpendicular faces joined at an edge are one
  net too;
- `handleNetNeighborChanged` (`:1062`): `pmax` is the strongest of **all** the
  net's neighbours (`:1070-1096`), the net stores it (`:1098-1101`) and copies
  it to **every** internal side (`:1103-1106`) — a T is three arms at one
  power;
- `modifySegments` (`:826-931`) toggles the clicked wire bit and, when adding,
  also adds the neighbouring segments that point back (`:830-857`,
  `isAdjacentWireSegmentAddable` `:934-944`);
- the explicit remove is a separate gesture: `useWithoutItem` (`:473-477`) and
  the pen's left-click `attack` (`RedstonePenItem.java:227-245`) both call
  `modifySegments(..., no_add=true, no_remove=false)`; `modifySegments` also
  drops a face's left-over bulk connector (`:860-880`).

Behaviour extracted: a net is a group of segments joined by a shared face or an
internal edge; it takes the strongest external supply (track/wire −1, other
source full) and copies that one power to every internal side; a pen add joins
the clicked segment plus the adjacent segments that point back. The port's
`Shape` is that net reduced to the two or three connections a single block
state can hold; no Java was copied. The clone is kept at the pinned commit in
the git-ignored upstream folder for the next slice (it is not shipped).

## Skills used

- superpowers `test-driven-development` (slice 1: six cycles, 17 tests; slice 2:
  seven RED→GREEN tests for the relay variants; slice 3: eight RED→GREEN tests
  for `Shape` names/arms, corner signal and power, corner connectivity, the
  pen's extend/bend/erase/refuse gesture and the state's `axis`; slice 4: five
  RED→GREEN tests for T names/arms, the bounded 1584-state model, three-arm
  signal/power, T connectivity, the widen/narrow/erase click table — the RED
  run was 102 compile errors against the missing `Shape::tee`/`ClickMode`/
  `track_power(&[..])` API, then 38/38 green), the
  `systematic-debugging` loop for the e2e acceptance-row cross-talk, the
  pen-face visibility in `ctl interact` and the erase-vs-bend ordering, and
  `verification-before-completion` for every claim above.
- Beautiful Rust `rust-best-practices` (read
  `ports/_shared/skills/BeautifulRust/skills/rust-best-practices/SKILL.md`):
  expressive types (`Dir`, `Axis`, `Shape` with a private padded arm array and
  an `Arity` enum, `ClickMode`, `ClickAction`, `Supply`, `RelayKind`,
  `RelayPhase`), fallible `Shape::corner`/`Shape::tee`, a slice-returning
  `Shape::arms` (no allocation in the hot solve), `Result`-free pure helpers
  with no panics in the model, narrow imports, no `unsafe`, `fmt` + clippy
  `-D warnings` clean.

## Re-pin to `83ad384f` and the exact corner/T collision (slice 5)

**Class / tier:** `content` / `logic` (ledger). Approach from
`horizon/docs/modpacks.md`: carry-over for the content, one agent change for the
behaviour the new content schema makes expressible; no extra client or server
half. Licence unchanged (MIT; local-only until publishing is approved).

Migrated to Horizon `83ad384f48d0742746d518737511882680aab5b7` / horizon-mixins
`e8cd0aa38cffef49cb2e088ef716baa107148c02`:

- `compatibility.json`: new `horizon_revision`, `game.rs` hash `996432b9…`,
  new `horizon_mixins_revision`, `mixin_schema: 6` unchanged.
- `mixins/redstonepen/horizon-mixin.toml`: same revision.
- `tools/horizon.py`: `test` now runs `-p horizon-client --lib` (the game lives
  only in the library target at this pin, so `--bin` reported 0 tests
  vacuously); `check`/`build` keep `--bin horizon-client`; new `ctl` action
  builds `-p horizon-client-lib --bin horizon-ctl`.

### Feature: `collision_shapes` keyed on `("bend", "axis")`

The slice-1-4 gap 2 was that a content `shapes` table could only key a
**template** property, so a corner's or T's collision bar followed `axis` (its
first arm) while the model showed all arms. At `83ad384f` a shape table takes an
ordered tuple of template **and declared** properties, and a block has an
independent `collision_shapes` table (`crates/horizon-mods/src/content.rs:150-158`,
`mod_content.rs:1618`, `docs/mods.md:173-203`).

`tools/gen_track_shapes.py` regenerates both tables from the block's own `bend`
list: one 2-px half-bar from the cell centre to each arm, a straight track
keeping its full bar — 33 bends × 3 axes = **99 entries**, identical outline and
collision. The state set is unchanged (1584 states, 35 variants), so no id moves.

`content/data/redstonepen/horizon/block/track.json` was regenerated
(`shapes` + `collision_shapes`, `properties: ["bend","axis"]`) and the delivered
`content/redstonepen.hmod` rebuilt.

### Verification

All runs on 2026-11-06, `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, under the
campaign build lock, against the pins above.

| Check | Command | Result |
|---|---|---|
| stage | `tools/horizon.py stage --horizon <pin>` | exit 0, `stage_changed: true`, revision `83ad384f…` |
| check | `tools/horizon.py check` | `cargo_exit: 0`, **0 warnings** |
| test | `tools/horizon.py test` | `cargo_exit: 0`, **38 passed; 0 failed; 913 filtered out**, 0 warnings (`--lib`) |
| build | `tools/horizon.py build` | `cargo_exit: 0`, **0 warnings**, 18m22s, `target/horizon-cargo/fast/horizon-client` (96,217,784 B) |
| content build | `cargo-horizon-mod horizon-mod build` (pin-built tool) | exit 0, `Built ./target/horizon-mod/redstonepen.hmod` |
| content check | `cargo-horizon-mod horizon-mod check content/redstonepen.hmod --json` | `ok: true`, 26,996 B, SHA-256 `e35a0b9d…938c`, 68 entries, unsigned |
| state binding | `tools/check_binding.py /tmp/opencode/redstonepen-model-states.json` | **PASS**: track 1584 states / 35 variants / **0 unbound** (every state matches exactly one variant), relays 12/12/0, 12/12/0, 24/24/0, 24/24/0 |
| e2e | `tools/e2e.sh` | **PASS**, exit 0 (`/tmp/opencode/rp-e2e-default.log`): the 5/20-track lines, relay, corner, T, pen gestures, sneak erase, relay variants, and 48 straight + 192 corner + 320 T + 72 relay states all pass |

The e2e log's plan `horizon-ctl` came from the `heyberryshutup` pilot's pin build
(the control client carries no mod and is pin-compatible); the port's own
`tools/horizon.py ctl` action is the documented way to build it.

Gap 2 is closed. Still open: the fourth arm (the pin's limit is 2^20 states, so
2304 is legal), the bridge relay (per-side direct read still absent), pen
economy, the custom renderer (needs a block-entity template), and the
comparator read for neighbouring blocks.

## Re-pin to Horizon `1d63e0a` and artifact rebuild (2026-11-09)

Mode A migration: the port pinned `83ad384f` and shipped only the `.hmod`; the
`.hnmod` half is new. The stamped hook catalog is byte-identical between the two
pins, so the seven injections, two modules and every binding are unchanged.

| | Before | After |
|---|---|---|
| Horizon | `83ad384f48d0742746d518737511882680aab5b7` | **`1d63e0a5a37a7a708eb0b32b49bf0dfeb28ed8a9`** |
| horizon-mixins | `e8cd0aa38cffef49cb2e088ef716baa107148c02` | **`5a64af938ec86b00d53d16a408aabc0038fb3e57`** |
| `game.rs` SHA-256 | `996432b9…296463` | `64813cf4db0b9c3983d42908609bb633b9fbbc705f0c890396f940a6c7cef80f` |

### Ladder (through `ports/tools/build-slot.sh`, `CARGO_INCREMENTAL=0`)

| Step | Result |
|---|---|
| `cargo fmt` + `--check` | exit 0 |
| `cargo clippy --all-targets -- -D warnings` | exit 0, **0 warnings** |
| `cargo test --locked --offline` | **38 passed**, 0 failed |
| `horizon.py stage` | exit 0 at `1d63e0a` |
| `horizon.py check` | `cargo_exit: 0`, **0 warnings** (80 s) |
| `horizon.py test` | **38 passed**, 1240 filtered (169 s) |
| `horizon.py build` (fast) | `cargo_exit: 0` (477 s) |

### Delivered artifacts (reproducible)

| File | Size | SHA-256 | Entries |
|---|---:|---|---:|
| `content/redstonepen.hmod` | 26,996 B | `e35a0b9dd23bec012f3a569ef395787a031fd1f74c6d9d0203fd8a46c918938c` | 68 |
| `redstonepen.hnmod` | 23,558 B | `639fd9f4674c6729fa1b486ae6dbdbcbad43da94022b93330f30a70650a78009` | 11 |

The `.hmod` is now built and checked by the pin's own `cargo-horizon-mod`
(`no code: content only`); its bytes match the previous package. The `.hnmod`
is `tools/package.py`'s deterministic archive. A second `tools/package.sh` run
reproduced both hashes.

### In-game e2e (fast build, shared `horizon-ctl`, The End)

| Leg | Verdict |
|---|---|
| default `tools/e2e.sh` | `PASS` + `EXIT:0` (`e2e.log`) |
| `SELFTEST=1 tools/e2e.sh` | `FAIL: expected lamp at 21,64,6 lit=true, got 'minecraft:redstone_lamp[lit=false]'` + `EXIT:1` (`e2e-negative.log`) |

`python3 ports/tools/check-e2e-logs.py redstonepen --strict` → 2/2 ok.

### Pin-drift fix in the harness

At `1d63e0a` the content overlay is built in `.horizon-cache/mod-content/<stamp>/`
(one folder per mod set, `mod_content.rs` `resource_root`), so the e2e's copies
of `model-states.json` and `horizon-mod-content-warnings.txt` are now located
with `find` under `mod-content/` instead of the old flat path. No port code
changed; this keeps the offline binding check fed.

### Submission gate

`python3 ports/tools/upstream_ready.py redstonepen` exits 1 on the
split content+code rule (one file per registry project). Everything else the
tool checks is green.

Skills: superpowers/using-superpowers,verification-before-completion,systematic-debugging; BeautifulRust/rust-best-practices

> **2026-11-10 (after this record).** A parallel campaign session converged this
> port to the registry's one-file rule by embedding the carried content in the
> `.hnmod` (`mixins/<id>/embedded_content.rs` + `mixins/<id>/data/<id>-content.hmod`).
> The `.hnmod` size/SHA-256 above is therefore from this job's split build and is
> **superseded** by that port's own later record; the pin migration, the ladder
> results and the e2e findings in this section describe the code that was tested
> here and remain the basis for the harness fixes listed.

## Embedded content — one `.hnmod` deliverable (2026-11-10, done and verified)

The registry takes one `file` per version, so a split content+code port cannot
be listed (`ports/_shared/horizon-mods/AGENT_PLAYBOOK.md` §1.0). The port now
ships its content inside the native bundle, the AE2 pattern proved on
`charginggadgets` (ARCHITECTURE.md §8):

- `mixins/redstonepen/data/redstonepen-content.hmod` — the built
  `content/redstonepen.hmod`, byte for byte (26,996 B);
- `mixins/redstonepen/embedded_content.rs` — `native_data!` compiles the file
  in, writes `.horizon-cache/redstonepen-embedded-content.hmod` when the copy on
  disk differs, appends it to the dev list and breaks with
  `content_root(root, enabled, &with)`;
- one `[[injection]] id = "embedded-content"` on
  `horizon_client::game::mods::content_root` (`kind = "HEAD"`, `cancellable =
  true`, bindings `root`/`enabled`/`dev`);
- `tools/package.py` packs `mixins/<id>/data/` and refuses a data file that
  differs from `content/<id>.hmod`.

`content/redstonepen.hmod` stays as the input of the embedding; the deliverable
is one file:

| Artifact | Size | SHA-256 | Entries |
|---|---:|---|---:|
| `redstonepen.hnmod` | 39,886 B | `215bf23578f0f2c7e8df253da00190d237edb5d20a72c4a1b88f40fac2915e06` | 13 |
| …of which `mixins/redstonepen/data/redstonepen-content.hmod` | 26,996 B | `e35a0b9d…c918938c` (unchanged content) | — |

`cargo-horizon-mod horizon-mod check redstonepen.hnmod --kind native-mod` →
`redstonepen 0.1.0 (native-mod, format native-1/1d63e0a5…/5a64af93…, for 26.3)`,
39,886 bytes, same SHA-256, unsigned, **exit 0**.

Ladder at `1d63e0a` through the slot registry: `cargo fmt --check`,
`cargo clippy --all-targets --locked --offline -D warnings`, `cargo test
--locked --offline` all clean; `horizon.py stage` → `check` → `test`
(`cargo_exit: 0`, `game::redstonepen::tests`), `stage_changed: false`.
`tools/horizon.py` now defaults `--horizon`/`HORIZON_MIXINS` to the campaign pin
checkouts (`ports/_shared/pins/horizon-1d63e0a`,
`…/horizon-mixins-5a64af9`) instead of a stale `ports/horizon-mixins`, so the
standard runner can rebuild for an e2e.

`tools/test_template.py` / `tools/test_stage.py` were **not** part of this
record: at this pin the template fixture has no `native-hooks.json`, so the
planner answers `E_HOOK_CATALOG` for this port's hook injections. The untouched
reference port `charginggadgets` fails the same way at the same pin, so the
failure is a fixture limitation, not this change; the real staged
`stage`/`check`/`test` above is the stronger check and passes.

**In-game proof from one file** (`e2e-embedded.log`, `EMBEDDED=1`, exit 0): the
game is launched with **no `--dev-mod`**, the bundle writes
`26996 B at …/.horizon-cache/redstonepen-embedded-content.hmod, identical to
content/redstonepen.hmod`, and the whole port scenario passes from that file —
the 5-track line (15..11), the cut, the 20-track decay to 0 at 16, the relay
re-strengthen (12..20), the corner/T rigs with their negative controls, the pen
draw/extend/erase/bend/widen rigs, all 48/192/320/72 state sweeps and the
creative-tab/recipe checks, ending
`PASS: …` + `EXIT:0`.

**Control** (same run, before the main leg): `.horizon-cache` is a plain file in
both the scratch root and its game folder, so the helper cannot write. The same
binary then answers
`{"chat":["Unknown block type 'redstonepen:track'", …]}` to `setblock`, leaves
the track block `minecraft:air`, gives no `redstonepen:pen`, and still gives a
vanilla stick — the ids come from the embedded file, not from the hook code.

**Fail-capable selftest** (`e2e-embedded-selftest.log`, `EMBEDDED=1
SELFTEST=1`): `FAIL: expected lamp at 21,64,6 lit=true, got
'minecraft:redstone_lamp[lit=false]'`, `EXIT:1`.

`python3 ports/tools/check-e2e-logs.py redstonepen --strict` → 4 logs, all
with a matching exit status (0); `python3 ports/tools/upstream_ready.py
redstonepen` → **READY**.

Tooling note: the forwarded e2e's remote wrapper printed
`remote-slot.sh: line 156: port: unbound variable` after the run (the game leg
itself returned 0 and the log ends with the script's own `EXIT:0`); it is in
`ports/tools/`, outside this port's write ownership.

## Review repairs (PR #588)

1. Staging is the orchestrator's (horizon-mods#686).
2. *"Fix the original-mod credit link … use `https://github.com/stfwi/redstonepen`
   and credit original author Stefan Wilhelm (wile)"* — done.
3. **[P2] "Recheck relay input when a delayed turn-off is delivered … Recompute
   the current input at scheduled delivery … Add a regression covering
   loss/restoration within the delay"** — **fixed**: the delivery path no longer
   applies the captured `PendingWrite::Phase`; it recomputes the edge from the
   input it finds (`runtime::delivered_phase`, whose test
   `a_delayed_turn_off_rechecks_the_input_it_finds` covers loss and restoration
   inside the two-tick window).
4. `mixins/redstonepen/LICENSE` and `NOTICE.md` travel inside the artifact.

### Verification after the repairs

- Artifact: `redstonepen.hnmod`, 43,420 B, SHA-256 `9052a2e52d1a9ec48ffbcb7f639422a99403529029164e697ed9b896f0694e6a` (rebuilt twice, identical bytes; `tools/package.py`; the embedded content equals `content/redstonepen.hmod`).
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --locked --offline`: clean, 39 passed; the embedded run (`python3 tools/horizon.py test --horizon ports/_shared/pins/horizon-1d63e0a`) is 39 passed, 0 failed (`target/horizon-test.log`).
- `build-slot.sh e2e --job … --port ports/redstonepen -- tools/e2e.sh`: PASS, log `ports/_shared/evidence/redstonepen/e2e-repair.log` ends `EXIT:0` (`check-e2e-logs.py redstonepen --strict` exit 0).
- `python3 ports/tools/upstream_ready.py redstonepen` and `check-skills.py --notes ports/redstonepen`: exit 0.

## Migration to the SDK format (0.2.0, 2026-11-18)

Horizon's mod-API merge (#16941, launcher ≥ 0.2.70, PR #17992) retired `.hnmod`: the registry
closed every `.hnmod` submission on 2026-11-18 with one reason, and an `.hnmod` no longer
loads. This port's former submission, **#882** (`redstonepen` 0.1.0), was closed by that purge.
**0.2.0** is the same behaviour republished as one flat SDK `.hmod`; the changelog text is at
the end of this section.

Pins: SDK nightly **`0b14f8acfd97fa2c750e42795249dcc981178ca4`** (2026-11-18,
`package_format` 6, `content_api` "0.2"), mixins
**`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`**, launcher 0.2.78 (`cargo-horizon-mod` from
`horizon-rust/horizon@main`). Recipe: `ports/imfast/notes.md` → "Migration to the SDK format",
then `ports/naturesaura/notes.md` (Parts, `Registrar`), `ports/enderstorage/notes.md` and
`ports/little_big_redstone/notes.md` + `ports/morered/notes.md` for the signal read.

**Done in this order:** the old layout was moved into
`ports/_shared/evidence/redstonepen/retired-hnmod-0.1.0/` **before** the first
`cargo horizon-mod build` (the tool refuses a root holding retired package paths), then
`src/runtime.rs` was copied with `cp` and proved with `cmp` + `sha256sum`, `cargo test` ran on
the new crate **before any glue existed** (39 passed), and only then were `src/lib.rs` and
`src/support.rs` written.

### File map (0.1.0 → 0.2.0)

| 0.1.0 path | Now | Evidence |
|---|---|---|
| `mixins/redstonepen/runtime.rs` | `src/runtime.rs`, **byte-identical** (`0191d6b1…`, `cmp`-checked) | `retired-hnmod-0.1.0/mixins/redstonepen/runtime.rs` |
| `mixins/redstonepen/support.rs` (module `redstonepen_state`) | `src/support.rs` (glue; the bounded solve, the pen click, the Parts) | `retired-hnmod-0.1.0/mixins/` |
| `mixins/redstonepen/{signal,source,neighbor,changed,state_for_placement,use_item_on,tick_post}.rs` | methods of `src/support.rs` / `src/lib.rs`; the per-hook files are gone | `retired-hnmod-0.1.0/mixins/` |
| `mixins/redstonepen/embedded_content.rs` + `data/redstonepen-content.hmod` (the AE2 write-out) | **gone**: code and content are one package, so there is nothing to embed | `retired-hnmod-0.1.0/mixins/` |
| `mixins/redstonepen/horizon-mixin.toml` (2 `[[module]]`, 8 `[[injection]]`) | no plan: `Registrar` declarations and typed `Hooks`/Part methods | `retired-hnmod-0.1.0/mixins/` |
| `content/assets/`, `content/data/` (49 + 18 files) | the package root `assets/`, `data/`, **every file unchanged** (sha256 diff empty) | `retired-hnmod-0.1.0/content/` |
| `content/horizon-mod.toml` (`version = "1.21.1-1.11.42"`) | merged into the root `horizon-mod.toml`, `version = "0.2.0"` | `retired-hnmod-0.1.0/content/` |
| `content/redstonepen.hmod` (26,996 B content carry) + `redstonepen.hnmod` (43,420 B) | `redstonepen.hmod` (62,020 B, native + content) | `retired-hnmod-0.1.0/` |
| `compatibility.json`, `rust-toolchain.toml` | `[native]` in `horizon-mod.toml`; the pinned commit decides the toolchain | `retired-hnmod-0.1.0/` |
| `tools/horizon.py`, `package.py`, `package.sh`, `test_stage.py`, `test_template.py`, `add-notices.py`, `audit/` | deleted: staging, `.hnmod` packaging and the plugin-injection audit belong to the retired format | `retired-hnmod-0.1.0/tools/` |
| `Cargo.lock` | not kept: it is packed and changes the artifact (trap 5 of the recipe) | not kept |
| the old repo `AGENTS.md` (it says to open `compatibility.json` and run `tools/horizon.py`) | retired into evidence, as `naturesaura` did | `retired-hnmod-0.1.0/AGENTS.md` |

**Kept and updated:** `tools/e2e.sh` (the scenario and its assertions are format-independent;
the launch line now needs a launcher-built game), `tools/gen_track_shapes.py` and
`tools/check_binding.py` (their paths now point at the package root),
`notes.md`, `README.md` (rewritten: the root is packed), `REPIN-AUDIT.md`, `VALIDATION.md`,
`LICENSE`, `LICENSE-NOTICE.md`.

### Helper → hook table

Each former `[[injection]]` helper is now one typed method; no injection was dropped, and
`embedded-content` has no successor because the flat package replaces it.

| 0.1.0 injection (hook) | 0.2.0 home | Mapping notes |
|---|---|---|
| `embedded-content` (`content_root` HEAD, cancellable) | **none** | the SDK `.hmod` is one package: `assets/` and `data/` are loaded because they are in it |
| `block-signal` (`block_signal` v1, `signal.rs`) | `signals::SignalPart::block_signal` (**BlockPart**, attached to the track and the four relays) | the event carries `state`, `face`, `direct` and the mutable `signal`; the answer is the same `track_signal`/`relay_variant_signal` on both the weak and the direct ask |
| `signal-source` (`is_signal_source`, `source.rs`) | `SignalPart::is_signal_source` | `event.source = true` for the five blocks |
| `neighbor-changed` (`neighbor_changed`, `neighbor.rs`) | `SignalPart::neighbor_changed` | a block-scoped listen hook: queue the track or the relay for the next level tick |
| `block-changed` (`block_changed`, `changed.rs`) | **`Hooks::block_changed`** → `support::block_changed` | global: every block write is a candidate trigger; the `writing` sentinel swallows this port's own writes |
| `state-for-placement` (`state_for_placement`, `state_for_placement.rs`) | `SignalPart::state_for_placement` | `Some(Some(state))` via `level.state_with_property`; the relay's `state` latch is set only for the pulse/bistable kinds |
| `use-item-on` (`use_item_on` v1, `use_item_on.rs`, a **global** `Game` hook with `claimed`) | **`PenPart::use_on`** (**ItemPart** on `redstonepen:pen`) | `use_item_on` is **block-scoped** in 0.2, and the pen clicks any block (including vanilla supports), so the item side is the only home; `Some(InteractionResult::Success)` is the old `claimed = true`. It fires after the block's own use, exactly vanilla's `ItemStack.useOn` |
| `level-tick-post` (`level_tick_post`, `tick_post.rs`) | **`Hooks::level_tick_post`** → `support::level_tick_post` | `HostSide::Server(host)` + `event.level`; the same bounded solve |
| `[[module]] runtime` | `src/runtime.rs` (`pub mod runtime;`) | byte-identical, 39 tests |
| `[[module]] state` (support) | `src/support.rs` (`mod support;`) | one ordinary module; no plan section exists any more |

**No hook is left without a home.** The behaviours that changed with the API are listed under
"Behaviour that changed with the format" below.

### The ladder (measured 2026-11-18)

Every step ran in a `ports/tools/build-slot.sh` lease with `BUILD_LOCAL=1` (the runner still
speaks the retired `tools/horizon.py` ladder, so it is used as the slot guard: **no staging, no
staged tree**, the SDK crate compiles in place against `.horizon/sdk/`), `CARGO_INCREMENTAL=0`,
no `--target-dir`.

| Step | Command | Result | Evidence |
|---|---|---|---|
| runtime tests, before any glue | `cargo test --offline` | **39 passed**, 0 failed | `evidence/redstonepen/sdk-test.log` (first run) |
| glue RED | `cargo test --offline` with the three state helpers on `todo!()` | **failed**: 6 compile errors, the intended red | `sdk-glue-red.log` |
| tests | `cargo test --offline` | **49 passed**, 0 failed (39 `runtime::`, 10 `support::`) | `sdk-test.log`, `sdk-test-glue-green.log` |
| lints | `cargo clippy --all-targets --offline -- -D warnings` | exit 0, **0 warnings** | `sdk-clippy.log` |
| format | `cargo fmt --check` (**not** `--all`) | exit 0 after `rustfmt` of `src/lib.rs` + `src/support.rs`; `runtime.rs` untouched (`0191d6b1…` before and after) | `sdk-fmt.log` |
| package | `rm -f Cargo.lock && cargo horizon-mod build` | `Built ./target/horizon-mod/redstonepen.hmod` | `sdk-build.log` |
| registry check | `cargo horizon-mod check redstonepen.hmod` | **62,020 B**, SHA-256 `b6b496daf827680589ed9a449312b135c759e637a65ac90a4891ecc03d4a06a5`, **EXIT 0** | `sdk-check.log` |
| fits the pinned build | `cargo horizon-mod fits redstonepen.hmod --build nightly` | `Redstone Pen:` / `0b14f8a  made for`, **EXIT 0** | `sdk-fits.log` |
| reproducible | second `rm -f Cargo.lock && cargo horizon-mod build` | identical bytes (`cmp`), same SHA-256 | `sdk-build-second.log`, `sdk-sha256.log` |
| `runtime.rs` untouched | `unzip -p redstonepen.hmod src/runtime.rs \| sha256sum` | `0191d6b1…7434d` = `src/runtime.rs` = the retired file | `sdk-sha256.log` |

Package entry list (`unzip -Z1`, 76 entries): `horizon-mod.toml`, `Cargo.toml`, `LICENSE`,
`LICENSE-NOTICE.md`, `README.md`, `src/lib.rs`, `src/runtime.rs`, `src/support.rs`,
`uses.json`, 49 `assets/…`, 18 `data/…`; **no `Cargo.lock`**, both licence files inside
(`sdk-entries.log`). `gen_track_shapes.py` re-emits the carried `track.json`'s 99 `(bend, axis)`
shape entries byte for byte, which is the content carry's own proof.

`uses.json` (`sdk-uses.json`, summary `sdk-uses-summary.log`) is the migration's best summary:
**250 surface entries** — the seven hooks (`block_changed`, `block_signal`, `is_signal_source`,
`level_tick_post`, `neighbor_changed`, `state_for_placement`, `use_on`), the four `BlockPart`
methods plus `ItemPart::use_on`, `Hooks::register`, the three Part kinds, the two registrar
calls (`block_key`, `item_key`), the `BlockStates`/`Level`/`Player` handles the glue reads, both
macros (`horizon_mod`, `horizon_part`) and `capability:registries`.

### Behaviour that changed with the format (deliberate, and why)

- **The signal read is the engine's `getSignal`.** 0.1.0 rebuilt "what does this block give
  toward me" itself (`facts.weak`, plus a special case for vanilla wire), because the old pin
  had no per-direction read: another mod's `block_signal` was invisible and conduction was not
  modelled (0.1.0 gap 9). `LevelRef::signal` **is** `SignalGetter.getSignal`, conduction
  included, and upstream's `RedstoneTrack.getRedstonePower` calls exactly that
  (`RedstoneTrack.java:780-793`); `little_big_redstone` and `morered` migrated to the same read.
  So the port is now closer to upstream on two points: a conducting block between two tracks
  passes power as vanilla does, and another mod's `block_signal` counts. **Consequence:** the
  0.1.0 e2e's `stone at the turn` control (a stone cell between two tracks) is expected to
  *light* the far side now, and the `stone between source and lamp` row is unchanged (a lamp is
  not a track). Those rows must be re-run with a launcher-built game before they are claimed
  again — they are **not** claimed here (see the gaps).
- **The sneak is the acting player's own flag.** 0.1.0 read the local key state
  (`game.keys` + `ShiftLeft`) because the old pin's `NativeServerPlayer` had no sneaking field,
  so a dedicated-server guest's erase gesture was invisible (0.1.0 gap 3). `use_on` carries
  `player`, and `PlayerRef::is_secondary_use_active` is the server's shift-key flag, so the
  gesture works for every player now.
- **The pen's click is the item's moment, not the block's.** 0.1.0's global `use_item_on` saw
  every click; 0.2.0's `use_item_on` is block-scoped, so the port attaches `use_on` to the pen.
  The observable difference is vanilla's own rule: a click that the clicked block consumes (a
  chest still opens) no longer reaches the pen. That is what upstream's
  `Item.useOn`/`RedstonePenItem.useOn` does.
- **No logger.** The 0.1.0 embedded-content helper was the only `log::warn!`; it is gone with
  the helper, and the rest of the port never logged. Nothing to route to the player.

### Traps this migration hit (new to this port)

1. **A `BlockPart` cannot read a state property.** It is handed `&dyn LevelHost`, which has
   `state_string`, `state_block`, `state_with_property` and `block_state_id`, but no
   `property`. `host.block_states()` (with `BlockMetadata`) is a `Host` method and is not
   reachable there. This port formats the state once, parses it, and caches the answer by
   `BlockState` (`Decoded`), so `block_signal` — asked very often — is a hash lookup after its
   first call. A state's properties never change for one ordinal, so the cache cannot go stale.
2. **`HashMap::new()` is not usable in a `static Mutex<HashMap<…>>`** on this toolchain
   (`E0015: cannot call non-const associated function in statics`). Use
   `LazyLock<Mutex<HashMap<…>>>`.
3. **`HostBlockStates` and `BlockMetadata` must both be in scope.** Without them the compiler
   reports `no method named block_states` and then helpfully suggests `block_state_string`,
   which points away from the real fix.
4. **The pen must be an `ItemPart`.** `use_item_on` is block-scoped, so a pen attached to the
   track's Part would never fire on a stone support. `right_click_block` is the pre-block route
   and would also draw on a chest, which vanilla's `ItemStack.useOn` does not.
5. **`HostSide` is not always a server.** `Hooks::level_tick_post` and `ItemPart::use_on` both
   carry `HostSide`; the client calls the item part for prediction. Match `HostSide::Server`,
   never unwrap.
6. **Two dimension-id shapes.** `event.dimension.key().id()` is a `&'static str`
   (`LevelKey`); `level.dimension().id(host)` is `Option<String>` (`DimensionKey`). The dirty
   sets are keyed on the string, so the Parts' queue and the tick's drain agree.
7. **The 0.1.0 neighbour helper was `updateNeighborsAt`, not the comparator one.** In 0.2.0
   that is `NeighbourUpdate::Neighbours`; `NeighbourUpdate::OutputSignal` is
   `updateNeighbourForOutputSignal` and would leave wire and pistons deaf.
8. **`level.block_state` answers `Some(air)` out of height and in unloaded chunks** (only
   `None` when the state index refuses). "Is this cell empty" is therefore a block-id test, not
   an `Option` test.
9. **`BlockState::with` fails on a missing property.** The relay's `state` latch may only be
   written for the pulse and bistable variants; writing it for the plain relay is an `Err`, so
   `relay_state` branches on `RelayKind::has_state`.
10. **The content moved, and its tools must follow.** `content/` is the package root now:
    `tools/gen_track_shapes.py` writes `data/redstonepen/horizon/block/track.json` and
    reproduces the 99-entry table unchanged; `check_binding.py` still takes the client's
    `model-states.json` as an argument.
11. **The root is packed.** `README.md` was rewritten *before* the build (it describes the SDK
    ladder, not `tools/horizon.py`), and the old repo `AGENTS.md` was retired into evidence
    because it told the reader to open `compatibility.json` and run a deleted tool.

### Gaps

1. **No in-game run in this environment.** The e2e needs a game built by
   `horizon-launcher native-build . --trust-native-code` with this package installed, and
   `horizon-launcher` is not installed here, so no `0b14f8a` client exists. `tools/e2e.sh` now
   takes `GAME=` (and `CONTROL_GAME=` for the differential) and keeps every failure-capable
   `SELFTEST*` leg; the 0.1.0 logs in `ports/_shared/evidence/redstonepen/` are **historical**
   (they drove a source-built game with the retired `.hnmod`). The behaviour they proved for
   `runtime.rs` — which is byte-identical — still stands; the *transport* and the two behaviour
   changes above are what a 0.2.0 leg must re-prove. This is `ported-partial` until that run
   exists.
2. **The conduction rows** (a conductor cell between two tracks) are the one place where 0.1.0's
   recorded e2e expectations are expected to change, because the signal read is now the engine's
   `getSignal` (see "Behaviour that changed"). Re-run them before quoting them.
3. Everything in "Gaps and deviations" above is unchanged: one track per block, no fourth arm,
   the bridge relay, pen economy, the support rule, the custom renderer, the simplified recipes
   and the out-of-scope content. The corner/T collision box gap was already closed in slice 5.
4. **No diagnostics.** A native mod may not add a crate and the SDK exports no logger; the port
   has no readout command, so a refused `set_block` is silent. That is the same as 0.1.0.

### Changelog (0.2.0), and the supersede text

> **0.2.0 — republished as an SDK `.hmod`.** Horizon's mod-API merge (launcher ≥ 0.2.70)
> retired `.hnmod` and closed **#882** (`redstonepen` 0.1.0); this version supersedes it. One flat
> `redstonepen.hmod` now carries the code **and** the content (the pen, the track with its
> straight/corner/T shapes, the four relays, models, loot, recipes, tag, creative tab and lang):
> `src/lib.rs` is the one `impl mod_api::Hooks`, `src/support.rs` the game glue (a `BlockPart` on
> the track and the relays, an `ItemPart` on the pen, the bounded level-tick solve), and
> `src/runtime.rs` is byte-identical to 0.1.0 with its 39 tests. Built against SDK `0b14f8a` /
> mixins `e59da90` for 26.3. The `.hnmod`, the mixin plan, the seven per-hook helpers and the
> embedded-content injection are gone; the pen's click is the item's `use_on`, the block moments
> are `BlockPart` methods, the signal read is the engine's `getSignal` (conduction and other
> mods' answers included) and the sneak-erase uses the acting player's own flag. Same feature
> set and the same gaps (one track per block, no fourth arm, no bridge relay, no custom
> renderer).

Skills: superpowers/using-superpowers, brainstorming, writing-plans, test-driven-development, systematic-debugging, verification-before-completion; BeautifulRust/rust-best-practices, rust-port
