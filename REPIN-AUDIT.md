# Re-pin audit — `redstonepen` at Horizon `83ad384f`

Read-only audit (2026-11-05). No port code, content, tools or campaign docs were
changed; no build, stage or e2e was run. Verdicts are source reads.

| | |
|---|---|
| New Horizon | `83ad384f48d0742746d518737511882680aab5b7` (427 hooks / 159 helpers) at `ports/_shared/pins/horizon-83ad384f` |
| New mixins | `e8cd0aa38cffef49cb2e088ef716baa107148c02` |
| Old Horizon (reference) | `c9d474dd…` — 108 hooks (`ports/_shared/repin-83ad384f.md`); the old content schema was read from a staged `c9d474d` tree during this audit (since removed by a concurrent target clean) |
| Port pin | still `c9d474dd…` (`compatibility.json`, `mixins/redstonepen/horizon-mixin.toml`) |
| Source shorthand | `hooks.rs` = `crates/horizon-client/src/game/native_hooks.rs`, `support.rs` = `mixins/redstonepen/support.rs`, `content.rs` = `crates/horizon-mods/src/content.rs`, `mod_content.rs` = `crates/horizon-client/src/mod_content.rs`, `docs` = `docs/mods.md` |

## 1. Deferred inventory and verdicts

Deferred items from `notes.md` §"Gaps and deviations" (1-10) + ARCHITECTURE
item 19: **10**.

| # | Deferred feature | Old pin | New pin verdict | Symbol / evidence | Next step |
|---|---|---|---|---|---|
| 1 | One track per block, max three arms; no fourth arm | "over the ~2000 state budget" | **not blocked** — the pin's limit is 2^20 | `MAX_STATES = 1 << 20` `mod_content.rs:1500`; docs `:144-145`; a fourth arm = 1+12+20+15 = 48 bends × 3 axis × 16 power = **2304** states; a one-arm stub = 39 bends = **1872** | Content work only, if the extra states are wanted; the "~2000 budget" was the port's own, not the pin's |
| 2 | Corner/T collision follows `axis` (first arm) only; the model shows all arms | blocked ("shapes keyed on a template property only") | **usable — content `collision_shapes` + property tuples** | `BlockShapes` gained `properties`, `default` and `collision_shapes` (`content.rs:150-158`, `mod_content.rs:1618`); docs `:173-203`, `:194-196` ("template and/or declared state properties") | Key `shapes`/`collision_shapes` on `["bend"]` (33 boxes) or `["bend","axis"]`; `content/.../block/track.json:11` currently keys on `axis` |
| 3 | Sneak is read from the local key state, so a guest's sneak is invisible | limitation | **usable — `player_toggle_sneak`** | `hooks.rs:4107` (API new at this pin; absent from `c9d474d`), state swapped in, "only actual changes are reported" | Keep a per-player sneak flag from the hook; delete `sneaking()`'s key read (`support.rs:410-412`) |
| 4 | Bridge relay: needs a per-side direct read (`getDirectSignal(wire, side) − 2`) | blocked | **partly usable** — no per-side API, but the port already rebuilds it | only `native_neighbor_signal` (best neighbour) `hooks.rs:8501`; `signal_toward` `support.rs:313` already reads per side from state/`facts.weak` | Implement the bridge on `signal_toward`: exact for tracks, vanilla wire and full-strength sources; still blind to through-conductor direct signals and other mods' `block_signal` |
| 5 | Pen economy: no redstone use, no durability (upstream 256) | deferred | **partly realizable** | item `max_damage` component: docs `:205-218`; redstone count/take: `native_count_items` `native_server.rs:637`, `native_take_items` `:657`; wear: `carried_stack` `hooks.rs:8450` / `inventory_tick` (API 39), `item_damaged` `:7476` is cancel-only | Declare `max_damage` 256 on the pen item; consume one redstone and write the worn stack from `inventory_tick` |
| 6 | Support rule: a track neither needs nor loses its support | deferred | **usable — `can_survive` + `update_shape`** | `can_survive` `hooks.rs:1643` (API 70), `update_shape` `hooks.rs:1609` (API 32, may set air with drops) | Answer for tracks/relays; note the support face must be inferred from the arms or stored (the state has no `support` property) |
| 7 | No custom renderer / lit tint / particles / overlay | deferred | **hook exists, content blocks it** | `block_entity_submit` `hooks.rs:3190`, `block_entity_render_bounding_box` `:3215`, `native_draw_cube/quad/block`; `render_shape: ENTITYBLOCK_ANIMATED` docs `:110-114` | Needs a block entity, and a content block entity needs a **BE-capable template** (`minecraft:basalt` has none) — see §3 |
| 8 | Simplified recipes (pen, variant chain) | deferred, low value | content work; custom types usable | `recipes_of_type` (API 36, docs `:36`) | Low priority |
| 9 | Another mod's `block_signal` invisible to the solve; comparators not special-cased | recorded deviation | **comparator output for own blocks is new**, reading others still absent | `has_analog_output` `hooks.rs:1539`, `analog_output` `:1555` (API 64) | Optional: answer a comparator for a track; reading a *neighbouring* comparator per side stays absent |
| 10 | Out of scope: control box, gauge, lever, buttons, remote, RCA bridge, coloured tracks, bundles, quill | deferred | unchanged | — | none |

