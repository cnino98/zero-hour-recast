//! Common AI state clock and native weapon status, SH 2bfaea60.
mod aircraft_path;
mod ammunition;
mod attack_move;
mod busy;
mod commands;
mod death;
pub(in crate::skirmish) mod dozer;
mod enter;
mod entry;
mod exit;
mod face;
mod follow_path;
mod hunt;
mod move_object;
mod move_position;
mod primary;
mod rappel;
mod repulsor;
pub(in crate::skirmish) mod stored_command;
pub(in crate::skirmish) mod supply;
mod waypoint;
pub(in crate::skirmish) use repulsor::State as RepulsorState;
pub(in crate::skirmish) use waypoint::State as WaypointState;
mod team_target;
mod transport;
pub use entry::State;
mod weapon;
pub(super) use weapon::Status as WeaponStatus;
pub(super) const fn native_frame(frame: u64) -> u32 {
    let bytes = frame.to_le_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
