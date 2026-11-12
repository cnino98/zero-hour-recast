//! `SidesList::prepareForMP_or_Skirmish` and retained civilian `Player::init`.
use crate::skirmish::{Faction, Skirmish, new_player};

pub(in crate::skirmish) struct SideState {
    pub(super) owners: std::collections::BTreeMap<String, usize>,
    relations: [[Option<u8>; 16]; 16],
    pub(super) members: std::collections::BTreeMap<usize, String>,
    pub(super) team_relations:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, u8>>,
    computer: [bool; 16],
    permissions: [u8; 16],
}

impl SideState {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            owners: std::collections::BTreeMap::new(),
            relations: [[None; 16]; 16],
            computer: [true; 16],
            permissions: [3; 16],
            members: std::collections::BTreeMap::new(),
            team_relations: std::collections::BTreeMap::new(),
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn map_unit_produced(&mut self, factory: Option<usize>, unit: usize) {
        if let Some((name, _)) = self
            .map_side_state
            .owners
            .iter()
            .find(|(_, owner)| **owner == self.entities[unit].owner)
        {
            self.register_map_member(unit, format!("team{name}"));
        }
        let team = self.team_membership.default_team(self.entities[unit].owner);
        self.team_membership.set_team(unit, Some(team));
        if let Some(factory) = factory {
            self.map_npc_unit_produced(factory, unit);
        }
    }

    pub(in crate::skirmish) const fn map_build_allowed(&self, owner: usize, units: bool) -> bool {
        let flag = if units { 2 } else { 1 };
        self.map_side_state.permissions[owner] & flag != 0
    }

    pub(in crate::skirmish) const fn set_map_build_allowed(
        &mut self,
        owner: usize,
        units: bool,
        enabled: bool,
    ) {
        let flag = if units { 2 } else { 1 };
        let permissions = &mut self.map_side_state.permissions[owner];
        if enabled {
            *permissions |= flag;
        } else {
            *permissions &= !flag;
        }
    }

    pub(in crate::skirmish) fn map_supply_owner(&self, index: usize) -> usize {
        self.entities[self.supplies[index].object].owner
    }

    pub(in crate::skirmish) fn computer_player(&self, owner: usize) -> bool {
        self.slots.get(owner).map_or_else(
            || {
                self.map_side_state
                    .computer
                    .get(owner)
                    .copied()
                    .unwrap_or(true)
            },
            |slot| slot.difficulty.is_some(),
        )
    }

    pub(in crate::skirmish) fn map_side_relationship(
        &self,
        first: usize,
        second: usize,
    ) -> Option<u8> {
        self.map_side_state
            .relations
            .get(first)
            .and_then(|row| row.get(second))
            .copied()
            .flatten()
    }

    pub(in crate::skirmish) fn placed_map_team(&self, name: &str) -> (usize, String) {
        // prepareForMP_or_Skirmish clears unqualified authored prototypes.
        // PlayerList::validateTeam falls back to the neutral default in release.
        self.map_side_state
            .owners
            .iter()
            .find(|(side, _)| format!("team{side}") == name)
            .map_or_else(
                || (self.neutral_owner(), "team".into()),
                |(side, owner)| (*owner, format!("team{side}")),
            )
    }

    fn initialize_map_default_teams(&mut self) {
        self.team_membership = crate::skirmish::team_membership::Membership::new();
        // prepareForMP_or_Skirmish retains civilian/default prototypes and creates
        // generated player defaults. Each owner's default precedes imported AI prototypes.
        for owner in 0..self.players.len() {
            let name = self
                .map_side_state
                .owners
                .iter()
                .find(|(_, id)| **id == owner)
                .map_or_else(
                    || format!("teamplayer{owner}"),
                    |(name, _)| format!("team{name}"),
                );
            let prototype = self.team_membership.add_prototype(owner, name);
            self.team_membership
                .set_prototype_singleton(prototype, true);
            let team = self.team_membership.create_team(prototype);
            self.team_membership.set_default(owner, team);
        }
    }

