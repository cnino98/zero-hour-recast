//! Native `StealthUpdate` team disguise. Logic ownership and the drawable identity
//! remain separate even while the copied vehicle is being faded in or out.
use super::{DETECTED, Event, STEALTHED, Skirmish};
use crate::skirmish::{
    Entity, ai_goals::COMMAND_FROM_PLAYER, combat::attack::Goal, object_sounds::Action,
    progression::integer_real, special_ability::PowerType, weapons::Lock,
};

const DISGUISE: &str = "SpecialAbilityDisguiseAsVehicle";

pub struct Rules {
    pub transition: u32,
    pub reveal_transition: u32,
    pub effect: String,
    pub reveal_effect: String,
    pub sounds: [String; 3],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Appearance {
    pub type_index: usize,
    pub owner: usize,
}

#[derive(Default)]
pub(super) struct State {
    wanted: Option<Appearance>,
    shown: Option<Appearance>,
    remaining: u32,
    midpoint: bool,
    revision: u32,
}

impl Entity {
    pub(in crate::skirmish) fn disguised_player(&self) -> Option<usize> {
        self.stealth
            .as_ref()?
            .disguise
            .wanted
            .map(|appearance| appearance.owner)
    }

    #[must_use]
    pub fn disguised_appearance(&self) -> Option<Appearance> {
        self.stealth.as_ref()?.disguise.shown
    }

    /// `StealthUpdate`'s disguise template used by native mouseover hints.
    #[must_use]
    pub fn disguise_hover_appearance(&self) -> Option<Appearance> {
        self.stealth.as_ref()?.disguise.wanted
    }

    #[must_use]
    pub fn disguise_drawable_revision(&self) -> u32 {
        self.stealth
            .as_ref()
            .map_or(0, |state| state.disguise.revision)
    }
}

impl Skirmish {
    pub fn bomb_truck_disguise_ready(&mut self, id: usize) -> bool {
        self.ready_actor(id).is_ok_and(|actor| {
            !actor.masked()
                && !self.object_disabled(id)
                && self
                    .first_special_ability(id, PowerType::SpecialDisguiseAsVehicle)
                    .is_some_and(|ordinal| !actor.special_ability_in_use(ordinal))
        }) && self.special_power_ready(id, DISGUISE)
    }

    pub fn bomb_truck_disguise_target(&mut self, id: usize, target: usize) -> bool {
        self.bomb_truck_disguise_ready(id)
            && self
                .entities
                .get(target)
                .is_some_and(|victim| victim.exists() && !victim.masked())
            && self.can_special_power_at_object_from(
                id,
                DISGUISE,
                PowerType::SpecialDisguiseAsVehicle,
                target,
                COMMAND_FROM_PLAYER,
            )
    }

    /// # Errors
    /// Rejects unavailable disguise commands and vehicles the actor cannot copy.
    pub fn bomb_truck_order_disguise(
        &mut self,
        id: usize,
        target: usize,
    ) -> Result<(), &'static str> {
        if self.bomb_truck_disguise_target(id, target)
            && self.special_power_at_object(id, DISGUISE, target)
        {
            Ok(())
        } else {
            Err("Choose a visible vehicle to disguise as")
        }
    }

    pub fn can_detonate_bomb_truck(&mut self, owner: usize, id: usize) -> bool {
        self.ready_actor(id).is_ok_and(|actor| {
            actor.owner == owner
                && !actor.masked()
                && !self.object_disabled(id)
                && self.catalog[actor.type_index]
                    .behavior
                    .stealth
                    .as_ref()
                    .is_some_and(|rules| rules.disguise.is_some())
                && self
                    .allocated_weapon(id, 0)
                    .is_some_and(|weapon| weapon.name == "SuicideWeapon")
        }) && self.weapon_button_ready(id, 0)
    }

