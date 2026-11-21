//! Native `SpecialAbilityUpdate` intent, phases and object lifetime ownership.
use super::{Entity, Skirmish, module_updates::FOREVER};
mod burton;
pub use burton::BurtonCharge;
mod capture;
mod effects;
#[cfg(test)]
pub(in crate::skirmish) use capture::fixture as capture_fixture;
mod objects;
mod phases;
mod power;
pub use power::PowerType;
mod preparation;
mod range;

pub struct Power {
    pub name: String,
    pub kind: PowerType,
}

#[derive(Clone, Copy)]
pub struct Approach {
    pub start_range: f32,
    pub abort_range: f32,
    pub requires_los: bool,
    pub face_target: bool,
    pub flee_range: f32,
}

#[derive(Clone, Copy)]
pub struct Timings {
    pub preparation: u32,
    pub persistent: u32,
    pub pack: u32,
    pub unpack: u32,
    pub unstealth: u32,
    pub effect: u32,
    pub variation: f32,
}

pub struct Objects {
    pub name: String,
    pub bone: String,
    pub maximum: u32,
    pub persistent: bool,
    pub unique_targets: bool,
    pub persist_on_death: bool,
}

#[derive(Clone, Copy)]
pub struct Packing {
    pub skip_no_target: bool,
    pub flip_after_pack: bool,
    pub flip_after_unpack: bool,
}

#[derive(Clone, Copy)]
pub struct Preparation {
    pub always_validate: bool,
    pub capture_fx: bool,
    pub persistence_recharges: bool,
}

#[derive(Clone, Copy)]
pub struct Effects {
    pub value: i32,
    pub experience: i32,
    pub skill_points: i32,
    pub lose_stealth: bool,
}

pub struct Feedback {
    pub disable_particle: Option<String>,
    /// Pack, Unpack, `PrepSoundLoop`, `TriggerSound` in source field order.
    pub sounds: [String; 4],
    pub completion_voice: String,
    pub capture_tick: String,
}

pub struct Rules {
    pub power: Option<Power>,
    pub approach: Approach,
    pub timing: Timings,
    pub objects: Objects,
    pub packing: Packing,
    pub preparation: Preparation,
    pub effects: Effects,
    pub feedback: Feedback,
}

#[derive(Clone, Copy)]
pub(super) enum PackingState {
    None,
    Packed,
    Unpacking,
    Unpacked,
    Packing,
}

pub(super) struct Facing {
    initiated: bool,
    complete: bool,
}

pub(super) struct Preparing {
    frames: u32,
    within_start_range: bool,
    disable_particles: bool,
}

pub(super) struct State {
    kind: PowerType,
    active: bool,
    no_target: bool,
    capture_flash: f32,
    animation_frames: u32,
    target: Option<usize>,
    target_position: [f32; 3],
    location_count: i32,
    object_entries: u32,
    objects: Vec<usize>,
    packing: PackingState,
    facing: Facing,
    preparing: Preparing,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_special_ability(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.special_abilities.len(), ordinal);
        let kind = self.ability_kind(id, ordinal);
        self.entities[id].behaviors.special_abilities.push(State {
            kind,
            active: false,
            no_target: false,
            capture_flash: 0.0,
            animation_frames: 0,
            target: None,
            target_position: [0.0; 3],
            location_count: 0,
            object_entries: 0,
            objects: Vec::new(),
            packing: PackingState::None,
            facing: Facing {
                initiated: false,
                complete: false,
            },
            preparing: Preparing {
                frames: 0,
                within_start_range: false,
                disable_particles: true,
            },
        });
        self.wake_family_update(id, "SpecialAbilityUpdate", ordinal, FOREVER);
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn native_ability_active(&self, id: usize, ordinal: usize) -> bool {
        self.entities[id].behaviors.special_abilities[ordinal].active
    }

    pub(in crate::skirmish) fn native_charge_target_available(
        &self,
        id: usize,
        kind: PowerType,
        target: usize,
    ) -> bool {
        let Some(ordinal) = self.first_special_ability(id, kind) else {
            return false;
        };
        if self.entities[id].behaviors.special_abilities[ordinal].object_entries
            >= self.ability_rules(id, ordinal).objects.maximum
        {
            return false;
        }
        let other = match kind {
            PowerType::SpecialRemoteCharges => {
                self.first_special_ability(id, PowerType::SpecialTimedCharges)
            }
            PowerType::SpecialTimedCharges => {
                self.first_special_ability(id, PowerType::SpecialRemoteCharges)
            }
            _ => None,
        };
        [Some(ordinal), other].into_iter().flatten().all(|index| {
            !self.entities[id].behaviors.special_abilities[index]
                .objects
                .iter()
                .any(|&object| {
                    self.entities[object].exists() && self.entities[object].producer == Some(target)
                })
        })
    }

    fn ability_rules(&self, id: usize, ordinal: usize) -> &Rules {
        &self.catalog[self.entities[id].type_index]
            .behavior
            .special_abilities[ordinal]
    }

    fn ability_kind(&self, id: usize, ordinal: usize) -> PowerType {
        self.ability_rules(id, ordinal)
            .power
            .as_ref()
            .map_or(PowerType::SpecialInvalid, |power| power.kind)
    }

