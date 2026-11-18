//! Game-side glue for the Redstone Pen SDK package: the pen click (draw, extend, bend,
//! widen, erase), the track power solve, the relays and the signal answers.
//!
//! Upstream keeps 24 wire segments in a block entity and recomputes each net from every
//! external neighbour. This port has one track per block, whose two or three arms (`axis` +
//! `bend`) and `power` are block-state properties, so the solve is a small worklist over the
//! tracks a neighbour change queued. It runs at [`level_tick_post`], where the level is in
//! place and a write works, and it is bounded by step and visit budgets.
//!
//! A track answers its power on its arms only. A track-to-track step costs one, a vanilla
//! redstone wire costs one, and any other source seeds its full strength (upstream's
//! `handleNetNeighborChanged`); a relay re-emits 15 from its output face. The relay's `facing`
//! is its output direction in this port (upstream's is the mounting face plus a rotation).
//!
//! The pen is an [`ItemPart`] because a click lands on **any** block: the pen draws a track on
//! the face it was clicked on, and a block-scoped part would only ever see the port's own five
//! blocks.
//!
//! Every state this module reads is decoded once into [`Decoded`] and cached by [`BlockState`]:
//! a `BlockPart` is handed a `&dyn LevelHost`, which can format a state but not read one
//! property, and the cached text parse keeps `block_signal` (asked very often) allocation-free
//! after its first call. A state's properties never change for one ordinal, so the cache cannot
//! go stale.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock};

use mod_api::{
    BlockChangedEvent, BlockKey, BlockMetadata, BlockPart, BlockSignalEvent, BlockState,
    BlockUpdateFlags, Direction, Host, HostBlockStates, InteractionResult, IsSignalSourceEvent,
    ItemKey, ItemPart, LevelHost, LevelRef, LevelTicksHost, NeighborChangedEvent, NeighbourUpdate,
    Registrar, ServerHost, StateForPlacementEvent, UseOnEvent,
};

use crate::runtime::{
    ClickAction, ClickMode, Dir, PULSE_TICKS, RELAY_OFF_DELAY_TICKS, RelayKind, RelayPhase, Shape,
    Supply, click_action, connected, delivered_phase, placement_axis, relay_delayed_off,
    relay_edge, relay_facing, relay_notifies, relay_powered, relay_variant_signal, track_power,
    track_signal,
};

/// Entries one tick may apply; a safety budget against runaway feedback.
const MAX_DIRTY: usize = 4096;
/// Track recomputes one level tick may do (all its tracks together).
const MAX_STEPS: usize = 16_384;
/// Times one track may be recomputed in a single level tick.
const MAX_VISITS: u8 = 8;
/// The one track block this port ships (`RedstoneTrack.RedstoneTrackBlock`).
const TRACK_BLOCK: &str = "redstonepen:track";
/// The pen (`ModContent.initItems`); upstream's `quill` is the same item logic.
const PEN_ITEM: &str = "redstonepen:pen";
/// The vanilla wire the pen can also draw into (`RedStoneWireBlock.POWER`).
const WIRE_BLOCK: &str = "minecraft:redstone_wire";
/// Air, the state an erased track takes.
const AIR_BLOCK: &str = "minecraft:air";

/// The registration keys, so a hot read compares identities instead of allocating a string.
/// Registration runs before any hook.
static TRACK_KEY: OnceLock<BlockKey> = OnceLock::new();
static RELAY_KEYS: OnceLock<[(BlockKey, RelayKind); 4]> = OnceLock::new();
static PEN_KEY: OnceLock<ItemKey> = OnceLock::new();

/// The decoded value of one of this port's block states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decoded {
    /// A track read from its state: its two or three arms and one power.
    Track { shape: Shape, power: u8 },
    /// A relay read from its state (`facing` is this port's output direction).
    Relay {
        kind: RelayKind,
        output: Dir,
        phase: RelayPhase,
    },
}

/// One state decoded, keyed by the state itself.
static DECODED: LazyLock<Mutex<HashMap<BlockState, Decoded>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Default)]
struct State {
    /// Tracks queued for the next power solve.
    tracks: BTreeSet<(String, [i32; 3])>,
    /// Relays queued for the next input evaluation.
    relays: BTreeSet<(String, [i32; 3])>,
    /// Delayed relay writes (`scheduleTick(pos, this, 2)` upstream).
    pending: Pending,
    /// Positions this port is writing right now. The engine reports the write as
    /// `block_changed`; that is a state refinement, not a placement, so it must not seed
    /// another evaluation.
    writing: BTreeSet<(String, [i32; 3])>,
}

/// A write upstream would deliver with a 2-tick block tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Scheduled {
    ticks: u8,
    kind: RelayKind,
    write: PendingWrite,
    notify: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingWrite {
    /// Turn a plain/inverted relay's output off after the delay.
    Phase(RelayPhase),
    /// Clear a pulse relay's `STATE` after its 2-tick pulse.
    PulseClear,
}

