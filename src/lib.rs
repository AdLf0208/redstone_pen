//! Redstone Pen for Horizon — the pen, the two-or-three-arm track and the four relays.
//!
//! Ported from Redstone Pen 1.11.42 (stfwi/redstonepen, MIT); `notes.md` records the behaviour
//! read from the upstream repository, the hook map, the gaps and the migration to the SDK
//! format. The rules that decide power, the pen's gestures and the relays' edges live in
//! [`runtime`], which has no `mod_api` type and is unit-tested by `cargo test` alone; the game
//! side — the scoped Parts, the click and the tick's bounded solve — is [`support`].
//!
//! Three moments are global, not scoped: [`Hooks::block_changed`] (every block write in the
//! world is a candidate trigger for the tracks beside it) and [`Hooks::level_tick_post`] (the
//! solve runs once per level tick, where a write is allowed). Everything that belongs to one
//! of the five blocks is a `BlockPart`; the pen's click is an `ItemPart`, because a click lands
//! on any block, including vanilla supports.

pub mod runtime;
mod support;

use mod_api::{
    self, BlockChangedEvent, Hooks, HostSide, LevelHost, LevelTickPostEvent, Registrar,
    RegistrationContext,
};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// Declare the track, the four relays and the pen, and attach the two Parts.
    fn register(&self, registrar: &mut dyn Registrar, _context: &RegistrationContext) {
        support::register(registrar);
    }

    /// A block was placed, replaced or removed: seed the first evaluation of the tracks and
    /// relays beside it. A state write of this port is swallowed by its own sentinel.
    fn block_changed(&self, level: &dyn LevelHost, event: &mut BlockChangedEvent) {
        support::block_changed(level, event);
    }

    /// End of the level tick: apply the queued delayed relay writes, the relays and the track
    /// solve. A client's tick runs nothing.
    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        let HostSide::Server(host) = host else {
            return;
        };
        support::level_tick_post(host, event.level);
    }
}
