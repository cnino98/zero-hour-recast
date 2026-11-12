//! SH Team.cpp1500–1550: common attack target belongs to the actual Team instance.
use crate::skirmish::Skirmish;

impl Skirmish {
    /// `Team::setTeamTargetObject`: null always clears; only non-Easy Computer
    /// owners may publish a victim. Script-created AI players use the admitted
    /// `ScriptEngine` global NORMAL default, as in map build-list selection.
    pub(in crate::skirmish) fn set_team_attack_target(
        &mut self,
        team: usize,
        target: Option<usize>,
    ) {
        let owner = self.team_membership.team_owner(team);
        if target.is_none()
            || self.computer_player(owner)
                && !self.slots.get(owner).is_some_and(|slot| {
                    matches!(
                        slot.difficulty,
                        Some(crate::skirmish::OpponentDifficulty::Easy)
                    )
                })
        {
            self.team_membership.set_common_target(team, target);
        }
    }

    pub(in crate::skirmish) fn team_attacks_common_target(&self, actor: usize) -> bool {
        self.team_membership
            .team_of(actor)
            .is_some_and(|team| self.team_membership.attacks_common_target(team))
    }

    pub(in crate::skirmish) fn team_attack_target(&mut self, actor: usize) -> Option<usize> {
        let team = self.team_membership.team_of(actor)?;
        let target = self.team_membership.common_target(team)?;
        let target_valid = self.entities.get(target).is_some_and(|target| {
            target.exists()
                && !target.is_effectively_dead()
                && (!target.stealthed() || target.detected() || target.has_native_disguise())
                && target.contained_by().is_none()
                && !self.catalog[target.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "AIRCRAFT")
        });
        if target_valid {
            Some(target)
        } else {
            self.set_team_attack_target(team, None);
            None
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn getter_rejects_dead_and_aircraft_and_clears_retained_id() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let target = game.spawn(game.entities[actor].type_index, 1, [350.0, 150.0], true);
        let team = game.team_membership.team_of(actor).unwrap();
        game.team_membership.set_common_target(team, Some(target));
        game.entities[target].set_effectively_dead(true);
        assert!(game.team_attack_target(actor).is_none());
        assert!(game.team_membership.common_target(team).is_none());
        game.entities[target].set_effectively_dead(false);
        game.catalog[game.entities[target].type_index]
            .kind_of
            .push("AIRCRAFT".into());
        game.team_membership.set_common_target(team, Some(target));
        assert!(game.team_attack_target(actor).is_none());
    }
    #[test]
    fn setter_only_publishes_computer_non_easy_targets_but_null_always_clears() {
        use crate::skirmish::OpponentDifficulty::{Easy, Hard, Normal};
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let target = game.spawn(game.entities[actor].type_index, 1, [350.0, 150.0], true);
        let human = game.team_membership.team_of(actor).unwrap();
        game.set_team_attack_target(human, Some(target));
        assert_eq!(game.team_membership.common_target(human), None);
        let computer = game.team_membership.team_of(target).unwrap();
        for (difficulty, accepted) in [(Easy, false), (Normal, true), (Hard, true)] {
            game.slots[1].difficulty = Some(difficulty);
            game.set_team_attack_target(computer, Some(actor));
            assert_eq!(
                game.team_membership.common_target(computer),
                accepted.then_some(actor)
            );
            game.set_team_attack_target(computer, None);
            assert_eq!(game.team_membership.common_target(computer), None);
        }
        game.slots[1].difficulty = Some(Hard);
        game.set_team_attack_target(computer, Some(actor));
        game.slots[1].difficulty = Some(Easy);
        game.set_team_attack_target(computer, None);
        assert_eq!(game.team_membership.common_target(computer), None);
        game.team_membership.set_common_target(human, Some(target));
        game.set_team_attack_target(human, None);
        assert_eq!(game.team_membership.common_target(human), None);
    }
}
