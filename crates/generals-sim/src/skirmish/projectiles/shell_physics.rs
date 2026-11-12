//! Each authored `DumbProjectileBehavior` runs at NORMAL in the common update heap.
use super::{Control, FlightRules};
use crate::skirmish::{ProjectileFxStage, Skirmish, fire::Impact};

impl Skirmish {
    pub(in crate::skirmish) fn update_shell_module(
        &mut self,
        id: usize,
        registration: usize,
    ) -> i32 {
        let ordinal = self.projectile_control_index(id, registration);
        let now = self.update_frame();
        let target = self
            .projectile_component(id)
            .and_then(|index| self.projectiles[index].target)
            .filter(|&target| self.entities[target].exists())
            .map(|target| self.target_center(target));
        let type_index = self.entities[id].type_index;
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("shell object");
        let Control::Shell(shell) = &mut flight.controls[ordinal] else {
            unreachable!("DumbProjectileBehavior control");
        };
        let FlightRules::Shell(rules) = &self.catalog[type_index].projectile_rules[ordinal] else {
            unreachable!("DumbProjectileBehavior rules");
        };
        let arrived = if shell.path.is_empty() {
            true
        } else {
            shell.update(
                &mut flight.motion,
                target.filter(|_| shell.launched),
                rules,
                self.projectile_terrain
                    .as_ref()
                    .expect("launched shell terrain"),
                now,
            )
        };
        if arrived {
            self.detonate_shell(id, ordinal);
            return 1;
        }
        self.synchronize_projectile_transform(id);
        if self.shell_bridge_contact(id) {
            self.detonate_shell(id, ordinal);
        } else {
            let flight = self.entities[id]
                .projectile_flight
                .as_mut()
                .expect("shell object");
            let Control::Shell(shell) = &mut flight.controls[ordinal] else {
                unreachable!()
            };
            shell.step += 1;
        }
        1
    }

    fn shell_bridge_contact(&mut self, id: usize) -> bool {
        let position = self.object_position(id);
        let ground = self.ground_height([position[0], position[1]]);
        let old = self.entities[id].layer;
        let layer = self
            .movement_terrain
            .get_highest_layer_for_destination(position, ground, false);
        self.entities[id].layer = layer;
        if old == 1 || layer != 1 {
            return false;
        }
        let above = [position[0], position[1], 9999.0];
        if self
            .movement_terrain
            .get_highest_layer_for_destination(above, ground, false)
            != old
        {
            return false;
        }
        let height =
            self.movement_terrain
                .destination_layer_height(old, [position[0], position[1]], ground);
        self.set_object_position(id, [position[0], position[1], height + 2.0]);
        true
    }

    pub(in crate::skirmish::projectiles) fn detonate_shell(&mut self, id: usize, ordinal: usize) {
        let flight = self.entities[id]
            .projectile_flight
            .as_ref()
            .expect("shell object");
        let Control::Shell(shell) = &flight.controls[ordinal] else {
            unreachable!()
        };
        if shell.detonated {
            return;
        }
        let component = shell
            .launched
            .then(|| self.projectile_component(id))
            .flatten();
        let FlightRules::Shell(rules) =
            &self.catalog[self.entities[id].type_index].projectile_rules[ordinal]
        else {
            unreachable!()
        };
        let calls_kill = rules.detonate_calls_kill;
        if let Some(index) = component {
            let projectile = &self.projectiles[index];
            let position = flight.motion.position;
            let event = projectile.callback_snapshot(&self.entities);
            let impact = Impact::new(
                projectile,
                &self.entities,
                position,
                self.projectile_damage_bonus(projectile),
                self.projectile_bonus_factor(
                    projectile,
                    crate::skirmish::containment::bonuses::RADIUS,
                ),
            );
            self.record_projectile_event(&event, position, ProjectileFxStage::Detonation);
            self.apply_projectile_impact(&impact);
            if calls_kill {
                self.damage_projectile_on_detonation(id);
            } else {
                self.destroy_maintenance_object(id);
            }
        } else {
            self.damage_projectile_on_detonation(id);
        }
        // Native effects and damage callbacks precede both assignments.
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("deferred shell destruction");
        flight.hidden = true;
        let Control::Shell(shell) = &mut flight.controls[ordinal] else {
            unreachable!()
        };
        shell.detonated = true;
    }
}

#[cfg(test)]
mod tests;
