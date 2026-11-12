//! Original salvage context is an ordinary group move with its own voice.
use super::{Match, Mouse, Voice, VoiceRequest, move_group};
use bevy::prelude::Vec3;

pub fn can_target(battle: &Match, target: usize) -> bool {
    battle.simulation.crate_visible_to(0, target)
        && battle.selected.iter().any(|&id| {
            let actor = &battle.simulation.entities[id];
            actor.owner == 0
                && (battle.simulation.can_collect_money_crate(id, target)
                    || battle.simulation.catalog[actor.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "SALVAGER"))
        })
}

pub fn issue(battle: &mut Match, mouse: &mut Mouse, target: usize) -> bool {
    if mouse.waypoint_mode || !can_target(battle, target) {
        return false;
    }
    let point = Vec3::from_array(battle.simulation.object_position(target));
    if !move_group(battle, point, false) {
        return false;
    }
    mouse.voices.push(VoiceRequest::new(
        &battle.simulation,
        &battle.selected,
        Voice::Salvage,
        false,
    ));
    mouse.move_hints.push(point.truncate().to_array());
    true
}