    fn ability_loop_index(&self, id: usize, ordinal: usize) -> usize {
        self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.family == "SpecialAbilityUpdate")
            .nth(ordinal)
            .map(|(index, _)| index)
            .expect("authored special ability instance")
    }

    pub(in crate::skirmish) fn first_special_ability(
        &self,
        id: usize,
        kind: PowerType,
    ) -> Option<usize> {
        self.catalog[self.entities[id].type_index]
            .behavior
            .special_abilities
            .iter()
            .position(|rules| rules.power.as_ref().is_some_and(|power| power.kind == kind))
    }

    fn present_ability_target(&self, id: usize, ordinal: usize) -> Option<usize> {
        self.entities[id].behaviors.special_abilities[ordinal]
            .target
            .filter(|&target| self.entities[target].exists())
    }
}

impl Entity {
    #[must_use]
    pub fn special_ability_in_use(&self, ordinal: usize) -> bool {
        let state = &self.behaviors.special_abilities[ordinal];
        !matches!(state.packing, PackingState::None) && state.preparing.within_start_range
    }
}

const FIRING_A: u128 = 1 << 36;
const PACKING: u128 = 1 << 73;
const UNPACKING: u128 = 1 << 74;
const RAISING_FLAG: u128 = 1 << 87;
const ABILITY_MODEL_FLAGS: u128 = FIRING_A | PACKING | UNPACKING | RAISING_FLAG;

/// Client effects retain the source/victim ownership at their actual callback.
pub enum FeedbackEffect {
    Infiltration {
        building_stolen_warning: bool,
    },
    CaptureFlash,
    BuildingStolen,
    CashStolen(u32),
    PowerCashStolen(u32),
    DisableParticles {
        name: String,
        lifetime: u32,
        geometry: crate::collision_geometry::Geometry,
    },
}

pub struct FeedbackEvent {
    pub object: usize,
    pub target: usize,
    pub owner: usize,
    pub victim: usize,
    pub frame: u64,
    pub source_position: [f32; 3],
    pub target_position: [f32; 3],
    pub effect: FeedbackEffect,
}

impl Skirmish {
    pub(in crate::skirmish) fn native_ability_feedback(
        &mut self,
        id: usize,
        target: usize,
        effect: FeedbackEffect,
    ) {
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::AbilityFeedback(
                self.presentation.ability_feedback.len(),
            ));
        self.presentation.ability_feedback.push(FeedbackEvent {
            object: id,
            target,
            owner: self.entities[id].owner,
            victim: self.entities[target].owner,
            frame: self.tick,
            source_position: self.object_position(id),
            target_position: self.object_position(target),
            effect,
        });
    }
}

/// Require the native `StealthUpdate` consumer for an authored vehicle disguise.
pub(in crate::skirmish) fn validate(unit: &super::UnitType) -> Result<(), String> {
    for (ordinal, ability) in unit.behavior.special_abilities.iter().enumerate() {
        if ability
            .power
            .as_ref()
            .is_some_and(|power| power.kind == PowerType::SpecialDisguiseAsVehicle)
            && unit
                .behavior
                .stealth
                .as_ref()
                .is_none_or(|rules| rules.disguise.is_none())
        {
            let module = unit
                .behavior_modules
                .iter()
                .filter(|module| module.family == "SpecialAbilityUpdate")
                .nth(ordinal)
                .expect("authored SpecialAbilityUpdate data");
            return Err(format!(
                "{}: SpecialAbilityUpdate {} at {}:{} requires native StealthUpdate disguise drawable replacement",
                unit.name, module.tag, module.source.path, module.source.line
            ));
        }
    }
    Ok(())
}

impl Entity {
    pub(in crate::skirmish) fn ability_approaching(&self) -> bool {
        self.behaviors
            .special_abilities
            .iter()
            .any(|state| state.active && !state.preparing.within_start_range)
    }
}

impl Entity {
    #[must_use]
    pub fn laser_target(&self) -> Option<usize> {
        self.behaviors
            .special_abilities
            .iter()
            .find(|state| state.kind == PowerType::SpecialMissileDefenderLaserGuidedMissiles)
            .filter(|state| state.active && state.preparing.frames > 0)
            .and_then(|state| state.target)
    }
}

#[cfg(test)]
#[path = "laser_lock/tests.rs"]
mod laser_tests;

impl Entity {
    #[must_use]
    pub fn helix_bomb_object(&self) -> Option<usize> {
        self.behaviors
            .special_abilities
            .iter()
            .find(|state| state.kind == PowerType::SpecialHelixNapalmBomb)
            .and_then(|state| state.objects.first().copied())
    }
}

#[cfg(test)]
#[path = "helix/native_bomb_tests.rs"]
mod helix_bomb_tests;

impl Entity {
    #[must_use]
    pub fn hacker_disable_target(&self) -> Option<usize> {
        self.behaviors
            .special_abilities
            .iter()
            .find(|state| state.kind == PowerType::SpecialHackerDisableBuilding)
            .filter(|state| state.active && state.preparing.frames > 0)
            .and_then(|state| state.target)
    }
}

#[cfg(test)]
#[path = "hacker_disable/tests.rs"]
mod hacker_disable_tests;
