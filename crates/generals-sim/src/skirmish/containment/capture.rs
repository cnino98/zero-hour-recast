//! Authored container onCapture virtual callbacks, SH 2bfaea60.
use crate::skirmish::Skirmish;

impl Skirmish {
    pub(in crate::skirmish) fn capture_transport_contain(
        &mut self,
        id: usize,
        old: usize,
        new: usize,
    ) {
        if old == new {
            return;
        }
        let riders = self.passengers(id).to_vec();
        if self.entities[id].unmanned() {
            for rider in riders {
                self.remove_passenger(rider, id);
            }
        } else {
            let allied = self
                .map_side_relationship(old, new)
                .map_or_else(|| self.allied(old, new), |relation| relation == 2);
            if !allied {
                for rider in riders {
                    self.native_ai_exit(rider, id);
                }
            }
        }
    }

    pub(in crate::skirmish) fn capture_overlord_contain(&mut self, id: usize, new: usize) {
        if let Some(&rider) = self.entities[id].containment.passengers.first() {
            let team = self.team_membership.default_team(new);
            self.transfer_maintenance_team(rider, team);
        }
    }

    pub(in crate::skirmish) fn capture_helix_contain(&mut self, id: usize, new: usize) {
        if let Some(portable) = self.entities[id]
            .attachment_child()
            .filter(|&portable| self.entities[portable].exists())
        {
            let team = self.team_membership.default_team(new);
            self.transfer_maintenance_team(portable, team);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Order, containment, tests};

    #[test]
    fn enemy_transport_capture_orders_native_ai_exit_without_player_command_side_effects() {
        let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.add_to_contain(rider, carrier);
        let team = game.team_membership.default_team(1);
        game.transfer_maintenance_team(carrier, team);
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
        assert!(matches!(
            game.entities[rider].ai.order,
            Order::ExitContainer
        ));
        assert_eq!(game.entities[rider].ai.command.goal_object, Some(carrier));
        assert!(game.entities[rider].last_command_from_ai());
        assert!(game.entities[carrier].containment.waiting.contains(&rider));
        game.update_container_exit(rider);
        assert_eq!(game.entities[rider].contained_by(), None);
        assert_eq!(game.entities[rider].owner, 0);
    }

    #[test]
    fn allied_transport_capture_keeps_riders_but_unmanned_transfer_removes_them_immediately() {
        let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
        game.slots[0].team = Some(0);
        game.slots[1].team = Some(0);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.add_to_contain(rider, carrier);
        game.ai_busy_from_ai(rider);
        let team = game.team_membership.default_team(1);
        game.transfer_maintenance_team(carrier, team);
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
        assert!(game.entities[rider].ai_busy());
        game.set_unmanned(carrier);
        let team = game.team_membership.default_team(0);
        game.transfer_maintenance_team(carrier, team);
        assert_eq!(game.entities[rider].contained_by(), None);
        assert!(!game.entities[rider].drawable_effectively_hidden());
    }

    #[test]
    fn overlord_capture_transfers_literal_first_rider_even_without_portable_kind() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(
            crate::skirmish::Faction::China,
            crate::skirmish::Kind::Vehicle,
        );
        unit.contain = Some(containment::tests::rules(0.0, 0.0));
        tests::register_update(&mut unit, "OverlordContain");
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let carrier = game.spawn(kind, 0, [355.0, 255.0], true);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.add_to_contain(rider, carrier);
        assert!(!game.attack_kind(rider, "PORTABLE_STRUCTURE"));
        let team = game.team_membership.default_team(1);
        game.transfer_maintenance_team(carrier, team);
        assert_eq!(game.entities[rider].owner, 1);
        assert_eq!(game.team_membership.team_of(rider), Some(team));
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    }
}