    pub(in crate::skirmish) fn initialize_map_side_players(&mut self) -> Result<(), &'static str> {
        self.map_side_state = SideState::new();
        self.map_side_state
            .owners
            .insert(String::new(), self.neutral_owner());
        for (owner, _) in self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.occupied)
        {
            self.map_side_state
                .owners
                .insert(format!("player{owner}"), owner);
        }
        for side in &self.layout.map_sides {
            if side.name.is_empty() || side.faction != "FactionCivilian" {
                continue;
            }
            if self.map_side_state.owners.contains_key(&side.name) {
                return Err("Authored side conflicts with a generated playable player name");
            }
            if self.players.len() >= 16 {
                return Err("Authored sides exceed native MAX_PLAYER_COUNT");
            }
            let owner = self.players.len();
            let initial = if side.template_money == 0 {
                self.rules.starting_cash
            } else {
                side.template_money
            };
            self.players.push(new_player(
                Faction::Civilian,
                initial.wrapping_add(side.start_money),
            ));
            self.map_side_state.owners.insert(side.name.clone(), owner);
            self.map_side_state.computer[owner] = !side.human;
            if !side.human {
                self.map_side_state.permissions[owner] = 1;
            }
        }
        self.initialize_map_default_teams();
        for first in 0..self.players.len() {
            for second in 0..self.players.len() {
                let first_name = self
                    .map_side_state
                    .owners
                    .iter()
                    .find(|(_, owner)| **owner == first)
                    .map(|(name, _)| name);
                let second_name = self
                    .map_side_state
                    .owners
                    .iter()
                    .find(|(_, owner)| **owner == second)
                    .map(|(name, _)| name);
                if first < self.neutral_owner() && second < self.neutral_owner() {
                    continue;
                }
                let mut relation = 1;
                if let (Some(first_name), Some(second_name)) = (first_name, second_name)
                    && let Some(side) = self
                        .layout
                        .map_sides
                        .iter()
                        .find(|side| side.name == *first_name)
                {
                    if side.enemies.contains(second_name) {
                        relation = 0;
                    }
                    if side.allies.contains(second_name) {
                        relation = 2;
                    }
                }
                if first == second {
                    relation = 2;
                } else if second == self.neutral_owner() {
                    relation = 1;
                }
                self.map_side_state.relations[first][second] = Some(relation);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::map_scripts::SideDefinition;

    fn side(name: &str, template_money: u32, start_money: u32) -> SideDefinition {
        SideDefinition {
            gatherers: [0; 3],
            name: name.into(),
            faction: "FactionCivilian".into(),
            human: false,
            template_money,
            start_money,
            allies: Vec::new(),
            enemies: Vec::new(),
        }
    }

    #[test]
    fn retained_civilian_players_have_separate_money_and_no_playable_slot() {
        let mut game = crate::skirmish::tests::game();
        game.layout.map_sides = vec![side("PlyrCivilian", 0, 300), side("Civ_Gat", 700, 20)];
        game.layout.map_sides[0].enemies.push("Civ_Gat".into());
        game.layout.map_sides[1].human = true;
        game.layout.map_sides[1].allies.push("PlyrCivilian".into());
        game.reset_slots(game.slots.clone()).unwrap();
        let neutral = game.neutral_owner();
        let first = game.map_side_state.owners["PlyrCivilian"];
        let second = game.map_side_state.owners["Civ_Gat"];
        assert_eq!(neutral, game.layout.starts.len());
        assert_eq!(first, neutral + 1);
        assert_eq!(second, first + 1);
        assert_eq!(game.players[neutral].cash, 0);
        assert_eq!(game.players[first].cash, game.rules.starting_cash + 300);
        assert_eq!(game.players[second].cash, 720);
        assert!(!game.player_active(first));
        assert!(!game.player_active(second));
        assert!(game.computer_player(neutral));
        assert!(game.computer_player(first));
        assert!(!game.computer_player(second));
        assert!(game.enemies(first, second));
        assert!(!game.allied(first, second));
        assert!(game.allied(second, first));
        assert!(!game.enemies(second, first));
        assert_eq!(
            game.placed_map_team("teamCiv_Gat"),
            (second, "teamCiv_Gat".into())
        );
        assert_eq!(game.placed_map_team("GAT - Team"), (neutral, "team".into()));
        game.players[first].cash = 1;
        assert_eq!(game.players[second].cash, 720);
    }

    #[test]
    fn generated_player_names_bind_authored_owners_without_replacing_live_slot_alliances() {
        let mut game = crate::skirmish::tests::game();
        game.slots[0].team = Some(2);
        game.slots[1].team = Some(2);
        let mut npc = side("NPC", 1, 0);
        npc.enemies.push("player0".into());
        npc.allies.push("player1".into());
        game.layout.map_sides = vec![npc];
        game.layout.neutral_objects[game.layout.supplies[0]].original_team = "teamplayer0".into();
        game.reset_slots(game.slots.clone()).unwrap();
        let npc = game.map_side_state.owners["NPC"];
        assert_eq!(
            game.placed_map_team("teamplayer0"),
            (0, "teamplayer0".into())
        );
        assert_eq!(game.map_supply_owner(0), 0);
        let starting: Vec<_> = game
            .entities
            .iter()
            .enumerate()
            .filter(|(_, actor)| actor.owner == 0 && actor.alive())
            .map(|(id, _)| id)
            .collect();
        assert!(starting.len() >= 2);
        assert!(starting.iter().all(|id| {
            game.team_membership.team_name(
                game.team_membership
                    .team_of(*id)
                    .expect("object native team"),
            ) == "teamplayer0"
        }));
        let area = crate::skirmish::AiArea {
            name: "All starting units".into(),
            points: vec![[0.0, 0.0], [2000.0, 0.0], [2000.0, 2000.0], [0.0, 2000.0]],
        };
        assert!(super::super::teams::side_units_in_area(
            &game,
            game.map_script_player(&game.map_scripts, "player0"),
            &area
        ));
        assert!(!super::super::teams::side_units_in_area(
            &game,
            game.map_script_player(&game.map_scripts, "UnresolvedPlayer"),
            &area
        ));
        assert!(game.enemies(npc, 0));
        assert!(game.allied(npc, 1));
        assert!(!game.enemies(0, npc));
        assert!(game.allied(0, 1));
        assert_eq!(game.map_side_relationship(0, 1), None);
        game.layout.map_sides[0].name = "player0".into();
        assert!(game.reset_slots(game.slots.clone()).is_err());
    }

    #[test]
    fn capture_and_tech_neutralization_replace_original_default_team_membership() {
        use crate::skirmish::{DeathType, Faction, Kind, MaintenanceRules};
        let mut game = crate::skirmish::tests::game();
        game.layout.map_sides = vec![side("NPC", 1, 0)];
        game.reset_slots(game.slots.clone()).unwrap();
        let npc = game.map_side_state.owners["NPC"];
        let mut tech = crate::skirmish::tests::fixture_type(Faction::Civilian, Kind::TechBuilding);
        tech.maintenance.kind_flags |= MaintenanceRules::TECH_BUILDING;
        crate::skirmish::neutral_tech::tests::bind_tech(&mut tech);
        let index = game.catalog.len();
        game.catalog.push(tech);
        let target = game.spawn(index, npc, [310.0, 300.0], true);
        let player_unit = game.spawn(6, 0, [330.0, 300.0], true);
        game.map_side_state.team_relations.insert(
            "teamNPC".into(),
            std::collections::BTreeMap::from([("teamplayer0".into(), 0)]),
        );
        assert_eq!(
            game.team_membership.team_name(
                game.team_membership
                    .team_of(target)
                    .expect("object native team")
            ),
            "teamNPC"
        );
        assert_eq!(game.map_team_relationship(target, player_unit), Some(0));
        game.transfer_maintenance_owner(target, 0);
        assert_eq!(
            game.team_membership.team_name(
                game.team_membership
                    .team_of(target)
                    .expect("object native team")
            ),
            "teamplayer0"
        );
        assert_eq!(
            game.team_membership.team_of(target),
            Some(game.team_membership.default_team(0))
        );
        assert_eq!(game.map_team_relationship(target, player_unit), None);
        game.apply_native_damage(target, None, 10000.0, DeathType::Normal);
        assert_eq!(game.entities[target].owner, game.neutral_owner());
        assert_eq!(
            game.team_membership.team_name(
                game.team_membership
                    .team_of(target)
                    .expect("object native team")
            ),
            "team"
        );
        assert_eq!(
            game.team_membership.team_of(target),
            Some(game.team_membership.default_team(game.neutral_owner()))
        );
    }

    #[test]
    fn authored_warehouse_ownership_resolves_once_and_keeps_directional_relationships() {
        let mut game = crate::skirmish::tests::game();
        let mut first = side("NPC_A", 1, 0);
        first.enemies.push("NPC_B".into());
        game.layout.map_sides = vec![first, side("NPC_B", 1, 0)];
        game.layout.neutral_objects[game.layout.supplies[0]].original_team = "teamNPC_A".into();
        game.layout.neutral_objects[game.layout.supplies[1]].original_team = "teamNPC_B".into();
        game.reset_slots(game.slots.clone()).unwrap();
        let first = game.map_side_state.owners["NPC_A"];
        let second = game.map_side_state.owners["NPC_B"];
        assert_eq!(game.map_supply_owner(0), first);
        assert_eq!(game.map_supply_owner(1), second);
        assert!(game.enemies(first, game.map_supply_owner(1)));
        assert!(!game.enemies(second, game.map_supply_owner(0)));
        game.layout.neutral_objects[game.layout.supplies[0]].original_team =
            "UndefinedEditorTeam".into();
        game.reset_slots(game.slots.clone()).unwrap();
        assert_eq!(game.map_supply_owner(0), game.neutral_owner());
        assert_ne!(game.map_supply_owner(0), game.players.len() - 1);
    }

    #[test]
    fn player_list_allies_override_enemies_before_final_self_and_neutral_defaults() {
        let mut game = crate::skirmish::tests::game();
        let mut first = side("NPC_A", 1, 0);
        first.enemies = vec!["NPC_B".into(), "NPC_A".into(), String::new()];
        first.allies = vec!["NPC_B".into(), String::new()];
        game.layout.map_sides = vec![first, side("NPC_B", 1, 0)];
        game.reset_slots(game.slots.clone()).unwrap();
        let first = game.map_side_state.owners["NPC_A"];
        let second = game.map_side_state.owners["NPC_B"];
        assert!(game.allied(first, second));
        assert!(!game.enemies(first, second));
        assert!(game.allied(first, first));
        assert_eq!(
            game.map_side_relationship(first, game.neutral_owner()),
            Some(1)
        );
        assert_eq!(game.map_side_relationship(second, first), Some(1));
    }
}
