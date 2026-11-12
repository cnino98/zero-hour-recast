//! `JetAIUpdate::addTargeter`, `positionLockon` and targeter pruning, SH 2bfaea60.
use crate::skirmish::{Entity, Skirmish, ai_dispatch::native_frame};
use num_traits::ToPrimitive;

pub struct Rules {
    pub frames: u32,
    pub cursor: String,
    pub tick_sound: String,
    pub initial_distance: f32,
    pub frequency: f32,
    pub angle_spin: f32,
    pub blinky: bool,
}

impl Rules {
    #[must_use]
    pub const fn native() -> Self {
        Self {
            frames: 0,
            cursor: String::new(),
            tick_sound: String::new(),
            initial_distance: 100.0,
            frequency: 0.5,
            angle_spin: 720.0,
            blinky: false,
        }
    }
}

pub struct Drawable {
    pub template: usize,
    pub position: [f32; 3],
    pub facing: f32,
    pub hidden: bool,
}

impl Entity {
    #[must_use]
    pub fn lockon_drawable(&self) -> Option<&Drawable> {
        self.aircraft.as_ref()?.lockon_drawable.as_ref()
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn add_attack_targeter(&mut self, victim: usize, attacker: usize) {
        let Some(actor) = self.entities.get_mut(victim) else {
            return;
        };
        let Some(state) = &mut actor.aircraft else {
            return;
        };
        let rules = &self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("JetAIUpdate data")
            .lockon;
        if rules.frames == 0 || state.targeters.contains(&attacker) {
            return;
        }
        state.targeters.push(attacker);
        if state.untargetable_expire == 0 && state.targeters.len() == 1 {
            state.untargetable_expire = native_frame(self.tick).wrapping_add(rules.frames);
            if state.untargetable_expire != 0 {
                if state.lockon_drawable.is_none()
                    && !rules.cursor.is_empty()
                    && let Some(template) = self
                        .catalog
                        .iter()
                        .position(|unit| unit.name == rules.cursor)
                {
                    state.lockon_drawable = Some(Drawable {
                        template,
                        position: [0.0; 3],
                        facing: 0.0,
                        hidden: false,
                    });
                }
                self.position_aircraft_lockon(victim);
            }
        }
    }

    pub(in crate::skirmish) fn remove_attack_targeter(&mut self, victim: usize, attacker: usize) {
        let Some(actor) = self.entities.get_mut(victim) else {
            return;
        };
        let Some(state) = &mut actor.aircraft else {
            return;
        };
        let rules = &self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("JetAIUpdate data")
            .lockon;
        if rules.frames != 0
            && let Some(index) = state.targeters.iter().position(|&id| id == attacker)
        {
            state.targeters.remove(index);
            if state.targeters.is_empty() {
                state.untargetable_expire = 0;
            }
        }
    }

    pub(in crate::skirmish) fn prevents_attack_aim(&self, victim: usize, _attacker: usize) -> bool {
        self.entities
            .get(victim)
            .and_then(|actor| actor.aircraft.as_ref())
            .is_some_and(|state| {
                state.untargetable_expire != 0
                    && native_frame(self.tick) < state.untargetable_expire
            })
    }

    pub(in crate::skirmish) fn prune_attack_targeters(&mut self, id: usize) {
        let mut targeters = std::mem::take(
            &mut self.entities[id]
                .aircraft
                .as_mut()
                .expect("JetAIUpdate")
                .targeters,
        );
        targeters.retain(|&target| self.entities.get(target).is_some_and(Entity::exists));
        self.entities[id]
            .aircraft
            .as_mut()
            .expect("JetAIUpdate")
            .targeters = targeters;
    }

    pub(in crate::skirmish) fn update_aircraft_lockon(&mut self, id: usize) {
        let state = self.entities[id].aircraft.as_mut().expect("JetAIUpdate");
        if state.untargetable_expire != 0 && native_frame(self.tick) >= state.untargetable_expire {
            state.untargetable_expire = 0;
        }
        self.position_aircraft_lockon(id);
    }

    fn position_aircraft_lockon(&mut self, id: usize) {
        let position = self.object_position(id);
        let final_distance = self.object_radius(id);
        let actor = &mut self.entities[id];
        let state = actor.aircraft.as_mut().expect("JetAIUpdate");
        if state.lockon_drawable.is_none() {
            return;
        }
        if state.untargetable_expire == 0 {
            state.lockon_drawable = None;
            return;
        }
        let rules = &self.catalog[actor.type_index]
            .aircraft
            .as_ref()
            .expect("JetAIUpdate data")
            .lockon;
        let remaining = state
            .untargetable_expire
            .wrapping_sub(native_frame(self.tick));
        let elapsed = rules.frames.wrapping_sub(remaining);
        let duration = rules.frames.to_f32().expect("UnsignedInt to Real");
        let fraction = remaining.to_f32().expect("UnsignedInt to Real") / duration;
        let radial_change = (rules.initial_distance - final_distance) * fraction;
        let distance = final_distance + radial_change;
        let angle = rules.angle_spin * fraction;
        let drawable = state
            .lockon_drawable
            .as_mut()
            .expect("created lockon drawable");
        let offset = [angle.cos() * distance, angle.sin() * distance];
        drawable.position = [
            position[0] + offset[0],
            position[1] + offset[1],
            position[2],
        ];
        let dx = position[0] - drawable.position[0];
        let dy = position[1] - drawable.position[1];
        if dx != 0.0 || dy != 0.0 {
            drawable.facing = dy.atan2(dx);
        }
        let previous = 0.5
            * elapsed
                .wrapping_sub(1)
                .to_f32()
                .expect("UnsignedInt to Real")
            * elapsed.to_f32().expect("UnsignedInt to Real");
        let current = previous + elapsed.to_f32().expect("UnsignedInt to Real");
        let factor = rules.frequency / duration;
        let last_phase = (factor * previous).to_i32().unwrap_or(i32::MIN) & 1 != 0;
        let this_phase = (factor * current).to_i32().unwrap_or(i32::MIN) & 1 != 0;
        let tick = last_phase && !this_phase;
        if rules.blinky {
            drawable.hidden = !tick;
        }
        if tick {
            let sound = rules.tick_sound.clone();
            self.object_sound(id, sound, crate::skirmish::object_sounds::Action::Play);
        }
    }
}
