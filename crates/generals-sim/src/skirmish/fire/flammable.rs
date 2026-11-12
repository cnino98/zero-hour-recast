//! SH `FlammableUpdate`: independent module timers and real damage event wakes.
use crate::skirmish::{DeathType, Entity, Skirmish, module_updates::FOREVER};

pub struct Rules {
    pub burned_delay: u32,
    pub duration: u32,
    pub damage_delay: u32,
    pub damage: f32,
    pub limit: f32,
    pub expiration: u32,
    pub sound: String,
}

pub struct SpreadRules {
    pub delay: [u32; 2],
    pub range: f32,
    pub embers: Option<crate::skirmish::physical_ocl::List>,
}

pub enum SoundEvent {
    Start {
        object: usize,
        ordinal: usize,
        sound: String,
        position: [f32; 3],
    },
    Stop {
        object: usize,
        ordinal: usize,
    },
}

/// Timers belong to each module; Object status and Drawable output are shared.
pub(in crate::skirmish) struct ObjectState {
    pub(in crate::skirmish) instances: Vec<State>,
    pub(in crate::skirmish) aflame: bool,
    pub(in crate::skirmish) burned: bool,
    pub(in crate::skirmish) smoldering: bool,
}

impl ObjectState {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            instances: Vec::new(),
            aflame: false,
            burned: false,
            smoldering: false,
        }
    }
}

enum Phase {
    Normal,
    Aflame { end: u32, burned: u32, damage: u32 },
    Burned,
}

pub struct State {
    phase: Phase,
    limit: f32,
    source: Option<usize>,
    last_damage: u32,
}

impl State {
    pub(in crate::skirmish) const fn new(rules: &Rules) -> Self {
        Self {
            phase: Phase::Normal,
            limit: rules.limit,
            source: None,
            last_damage: 0,
        }
    }

    fn expose(
        &mut self,
        rules: &Rules,
        source: Option<usize>,
        amount: f32,
        now: u32,
        can_ignite: bool,
    ) -> bool {
        if now.wrapping_sub(rules.expiration) > self.last_damage {
            self.limit = rules.limit;
        }
        self.last_damage = now;
        self.source = source;
        if can_ignite {
            self.limit -= amount;
            self.limit <= 0.0
        } else {
            false
        }
    }

    fn sleep(&self, now: u32) -> u32 {
        let Phase::Aflame {
            end,
            burned,
            damage,
        } = self.phase
        else {
            return FOREVER;
        };
        if end == 0 || end <= now {
            return FOREVER;
        }
        [end, burned, damage]
            .into_iter()
            .filter(|&frame| frame > now && frame != 0)
            .min()
            .expect("aflame end")
            - now
    }
}

impl Entity {
    #[must_use]
    pub const fn aflame(&self) -> bool {
        self.flammable.aflame
    }

    pub(in crate::skirmish) fn flammable_would_ignite(&self) -> bool {
        self.flammable
            .instances
            .first()
            .is_some_and(|state| matches!(state.phase, Phase::Normal))
    }

