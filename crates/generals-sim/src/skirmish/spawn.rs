//! `SpawnBehavior` and its `SpawnPointProductionExitUpdate`, SH 2bfaea60.
use super::{Entity, Skirmish, UnitType};

mod exits;
mod mob;
pub(super) use exits::ExitState;
pub use mob::MobRules;
pub(super) use mob::MobState;

pub struct Rules {
    pub templates: Vec<String>,
    pub count: i32,
    pub replace_frames: i32,
    pub flags: u8,
    pub initial_burst: i32,
    pub conditions: super::death_callbacks::Conditions,
}

impl Rules {
    pub const ONE_SHOT: u8 = 1;
    pub const REQUIRE_SPAWNER: u8 = 2;
    pub const FREE_WILL: u8 = 4;
    pub const EXIT_BY_BUDDING: u8 = 8;
    pub const AGGREGATE_HEALTH: u8 = 16;
    pub const RECLAIM_ORPHANS: u8 = 32;

    #[must_use]
    pub const fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

const ACTIVE: u8 = 1;
const NEXUS: u8 = 2;
const INITIALIZED: u8 = 8;

pub struct Point {
    pub position: [f32; 3],
    pub basis: [[f32; 3]; 3],
}

pub(super) struct State {
    flags: u8,
    self_taskers: i32,
    frames_to_wait: i32,
    next_template: usize,
    remaining: i32,
    replacements: Vec<u32>,
    spawn_count: i32,
    children: Vec<usize>,
    burst_left: u32,
}

impl State {
    pub(super) const fn new(rules: &Rules) -> Self {
        Self {
            flags: ACTIVE
                | if rules.has(Rules::AGGREGATE_HEALTH) {
                    NEXUS
                } else {
                    0
                },
            self_taskers: 0,
            frames_to_wait: 0,
            next_template: 0,
            remaining: if rules.has(Rules::ONE_SHOT) {
                rules.count
            } else {
                -1
            },
            replacements: Vec::new(),
            spawn_count: -1,
            children: Vec::new(),
            burst_left: u32::from_ne_bytes(rules.initial_burst.to_ne_bytes()),
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn health_box_offset(&self) -> [f32; 3] {
        self.health_box_offset
    }

    #[must_use]
    pub fn spawn_nexus(&self) -> bool {
        self.behaviors
            .spawns
            .first()
            .is_some_and(|state| state.flags & NEXUS != 0)
    }
}

pub(super) fn validate(unit: &UnitType, catalog: &[UnitType]) -> Result<(), &'static str> {
    for mob in &unit.behavior.mobs {
        if [mob.squirrelliness, mob.wander_speed, mob.panic_speed]
            .into_iter()
            .any(|value| !value.is_finite())
        {
            return Err("Invalid original MobMemberSlavedUpdate parameters");
        }
    }
    if !unit.behavior.spawns.is_empty()
        && unit.behavior_modules.iter().any(|module| {
            matches!(
                module.family.as_str(),
                "ParkingPlaceBehavior" | "FlightDeckBehavior"
            )
        })
    {
        return Err("SpawnBehavior requires untranslated aircraft exit interface");
    }
    for rules in &unit.behavior.spawns {
        if rules.templates.is_empty()
            || rules
                .templates
                .iter()
                .any(|name| !catalog.iter().any(|unit| &unit.name == name))
            || (rules.has(Rules::EXIT_BY_BUDDING)
                && unit
                    .behavior
                    .production_exit
                    .as_ref()
                    .is_none_or(|exit| exit.queue.is_none()))
        {
            return Err("Invalid original spawn templates or exit");
        }
    }
    Ok(())
}

impl Skirmish {
    /// Client `Drawable::select/unselect` state, distinct from each player's logic Squad.
    pub fn sync_drawable_selection(&mut self, selected: &[usize]) {
        for (id, actor) in self.entities.iter_mut().enumerate() {
            actor.drawable.selected = u8::from(actor.has_drawable() && selected.contains(&id));
        }
    }

    pub(super) fn uses_spawn_weapons(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            let unit = &self.catalog[actor.type_index];
            !unit.behavior.spawns.is_empty()
                && unit
                    .kind_of
                    .iter()
                    .any(|kind| kind == "SPAWNS_ARE_THE_WEAPONS")
        })
    }

