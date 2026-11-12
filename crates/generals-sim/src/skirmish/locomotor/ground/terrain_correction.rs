use super::{CELL_SIZE, Entity, GroundRules, motive_force};

// Locomotor::fixInvalidPosition. The caller has checked the actual terrain
// class, not structure occupancy, and the represented locomotor disallows it.
pub(super) fn fix_invalid_position(
    actor: &mut Entity,
    rules: &GroundRules,
    mass: f32,
    frame: u64,
    terrain_valid: &mut impl FnMut([f32; 2]) -> bool,
) -> bool {
    if rules.is_dozer {
        return false;
    }
    let mut correction = [0_i8; 2];
    for y in -1_i8..=1 {
        for x in -1_i8..=1 {
            let dx = f32::from(x) * CELL_SIZE;
            let dy = f32::from(y) * CELL_SIZE;
            if !terrain_valid([actor.position[0] + dx, actor.position[1] + dy]) {
                correction[0] -= x.signum();
                correction[1] -= y.signum();
            }
        }
    }
    if correction == [0; 2] {
        return false;
    }
    let force = [
        f32::from(correction[0]) * mass / 5.0,
        f32::from(correction[1]) * mass / 5.0,
    ];
    let x_squared = force[0] * force[0];
    let y_squared = force[1] * force[1];
    let length = (x_squared + y_squared).sqrt();
    // Coord3D::normalize divides each component by the computed length.
    let mut direction = force;
    if length != 0.0 {
        direction[0] /= length;
        direction[1] /= length;
    }
    let x = actor.ai.motion.velocity[0] * direction[0];
    let y = actor.ai.motion.velocity[1] * direction[1];
    let dot = x + y;
    if dot > 0.25 {
        return false;
    }
    if dot < 0.0 {
        let correction_scale = (-dot).sqrt() * mass;
        direction[0] *= correction_scale;
        direction[1] *= correction_scale;
        motive_force(actor, mass, direction, frame);
    }
    motive_force(actor, mass, force, frame);
    true
}