Net: **4 of 10 newly realizable** (#2, #3, #6, and #1 reclassified as
unblocked), **2 partly realizable** (#4, #5), **1 blocked by content** (#7),
1 new-but-optional (#9), 2 unchanged (#8, #10). #2 is the only one that changes
what a player sees or bumps into.

## 2. The two findings that matter

### 2.1 The corner/T collision can be exact (new content field)

At `c9d474d`, `BlockShapes` was one property plus values (read from a staged
`c9d474d` tree during this audit; that tree has since been cleaned), so
the track's shape had to follow a **template** property — `axis` — and the
notes recorded "the collision bar covers the first arm"
(`notes.md` gap 2). At `83ad384f` the shape table takes an ordered tuple of
template **and declared** properties (`content.rs:150-158`, docs `:194-196`),
and a block has an independent collision table (`collision_shapes`,
`mod_content.rs:1618`, docs `:173-176`).

The track already declares `bend` with 33 values (`content/.../block/track.json:68`),
so a 33-entry table keyed on `bend` gives every corner and T its own exact
outline/collision — no new state, no code, and the `axis` property can stay as
the template's (harmless) state property. This removes the last *geometric*
simplification of the port.

### 2.2 The "state budget" claim is wrong at this pin

`MAX_STATES = 1 << 20` is unchanged from the old pin
(`mod_content.rs:1500`; docs `:144-145` "2^20"). The port's 1584-state track and
a 2304-state four-arm track are both far below it. The "~2000 budget" in
`notes.md` is a self-imposed design limit (binding/verification time), not an
engine limit, and should be re-worded so a later slice does not treat a
four-arm/cross junction as impossible.

## 3. Still blocked, with the smallest unblocker

| Need | Evidence | Unblocker |
|---|---|---|
| Custom track/relay renderer, lit tint, overlay | the renderer hooks need a block entity (`block_entity_submit` `hooks.rs:3190`); a content block entity requires a template with `facts.block_entity` (the same rule `restrictions` hit), and `minecraft:basalt` has none | Move the track onto a BE-capable template (a full-cube one, e.g. `minecraft:beacon`, brings beacon behaviour) **or** get a content declaration for a plain block entity |
| Per-side direct signal from another block (through a conductor, or another mod's `block_signal`) | `native_neighbor_signal` `hooks.rs:8501` is the best neighbour only; docs `:962`; `block_signal` `hooks.rs:1504` is asked only for **your own** block | A `getSignal(pos, direction, direct)` helper |
| Comparator *reading* a track: own block can now answer | `has_analog_output` `hooks.rs:1539` | Nothing — implement if wanted |
| Stack damage from a hook (pen wear on use) | no `native_damage_item` in the 159 helpers; `item_damaged` `:7476` is pre-damage | A `damage_item` helper, or `carried_stack` `hooks.rs:8450` inside `inventory_tick` (API 39) |
| Fourth arm / one-arm stub | not a pin limit (§2.2) | Content/model work: a fourth arm is 2304 states, 15 more bend names and models; a one-arm stub is 1872 states, 6 more |

## 4. Stale claims in `notes.md` / ARCHITECTURE (do not re-derive)

- Gap 2's premise ("content `shapes` can only be keyed on a template property")
  is false at `83ad384f` — see §2.1.
- Gap 1 / the slice-4 note "over the ~2000 budget" is not an engine limit (§2.2).
- Gap 3 says a guest's sneak is invisible because the pin's
  `NativeServerPlayer` has no sneaking flag. Still true of the struct, but
  `player_toggle_sneak` (`hooks.rs:4107`) now supplies the state — the
  limitation is removable.
- Gap 4 says "the pin exposes no per-side direct read, so only the four plain
  variants are implemented". Still true for *direct* signals, but the port's own
  `signal_toward` (`support.rs:313`) is already a per-side read for tracks, wire
  and `facts.weak`, which is most of what the bridge needs. Do not repeat the
  blanket "blocked" wording.
- Gap 9's "comparator output is not special-cased" now has an own-block hook
  (API 64); the neighbour read is the part that stays absent.

## 5. Highest-value next step (one)

**Make the corner/T collision exact with a `bend`-keyed `collision_shapes`
table** (content only: `content/data/redstonepen/horizon/block/track.json`, 33
entries, `tools/check_binding.py` unchanged). It is the cheapest change in the
whole port that removes a recorded, player-visible deviation ("the collision bar
covers the first arm", `notes.md` gap 2), needs no new hook and no new state, and
its e2e already exists (`tools/e2e.sh` walks a corner and a T) — extend it with a
"stand on the outside of the corner" assertion so the collision, not just the
signal, is proven.
