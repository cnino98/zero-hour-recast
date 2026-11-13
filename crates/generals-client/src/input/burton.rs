//! Native object-weapon and special-power buttons for Colonel Burton.
use super::{CommandMode, Mouse, locally_controlled};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};
use generals_sim::skirmish::special_ability::BurtonCharge;

#[derive(Clone, Copy)]
pub enum TargetCommand {
    Knife,
    Charge(BurtonCharge),
}

pub fn can_target(battle: &mut Match, command: TargetCommand, target: usize) -> bool {
    for index in 0..battle.selected.len() {
        let id = battle.selected[index];
        if locally_controlled(battle, id)
            && match command {
                TargetCommand::Knife => battle.simulation.burton_knife_target(id, target),
                TargetCommand::Charge(charge) => {
                    battle.simulation.burton_charge_target(id, charge, target)
                }
            }
        {
            return true;
        }
    }
    false
}

pub(super) fn issue(battle: &mut Match, mouse: &mut Mouse, command: TargetCommand, target: usize) {
    if !can_target(battle, command, target) {
        return;
    }
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) {
            let result = match command {
                TargetCommand::Knife => battle.simulation.burton_order_knife(id, target),
                TargetCommand::Charge(charge) => {
                    battle.simulation.burton_order_charge(id, charge, target)
                }
            };
            issued |= result.is_ok();
        }
    }
    if issued {
        match command {
            TargetCommand::Knife => mouse.voices.push(VoiceRequest::new(
                &battle.simulation,
                &battle.selected,
                Voice::BurtonKnifeAttack,
                false,
            )),
            TargetCommand::Charge(charge) => {
                mouse.voices.extend(charge_response(battle, charge));
            }
        }
        mouse.pending_command = CommandMode::Normal;
    }
}

pub fn detonate(battle: &mut Match, mouse: &mut Mouse) {
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) {
            issued |= battle.simulation.burton_detonate(id).is_ok();
        }
    }
    if issued {
        mouse.pending_command = CommandMode::Normal;
    }
}

fn charge_response(battle: &Match, charge: BurtonCharge) -> Option<VoiceRequest> {
    let power = match charge {
        BurtonCharge::Timed => "SPECIAL_TIMED_CHARGES",
        BurtonCharge::Remote => "SPECIAL_REMOTE_CHARGES",
    };
    VoiceRequest::special_power(&battle.simulation, &battle.selected, &[power])
}
