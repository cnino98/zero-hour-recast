//! Original dozer mine-clear weapon condition and attack continuation state.

use super::{DamageType, Order, Skirmish, UnitType};

mod location;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    Player,
    Script,
    Ai,
    Dozer,
}

impl Source {
    pub(super) const fn mask(self) -> u8 {
        match self {
            Self::Player => 1,
            Self::Script => 2,
            Self::Ai => 4,
            Self::Dozer => 8,
        }
    }
}

pub(super) struct Attack {
    pub source: Source,
    original_position: Option<[f32; 2]>,
    pub ignoring_stealth: bool,
    pub forced: bool,
}

impl Attack {
    pub(super) const fn new() -> Self {
        Self {
            source: Source::Player,
            original_position: None,
            ignoring_stealth: false,
            forced: false,
        }
    }
}

impl UnitType {
    pub(super) fn clears_mines(&self) -> bool {
        self.weapon_set_rules
            .replacements
            .iter()
            .any(|set| set.flags & 16 != 0)
    }
}

impl Skirmish {
    pub(super) fn set_mine_clearing(&mut self, id: usize, enabled: bool) {
        let actor = &mut self.entities[id];
        if !self.catalog[actor.type_index].clears_mines()
            || actor.weapon_set.mine_clearing == enabled
        {
            return;
        }
        actor.weapon_set.mine_clearing = enabled;
        self.replace_weapon_set(id);
    }

    pub(super) fn update_builder_weapon(&mut self, id: usize) {
        // DozerAIUpdate checks its task after AIUpdate. WorkerAIUpdate also sets
        // this while its supply submachine runs. A task's approach retains it;
        // only DozerActionDoActionState clears it while actually doing work.
        if !matches!(
            self.entities[id].ai.order,
            Order::Build(_) | Order::Repair(_)
        ) {
            self.set_mine_clearing(id, true);
        }
    }

    pub(super) fn begin_continuing_attack(&mut self, id: usize, target: usize) {
        let actor = &self.entities[id];
        if actor.ai.attack_context.original_position.is_some()
            || actor
                .active_weapon(&self.catalog[actor.type_index])
                .continue_attack_range
                <= 0.0
        {
            return;
        }
        let position = self.entities[target].position;
        let state = &mut self.entities[id].ai.attack_context;
        state.original_position = Some(position);
        state.ignoring_stealth = true;
    }

    pub(super) fn continue_disarm_attack(&mut self, id: usize, victim: usize, original: [f32; 3]) {
        let actor = &self.entities[id];
        let range = actor
            .active_weapon(&self.catalog[actor.type_index])
            .continue_attack_range;
        self.entities[id].ai.attack_context.ignoring_stealth = false;
        if range <= 0.0 {
            return;
        }
        let position = [original[0], original[1]];
        let target = &self.entities[victim];
        if !(self.update_objects[target.update_owner].destroy_requested
            || target.is_effectively_dead()
            || self.attack_kind(victim, "MINE") && target.masked())
        {
            return;
        }
        let owner = target.owner;
        let source = self.entities[id].ai.attack_context.source;
        if let Some(next) = self.closest_disarm_target(id, position, range, Some(owner), source) {
            // notifyNewVictimChosen changes both attack goals, preserving the
            // original victim position and command source for the whole chain.
            if !self.retarget_hunt_goal(id, Some(next)) && !self.retarget_attack_move_goal(id, next)
            {
                self.entities[id].ai.order = Order::Attack(next);
                self.entities[id].ai.ai_target = None;
                self.entities[id].ai.command.attack_approach = None;
                self.set_ai_goal_object(id, next);
            }
            self.retarget_native_attack(id, crate::skirmish::combat::attack::Goal::Object(next));
        }
    }

    pub(super) fn bored_disarm(&mut self, id: usize, range: f32) {
        self.set_mine_clearing(id, true);
        let position = self.entities[id].position;
        if let Some(target) = self.closest_disarm_target(id, position, range, None, Source::Dozer) {
            self.set_last_command_source(id, crate::skirmish::ai_goals::COMMAND_FROM_DOZER);
            self.set_order(id, Order::Attack(target));
            if self.choose_weapon(id, target) {
                self.begin_continuing_attack(id, target);
                let slot = self.entities[id].weapon_slot();
                // privateAttackObject sets this after AIAttackState::onEnter.
                self.entities[id].weapon_set.weapons[slot].max_shots = Some(1);
            }
        }
    }

    fn closest_disarm_target(
        &self,
        id: usize,
        position: [f32; 2],
        range: f32,
        owner: Option<usize>,
        source: Source,
    ) -> Option<usize> {
        let mut best = None;
        let mut distance = range * range;
        let mut first_ring = None;
        for (ring, target) in self.partition.outward_rings(position) {
            if first_ring.is_some_and(|first| ring > first) {
                break;
            }
            let victim = &self.entities[target];
            let kind = &self.catalog[victim.type_index].kind_of;
            if target == id
                || !victim.alive()
                || self.update_objects[victim.update_owner].destroy_requested
                || victim.masked()
                || self.allied(self.entities[id].owner, victim.owner)
                || (source == Source::Player
                    && !self.enemies(self.entities[id].owner, victim.owner)
                    && !kind.iter().any(|kind| kind == "MINE"))
                || kind.iter().any(|kind| kind == "UNATTACKABLE")
                || owner.is_some_and(|owner| victim.owner != owner)
                || !self.weapon_can_target(id, target)
                || !(0..3).any(|slot| {
                    self.allocated_weapon(id, slot).is_some_and(|weapon| {
                        matches!(weapon.damage_type, DamageType::Disarm)
                            && weapon.auto_choose & source.mask() != 0
                    })
                })
            {
                continue;
            }
            let squared = super::movement::distance_squared(position, victim.position);
            if squared < distance {
                best = Some(target);
                distance = squared;
                first_ring = Some(ring);
            }
        }
        best
    }
}

#[cfg(test)]
mod tests;