    pub(in crate::skirmish) fn group_spawn_attack(
        &mut self,
        id: usize,
        target: usize,
        source: i32,
    ) -> bool {
        if self.catalog[self.entities[id].type_index]
            .behavior
            .spawns
            .first()
            .is_none_or(|rules| rules.has(Rules::FREE_WILL))
        {
            return false;
        }
        let mut commanded = false;
        for child in self.spawned_children(id).to_vec() {
            if self.entities[child].exists() && self.entities[child].ai.registration.is_some() {
                self.native_attack_goal_from(
                    child,
                    super::combat::attack::Goal::Object(target),
                    i32::MAX.unsigned_abs(),
                    source,
                    true,
                );
                commanded = true;
            }
        }
        commanded
    }

    pub(in crate::skirmish) fn group_spawn_idle(&mut self, id: usize, source: i32) {
        for child in self.spawned_children(id).to_vec() {
            if self.entities[child].exists() && self.entities[child].ai.registration.is_some() {
                self.native_idle_from(child, source);
            }
        }
    }

    #[must_use]
    pub fn spawned_children(&self, id: usize) -> &[usize] {
        self.entities
            .get(id)
            .and_then(|actor| actor.behaviors.spawns.first())
            .map_or(&[], |state| state.children.as_slice())
    }

    pub(in crate::skirmish) fn construct_spawn(&mut self, id: usize, ordinal: usize) {
        let state =
            State::new(&self.catalog[self.entities[id].type_index].behavior.spawns[ordinal]);
        assert_eq!(self.entities[id].behaviors.spawns.len(), ordinal);
        self.entities[id].behaviors.spawns.push(state);
    }

    pub(in crate::skirmish) fn update_spawn(&mut self, id: usize, ordinal: usize) -> i32 {
        let rules = &self.catalog[self.entities[id].type_index].behavior.spawns[ordinal];
        if rules.has(Rules::AGGREGATE_HEALTH) {
            self.aggregate_spawn_health(id, ordinal);
        }
        let now = self.update_frame();
        let neutral = self.neutral_owner();
        let actor = &mut self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.spawns[ordinal];
        let reconstructing = actor.reconstructing();
        let state = &mut actor.behaviors.spawns[ordinal];
        if state.flags & INITIALIZED == 0 {
            state.flags |= INITIALIZED;
            for index in 0..rules.count {
                // The pinned implementation inserts runtimeProduced itself,
                // not its computed birthFrame, for every initial burst slot.
                state.replacements.push(if rules.initial_burst > 0 {
                    u32::from(actor.producer.is_some())
                } else {
                    u32::from_ne_bytes(index.to_ne_bytes())
                });
            }
        }
        state.frames_to_wait = state.frames_to_wait.wrapping_sub(1);
        if state.frames_to_wait > 0 {
            return 1;
        }
        state.frames_to_wait = 15;
        if state.flags & ACTIVE == 0 {
            return 1;
        }
        if rules.has(Rules::ONE_SHOT) && reconstructing {
            state.flags &= !ACTIVE;
            return 1;
        }
        if actor.under_construction() || actor.sold_model() || actor.owner == neutral {
            return 1;
        }
        let mut index = 0;
        while index
            < self.entities[id].behaviors.spawns[ordinal]
                .replacements
                .len()
        {
            if now > self.entities[id].behaviors.spawns[ordinal].replacements[index]
                && self.create_spawn(id, ordinal)
            {
                self.entities[id].behaviors.spawns[ordinal]
                    .replacements
                    .remove(index);
            } else {
                index += 1;
            }
        }
        let rules = &self.catalog[self.entities[id].type_index].behavior.spawns[ordinal];
        let state = &mut self.entities[id].behaviors.spawns[ordinal];
        if rules.has(Rules::ONE_SHOT) && state.remaining <= 0 {
            state.flags &= !ACTIVE;
        }
        1
    }

