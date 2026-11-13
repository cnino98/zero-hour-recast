//! SH JetSlowDeathBehavior.cpp147–335; NORMAL extends the actual `SlowDeath` callback.
use super::{Jet, NativeState, Phase};
use crate::skirmish::{
    Entity, Skirmish,
    death_lifecycle::{Derived, native::Extension},
    object_sounds::Action,
    partition::Identity,
};

const DECK_HEIGHT_OFFSET: u64 = 1 << 29;

impl Entity {
    #[must_use]
    pub fn jet_corpse(&self) -> bool {
        !self.masked()
            && self.death_lifecycle.as_ref().is_some_and(|state| {
                state
                    .native
                    .iter()
                    .any(|state| state.active() && matches!(state.extension, Extension::Jet(_)))
            })
    }

    fn native_jet(&self, index: usize) -> &NativeState {
        let Extension::Jet(state) = &self
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native[index]
            .extension
        else {
            unreachable!("JetSlowDeath state")
        };
        state
    }

    fn native_jet_mut(&mut self, index: usize) -> &mut NativeState {
        let Extension::Jet(state) = &mut self
            .death_lifecycle
            .as_mut()
            .expect("SlowDeath state")
            .native[index]
            .extension
        else {
            unreachable!("JetSlowDeath state")
        };
        state
    }
}

impl Skirmish {
    fn native_jet_rule(&self, id: usize, index: usize) -> &Jet {
        let Derived::Jet(rule) = &self.native_slow_rule(id, index).derived else {
            unreachable!("JetSlowDeath data")
        };
        rule
    }

    pub(in crate::skirmish) fn jet_slow_death_on_die(&mut self, id: usize, ordinal: usize) {
        let index = self.slow_death_index(id, "JetSlowDeathBehavior", ordinal);
        let above = self.height_above_ground(id);
        if above <= -9.0 * self.gravity || self.entities[id].object_status & DECK_HEIGHT_OFFSET != 0
        {
            // The ground branch precedes DieMux applicability and weighted selection.
            self.native_jet_phase(id, index, 0);
            self.destroy_maintenance_object(id);
        } else {
            self.native_slow_death_on_die(id, index);
        }
        self.entities[id].object_status &= !DECK_HEIGHT_OFFSET;
    }

    pub(in crate::skirmish) fn begin_native_jet_death(&mut self, id: usize, index: usize) {
        self.activate_slow_death(id, index);
        let now = self.update_frame();
        self.entities[id].native_jet_mut(index).dead_frame = now;
        self.native_jet_phase(id, index, 1);
        let rule = self.native_jet_rule(id, index);
        let sound = rule.loop_sound.clone();
        let roll = rule.roll_rate;
        let lift = -self.gravity * (1.0 - rule.fall_speed);
        self.object_sound(
            id,
            sound,
            Action::PlayLoop(self.slow_death_module_index(id, index)),
        );
        self.entities[id].native_jet_mut(index).roll = roll;
        self.set_flight_lift_limit(id, lift);
        self.set_flight_turn_limit(id, 0.0);
    }

    fn native_jet_phase(&mut self, id: usize, index: usize, phase: usize) {
        let Phase { effect, objects } = self.native_jet_rule(id, index).phases[phase].clone();
        self.object_fx(id, effect);
        if let Some(list) = objects {
            self.create_physical_ocl(id, &list)
                .expect("bound Jet death OCL");
        }
    }

    pub(in crate::skirmish) fn update_jet_slow_death(&mut self, id: usize, ordinal: usize) -> i32 {
        let index = self.slow_death_index(id, "JetSlowDeathBehavior", ordinal);
        self.update_slow_death_base(id, index);
        if !self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native[index]
            .active()
        {
            return 1;
        }
        let roll = self.entities[id].native_jet(index).roll;
        self.entities[id].set_physics_roll_rate(roll);
        let delta = self.native_jet_rule(id, index).roll_delta;
        self.entities[id].native_jet_mut(index).roll *= delta;
        let now = self.update_frame();
        let ground_frame = self.entities[id].native_jet(index).ground_frame;
        if ground_frame == 0 {
            let point = self.object_position(id);
            let terrain = self.ground_height(self.entities[id].position);
            let layer = self
                .movement_terrain
                .get_layer_for_destination(point, terrain);
            self.entities[id].layer = layer;
            let ground = self.movement_terrain.destination_layer_height(
                layer,
                [point[0], point[1]],
                terrain,
            );
            let mut height = point[2] - ground;
            if layer != 1 && (0.0..=1.0).contains(&height) {
                height = 0.0;
            }
            if height <= 0.0 || self.physics_collided_with_tree(id) {
                let sound = self.native_jet_rule(id, index).loop_sound.clone();
                self.object_sound(
                    id,
                    sound,
                    Action::StopLoop(self.slow_death_module_index(id, index)),
                );
                self.native_jet_phase(id, index, 3);
                self.entities[id].native_jet_mut(index).ground_frame = now;
                let pitch = self.native_jet_rule(id, index).pitch_rate;
                self.entities[id].set_physics_pitch_rate(pitch);
            }
            let death_frame = self.entities[id].native_jet(index).dead_frame;
            if death_frame != 0
                && now.wrapping_sub(death_frame) >= self.native_jet_rule(id, index).secondary_delay
            {
                self.native_jet_phase(id, index, 2);
                self.entities[id].native_jet_mut(index).dead_frame = 0;
            }
        } else if now.wrapping_sub(ground_frame) >= self.native_jet_rule(id, index).final_delay {
            self.native_jet_phase(id, index, 4);
            self.destroy_maintenance_object(id);
        }
        1
    }

    pub(in crate::skirmish) fn physics_collided_with_tree(&self, id: usize) -> bool {
        let kinds = match self.entities[id].physics_last_collidee() {
            Some(Identity::Actor(other)) => {
                let actor = &self.entities[other];
                if actor.maintenance.deleted {
                    return false;
                }
                &self.catalog[actor.type_index].kind_of
            }
            Some(Identity::Map(index)) => &self.layout.world_objects[index].kind_of,
            None => return false,
        };
        kinds.iter().any(|kind| kind == "SHRUBBERY")
    }
}
