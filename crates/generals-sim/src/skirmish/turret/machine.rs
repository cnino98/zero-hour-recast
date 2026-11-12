//! Independent `TurretAI` state clocks, target ownership, and FIRE callbacks.
use super::{Phase, Rules, State};
use crate::skirmish::{
    Skirmish,
    combat::attack::{Goal, Return},
    startup,
};

impl Skirmish {
    pub(in crate::skirmish) fn native_turret_is_natural(&self, id: usize, which: usize) -> bool {
        self.entities[id].under_construction()
            || self
                .turret_state(id, which)
                .natural(self.turret_rules(id, which))
    }

    pub(in crate::skirmish) fn center_native_turret(&mut self, id: usize, which: usize) {
        self.enter_native_turret_phase(id, which, Phase::Recenter);
    }

    pub(in crate::skirmish) fn set_native_turret_enabled(
        &mut self,
        id: usize,
        which: usize,
        enabled: bool,
    ) {
        let now = self.update_frame();
        let turret = self.turret_state_mut(id, which);
        if enabled && !turret.enabled {
            turret.sleep_until = now;
        }
        turret.enabled = enabled;
    }

    pub(in crate::skirmish) fn transfer_turret_targets(
        &mut self,
        id: usize,
        from: usize,
        to: usize,
    ) {
        for which in 0..2 {
            if self.has_native_turret(id, which)
                && self.entities[from].exists()
                && matches!(self.turret_state(id, which).target, Some(Goal::Object(target)) if target == from)
            {
                // The native query explicitly keeps effectively dead targets.
                self.set_native_turret_target(id, which, Some(Goal::Object(to)), true, false);
            }
        }
    }

    pub(in crate::skirmish) fn has_native_turret(&self, id: usize, which: usize) -> bool {
        let unit = &self.catalog[self.entities[id].type_index];
        if which == 0 {
            unit.turret.is_some()
        } else {
            unit.secondary_turret.is_some()
        }
    }

    pub(in crate::skirmish) fn turret_rules(&self, id: usize, which: usize) -> &Rules {
        let unit = &self.catalog[self.entities[id].type_index];
        if which == 0 {
            unit.turret.as_ref()
        } else {
            unit.secondary_turret.as_ref()
        }
        .expect("allocated turret")
    }

    pub(super) fn turret_state(&self, id: usize, which: usize) -> &State {
        if which == 0 {
            &self.entities[id].ai.turret
        } else {
            self.entities[id]
                .ai
                .secondary_turret
                .as_ref()
                .expect("allocated turret")
        }
    }

    fn turret_state_mut(&mut self, id: usize, which: usize) -> &mut State {
        if which == 0 {
            &mut self.entities[id].ai.turret
        } else {
            self.entities[id]
                .ai
                .secondary_turret
                .as_mut()
                .expect("allocated turret")
        }
    }

    pub(in crate::skirmish) fn current_weapon_turret(&self, id: usize) -> Option<usize> {
        let slot = self.entities[id].weapon_slot();
        (0..2).find(|&which| {
            self.has_native_turret(id, which)
                && self.turret_rules(id, which).controlled_slots & (1 << slot) != 0
        })
    }

    pub(in crate::skirmish) fn weapon_turret_aims_at(
        &self,
        id: usize,
        slot: usize,
        target: usize,
    ) -> bool {
        (0..2).find(|&which| self.has_native_turret(id, which) && self.turret_rules(id, which).controlled_slots & (1 << slot) != 0)
            .is_some_and(|which| matches!(self.turret_state(id, which).phase, Phase::Aim)
                && matches!(self.turret_state(id, which).target, Some(Goal::Object(victim)) if victim == target && self.entities[victim].exists() && !self.entities[victim].is_effectively_dead()))
    }

    pub(in crate::skirmish) fn set_attack_turret_targets(
        &mut self,
        id: usize,
        goal: Goal,
        forced: bool,
    ) {
        if self.catalog[self.entities[id].type_index].turrets_linked {
            for which in 0..2 {
                if self.has_native_turret(id, which) {
                    self.set_native_turret_target(id, which, Some(goal), forced, false);
                }
            }
        } else if let Some(which) = self.current_weapon_turret(id) {
            self.set_native_turret_target(id, which, Some(goal), forced, false);
        }
    }