    fn create_spawn(&mut self, parent: usize, ordinal: usize) -> bool {
        let Some(exit) = self.reserve_spawn_exit(parent) else {
            return false;
        };
        let actor = &self.entities[parent];
        let rules = self.catalog[actor.type_index]
            .behavior
            .spawns
            .get(ordinal)
            .expect("spawn rules");
        let template_count = rules.templates.len();
        let template = &rules.templates[actor.behaviors.spawns[ordinal].next_template];
        let type_index = self
            .catalog
            .iter()
            .position(|unit| &unit.name == template)
            .expect("validated spawn template");
        let reclaim = rules.has(Rules::RECLAIM_ORPHANS) && !rules.has(Rules::ONE_SHOT);
        let reclaimed = if reclaim {
            self.reclaim_orphan_spawn(parent, ordinal)
        } else {
            None
        };
        let spawned = if let Some(reclaimed) = reclaimed {
            reclaimed
        } else {
            let team = self.team_membership.team_of(parent);
            let spawned = self.new_object_on_team(type_index, actor.owner, team);
            self.record_built(spawned);
            self.opponent_unit_produced(parent, spawned);
            let state = &mut self.entities[parent].behaviors.spawns[ordinal];
            state.next_template = (state.next_template + 1) % template_count;
            spawned
        };
        self.entities[spawned].producer = Some(parent);
        self.enslave_spawn(spawned, parent);
        self.entities[parent].behaviors.spawns[ordinal]
            .children
            .push(spawned);
        if reclaimed.is_none() {
            match exit {
                exits::Exit::Points(ordinal) => self.exit_spawn_point(parent, ordinal, spawned),
                exits::Exit::Production => {
                    if self.catalog[self.entities[parent].type_index]
                        .behavior
                        .spawns[ordinal]
                        .has(Rules::EXIT_BY_BUDDING)
                    {
                        self.exit_budding_spawn(parent, ordinal, spawned);
                    } else {
                        self.exit_spawn_from_producer(parent, spawned);
                    }
                }
                exits::Exit::Contain => self.exit_spawned_from_container(spawned, parent),
            }
        }
        let one_shot = self.catalog[self.entities[parent].type_index]
            .behavior
            .spawns[ordinal]
            .has(Rules::ONE_SHOT);
        let state = &mut self.entities[parent].behaviors.spawns[ordinal];
        if one_shot {
            state.remaining = state.remaining.wrapping_sub(1);
        }
        state.spawn_count = if state.spawn_count == -1 {
            1
        } else {
            state.spawn_count.wrapping_add(1)
        };
        true
    }

    fn reclaim_orphan_spawn(&self, parent: usize, ordinal: usize) -> Option<usize> {
        let mut previous = "";
        let mut closest = None;
        for name in &self.catalog[self.entities[parent].type_index]
            .behavior
            .spawns[ordinal]
            .templates
        {
            // Pinned cpp582 compares rather than testing equality: ordinary nonempty
            // names are skipped while previous is still empty.
            if previous != name {
                continue;
            }
            closest = None;
            let mut best = 99_999_999.9;
            for id in self
                .team_membership
                .player_objects(self.entities[parent].owner)
            {
                let actor = &self.entities[id];
                if self.catalog[actor.type_index].name != *name || actor.producer.is_some() {
                    continue;
                }
                let distance = super::movement::distance_squared(
                    self.entities[parent].position,
                    actor.position,
                );
                if distance < best {
                    best = distance;
                    closest = Some(id);
                }
            }
            previous = name;
        }
        closest
    }

    pub(in crate::skirmish) fn spawn_on_damage(&mut self, id: usize, ordinal: usize, actual: f32) {
        for child in self.entities[id].behaviors.spawns[ordinal].children.clone() {
            if self.entities[child].exists() && self.first_slaved_interface(child).is_some() {
                self.slaved_on_master_damage(child, actual);
            }
        }
    }

    pub(in crate::skirmish) fn spawn_on_die(&mut self, id: usize, ordinal: usize) {
        let rules = &self.catalog[self.entities[id].type_index].behavior.spawns[ordinal];
        if !self.die_conditions_match(id, &rules.conditions) {
            return;
        }
        let require = rules.has(Rules::REQUIRE_SPAWNER);
        let children = self.entities[id].behaviors.spawns[ordinal].children.clone();
        for &child in &children {
            if self.entities[child].exists() {
                self.slow_death_release_slave(child);
                self.entities[child].producer = None;
            }
        }
        if require {
            for child in children {
                if self.entities[child].exists() && !self.entities[child].is_effectively_dead() {
                    self.kill_object(child, super::DeathType::Normal);
                }
            }
        }
    }

