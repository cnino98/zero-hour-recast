//! `Team::getRelationship`: directional team override before named side relations.
use super::super::Skirmish;
use std::collections::BTreeMap;

fn name<'a>(game: &Skirmish, members: &'a BTreeMap<usize, String>, id: usize) -> Option<&'a str> {
    if let Some(name) = members.get(&id) {
        return Some(name);
    }
    let actor = game.entities.get(id)?;
    (actor.owner == game.neutral_owner()).then_some("team")
}

pub(super) fn members(
    game: &Skirmish,
    registered: &BTreeMap<usize, String>,
    team: &str,
) -> Vec<usize> {
    game.entities
        .iter()
        .enumerate()
        .filter_map(|(id, actor)| {
            (actor.alive() && name(game, registered, id) == Some(team)).then_some(id)
        })
        .collect()
}

pub(super) fn side_units_in_area(
    game: &Skirmish,
    owner: Option<usize>,
    area: &crate::skirmish::AiArea,
) -> bool {
    let Some(owner) = owner else {
        return false;
    };
    game.entities.iter().any(|actor| {
        let unit = &game.catalog[actor.type_index];
        actor.alive()
            && !unit
                .kind_of
                .iter()
                .any(|kind| matches!(kind.as_str(), "INERT" | "PROJECTILE"))
            && actor.owner == owner
            && crate::skirmish::opponent::map::inside(area, actor.position)
    })
}

impl Skirmish {
    pub(in crate::skirmish) fn observe_map_member_team(&mut self, id: usize) {
        if let Some(team) = self.team_membership.team_of(id) {
            let name = self.team_membership.team_name(team).to_owned();
            self.map_side_state.members.insert(id, name);
        }
    }

    pub(in crate::skirmish) fn object_player_relationship(&self, id: usize, owner: usize) -> u8 {
        if let Some(team) = self.team_membership.team_of(id) {
            let first = self.team_membership.team_name(team);
            let second = self
                .team_membership
                .team_name(self.team_membership.default_team(owner));
            if let Some(relation) = self
                .map_side_state
                .team_relations
                .get(first)
                .and_then(|relations| relations.get(second))
            {
                return *relation;
            }
        }
        let actor_owner = self.entities[id].owner;
        self.map_side_relationship(actor_owner, owner)
            .unwrap_or_else(|| {
                if self.allied(actor_owner, owner) {
                    2
                } else {
                    u8::from(!self.enemies(actor_owner, owner))
                }
            })
    }

    pub(in crate::skirmish) fn register_map_member(&mut self, id: usize, team: String) {
        let native = self
            .team_membership
            .find_team(self.entities[id].owner, &team)
            .expect("validated map team must have its native singleton");
        self.team_membership.set_team(id, Some(native));
        self.map_side_state.members.insert(id, team);
    }

    /// Native relationship values: enemy 0, neutral 1, ally 2. A missing map
    /// relationship delegates to the live slot/player relationship.
    pub(in crate::skirmish) fn map_team_relationship(
        &self,
        actor: usize,
        target: usize,
    ) -> Option<u8> {
        if !self.map_side_state.members.contains_key(&actor)
            && !self.map_side_state.members.contains_key(&target)
        {
            return None;
        }
        let first = name(self, &self.map_side_state.members, actor)?;
        let second = name(self, &self.map_side_state.members, target)?;
        if let Some(relation) = self
            .map_side_state
            .team_relations
            .get(first)
            .and_then(|relations| relations.get(second))
        {
            return Some(*relation);
        }
        self.map_side_relationship(self.entities[actor].owner, self.entities[target].owner)
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::map_scripts::{Action, Condition, Script, TeamDefinition};

    fn team(name: &str, owner: &str, faction: &str) -> TeamDefinition {
        TeamDefinition {
            name: name.into(),
            owner: owner.into(),
            singleton: true,
            attack_common_target: false,
            initial_idle_frames: 0,
            owner_faction: faction.into(),
            owner_allies: Vec::new(),
            owner_enemies: Vec::new(),
            owner_human: false,
        }
    }

    #[test]
    fn named_civilian_side_override_is_directional_and_restart_clears_live_relations() {
        let mut game = crate::skirmish::tests::game();
        game.layout
            .map_sides
            .push(crate::skirmish::map_scripts::SideDefinition {
                name: "Civ_Gat".into(),
                faction: "FactionCivilian".into(),
                human: false,
                gatherers: [0; 3],
                template_money: 0,
                start_money: 0,
                allies: Vec::new(),
                enemies: Vec::new(),
            });
        game.layout.map_teams = vec![
            team("teamCiv_Gat", "Civ_Gat", "FactionCivilian"),
            team("teamplayer0", "player0", "FactionAmerica"),
        ];
        game.reset_slots(game.slots.clone()).unwrap();
        let resistance = game.spawn(
            6,
            game.map_side_state.owners["Civ_Gat"],
            [250.0, 300.0],
            true,
        );
        let american = game.spawn(6, 0, [270.0, 300.0], true);
        let script = Script {
            owner: String::new(),
            name: "Civ Alliances".into(),
            active: true,
            one_shot: true,
            difficulty: [true; 3],
            delay_frames: 0,
            subroutine: false,
            group: None,
            conditions: vec![vec![Condition::True]],
            actions: vec![
                Action::TeamAttitude("teamCiv_Gat".into(), 2),
                Action::TeamRelation("teamCiv_Gat".into(), "teamplayer0".into(), 0),
            ],
            false_actions: Vec::new(),
        };
        game.install_map_scripts(vec![script]);
        game.register_map_member(resistance, "teamCiv_Gat".into());
        game.register_map_member(american, "teamplayer0".into());
        assert_eq!(game.map_team_relationship(resistance, american), Some(1));
        game.update_map_scripts();
        assert_eq!(game.entities[resistance].ai.mood.attitude, 2);
        assert_eq!(game.map_team_relationship(resistance, american), Some(0));
        assert_eq!(game.map_team_relationship(american, resistance), Some(1));
        game.reset_slots(game.slots.clone()).unwrap();
        let resistance = game.spawn(
            6,
            game.map_side_state.owners["Civ_Gat"],
            [250.0, 300.0],
            true,
        );
        let american = game.spawn(6, 0, [270.0, 300.0], true);
        game.register_map_member(resistance, "teamCiv_Gat".into());
        game.register_map_member(american, "teamplayer0".into());
        assert_eq!(game.map_team_relationship(resistance, american), Some(1));
    }
}