    pub(in crate::skirmish) fn set_native_turret_target(
        &mut self,
        id: usize,
        which: usize,
        mut goal: Option<Goal>,
        forced: bool,
        idle_mood: bool,
    ) {
        let linked = self.catalog[self.entities[id].type_index].turrets_linked;
        let slot_owned = self.turret_rules(id, which).controlled_slots
            & (1 << self.entities[id].weapon_slot())
            != 0;
        if !linked && (!slot_owned || goal.is_some_and(|goal| matches!(goal, Goal::Object(target) if !self.entities.get(target).is_some_and(|victim| victim.exists() && !victim.is_effectively_dead())))) {
            goal = None;
        }
        if (goal.is_none() || matches!(goal, Some(Goal::Position(_))))
            && let Some(Goal::Object(previous)) = self.turret_state(id, which).target
        {
            self.remove_attack_targeter(previous, id);
        }
        let team = match goal {
            Some(Goal::Object(target)) => self.team_membership.team_of(target),
            _ => None,
        };
        let opportunity = if idle_mood {
            match goal {
                Some(Goal::Object(target)) => Some((target, self.entities[target].owner)),
                _ => None,
            }
        } else {
            None
        };
        let turret = self.turret_state_mut(id, which);
        turret.target = goal;
        turret.target_team = team;
        turret.opportunity = opportunity;
        turret.forced = forced;
        let aim_or_fire = matches!(turret.phase, Phase::Aim | Phase::Fire);
        if goal.is_some() && !aim_or_fire {
            self.enter_native_turret_phase(id, which, Phase::Aim);
        } else if goal.is_none() && aim_or_fire {
            self.enter_native_turret_hold(id, which);
        }
    }

    fn enter_native_turret_phase(&mut self, id: usize, which: usize, phase: Phase) {
        if matches!(self.turret_state(id, which).phase, Phase::Fire) {
            self.exit_attack_fire(id);
        }
        let turret = self.turret_state_mut(id, which);
        turret.phase = phase;
        turret.sleep_until = 0;
        self.ai_state_changed(id);
    }

    fn enter_native_turret_hold(&mut self, id: usize, which: usize) {
        let deadline = self
            .update_frame()
            .wrapping_add(self.turret_rules(id, which).recenter_frames);
        self.enter_native_turret_phase(id, which, Phase::Hold(u64::from(deadline)));
        self.sleep_native_turret(id, which, deadline);
    }

    fn sleep_native_turret(&mut self, id: usize, which: usize, deadline: u32) {
        let now = self.update_frame();
        let mood = crate::skirmish::ai_dispatch::native_frame(self.entities[id].ai.mood.next_check);
        let wake = deadline.min(mood);
        self.turret_state_mut(id, which).sleep_until = if wake > now { wake } else { 0 };
    }

    pub(super) fn enter_native_turret_idle(&mut self, id: usize, which: usize, constructor: bool) {
        if !constructor {
            self.entities[id].ai.mood.next_check =
                self.tick + u64::from(self.ai_rules.force_idle_frames);
            self.entities[id].ai.mood.offset_check = true;
        }
        let [low, high] = self.turret_rules(id, which).idle_intervals;
        let delay =
            if i32::from_ne_bytes(low.to_ne_bytes()) >= i32::from_ne_bytes(high.to_ne_bytes()) {
                high
            } else {
                low.wrapping_add(self.random_between(high.wrapping_sub(low)))
            };
        let deadline = self.update_frame().wrapping_add(delay);
        self.enter_native_turret_phase(id, which, Phase::Idle(Some(u64::from(deadline))));
        if !constructor {
            self.sleep_native_turret(id, which, deadline);
        }
    }

