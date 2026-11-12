//! SH `CleanupHazardUpdate` scans independently for native cleanup-hazard Objects.
use super::{Entity, Order, Skirmish, movement::distance_squared, startup, weapons::Lock};

pub struct Rules {
    pub slot: usize,
    pub scan_frames: u32,
    pub scan_range: f32,
}
pub struct AreaRules {
    pub range: f32,
    pub power: String,
}

pub(in crate::skirmish) struct State {
    target: Option<usize>,
    scan_left: u32,
    shot_left: u32,
    in_range: bool,
    weapon_index: Option<usize>,
    area: [f32; 2],
    move_range: f32,
}

impl Entity {
    #[must_use]
    pub fn cleaning_area(&self) -> Option<[f32; 2]> {
        self.behaviors
            .cleanup
            .first()
            .filter(|state| state.move_range > 0.0)
            .map(|state| state.area)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_cleanup_module(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.cleanup.len(), ordinal);
        self.entities[id].behaviors.cleanup.push(State {
            target: None,
            scan_left: 0,
            shot_left: 0,
            in_range: false,
            weapon_index: None,
            area: [0.0; 2],
            move_range: 0.0,
        });
    }

    pub(in crate::skirmish) fn cleanup_on_created(&mut self, id: usize, ordinal: usize) {
        // Source sets VETERAN weapon-set flag; it does not change veterancy rank.
        self.entities[id].weapon_set.forced_veteran = true;
        self.replace_weapon_set(id);
        let unit = &self.catalog[self.entities[id].type_index];
        let rules = &unit.behavior.cleanup[ordinal];
        let index = self.entities[id].weapon_index(rules.slot);
        // CleanupHazardUpdate::onObjectCreated returns on a missing weapon. Its
        // scan-range diagnostic is debug-only and never discards the retail weapon.
        if unit.weapon_template(index).is_some() {
            self.entities[id].behaviors.cleanup[ordinal].weapon_index = Some(index);
        }
    }