    /// # Errors
    /// Rejects unavailable Bomb Trucks and trucks controlled by another player.
    pub fn detonate_bomb_truck(&mut self, owner: usize, id: usize) -> Result<(), &'static str> {
        if !self.can_detonate_bomb_truck(owner, id) {
            return Err("Bomb Truck cannot detonate");
        }
        // MSG_DO_WEAPON locks PRIMARY before attacking the truck's own position.
        if !matches!(self.entities[id].weapon_set.lock, Lock::Permanent) {
            self.entities[id].weapon_set.current = 0;
            self.entities[id].weapon_set.lock = Lock::Temporary;
        }
        self.native_attack_goal_from(
            id,
            Goal::Position(self.object_position(id)),
            i32::MAX.unsigned_abs(),
            COMMAND_FROM_PLAYER,
            false,
        );
        Ok(())
    }

    #[must_use]
    pub fn apparent_disguise_owner(&self, id: usize, viewer: usize) -> usize {
        let actor = &self.entities[id];
        if self.allied(actor.owner, viewer) || !self.player_active(viewer) {
            return actor.owner;
        }
        actor
            .disguised_appearance()
            .map_or(actor.owner, |appearance| appearance.owner)
    }

    pub(in crate::skirmish) fn disguise_as_vehicle(&mut self, id: usize, target: usize) {
        let victim = &self.entities[target];
        let appearance = victim
            .stealth
            .as_ref()
            .and_then(|state| state.disguise.wanted)
            .unwrap_or(Appearance {
                type_index: victim.type_index,
                owner: victim.owner,
            });
        let Some(rules) = self.catalog[self.entities[id].type_index]
            .behavior
            .stealth
            .as_ref()
            .and_then(|rules| rules.disguise.as_ref())
        else {
            return;
        };
        let state = self.entities[id].stealth.as_mut().expect("disguise module");
        state.enabled = true;
        state.disguise.wanted = Some(appearance);
        state.disguise.remaining = rules.transition;
        state.disguise.midpoint = false;
        self.wake_family_update(id, "StealthUpdate", 0, 1);
    }

    pub(super) fn reveal_vehicle_disguise(&mut self, id: usize, transition: u32) {
        let state = self.entities[id].stealth.as_mut().expect("disguise module");
        if state.disguise.wanted.is_some() && state.disguise.shown.is_some() {
            state.disguise.wanted = None;
            state.disguise.remaining = transition;
            state.disguise.midpoint = false;
        }
    }

    pub(super) fn update_disguise(&mut self, id: usize) -> bool {
        let actor = &mut self.entities[id];
        if !actor.has_drawable() {
            return true;
        }
        let state = actor.stealth.as_mut().expect("disguise module");
        let remaining = &mut state.disguise.remaining;
        if *remaining == 0 {
            self.presentation
                .stealth_events
                .push(Event::Pulse { object: id });
            return true;
        }
        *remaining -= 1;
        let rules = self.catalog[actor.type_index]
            .behavior
            .stealth
            .as_ref()
            .and_then(|rules| rules.disguise.as_ref())
            .expect("disguise rules");
        let toward_disguise = state.disguise.wanted.is_some();
        let duration = if toward_disguise {
            rules.transition
        } else {
            rules.reveal_transition
        };
        let factor = 1.0 - integer_real(*remaining) / integer_real(duration);
        let switch = factor >= 0.5 && !state.disguise.midpoint;
        let revealed = *remaining == 0 && !toward_disguise;
        if switch {
            state.disguise.midpoint = true;
            self.change_disguise_drawable(id);
        }
        self.presentation
            .stealth_events
            .push(Event::DisguiseOpacity {
                object: id,
                opacity: (1.0 - factor * 2.0).abs(),
            });
        if revealed {
            let state = self.entities[id].stealth.as_mut().expect("disguise module");
            state.enabled = false;
            state.set(STEALTHED | DETECTED, false);
            return false;
        }
        true
    }

    fn change_disguise_drawable(&mut self, id: usize) {
        // StealthUpdate destroys and prepends a new Drawable even for the same ObjectID.
        self.renew_drawable_birth(id);
        let actor = &mut self.entities[id];
        let state = actor.stealth.as_mut().expect("disguise module");
        state.disguise.shown = state.disguise.wanted;
        state.disguise.revision = state.disguise.revision.wrapping_add(1);
        let disguised = state.disguise.shown.is_some();
        let rules = self.catalog[actor.type_index]
            .behavior
            .stealth
            .as_ref()
            .and_then(|rules| rules.disguise.as_ref())
            .expect("disguise rules");
        let (sound, fx) = if disguised {
            actor.object_status |= 1 << 43;
            (rules.sounds[0].clone(), rules.effect.clone())
        } else {
            actor.object_status &= !(1 << 43);
            let sound = if actor.ai.current_victim.is_some() {
                1
            } else {
                2
            };
            (rules.sounds[sound].clone(), rules.reveal_effect.clone())
        };
        if self.object_physics_mass(id).is_some() {
            self.reset_dynamic_physics(id);
        }
        self.object_sound(id, sound, Action::Play);
        if !fx.is_empty() {
            self.positional_fx_with_radius(id, fx, self.object_position(id), 0.0);
        }
    }
}
