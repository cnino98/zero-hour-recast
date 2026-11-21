//! `CommandXlat::evaluateForceAttack` and `WeaponSet`'s player relationship gate.
use super::{Mouse, locally_controlled, selection};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};

pub fn ordinary_target(battle: &mut Match, id: usize, target: usize) -> bool {
    locally_controlled(battle, id)
        && battle.simulation.entities[id].complete()
        && !battle.simulation.entities[id].masked()
        && matches!(
            battle.simulation.player_attack_result(id, target),
            generals_sim::skirmish::CanAttackResult::Possible
                | generals_sim::skirmish::CanAttackResult::PossibleAfterMoving
        )
}

pub fn ordinary_selection_target(battle: &mut Match, target: usize) -> bool {
    for index in 0..battle.selected.len() {
        let id = battle.selected[index];
        if ordinary_target(battle, id, target) {
            return true;
        }
    }
    false
}

// Despite its name, native canAnyForceAttack returns the first Drawable object's result.
pub fn force_result(
    battle: &mut Match,
    target: Option<usize>,
    position: Option<bevy::prelude::Vec3>,
) -> generals_sim::skirmish::CanAttackResult {
    use generals_sim::skirmish::CanAttackResult;
    if target.is_none() && position.is_none() {
        return CanAttackResult::NotPossible;
    }
    let Some(id) = battle.selected.iter().copied().find(|&id| {
        battle
            .simulation
            .entities
            .get(id)
            .is_some_and(|actor| actor.exists() && actor.has_drawable())
    }) else {
        return CanAttackResult::NotPossible;
    };
    if !locally_controlled(battle, id) {
        return CanAttackResult::NotPossible;
    }
    let position = position.map_or([0.0; 3], |point| point.to_array());
    battle.simulation.force_attack_result(id, target, position)
}

pub(super) fn force_order(
    battle: &mut Match,
    target: Option<usize>,
    position: Option<bevy::prelude::Vec3>,
    mouse: &mut Mouse,
) {
    use generals_sim::skirmish::CanAttackResult;
    if matches!(mouse.pending_command, super::CommandMode::AttackMove) {
        mouse.pending_command = super::CommandMode::Normal;
    }
    if !matches!(
        force_result(battle, target, position),
        CanAttackResult::Possible | CanAttackResult::PossibleAfterMoving
    ) {
        return;
    }
    if !battle.commands.executing {
        super::commands::enqueue(
            battle,
            Some(mouse),
            super::commands::Kind::Force {
                target,
                point: position,
            },
        );
        return;
    }
    let response = VoiceRequest::new(
        &battle.simulation,
        battle.command_drawables(),
        Voice::Attack,
        false,
    );
    let members = selection::command_units(battle);
    let result = if let Some(target) = target {
        battle
            .simulation
            .group_attack_objects(&members, target, true)
    } else if let Some(position) = position {
        battle
            .simulation
            .group_force_attack_position(&members, position.to_array())
    } else {
        return;
    };
    if result.is_ok() {
        mouse.voices.push(response);
    }
}

// CommandXlat's isFiringGUICommand list, for the currently admitted GUI modes.
pub(super) const fn left_gui_command(command: &super::CommandMode) -> bool {
    matches!(
        command,
        super::CommandMode::Power { .. }
            | super::CommandMode::FireWeapon(_)
            | super::CommandMode::ClearMines(_)
            | super::CommandMode::SnipeVehicle(_)
            | super::CommandMode::Burton(_)
            | super::CommandMode::HelixBomb
            | super::CommandMode::CombatDrop
            | super::CommandMode::Hijack
            | super::CommandMode::CarBomb
            | super::CommandMode::Capture
            | super::CommandMode::LaserLock
            | super::CommandMode::DisableBuilding
            | super::CommandMode::Hack(_)
            | super::CommandMode::Cleanup
    )
}