    #[must_use]
    pub const fn smoldering(&self) -> bool {
        self.flammable.smoldering
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_flammable_module(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].flammable.instances.len(), ordinal);
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .flammable[ordinal];
        self.entities[id]
            .flammable
            .instances
            .push(State::new(rules));
    }

    pub(in crate::skirmish) fn flame_exposed_module(
        &mut self,
        target: usize,
        ordinal: usize,
        source: Option<usize>,
        amount: f32,
    ) {
        let now = self.update_frame();
        let can_ignite = !self.entities[target].aflame() && !self.entities[target].flammable.burned;
        let rules = &self.catalog[self.entities[target].type_index]
            .behavior
            .flammable[ordinal];
        if self.entities[target].flammable.instances[ordinal]
            .expose(rules, source, amount, now, can_ignite)
        {
            self.ignite_flammable(target, ordinal);
        }
    }

    pub(in crate::skirmish) fn ignite_flammable(&mut self, id: usize, ordinal: usize) {
        if !matches!(
            self.entities[id].flammable.instances[ordinal].phase,
            Phase::Normal
        ) {
            return;
        }
        let now = self.update_frame();
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .flammable[ordinal];
        let duration = rules.duration;
        let burned = rules.burned_delay;
        let damage = rules.damage_delay;
        let sound = rules.sound.clone();
        self.entities[id].flammable.instances[ordinal].phase = Phase::Aflame {
            end: now.wrapping_add(duration),
            burned: if burned == 0 {
                0
            } else {
                now.wrapping_add(burned)
            },
            damage: if damage == 0 {
                0
            } else {
                now.wrapping_add(damage)
            },
        };
        self.entities[id].flammable.aflame = true;
        self.update_body_particles(id);
        self.presentation
            .burning_sound_events
            .push(SoundEvent::Start {
                object: id,
                ordinal,
                sound,
                position: self.object_position(id),
            });
        // Source findUpdateModule picks the first actual FireSpreadUpdate.
        if !self.catalog[self.entities[id].type_index]
            .behavior
            .fire_spread
            .is_empty()
        {
            let delay = self.next_fire_spread_delay(id, 0);
            self.wake_family_update(id, "FireSpreadUpdate", 0, delay);
        }
        let sleep = self.entities[id].flammable.instances[ordinal].sleep(now);
        self.wake_family_update(id, "FlammableUpdate", ordinal, sleep);
    }

    pub(in crate::skirmish) fn update_flammable_module(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        let now = self.update_frame();
        let type_index = self.entities[id].type_index;
        let state = &mut self.entities[id].flammable.instances[ordinal];
        let Phase::Aflame { damage, .. } = &mut state.phase else {
            unreachable!("native FlammableUpdate callback requires FS_AFLAME");
        };
        let rules = &self.catalog[type_index].behavior.flammable[ordinal];
        let hit = if *damage != 0 && now >= *damage {
            *damage = now.wrapping_add(rules.damage_delay);
            Some((state.source, rules.damage))
        } else {
            None
        };
        if let Some((source, damage)) = hit {
            // attemptDamage supplies body guards, including effective death.
            self.apply_flammable_damage(id, source, damage);
        }
        let state = &self.entities[id].flammable.instances[ordinal];
        let Phase::Aflame { end, burned, .. } = state.phase else {
            unreachable!("aflame damage does not replace module status");
        };
        if burned != 0 && now >= burned {
            self.entities[id].flammable.burned = true;
            self.entities[id].flammable.smoldering = true;
        }
        if end != 0 && now >= end {
            self.entities[id].flammable.instances[ordinal].phase =
                if self.entities[id].flammable.burned {
                    Phase::Burned
                } else {
                    Phase::Normal
                };
            self.entities[id].flammable.aflame = false;
            self.update_body_particles(id);
            self.presentation
                .burning_sound_events
                .push(SoundEvent::Stop {
                    object: id,
                    ordinal,
                });
        }
        i32::try_from(self.entities[id].flammable.instances[ordinal].sleep(now))
            .expect("native update sleep")
    }

    fn apply_flammable_damage(&mut self, id: usize, source: Option<usize>, damage: f32) {
        if let Some(source) = source {
            self.apply_weapon_hit(
                id,
                source,
                damage,
                crate::skirmish::DamageType::Flame,
                DeathType::Burned,
            );
        } else {
            self.apply_hazard_hit(
                id,
                None,
                damage,
                crate::skirmish::DamageType::Flame,
                DeathType::Burned,
            );
        }
    }

    fn next_fire_spread_delay(&mut self, id: usize, ordinal: usize) -> u32 {
        let [min, max] = self.catalog[self.entities[id].type_index]
            .behavior
            .fire_spread[ordinal]
            .delay;
        let [low, high] = [min, max].map(|frame| i32::from_ne_bytes(frame.to_ne_bytes()));
        (if low >= high {
            max
        } else {
            min.wrapping_add(self.random_between(max.wrapping_sub(min)))
        })
        .max(1)
    }

    pub(in crate::skirmish) fn update_fire_spread_module(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        if !self.entities[id].aflame() {
            return i32::try_from(FOREVER).expect("native update sleep");
        }
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .fire_spread[ordinal];
        let range = rules.range;
        let embers = rules.embers.clone();
        if let Some(embers) = embers {
            self.create_physical_ocl(id, &embers)
                .expect("validated FireSpreadUpdate embers");
        }
        if range != 0.0
            && let Some(target) = self.closest_flammable(id, range)
        {
            self.ignite_flammable(target, 0);
        }
        i32::try_from(self.next_fire_spread_delay(id, ordinal)).expect("native update sleep")
    }

    pub(in crate::skirmish) fn destroy_flammable_module(&mut self, id: usize, ordinal: usize) {
        self.presentation
            .burning_sound_events
            .push(SoundEvent::Stop {
                object: id,
                ordinal,
            });
    }

    pub fn take_burning_sound_events(&mut self) -> Vec<SoundEvent> {
        std::mem::take(&mut self.presentation.burning_sound_events)
    }
}

#[cfg(test)]
mod tests;
