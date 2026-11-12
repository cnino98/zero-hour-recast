//! `AIAttackAreaState` and its retained attack-then-idle submachine.
use crate::skirmish::{AiArea, Order, Skirmish, UnitAiUpdate, movement::distance_squared, startup};

pub(in crate::skirmish) struct State {
    area: AiArea,
    next_scan: u64,
    target: Option<usize>,
}

impl Skirmish {
    pub(in crate::skirmish) fn attack_area(
        &mut self,
        id: usize,
        points: &[[f32; 2]],
    ) -> Result<(), &'static str> {
        let actor = self.active_actor(id)?;
        if matches!(
            self.catalog[actor.type_index].ai.update,
            UnitAiUpdate::Absent
        ) {
            return Ok(());
        }
        if points.len() < 3 || points.iter().flatten().any(|value| !value.is_finite()) {
            return Err("Attack area polygon is malformed");
        }
        self.native_attack_area_from(id, points.to_vec(), 1);
        Ok(())
    }

    pub(in crate::skirmish) fn native_attack_area_from(
        &mut self,
        id: usize,
        points: Vec<[f32; 2]>,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::AttackArea(
                    points.clone(),
                ),
            },
        ) || !self.native_ai_command_allowed(id)
            || self.catalog[self.entities[id].type_index].speed <= 0.0
            || self.attack_kind(id, "PROJECTILE")
        {
            return;
        }
        self.clear_ai_goal(id);
        self.set_last_command_source(id, source);
        self.set_order(id, Order::AttackArea);
        let next_scan = self.tick + u64::from(startup::random_between(&mut self.random, 30));
        self.entities[id].ai.command.attack_area = Some(State {
            area: AiArea {
                name: String::new(),
                points,
            },
            next_scan,
            target: None,
        });
        self.entities[id].ai.attack_context.source = match source {
            crate::skirmish::ai_goals::COMMAND_FROM_PLAYER => {
                crate::skirmish::builder_disarm::Source::Player
            }
            1 => crate::skirmish::builder_disarm::Source::Script,
            _ => crate::skirmish::builder_disarm::Source::Ai,
        };
    }

    pub(in crate::skirmish) fn update_attack_area(&mut self, id: usize) {
        let Some(mut state) = self.entities[id].ai.command.attack_area.take() else {
            return;
        };
        if self.tick >= state.next_scan {
            let projectile = self.catalog[self.entities[id].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "PROJECTILE");
            if self.native_actor_out_of_ammo(id) && !projectile {
                self.set_order(id, Order::Idle);
                return;
            }
            state.next_scan = self.tick + 30;
            let center = self.entities[id].position;
            let candidates = self
                .entities
                .iter()
                .enumerate()
                .filter(|(target, actor)| {
                    self.enemy(id, *target)
                        && distance_squared(center, actor.position) <= 9999.9 * 9999.9
                        && crate::skirmish::opponent::map::inside(&state.area, actor.position)
                })
                .map(|(target, _)| target);
            state.target = self.prioritized_target(id, candidates);
            if state.target.is_none() {
                self.set_order(id, Order::Idle);
                return;
            }
        }
        if let Some(target) = state.target
            && !self.update_attack(id, target)
        {
            state.target = None;
        }
        self.entities[id].ai.command.attack_area = Some(state);
    }
}
