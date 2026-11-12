//! SH 2bfaea60 Object.cpp6239: native defection around the team transition.
use crate::skirmish::{CaptureEffect, Skirmish, UnitAiUpdate, visibility::RadarPriority};

impl Skirmish {
    pub(in crate::skirmish) fn defect_to_team(
        &mut self,
        id: usize,
        team: usize,
        detection_frames: u32,
    ) {
        let actor = &self.entities[id];
        let old_owner = actor.owner;
        if actor.contained_by().is_some()
            || self.team_membership.default_team(old_owner) == team
            || actor.under_construction()
            || actor.sold_model()
        {
            return;
        }
        let new_owner = self.team_membership.team_owner(team);
        self.refund_production_on_death(id);
        if old_owner < self.slots.len()
            && new_owner < self.slots.len()
            && !matches!(
                self.catalog[self.entities[id].type_index]
                    .sight
                    .radar_priority,
                RadarPriority::None
            )
        {
            self.capture_event(id, id, CaptureEffect::Infiltrated(self.object_position(id)));
        }
        self.entities[id].helpers.defection.undetected = detection_frames > 0;
        self.start_defection_timer(id, detection_frames, true);
        self.transfer_maintenance_team(id, team);
        self.observe_visibility(id);
        if !matches!(
            self.catalog[self.entities[id].type_index].ai.update,
            UnitAiUpdate::Absent
        ) {
            self.ai_idle_from_ai(id);
        }
        let effect = if self.entities[id].has_drawable() {
            CaptureEffect::Captured
        } else {
            CaptureEffect::DefectedWithoutDrawable
        };
        self.capture_event(id, id, effect);
        let carrier = self.redirected_container(id);
        if self.container_kicks_on_capture(id) {
            if self.is_garrison(carrier) {
                self.remove_garrison_contents_exposed(carrier, true);
            } else {
                while let Some(&rider) = self.passengers(carrier).first() {
                    self.remove_passenger_exposed(rider, carrier, true);
                }
            }
        }
        self.defect_parked_aircraft(id, team, detection_frames);
        // GameLogic's intrusive object list is newest first.
        for mine in (0..self.entities.len()).rev() {
            if self.entities[mine].exists()
                && self.entities[mine].producer == Some(id)
                && self.attack_kind(mine, "MINE")
            {
                self.transfer_maintenance_team(mine, team);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{CaptureEffect, Kind, tests};

    #[test]
    fn named_team_defection_transfers_within_one_owner_and_preserves_exact_membership() {
        let mut game = tests::game();
        let first = game.team_membership.add_prototype(0, "FirstNamed".into());
        let first = game.team_membership.create_team(first);
        let second = game.team_membership.add_prototype(0, "SecondNamed".into());
        let second = game.team_membership.create_team(second);
        let kind = game.type_for(0, Kind::Tank).unwrap();
        let id = game.new_object_on_team(kind, 0, Some(first));
        game.defect_to_team(id, second, 0);
        assert_eq!(game.team_membership.team_of(id), Some(second));
        assert_eq!(game.entities[id].owner, 0);
        assert!(!game.entities[id].helpers.defection.undetected);
        assert!(game.entities[id].ai_idle());
        assert!(
            game.presentation
                .capture_events
                .iter()
                .any(|event| event.target == id && matches!(event.effect, CaptureEffect::Captured))
        );
        let events = game.presentation.capture_events.len();
        // Object::defect compares the old player's default team, not its current named team.
        game.defect_to_team(id, game.team_membership.default_team(0), 10);
        assert_eq!(game.team_membership.team_of(id), Some(second));
        assert_eq!(game.presentation.capture_events.len(), events);
    }

    #[test]
    fn defection_guard_prevents_foundation_transfer_and_accepted_transfer_moves_owned_mines() {
        let mut game = tests::game();
        let kind = game.type_for(0, Kind::Barracks).unwrap();
        let foundation = game.spawn(kind, 0, [350.0, 250.0], false);
        let team = game.team_membership.default_team(1);
        game.defect_to_team(foundation, team, 10);
        assert_eq!(game.entities[foundation].owner, 0);
        assert!(game.presentation.capture_events.is_empty());
        let object = game.spawn(kind, 0, [350.0, 350.0], true);
        let mut mine = tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Vehicle);
        mine.kind_of.push("MINE".into());
        let index = game.catalog.len();
        game.catalog.push(mine);
        let mine = game.spawn(index, 0, [355.0, 350.0], true);
        game.entities[mine].producer = Some(object);
        game.defect_to_team(object, team, 10);
        assert_eq!(game.team_membership.team_of(object), Some(team));
        assert_eq!(game.team_membership.team_of(mine), Some(team));
        assert_eq!(game.entities[mine].owner, 1);
        assert!(game.entities[object].helpers.defection.undetected);
        assert!(!game.entities[mine].helpers.defection.undetected);
    }
}