    pub(in crate::skirmish) fn native_turret_sleep_frames(&self, id: usize) -> u32 {
        let now = self.update_frame();
        (0..2)
            .filter(|&which| self.has_native_turret(id, which))
            .map(|which| {
                let turret = self.turret_state(id, which);
                if !turret.enabled && !matches!(turret.phase, Phase::Recenter) {
                    crate::skirmish::module_updates::FOREVER
                } else if turret.sleep_until != 0 && turret.sleep_until > now {
                    turret.sleep_until - now
                } else {
                    1
                }
            })
            .min()
            .unwrap_or(crate::skirmish::module_updates::FOREVER)
    }

    pub(super) fn update_native_turret(&mut self, id: usize, which: usize) {
        let now = self.update_frame();
        let turret = self.turret_state(id, which);
        if self.entities[id].unmanned()
            || turret.updated_at == Some(self.tick)
            || turret.sleep_until != 0 && now < turret.sleep_until
            || !turret.enabled && !matches!(turret.phase, Phase::Recenter)
        {
            return;
        }
        let tick = self.tick;
        self.turret_state_mut(id, which).updated_at = Some(tick);
        match self.turret_state(id, which).phase {
            Phase::Aim => self.update_native_turret_aim(id, which),
            Phase::Fire => self.update_native_turret_fire(id, which),
            Phase::Idle(None) => self.enter_native_turret_idle(id, which, false),
            Phase::Idle(Some(deadline)) => {
                let deadline = crate::skirmish::ai_dispatch::native_frame(deadline);
                if now >= deadline {
                    let [low, high] = self.turret_rules(id, which).idle_angles;
                    if low == 0.0 && high == 0.0 {
                        self.enter_native_turret_hold(id, which);
                    } else {
                        let mut angle =
                            low + startup::random_real(&mut self.random, 0.0, high - low);
                        if startup::random_value(&mut self.random).is_multiple_of(2) {
                            angle = -angle;
                        }
                        self.enter_native_turret_phase(id, which, Phase::Scan(angle));
                    }
                } else if !self.acquire_native_turret_target(id, which) {
                    self.sleep_native_turret(id, which, deadline);
                }
            }
            Phase::Hold(deadline) => {
                let deadline = crate::skirmish::ai_dispatch::native_frame(deadline);
                if now >= deadline {
                    self.enter_native_turret_phase(id, which, Phase::Recenter);
                } else if !self.acquire_native_turret_target(id, which) {
                    self.sleep_native_turret(id, which, deadline);
                }
            }
            Phase::Scan(angle) => {
                if self.entities[id].complete() && self.recenter_native_turret(id, which, angle) {
                    self.enter_native_turret_hold(id, which);
                }
            }
            Phase::Recenter => {
                if self.entities[id].complete() && self.recenter_native_turret(id, which, 0.0) {
                    self.enter_native_turret_idle(id, which, false);
                }
            }
        }
    }

    fn recenter_native_turret(&mut self, id: usize, which: usize, angle: f32) -> bool {
        let unit = &self.catalog[self.entities[id].type_index];
        let rules = if which == 0 {
            unit.turret.as_ref()
        } else {
            unit.secondary_turret.as_ref()
        }
        .expect("allocated turret");
        let ai = &mut self.entities[id].ai;
        let turret = if which == 0 {
            &mut ai.turret
        } else {
            ai.secondary_turret.as_mut().expect("allocated turret")
        };
        let old_angle = turret.angle;
        let yaw = turret.turn(rules.natural_angle + angle, rules.turn_rate * 0.5, 0.0);
        if turret.angle.partial_cmp(&old_angle) != Some(std::cmp::Ordering::Equal) {
            self.containment_transform_changed(id);
        }
        let unit = &self.catalog[self.entities[id].type_index];
        let rules = if which == 0 {
            unit.turret.as_ref()
        } else {
            unit.secondary_turret.as_ref()
        }
        .expect("allocated turret");
        let ai = &mut self.entities[id].ai;
        let turret = if which == 0 {
            &mut ai.turret
        } else {
            ai.secondary_turret.as_mut().expect("allocated turret")
        };
        let pitch = turret.pitch(rules.natural_pitch, rules, 0.5);
        yaw && pitch
    }

