//! SH AIUpdate.cpp3776 and AIStates.cpp6239–6432, native `AI_ENTER`.
use crate::skirmish::{Order, Skirmish, combat::targeting, ground_movement::MoveResult};

impl Skirmish {
    pub(in crate::skirmish) fn native_enter_from(
        &mut self,
        id: usize,
        carrier: usize,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Enter(carrier),
            },
        ) {
            return;
        }
        if self.entities[id].aircraft.is_some() {
            self.native_jet_landing_from(id, carrier, source, false);
            return;
        }
        if self.entities[id].ai.registration.is_none()
            || !self.native_ai_command_allowed(id)
            || self.attack_kind(id, "IMMOBILE")
            || self.object_disabled(id)
            || !(self.container_admits(id, carrier, false) || self.can_enter(id, carrier))
        {
            return;
        }
        self.set_order_state_from(id, Order::Enter(carrier), false, None, Some(source));
    }

    pub(in crate::skirmish) fn enter_native_enter(&mut self, id: usize, carrier: usize) -> bool {
        if !self.can_enter(id, carrier) {
            return false;
        }
        let goal = self.object_position(carrier);
        let actor = &mut self.entities[id];
        actor.ai.ignored_producer = Some(carrier);
        actor.ai.motion.set_allow_invalid_position(true);
        actor.ai.command.move_to.position = goal;
        actor.ai.command.move_to.adjusts = false;
        !matches!(self.enter_primary_ground_move(id), MoveResult::Failure)
    }

    pub(in crate::skirmish) fn update_enter(&mut self, id: usize, carrier: usize) {
        if !self
            .entities
            .get(carrier)
            .is_some_and(crate::skirmish::Entity::exists)
            || self.entities[carrier].contained_by().is_some()
                && self.height_above_ground(carrier) > 0.0
                && self.height_above_ground(id) <= 0.0
        {
            self.set_ai_order(id, Order::Idle);
            return;
        }
        self.entities[id].ai.command.move_to.position = self.object_position(carrier);
        self.set_ai_goal_object(id, carrier);
        if !self.can_enter(id, carrier) {
            if self.object_relationship(id, carrier) == 0 {
                let result = self.attack_target_result(
                    id,
                    Some(carrier),
                    self.object_position(carrier),
                    false,
                    false,
                    None,
                );
                if matches!(
                    result,
                    targeting::Result::Possible | targeting::Result::PossibleAfterMoving
                ) {
                    let source = self.entities[id].ai.command.last_command_source;
                    self.native_attack_goal_from(
                        id,
                        crate::skirmish::combat::attack::Goal::Object(carrier),
                        i32::MAX.unsigned_abs(),
                        source,
                        false,
                    );
                    return;
                }
            }
            self.set_ai_order(id, Order::Idle);
            return;
        }
        if self.entities[id].disabled_held() {
            self.set_ai_order(id, Order::Idle);
            return;
        }
        match self.update_internal_move(id) {
            MoveResult::Continue => {}
            MoveResult::Failure => self.set_ai_order(id, Order::Idle),
            MoveResult::Success => {
                if !self.vertical_overlap(id, carrier) {
                    return;
                }
                let geometry = &self.catalog[self.entities[carrier].type_index].geometry;
                let radius = if matches!(geometry.shape, crate::collision_geometry::Shape::Box) {
                    geometry.minor
                } else {
                    geometry.major
                };
                let delta = [
                    self.entities[id].position[0] - self.entities[carrier].position[0],
                    self.entities[id].position[1] - self.entities[carrier].position[1],
                ];
                let squared = delta.map(|component| component * component);
                if squared[0] + squared[1] < radius * radius
                    && self.container_rules(carrier).is_some()
                {
                    self.add_to_contain(id, carrier);
                }
                self.set_ai_order(id, Order::Idle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Order, ai_goals::COMMAND_FROM_AI};

    #[test]
    fn native_enter_installs_xyz_path_and_host_exclusion_before_the_first_update() {
        let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
        game.set_object_position(carrier, [355.0, 255.0, 30.0]);
        let rider = game.spawn(5, 0, [355.0, 335.0], true);
        game.native_enter_from(rider, carrier, COMMAND_FROM_AI);
        assert!(matches!(game.entities[rider].ai.order, Order::Enter(id) if id == carrier));
        assert_eq!(
            game.entities[rider].ai.command.last_command_source,
            COMMAND_FROM_AI
        );
        assert_eq!(game.entities[rider].ai.command.goal_object, Some(carrier));
        assert_eq!(
            game.entities[rider].ai_goal_position().map(f32::to_bits),
            [355.0_f32, 255.0, 30.0].map(f32::to_bits)
        );
        assert_eq!(game.entities[rider].ai.ignored_producer, Some(carrier));
        assert!(game.entities[carrier].containment.waiting.contains(&rider));
        assert!(!game.entities[rider].ai.command.move_to.adjusts);
        assert_eq!(
            game.entities[rider]
                .ai
                .path_request
                .pending
                .as_ref()
                .unwrap()
                .goal
                .map(f32::to_bits),
            [355.0_f32, 255.0, 30.0].map(f32::to_bits)
        );
        game.ai_idle_from_ai(rider);
        assert!(game.entities[rider].ai.ignored_producer.is_none());
        assert!(!game.entities[carrier].containment.waiting.contains(&rider));
    }

    #[test]
    fn native_enter_does_not_replace_an_order_when_mobile_admission_fails() {
        let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
        let rider = game.spawn(5, 0, [355.0, 335.0], true);
        game.move_to(rider, [500.0, 335.0]).unwrap();
        let revision = game.entities[rider].ai.command_revision;
        game.entities[rider].disabled_status |= 1 << 2;
        game.native_enter_from(rider, carrier, COMMAND_FROM_AI);
        assert_eq!(game.entities[rider].ai.command_revision, revision);
        assert!(matches!(game.entities[rider].ai.order, Order::Move(_)));
        assert!(!game.entities[carrier].containment.waiting.contains(&rider));
    }
}