/// The delayed relay writes, one per cell.
#[derive(Default)]
struct Pending {
    entries: BTreeMap<(String, [i32; 3]), Scheduled>,
}

impl Pending {
    /// Arm one delayed write, keeping an earlier deadline for the same cell (a pending tick
    /// is not re-armed, `hasScheduledTick` upstream).
    fn arm(
        &mut self,
        key: (String, [i32; 3]),
        ticks: u8,
        kind: RelayKind,
        write: PendingWrite,
        notify: bool,
    ) {
        if self.entries.len() < MAX_DIRTY {
            self.entries.entry(key).or_insert(Scheduled {
                ticks,
                kind,
                write,
                notify,
            });
        }
    }

    /// Advance one level tick of `dimension`, answering the writes that are due.
    fn due(&mut self, dimension: &str) -> Vec<((String, [i32; 3]), Scheduled)> {
        let keys: Vec<(String, [i32; 3])> = self
            .entries
            .keys()
            .filter(|(dim, _)| dim == dimension)
            .cloned()
            .collect();
        let mut ready = Vec::new();
        for key in keys {
            let Some(pending) = self.entries.get_mut(&key) else {
                continue;
            };
            if pending.ticks > 1 {
                pending.ticks -= 1;
            } else {
                let done = *pending;
                self.entries.remove(&key);
                ready.push((key, done));
            }
        }
        ready
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state() -> MutexGuard<'static, Option<State>> {
    STATE
        .lock()
        .expect("the redstone pen state lock is never held across a panic")
}

fn with_state<R>(act: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = state();
    act(guard.get_or_insert_with(State::default))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Declare the keys this mod answers for and attach the two Parts: the signal/placement Part
/// to the track and the four relays, the click Part to the pen.
pub fn register(registrar: &mut dyn Registrar) {
    let track = registrar.block_key(TRACK_BLOCK);
    let _ = TRACK_KEY.set(track);
    registrar.block_part(track, SignalPart);

    let relays: [(BlockKey, RelayKind); 4] = std::array::from_fn(|index| {
        let kind = RelayKind::ALL[index];
        let key = registrar.block_key(kind.block_id());
        registrar.block_part(key, SignalPart);
        (key, kind)
    });
    let _ = RELAY_KEYS.set(relays);

    // The key is bound first: `item_part(item_key(id), part)` is a double `&mut` registrar.
    let pen = registrar.item_key(PEN_ITEM);
    let _ = PEN_KEY.set(pen);
    registrar.item_part(pen, PenPart);
}

// ---------------------------------------------------------------------------
// Parts
// ---------------------------------------------------------------------------

/// The track's and the relays' scoped hooks: their signal, their placement state and the
/// neighbour change that queues them for the next solve.
struct SignalPart;

#[mod_api::horizon_part]
impl BlockPart for SignalPart {
    /// `block_signal`: a track answers its stored power on its arms, a relay 15 on its output
    /// face while powered. Both answer the direct (strong) query too, like upstream's
    /// `getDirectSignal`; the template block provides no conducted answer of its own.
    fn block_signal(&self, level: &dyn LevelHost, event: &mut BlockSignalEvent) {
        match decode(level, event.state) {
            Some(Decoded::Track { shape, power }) => {
                event.signal = track_signal(power, shape, dir(event.face));
            }
            Some(Decoded::Relay {
                kind,
                output,
                phase,
            }) => {
                event.signal = relay_variant_signal(kind, phase, output, dir(event.face));
            }
            None => {}
        }
    }

    /// `is_signal_source`: a track and a relay both answer the signal query, so redstone wire
    /// beside them connects.
    fn is_signal_source(&self, _level: &dyn LevelHost, event: &mut IsSignalSourceEvent) {
        if ours(event.state) {
            event.source = true;
        }
    }

    /// `neighborChanged`: queue the track or the relay for the next level tick.
    fn neighbor_changed(&self, _level: &mut dyn LevelTicksHost, event: &mut NeighborChangedEvent) {
        let dimension = event.dimension.key().id();
        let pos = xyz(event.pos);
        if is_track(event.state) {
            mark_track(dimension, pos);
        } else if relay_of(event.state).is_some() {
            mark_relay(dimension, pos);
        }
    }

    /// `getStateForPlacement`: a placed track is a straight track on the look's axis; a relay
    /// outputs away from the support it was clicked on.
    fn state_for_placement(
        &self,
        level: &dyn LevelHost,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        if is_track(event.state) {
            let axis = placement_axis(
                dir(event.context.face),
                event.context.yaw,
                event.context.pitch,
            );
            let shape = Shape::straight(axis);
            let placed = level.state_with_property(event.state, "axis", shape.axis().name())?;
            let placed = level.state_with_property(placed, "bend", shape.bend_name())?;
            return Some(Some(level.state_with_property(placed, "power", "0")?));
        }
        let kind = relay_of(event.state)?;
        let output = relay_facing(dir(event.context.face));
        let placed = level.state_with_property(event.state, "facing", output.name())?;
        let placed = level.state_with_property(placed, "powered", "false")?;
        // `state` is upstream's `STATE` latch: only the pulse and bistable variants carry it,
        // and a placed relay starts unlatched.
        let placed = if kind.has_state() {
            level.state_with_property(placed, "state", "0")?
        } else {
            placed
        };
        Some(Some(placed))
    }
}

/// The pen's click. It is the item's Part, not a block's: a click on a support lands on a
/// block this mod does not own.
struct PenPart;

#[mod_api::horizon_part]
impl ItemPart for PenPart {
    /// `use_on`: a pen click on a support draws a straight track on the clicked face; on a
    /// track it extends the clicked arm, bends a one-wired-arm net into a corner, widens a
    /// two-wired-arm net into a T on a side click, or erases. A sneak click is the erase
    /// modifier: it drops the clicked arm of a T and otherwise removes the cell.
    fn use_on(
        &self,
        host: mod_api::HostSide<'_>,
        event: &mut UseOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let mod_api::HostSide::Server(host) = host else {
            // The client's copy only predicts; every write is the server's.
            return None;
        };
        if event.hand != mod_api::Hand::Main {
            return None;
        }
        pen_click(host, event)
    }
}

// ---------------------------------------------------------------------------
// The pen click
// ---------------------------------------------------------------------------

/// One pen click: draw, extend, bend, widen, narrow or erase, as upstream's
/// `RedstonePenItem.useOn` does with its own segment bookkeeping.
fn pen_click(host: &mut dyn ServerHost, event: &UseOnEvent<'_>) -> Option<InteractionResult> {
    if event.stack.item() != *PEN_KEY.get()? {
        return None;
    }
    let face = dir_from_vector(event.face)?;
    let player = host.player(event.player)?;
    let level = player.level(host)?;
    let dimension = level.dimension().id(host)?;
    let pos = event.pos;
    let clicked = level.block_state(host, pos)?;
    if is_block(host, clicked, AIR_BLOCK) {
        return None;
    }
    // `isSecondaryUseActive` is the acting player's own sneak flag on the server, so a guest's
    // erase gesture works where the 0.1.0 local-key read could not see it.
    let mode = if player.is_secondary_use_active(host).unwrap_or(false) {
        ClickMode::Erase
    } else {
        ClickMode::Draw
    };
    if let Some(track_now) = track_at(host, clicked) {
        let wired = wired_arms(level, host, pos, track_now.shape);
        return match click_action(track_now.shape, face, &wired, mode) {
            ClickAction::Extend(arm) => {
                let target = arm.offset(pos);
                if !is_air(host, level, target) {
                    // The clicked arm's cell is occupied: consume the click.
                    return Some(InteractionResult::Success);
                }
                let state = new_track_state(host, Shape::straight(arm.axis()))?;
                write_block(host, level, &dimension, target, state, true).then(|| {
                    mark_track(&dimension, target);
                    InteractionResult::Success
                })
            }
            ClickAction::Bend(shape) | ClickAction::Widen(shape) | ClickAction::Narrow(shape) => {
                let state = with_shape(host, clicked, shape)?;
                write_block(host, level, &dimension, pos, state, true).then(|| {
                    mark_track(&dimension, pos);
                    mark_around_at(level, host, &dimension, pos);
                    InteractionResult::Success
                })
            }
            ClickAction::Erase => {
                let air = host.block_state_id(AIR_BLOCK)?;
                write_block(host, level, &dimension, pos, air, true).then(|| {
                    // The neighbours have to drain; this write's own `block_changed` is
                    // swallowed by the `writing` sentinel.
                    mark_around_at(level, host, &dimension, pos);
                    InteractionResult::Success
                })
            }
            ClickAction::Refuse => Some(InteractionResult::Success),
        };
    }
    if mode == ClickMode::Erase {
        // The erase modifier never draws: a sneak click on a support is inert.
        return Some(InteractionResult::Success);
    }
    let target = face.offset(pos);
    if !is_air(host, level, target) {
        return None;
    }
    let (yaw, pitch) = look(host, event.player_entity_id);
    let axis = placement_axis(face, yaw, pitch);
    let state = new_track_state(host, Shape::straight(axis))?;
    write_block(host, level, &dimension, target, state, true).then(|| {
        mark_track(&dimension, target);
        InteractionResult::Success
    })
}

/// The clicker's look direction, from the entity the event names. A player the host cannot
/// reach falls back to yaw 0, pitch 0 — a level look along `+z`, which
/// [`placement_axis`](crate::runtime::placement_axis) reads as that face plane's `z` axis (or
/// its horizontal fallback on a face whose normal is `z`).
fn look(host: &dyn Host, player_entity_id: i32) -> (f32, f32) {
    u32::try_from(player_entity_id)
        .ok()
        .and_then(|id| host.entity(id))
        .and_then(|entity| entity.rotation(host))
        .map_or((0.0, 0.0), |[yaw, pitch]| (yaw, pitch))
}

// ---------------------------------------------------------------------------
// The level tick
// ---------------------------------------------------------------------------

/// End of the level tick: apply the queued delayed writes, relays and tracks of this level.
pub fn level_tick_post(host: &mut dyn ServerHost, level: LevelRef) {
    let Some(dimension) = level.dimension().id(host) else {
        return;
    };
    advance_pending(host, level, &dimension);
    solve_relays(host, level, &dimension);
    solve_tracks(host, level, &dimension);
}

/// Advance upstream's scheduled block ticks. A pending write lands two level ticks after it
/// was armed, like `scheduleTick(pos, this, 2)`.
fn advance_pending(host: &mut dyn ServerHost, level: LevelRef, dimension: &str) {
    let ready = with_state(|state| state.pending.due(dimension));
    for ((_, pos), pending) in ready {
        let Some(current) = level.block_state(host, pos) else {
            continue;
        };
        let Some(relay_now) = relay_at(host, current) else {
            continue;
        };
        if relay_now.kind != pending.kind {
            continue;
        }
        // A delayed edge is re-decided from the input found now, as upstream's scheduled
        // `tick()` does; applying the captured phase would turn a relay off after its input
        // returned inside the window.
        let phase = match pending.write {
            PendingWrite::Phase(captured) => {
                let levels = relay_levels(level, host, pos, relay_now.output);
                let powered = relay_powered(&levels);
                let Some(phase) =
                    delivered_phase(relay_now.kind, powered, relay_now.phase, Some(captured))
                else {
                    continue;
                };
                phase
            }
            PendingWrite::PulseClear => RelayPhase {
                powered: relay_now.phase.powered,
                state: false,
            },
        };
        let Some(new) = relay_state(host, current, relay_now.kind, relay_now.output, phase) else {
            continue;
        };
        if write_block(host, level, dimension, pos, new, pending.notify) {
            mark_around_at(level, host, dimension, pos);
        }
    }
}

/// Re-evaluate the queued relays. A relay that switched marks its output neighbour, so the
/// first track past it re-strengthens in this same tick.
fn solve_relays(host: &mut dyn ServerHost, level: LevelRef, dimension: &str) {
    let batch: Vec<[i32; 3]> = with_state(|state| {
        let ready: Vec<(String, [i32; 3])> = state
            .relays
            .iter()
            .filter(|(dim, _)| dim == dimension)
            .cloned()
            .collect();
        for key in &ready {
            state.relays.remove(key);
        }
        ready.into_iter().map(|(_, pos)| pos).collect()
    });
    for pos in batch {
        let Some(current) = level.block_state(host, pos) else {
            continue;
        };
        let Some(relay_now) = relay_at(host, current) else {
            continue;
        };
        let levels = relay_levels(level, host, pos, relay_now.output);
        let powered = relay_powered(&levels);
        let before = relay_now.phase;
        let Some(next) = relay_edge(relay_now.kind, powered, before) else {
            // A pulse may have been placed (or loaded) with STATE=1; make sure its 2-tick
            // clear is still scheduled.
            arm_pulse_clear(dimension, pos, relay_now.kind, before);
            continue;
        };
        if relay_delayed_off(relay_now.kind, powered, before) {
            // Upstream leaves the state alone and schedules a 2-tick tick.
            with_state(|state| {
                state.pending.arm(
                    (dimension.to_owned(), pos),
                    RELAY_OFF_DELAY_TICKS,
                    relay_now.kind,
                    PendingWrite::Phase(next),
                    true,
                );
            });
            continue;
        }
        let Some(new) = relay_state(host, current, relay_now.kind, relay_now.output, next) else {
            continue;
        };
        let notify = relay_notifies(relay_now.kind, powered, before);
        if write_block(host, level, dimension, pos, new, notify) {
            mark_around_at(level, host, dimension, pos);
            arm_pulse_clear(dimension, pos, relay_now.kind, next);
        }
    }
}

/// Schedule a pulse relay's `STATE` clear. A pending tick is not re-armed, so an earlier
/// deadline wins.
fn arm_pulse_clear(dimension: &str, pos: [i32; 3], kind: RelayKind, phase: RelayPhase) {
    if kind == RelayKind::Pulse && phase.state {
        with_state(|state| {
            state.pending.arm(
                (dimension.to_owned(), pos),
                PULSE_TICKS,
                kind,
                PendingWrite::PulseClear,
                true,
            );
        });
    }
}

/// Recompute the queued tracks and everything a change reaches, in one tick.
fn solve_tracks(host: &mut dyn ServerHost, level: LevelRef, dimension: &str) {
    let mut queue: VecDeque<[i32; 3]> = with_state(|state| {
        let ready: Vec<(String, [i32; 3])> = state
            .tracks
            .iter()
            .filter(|(dim, _)| dim == dimension)
            .cloned()
            .collect();
        for key in &ready {
            state.tracks.remove(key);
        }
        ready.into_iter().map(|(_, pos)| pos).collect()
    });
    let mut visits: BTreeMap<[i32; 3], u8> = BTreeMap::new();
    let mut steps = MAX_STEPS;
    while let Some(pos) = queue.pop_front() {
        if steps == 0 {
            break;
        }
        let count = visits.entry(pos).or_insert(0);
        if *count >= MAX_VISITS {
            continue;
        }
        *count += 1;
        steps -= 1;
        let Some(current) = level.block_state(host, pos) else {
            continue;
        };
        let Some(track_now) = track_at(host, current) else {
            continue;
        };
        let ends = track_supply(level, host, pos, track_now.shape);
        let next = track_power(&ends);
        if next == track_now.power {
            continue;
        }
        let Some(new) = with_power(host, current, next) else {
            continue;
        };
        if !write_block(host, level, dimension, pos, new, true) {
            continue;
        }
        // Every track on one of the arms, and any relay reading this one.
        for arm in Dir::ALL {
            let neighbour_pos = arm.offset(pos);
            let Some(neighbour) = level.block_state(host, neighbour_pos) else {
                continue;
            };
            if let Some(neighbour_track) = track_at(host, neighbour) {
                if connected(track_now.shape, pos, neighbour_track.shape, neighbour_pos) {
                    queue.push_back(neighbour_pos);
                }
            } else if relay_of(neighbour).is_some() {
                mark_relay(dimension, neighbour_pos);
            }
        }
    }
}

/// Write `new` at `pos` and notify its neighbours. The `writing` sentinel hides the engine's
/// `block_changed` self-report from [`Hooks::block_changed`](crate::Mod).
fn write_block(
    host: &mut dyn ServerHost,
    level: LevelRef,
    dimension: &str,
    pos: [i32; 3],
    new: BlockState,
    notify: bool,
) -> bool {
    let key = (dimension.to_owned(), pos);
    with_state(|state| state.writing.insert(key.clone()));
    let wrote = level
        .set_block(
            host,
            pos,
            new,
            BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
        )
        .unwrap_or(false);
    // Also drop a sentinel the hook never consumed (a refused write), so it cannot swallow a
    // later placement.
    with_state(|state| state.writing.remove(&key));
    if wrote && notify {
        let _ = level.update_neighbors(host, pos, new, NeighbourUpdate::Neighbours);
    }
    wrote
}

// ---------------------------------------------------------------------------
// Queueing
// ---------------------------------------------------------------------------

fn mark_track(dimension: &str, pos: [i32; 3]) {
    with_state(|state| {
        if state.tracks.len() < MAX_DIRTY {
            state.tracks.insert((dimension.to_owned(), pos));
        }
    });
}

fn mark_relay(dimension: &str, pos: [i32; 3]) {
    with_state(|state| {
        if state.relays.len() < MAX_DIRTY {
            state.relays.insert((dimension.to_owned(), pos));
        }
    });
}

/// Queue every track and relay beside `pos` from a `BlockPart`'s level: a removed source, a
/// newly placed block or a solved power can all change their answer.
fn mark_around(level: &dyn LevelHost, dimension: &str, pos: [i32; 3]) {
    for arm in Dir::ALL {
        let neighbour_pos = arm.offset(pos);
        let Some(neighbour) = level.block_state(neighbour_pos) else {
            continue;
        };
        if is_track(neighbour) {
            mark_track(dimension, neighbour_pos);
        } else if relay_of(neighbour).is_some() {
            mark_relay(dimension, neighbour_pos);
        }
    }
}

/// [`mark_around`] for the tick, where the level is a handle.
fn mark_around_at(level: LevelRef, host: &dyn Host, dimension: &str, pos: [i32; 3]) {
    for arm in Dir::ALL {
        let neighbour_pos = arm.offset(pos);
        let Some(neighbour) = level.block_state(host, neighbour_pos) else {
            continue;
        };
        if is_track(neighbour) {
            mark_track(dimension, neighbour_pos);
        } else if relay_of(neighbour).is_some() {
            mark_relay(dimension, neighbour_pos);
        }
    }
}

/// A block was placed, replaced or removed: seed the first evaluation and re-evaluate the
/// blocks around it. A state write of this port is not a placement.
pub fn block_changed(level: &dyn LevelHost, event: &mut BlockChangedEvent) {
    let dimension = event.dimension.key().id();
    let pos = xyz(event.pos);
    let key = (dimension.to_owned(), pos);
    if with_state(|state| state.writing.remove(&key)) {
        return;
    }
    if is_track(event.new) {
        mark_track(dimension, pos);
    } else if relay_of(event.new).is_some() {
        mark_relay(dimension, pos);
    }
    mark_around(level, dimension, pos);
}

// ---------------------------------------------------------------------------
// The signal model
// ---------------------------------------------------------------------------

/// The signal a block at `neighbour` gives toward a reader `away` in that direction: the game's
/// own `getSignal` read, so vanilla wire power, another mod's `block_signal` and this port's
/// own answers all count (the 0.1.0 glue rebuilt this from behaviour facts and could not see
/// another mod's answer).
fn neighbour_signal(level: LevelRef, host: &dyn Host, neighbour: [i32; 3], away: Dir) -> u8 {
    u8::try_from(
        level
            .signal(host, neighbour, direction(away.opposite()))
            .unwrap_or(0)
            .clamp(0, 15),
    )
    .unwrap_or(0)
}

/// The arms of one track: a same-arm track step, a vanilla wire step, or a non-wire source at
/// full strength.
fn track_supply(level: LevelRef, host: &dyn Host, pos: [i32; 3], shape: Shape) -> Vec<Supply> {
    let mut ends = vec![Supply::None; shape.arity()];
    for (index, arm) in shape.arms().iter().enumerate() {
        let neighbour_pos = arm.offset(pos);
        let Some(neighbour) = level.block_state(host, neighbour_pos) else {
            continue;
        };
        let strength = neighbour_signal(level, host, neighbour_pos, *arm);
        if let Some(other) = track_at(host, neighbour) {
            // Only a track holding the arm that points back joins this arm.
            if connected(shape, pos, other.shape, neighbour_pos) {
                ends[index] = Supply::Track(strength);
            }
        } else if is_block(host, neighbour, WIRE_BLOCK) {
            ends[index] = Supply::Wire(strength);
        } else if strength > 0 {
            ends[index] = Supply::Source(strength);
        }
    }
    ends
}

/// Which of a track's arms reach a wired neighbour (a track pointing back, a vanilla wire, or a
/// powered source). The pen's bend gesture keeps the wired arm, and two wired arms widen the
/// net into a T.
fn wired_arms(level: LevelRef, host: &dyn Host, pos: [i32; 3], shape: Shape) -> Vec<bool> {
    let mut arms = vec![false; shape.arity()];
    for (index, arm) in shape.arms().iter().enumerate() {
        let neighbour_pos = arm.offset(pos);
        let Some(neighbour) = level.block_state(host, neighbour_pos) else {
            continue;
        };
        arms[index] = if let Some(other) = track_at(host, neighbour) {
            connected(shape, pos, other.shape, neighbour_pos)
        } else if is_block(host, neighbour, WIRE_BLOCK) {
            true
        } else {
            neighbour_signal(level, host, neighbour_pos, *arm) > 0
        };
    }
    arms
}

/// The five input sides of a relay: everything except its output face.
fn relay_levels(level: LevelRef, host: &dyn Host, pos: [i32; 3], output: Dir) -> [u8; 5] {
    let mut levels = [0u8; 5];
    let mut index = 0;
    for arm in Dir::ALL {
        if arm == output {
            continue;
        }
        let neighbour_pos = arm.offset(pos);
        levels[index] = neighbour_signal(level, host, neighbour_pos, arm);
        index += 1;
    }
    levels
}

// ---------------------------------------------------------------------------
// States: decoding (cached) and building
// ---------------------------------------------------------------------------

/// One of this port's states decoded through a `BlockPart`'s level.
fn decode(level: &dyn LevelHost, state: BlockState) -> Option<Decoded> {
    if let Some(hit) = cached(state) {
        return Some(hit);
    }
    remember(state, &level.state_string(state)?)
}

/// [`decode`] through a host, which is where the tick and the click read.
fn decode_at(host: &dyn Host, state: BlockState) -> Option<Decoded> {
    if let Some(hit) = cached(state) {
        return Some(hit);
    }
    remember(state, &host.block_state_string(state)?)
}

fn cached(state: BlockState) -> Option<Decoded> {
    DECODED
        .lock()
        .expect("the redstone pen decode cache is never held across a panic")
        .get(&state)
        .copied()
}

fn remember(state: BlockState, text: &str) -> Option<Decoded> {
    let (block, properties) = split_state(text)?;
    let decoded = decoded_from(block, &properties)?;
    DECODED
        .lock()
        .expect("the redstone pen decode cache is never held across a panic")
        .insert(state, decoded);
    Some(decoded)
}

/// A serialized state split into its block id and its properties
/// (`redstonepen:track[axis=y,bend=none,power=3]`).
fn split_state(text: &str) -> Option<(&str, Vec<(&str, &str)>)> {
    let (block, rest) = match text.split_once('[') {
        Some((block, rest)) => (block, Some(rest.strip_suffix(']')?)),
        None => (text, None),
    };
    let mut properties = Vec::new();
    for pair in rest.into_iter().flat_map(|rest| rest.split(',')) {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        properties.push(pair.split_once('=')?);
    }
    Some((block.trim(), properties))
}

/// The one of this port's states a serialized state names, or `None` for anything else.
fn decoded_from(block: &str, properties: &[(&str, &str)]) -> Option<Decoded> {
    let property = |key: &str| {
        properties
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    };
    if block == TRACK_BLOCK {
        let axis = crate::runtime::Axis::from_name(property("axis")?)?;
        // A state written without `bend` is a straight track on its axis.
        let shape = Shape::from_bend(axis, property("bend").unwrap_or("none"))?;
        let power = property("power")?.parse::<u8>().ok()?.min(15);
        return Some(Decoded::Track { shape, power });
    }
    let kind = RelayKind::from_block_id(block)?;
    let output = Dir::from_name(property("facing")?)?;
    let phase = RelayPhase {
        powered: property("powered") == Some("true"),
        state: kind.has_state() && property("state") == Some("1"),
    };
    Some(Decoded::Relay {
        kind,
        output,
        phase,
    })
}

fn track_at(host: &dyn Host, state: BlockState) -> Option<TrackNow> {
    if !is_track(state) {
        return None;
    }
    match decode_at(host, state)? {
        Decoded::Track { shape, power } => Some(TrackNow { shape, power }),
        Decoded::Relay { .. } => None,
    }
}

fn relay_at(host: &dyn Host, state: BlockState) -> Option<RelayNow> {
    relay_of(state)?;
    match decode_at(host, state)? {
        Decoded::Relay {
            kind,
            output,
            phase,
        } => Some(RelayNow {
            kind,
            output,
            phase,
        }),
        Decoded::Track { .. } => None,
    }
}

/// A track read from its state.
#[derive(Clone, Copy)]
struct TrackNow {
    shape: Shape,
    power: u8,
}

/// A relay read from its state.
#[derive(Clone, Copy)]
struct RelayNow {
    kind: RelayKind,
    output: Dir,
    phase: RelayPhase,
}

/// A fresh track state with `shape` (its arms) and `power`.
fn new_track_state(host: &dyn Host, shape: Shape) -> Option<BlockState> {
    let default = host.block_state_default(*TRACK_KEY.get()?)?;
    let metadata = host.block_states();
    let state = default.with(&metadata, "axis", shape.axis().name()).ok()?;
    let state = state.with(&metadata, "bend", shape.bend_name()).ok()?;
    state.with(&metadata, "power", "0").ok()
}

/// A track state with its `power` replaced.
fn with_power(host: &dyn Host, state: BlockState, power: u8) -> Option<BlockState> {
    let metadata = host.block_states();
    state
        .with(&metadata, "power", &power.min(15).to_string())
        .ok()
}

/// A track state with its `shape` (`axis` and `bend`) replaced.
fn with_shape(host: &dyn Host, state: BlockState, shape: Shape) -> Option<BlockState> {
    let metadata = host.block_states();
    let state = state.with(&metadata, "axis", shape.axis().name()).ok()?;
    state.with(&metadata, "bend", shape.bend_name()).ok()
}

/// A relay state with its output face and phase replaced.
fn relay_state(
    host: &dyn Host,
    state: BlockState,
    kind: RelayKind,
    output: Dir,
    phase: RelayPhase,
) -> Option<BlockState> {
    let metadata = host.block_states();
    let state = state.with(&metadata, "facing", output.name()).ok()?;
    let state = state
        .with(
            &metadata,
            "powered",
            if phase.powered { "true" } else { "false" },
        )
        .ok()?;
    if kind.has_state() {
        return state
            .with(&metadata, "state", if phase.state { "1" } else { "0" })
            .ok();
    }
    Some(state)
}

// ---------------------------------------------------------------------------
// Identity and geometry helpers
// ---------------------------------------------------------------------------

/// Whether `state` is the track block.
fn is_track(state: BlockState) -> bool {
    TRACK_KEY.get().is_some_and(|key| *key == state.block())
}

/// The relay variant `state` is, if it is one of the four.
fn relay_of(state: BlockState) -> Option<RelayKind> {
    RELAY_KEYS.get().and_then(|keys| {
        keys.iter()
            .find(|(key, _)| *key == state.block())
            .map(|(_, kind)| *kind)
    })
}

/// Whether `state` is one of this port's five blocks.
fn ours(state: BlockState) -> bool {
    is_track(state) || relay_of(state).is_some()
}

/// Whether `state` is the block registered under `id` (no allocation: the metadata table
/// borrows its name).
fn is_block(host: &dyn Host, state: BlockState, id: &str) -> bool {
    let metadata = host.block_states();
    metadata.name(state.block()).is_ok_and(|name| name == id)
}

fn is_air(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> bool {
    level.block_state(host, pos).is_some_and(|state| {
        is_block(host, state, AIR_BLOCK)
            || is_block(host, state, "minecraft:cave_air")
            || is_block(host, state, "minecraft:void_air")
    })
}

fn xyz(pos: mod_api::BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

fn dir(face: Direction) -> Dir {
    match face {
        Direction::Down => Dir::Down,
        Direction::Up => Dir::Up,
        Direction::North => Dir::North,
        Direction::South => Dir::South,
        Direction::West => Dir::West,
        Direction::East => Dir::East,
    }
}

fn direction(face: Dir) -> Direction {
    match face {
        Dir::Down => Direction::Down,
        Dir::Up => Direction::Up,
        Dir::North => Direction::North,
        Dir::South => Direction::South,
        Dir::West => Direction::West,
        Dir::East => Direction::East,
    }
}

fn dir_from_vector([x, y, z]: [i32; 3]) -> Option<Dir> {
    Some(match (x, y, z) {
        (0, -1, 0) => Dir::Down,
        (0, 1, 0) => Dir::Up,
        (0, 0, -1) => Dir::North,
        (0, 0, 1) => Dir::South,
        (-1, 0, 0) => Dir::West,
        (1, 0, 0) => Dir::East,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_text_splits_into_its_block_and_properties() {
        let (block, properties) =
            split_state("redstonepen:track[axis=y,bend=north_west,power=14]").expect("a state");
        assert_eq!(block, "redstonepen:track");
        assert_eq!(
            properties,
            vec![("axis", "y"), ("bend", "north_west"), ("power", "14")]
        );
    }

    #[test]
    fn a_state_text_without_properties_has_none() {
        let (block, properties) = split_state("minecraft:stone").expect("a state");
        assert_eq!(block, "minecraft:stone");
        assert!(properties.is_empty());
    }

    #[test]
    fn a_track_text_decodes_to_its_arms_and_its_power() {
        let decoded = decoded_from(
            "redstonepen:track",
            &[("axis", "y"), ("bend", "north_west"), ("power", "14")],
        )
        .expect("a track");
        let Decoded::Track { shape, power } = decoded else {
            panic!("a track, not {decoded:?}");
        };
        assert_eq!(shape.bend_name(), "north_west");
        assert_eq!(shape.arms(), &[Dir::North, Dir::West]);
        assert_eq!(power, 14);
    }

    #[test]
    fn a_track_text_without_bend_is_straight_on_its_axis() {
        let decoded =
            decoded_from("redstonepen:track", &[("axis", "x"), ("power", "3")]).expect("a track");
        let Decoded::Track { shape, power } = decoded else {
            panic!("a track, not {decoded:?}");
        };
        assert_eq!(shape.bend_name(), "none");
        assert_eq!(shape.axis().name(), "x");
        assert_eq!(power, 3);
    }

    #[test]
    fn a_relay_text_decodes_to_its_output_and_phase() {
        let decoded = decoded_from(
            "redstonepen:pulse_relay",
            &[("facing", "east"), ("powered", "true"), ("state", "1")],
        )
        .expect("a relay");
        assert_eq!(
            decoded,
            Decoded::Relay {
                kind: RelayKind::Pulse,
                output: Dir::East,
                phase: RelayPhase {
                    powered: true,
                    state: true,
                },
            }
        );
    }

    #[test]
    fn a_relay_without_a_state_latch_reads_no_latch() {
        let decoded = decoded_from(
            "redstonepen:relay",
            &[("facing", "up"), ("powered", "false"), ("state", "1")],
        )
        .expect("a relay");
        assert_eq!(
            decoded,
            Decoded::Relay {
                kind: RelayKind::Plain,
                output: Dir::Up,
                phase: RelayPhase {
                    powered: false,
                    state: false,
                },
            }
        );
    }

    #[test]
    fn a_foreign_block_text_decodes_to_nothing() {
        assert_eq!(decoded_from("minecraft:stone", &[]), None);
        assert_eq!(decoded_from("redstonepen:track", &[("power", "1")]), None);
    }

    #[test]
    fn an_earlier_deadline_wins_over_a_later_arm() {
        let mut pending = Pending::default();
        let key = ("minecraft:the_end".to_owned(), [1, 2, 3]);
        let first = RelayPhase {
            powered: false,
            state: false,
        };
        pending.arm(
            key.clone(),
            RELAY_OFF_DELAY_TICKS,
            RelayKind::Plain,
            PendingWrite::Phase(first),
            true,
        );
        pending.arm(
            key.clone(),
            9,
            RelayKind::Inverted,
            PendingWrite::PulseClear,
            false,
        );
        let scheduled = pending.entries.get(&key).expect("the armed entry");
        assert_eq!(scheduled.ticks, RELAY_OFF_DELAY_TICKS);
        assert_eq!(scheduled.kind, RelayKind::Plain);
        assert_eq!(scheduled.write, PendingWrite::Phase(first));
    }

    #[test]
    fn a_pending_write_lands_two_ticks_after_it_is_armed() {
        let mut pending = Pending::default();
        let key = ("minecraft:the_end".to_owned(), [4, 5, 6]);
        pending.arm(
            key.clone(),
            2,
            RelayKind::Plain,
            PendingWrite::Phase(RelayPhase::default()),
            true,
        );
        assert!(
            pending.due("minecraft:the_end").is_empty(),
            "one tick to go"
        );
        assert_eq!(
            pending.due("minecraft:the_end").len(),
            1,
            "due on the second"
        );
        assert!(pending.entries.is_empty(), "and forgotten afterwards");
    }

    #[test]
    fn another_dimension_does_not_advance_a_deadline() {
        let mut pending = Pending::default();
        pending.arm(
            ("minecraft:overworld".to_owned(), [0, 0, 0]),
            2,
            RelayKind::Plain,
            PendingWrite::Phase(RelayPhase::default()),
            true,
        );
        for _ in 0..3 {
            assert!(pending.due("minecraft:the_end").is_empty());
        }
        assert_eq!(pending.entries.len(), 1);
    }
}