    pub(in crate::skirmish) fn spawn_on_delete(&mut self, id: usize, ordinal: usize) {
        if self.catalog[self.entities[id].type_index].behavior.spawns[ordinal]
            .has(Rules::REQUIRE_SPAWNER)
        {
            for child in self.entities[id].behaviors.spawns[ordinal].children.clone() {
                if self.entities[child].exists() && !self.entities[child].is_effectively_dead() {
                    self.destroy_maintenance_object(child);
                }
            }
        }
    }

    pub(in crate::skirmish) fn notify_spawn_owner_death(
        &mut self,
        id: usize,
        killer: Option<usize>,
    ) {
        let Some(parent) = self.entities[id]
            .producer
            .filter(|&parent| self.entities[parent].exists())
        else {
            return;
        };
        let Some(state) = self.entities[parent].behaviors.spawns.first_mut() else {
            return;
        };
        let Some(index) = state.children.iter().position(|&child| child == id) else {
            return;
        };
        let rules = &self.catalog[self.entities[parent].type_index]
            .behavior
            .spawns[0];
        let delay = rules.replace_frames;
        let aggregate = rules.has(Rules::AGGREGATE_HEALTH);
        let now = super::ai_dispatch::native_frame(self.tick);
        let state = &mut self.entities[parent].behaviors.spawns[0];
        state
            .replacements
            .push(now.wrapping_add(u32::from_ne_bytes(delay.to_ne_bytes())));
        state.children.remove(index);
        state.spawn_count = state.spawn_count.wrapping_sub(1);
        if state.spawn_count == 0 && aggregate {
            if let Some(killer) = killer.filter(|&killer| self.entities[killer].exists()) {
                self.score_kill(killer, parent);
            }
            self.destroy_maintenance_object(parent);
        }
    }
}

#[cfg(test)]
mod callback_tests;
#[cfg(test)]
mod tests;

impl Skirmish {
    fn exit_budding_spawn(&mut self, parent: usize, ordinal: usize, child: usize) {
        if self.entities[parent].behaviors.spawns[ordinal].burst_left > 0
            && let Some(producer) = self.entities[parent]
                .producer
                .filter(|&id| self.entities[id].exists())
            && self.catalog[self.entities[producer].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "STRUCTURE")
            && let Some(exit) = self.spawn_exit_interface(producer)
        {
            // Native calls exit even if the returned door was unavailable.
            let _ = self.reserve_spawn_exit(producer);
            self.exit_spawn_via_door(producer, child, &exit);
            self.entities[child].producer = Some(parent);
            self.entities[parent].behaviors.spawns[ordinal].burst_left -= 1;
            return;
        }
        let mut closest = 999_999.9;
        let mut host = None;
        for &other in &self.entities[parent].behaviors.spawns[ordinal].children {
            if other == child || !self.entities[other].exists() {
                continue;
            }
            let distance = super::movement::distance_squared(
                self.entities[other].position,
                self.entities[parent].position,
            );
            if distance < closest {
                closest = distance;
                host = Some(other);
            }
        }
        let selected_host = host.unwrap_or(parent);
        let position = self.object_position(selected_host);
        let orientation = self.entities[selected_host].facing;
        self.set_object_position(child, position);
        self.set_object_orientation(child, orientation);
        if let Some(host) = host {
            self.entities[child].layer = self.entities[host].layer;
        }
        if self.entities[child].ai.registration.is_some() {
            let goal = self.object_position(child);
            self.construction_move(child, goal);
        }
        let delay = self.catalog[self.entities[parent].type_index]
            .behavior
            .production_exit
            .as_ref()
            .and_then(|exit| exit.queue.as_ref())
            .expect("budding queue exit")
            .delay_frames;
        self.entities[parent].queue_exit.exited(delay);
    }

    fn exit_spawn_from_producer(&mut self, producer: usize, child: usize) {
        let delay = self.catalog[self.entities[producer].type_index]
            .behavior
            .production_exit
            .as_ref()
            .expect("producer exit")
            .queue
            .as_ref()
            .map(|queue| queue.delay_frames);
        self.exit_delivered_unit(producer, child);
        if let Some(delay) = delay {
            self.entities[producer].queue_exit.exited(delay);
        }
    }
}
