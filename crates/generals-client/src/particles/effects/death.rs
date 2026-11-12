//! Projectile ignition visibility uses the actual projectile template footprint.
pub(super) fn visible(
    game: &generals_sim::skirmish::Skirmish,
    event: &generals_sim::skirmish::ProjectileFxEvent,
) -> bool {
    use generals_sim::{collision_geometry::Body, skirmish::visibility::Status};
    let kind = game.catalog[event.weapon_type]
        .weapon_template(event.weapon_index)
        .and_then(|weapon| weapon.motion.as_ref())
        .and_then(|motion| motion.projectile_type)
        .expect("projectile object FX template");
    let body = Body {
        position: event.position,
        angle: event.axes[0][1].atan2(event.axes[0][0]),
        geometry: &game.catalog[kind].geometry,
    };
    matches!(
        game.visibility_footprint(0, &body),
        Status::Clear | Status::Partial
    )
}
