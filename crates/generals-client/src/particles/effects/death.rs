//! Projectile ignition visibility uses the actual projectile template footprint.
pub(super) fn visible(
    game: &generals_sim::skirmish::Skirmish,
    event: &generals_sim::skirmish::ProjectileFxEvent,
    projectile_type: usize,
) -> bool {
    use generals_sim::{collision_geometry::Body, skirmish::visibility::Status};
    let body = Body {
        position: event.position,
        angle: event.axes[0][1].atan2(event.axes[0][0]),
        geometry: &game.catalog[projectile_type].geometry,
    };
    matches!(
        game.visibility_footprint(0, &body),
        Status::Clear | Status::Partial
    )
}