    fn update_native_turret_aim(&mut self, id: usize, which: usize) {
        let Some(goal) = self.turret_state(id, which).target else {
            self.enter_native_turret_hold(id, which);
            return;
        };
        let mut position = self.attack_goal_position(goal);
        let mut nothing_in_range = false;
        if let Goal::Object(target) = goal {
            let valid =
                self.entities[target].exists() && !self.entities[target].is_effectively_dead();
            let forced = self.turret_state(id, which).forced;
            let team = self.turret_state(id, which).target_team;
            let opportunity = self.turret_state(id, which).opportunity.is_some();
            nothing_in_range = !self.native_turret_weapon_in_range(id, which, target);
            let primary = self.entities[id].ai.command.goal_object == Some(target);
            if !valid
                || !self.attack_continued_legal(id, goal, forced)
                || !primary && nothing_in_range
                || self.team_membership.team_of(target) != team
            {
                if opportunity {
                    self.set_native_turret_target(id, which, None, false, false);
                } else {
                    self.enter_native_turret_hold(id, which);
                }
                return;
            }
            if self.attack_kind(target, "BRIDGE")
                && let Some(points) = self.bridge_attack_points(target)
            {
                let source = self.object_position(id);
                let distance = |point: [f32; 3]| {
                    let d = std::array::from_fn::<_, 3, _>(|axis| point[axis] - source[axis]);
                    {
                        let squared = d.map(|component| component * component);
                        squared[0] + squared[1] + squared[2]
                    }
                };
                position = if distance(points[0]) > distance(points[1]) {
                    points[1]
                } else {
                    points[0]
                };
            }
            self.add_attack_targeter(target, id);
        }
        if self
            .allocated_weapon(id, self.entities[id].weapon_slot())
            .is_none()
        {
            self.enter_native_turret_hold(id, which);
            return;
        }
        if self.rotate_native_turret(id, which, position)
            && self.attack_goal_in_range(id, goal)
            && !nothing_in_range
            && !self.attack_goal_prevents_aim(id, goal)
        {
            self.enter_native_turret_phase(id, which, Phase::Fire);
            if matches!(self.enter_attack_fire(id, goal), Return::Failure) {
                self.enter_native_turret_phase(id, which, Phase::Aim);
            }
        }
    }

    fn update_native_turret_fire(&mut self, id: usize, which: usize) {
        let Some(goal) = self.turret_state(id, which).target else {
            self.enter_native_turret_phase(id, which, Phase::Aim);
            return;
        };
        let revision = self.entities[id].ai.command_revision;
        let result = self.update_attack_fire(id, goal, Some(which), None);
        if self.entities[id].ai.command_revision != revision || !self.entities[id].exists() {
            return;
        }
        match result {
            Return::Continue => {
                if matches!(goal, Goal::Object(_)) && self.attack_out_of_range(id, goal) {
                    self.enter_native_turret_phase(id, which, Phase::Aim);
                }
            }
            Return::Success | Return::Failure => {
                if matches!(result, Return::Success) {
                    if matches!(goal, Goal::Object(_)) {
                        let victim = self.entities[id].ai.current_victim;
                        let turret = self.turret_state_mut(id, which);
                        turret.target = victim.map(Goal::Object);
                        if victim.is_none() {
                            turret.opportunity = None;
                        }
                    }
                    let tick = self.tick;
                    self.turret_state_mut(id, which).fired(tick);
                }
                self.enter_native_turret_phase(id, which, Phase::Aim);
            }
        }
    }

    fn native_turret_weapon_in_range(&self, id: usize, which: usize, target: usize) -> bool {
        let slots = self.turret_rules(id, which).controlled_slots;
        (0..3).any(|slot| {
            slots & (1 << slot) != 0
                && self.allocated_weapon(id, slot).is_some_and(|weapon| {
                    self.attack_range_for(id, target, self.entities[id].position, weapon)
                })
        })
    }

    fn acquire_native_turret_target(&mut self, id: usize, which: usize) -> bool {
        if !self.attack_mood_allows(id) {
            return false;
        }
        let Some(target) = self.mood_target(id, true, false) else {
            return false;
        };
        self.set_native_turret_target(id, which, Some(Goal::Object(target)), false, true);
        self.choose_weapon_from(id, target, 4);
        self.turret_state(id, which).target.is_some()
    }
}