    #[must_use]
    pub fn cleanup_ready(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            actor.complete() && !self.object_disabled(id) && !actor.behaviors.cleanup.is_empty()
        })
    }

    /// Executes the first admitted area power and native first matching update module.
    /// # Errors
    /// Rejects disabled actors, missing original power, or a position outside the map.
    pub fn clean_area(&mut self, id: usize, point: [f32; 2]) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.cleanup_ready(id) || self.cell_at(point).is_none() {
            return Err("Choose an active Ambulance and a location on the map");
        }
        let range = self.catalog[self.entities[id].type_index]
            .behavior
            .cleanup_area
            .first()
            .ok_or("Choose an Ambulance cleanup area power")?
            .range;
        self.set_cleanup_area_parameters(id, point, range);
        Ok(())
    }

    pub(in crate::skirmish) fn set_cleanup_area_parameters(
        &mut self,
        id: usize,
        point: [f32; 2],
        range: f32,
    ) {
        self.entities[id].behaviors.cleanup[0].move_range = range;
        self.entities[id].behaviors.cleanup[0].area = point;
        self.cleanup_order(id, Order::Move(point));
    }

    pub(super) fn cleanup_order(&mut self, id: usize, order: Order) {
        self.set_order(id, order);
        self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_AI);
        self.entities[id].ai.attack_context.source = crate::skirmish::builder_disarm::Source::Ai;
    }

    pub(super) fn update_cleanup_attack(&mut self, id: usize, field: u64) {
        let target = self
            .hazards
            .iter()
            .find(|hazard| hazard.id == field)
            .map(|hazard| hazard.object);
        if let Some(target) = target {
            self.attack_from_ai(id, target)
                .expect("native cleanup attack target");
        } else {
            self.cleanup_order(id, Order::Idle);
        }
    }

    pub(in crate::skirmish) fn update_cleanup_module(&mut self, id: usize, ordinal: usize) -> i32 {
        if self.entities[id].behaviors.cleanup[ordinal].move_range > 0.0 {
            if self.entities[id].ai_idle() {
                self.ai_busy_from_ai(id);
            } else if !self.entities[id].last_command_from_ai() {
                self.entities[id].behaviors.cleanup[ordinal].move_range = 0.0;
                return 1;
            }
        }
        if self.entities[id].behaviors.cleanup[ordinal].scan_left > 0 {
            self.entities[id].behaviors.cleanup[ordinal].scan_left -= 1;
            self.cleanup_fire_when_ready(id, ordinal);
            return 1;
        }
        self.entities[id].behaviors.cleanup[ordinal].scan_left =
            self.catalog[self.entities[id].type_index].behavior.cleanup[ordinal].scan_frames;
        if self.scan_cleanup_target(id, ordinal) {
            self.cleanup_fire_when_ready(id, ordinal);
        } else if self.entities[id].behaviors.cleanup[ordinal].move_range != 0.0
            && (self.entities[id].ai_idle() || self.entities[id].ai_busy())
        {
            let point = self.entities[id].behaviors.cleanup[ordinal].area;
            if distance_squared(self.entities[id].position, point).sqrt() < 25.0 {
                self.entities[id].behaviors.cleanup[ordinal].move_range = 0.0;
            } else {
                self.cleanup_order(id, Order::Move(point));
            }
        }
        1
    }

    fn scan_cleanup_target(&mut self, id: usize, ordinal: usize) -> bool {
        let actor = &self.entities[id];
        let state = &actor.behaviors.cleanup[ordinal];
        let rules = &self.catalog[actor.type_index].behavior.cleanup[ordinal];
        let (center, range) = if state.move_range > 0.0 {
            (state.area, rules.scan_range + state.move_range)
        } else {
            (actor.position, rules.scan_range)
        };
        let target = self.closest_cleanup_hazard(id, center, range);
        self.entities[id].behaviors.cleanup[ordinal].target = target;
        target.is_some()
    }

    fn cleanup_fire_when_ready(&mut self, id: usize, ordinal: usize) {
        let state = &self.entities[id].behaviors.cleanup[ordinal];
        let mut target = state
            .target
            .filter(|&target| !self.entities[target].masked());
        if let Some(victim) = target
            && state.move_range == 0.0
        {
            let index = state
                .weapon_index
                .expect("native onObjectCreated weapon template");
            let range = self.catalog[self.entities[id].type_index]
                .weapon_template(index)
                .expect("cleanup weapon template")
                .range;
            let inside =
                distance_squared(self.entities[id].position, self.entities[victim].position).sqrt()
                    < range;
            let previous = state.in_range;
            if inside {
                self.entities[id].behaviors.cleanup[ordinal].in_range = true;
            } else if previous {
                let delay = startup::random_between(&mut self.random, 3);
                self.entities[id].behaviors.cleanup[ordinal].scan_left = delay;
                self.entities[id].behaviors.cleanup[ordinal].target = None;
                if delay == 0 {
                    self.scan_cleanup_target(id, ordinal);
                    self.entities[id].behaviors.cleanup[ordinal].scan_left =
                        self.catalog[self.entities[id].type_index].behavior.cleanup[ordinal]
                            .scan_frames;
                    target = None;
                }
            } else {
                self.entities[id].behaviors.cleanup[ordinal].in_range = false;
            }
        }
        if self.entities[id].behaviors.cleanup[ordinal].shot_left > 0 {
            self.entities[id].behaviors.cleanup[ordinal].shot_left -= 1;
            return;
        }
        if let Some(target) = target
            && (self.entities[id].ai_idle() || self.entities[id].ai_busy())
        {
            let slot = self.catalog[self.entities[id].type_index].behavior.cleanup[ordinal].slot;
            self.entities[id].weapon_set.current = slot;
            self.entities[id].weapon_set.lock = Lock::Temporary;
            self.attack_from_ai(id, target)
                .expect("native cleanup AI attack admission");
        }
    }
}

#[cfg(test)]
mod tests;
