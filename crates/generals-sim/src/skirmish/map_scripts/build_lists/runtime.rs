//! Native NPC construction cadence and authored completion callbacks.
use super::{BuildEntry, Record, State};
use crate::skirmish::{Skirmish, opponent::OpponentRules};

pub(super) mod placement;

impl Skirmish {
    pub(in crate::skirmish) fn update_map_build_lists(&mut self) {
        let Some(rules) = self.opponent_rules.as_ref().map(std::sync::Arc::clone) else {
            return;
        };
        let mut states = std::mem::take(&mut self.map_scripts.npc_builds);
        for state in &mut states {
            if !self.map_build_allowed(state.owner, false) {
                super::work::update(self, state);
                super::repair::update(self, state);
                continue;
            }
            if !state.ready {
                state.structure_timer -= 1.0;
                if state.structure_timer <= 0.0 {
                    state.ready = true;
                    state.build_delay = 0;
                }
            }
            state.build_delay -= 1;
            if state.build_delay < 1 {
                if state.ready {
                    update(self, state, &rules);
                }
                if state.build_delay < 1 {
                    state.build_delay = 60;
                }
            }
            super::work::update(self, state);
            super::repair::update(self, state);
        }
        self.map_scripts.npc_builds = states;
    }

    pub(in crate::skirmish) fn map_structure_produced(&mut self, building: usize) {
        let owner = self.entities[building].owner;
        for state in &mut self.map_scripts.npc_builds {
            if state.owner == owner {
                state.build_delay = 0;
                state.team_delay = 0;
            }
        }
        let found = self
            .map_scripts
            .npc_builds
            .iter()
            .enumerate()
            .find_map(|(state, value)| {
                value
                    .records
                    .iter()
                    .position(|record| record.object == Some(building))
                    .map(|record| (state, record))
            });
        let Some((state, record)) = found else {
            if let Some(hole) = self.map_rebuilder(building) {
                for state in &mut self.map_scripts.npc_builds {
                    if let Some(record) = state
                        .records
                        .iter_mut()
                        .find(|record| record.object == Some(hole))
                    {
                        record.object = Some(building);
                        return;
                    }
                }
            }
            return;
        };
        let value = &mut self.map_scripts.npc_builds[state];
        value.records[record].object = Some(building);
        value.records[record].constructing = false;
        let entry = &value.entries[record];
        let name = entry.name.clone();
        let script = entry.script.clone();
        let unsellable = entry.properties & BuildEntry::UNSELLABLE != 0;
        if !name.is_empty() {
            self.map_objects.insert(name, building);
        }
        self.set_map_unsellable(building, unsellable);
        self.run_attached_map_script(&script, building);
        self.map_scripts.npc_builds[state].records[record].supply =
            super::collectors::quota(self, building);
    }
}

fn update(game: &mut Skirmish, state: &mut State, rules: &OpponentRules) {
    for index in 0..state.records.len() {
        if maintain(
            game,
            state.owner,
            &mut state.records[index],
            state.repair.worker,
        ) {
            super::work::queue_dozer(game, state);
        }
        let record = &mut state.records[index];
        if record.object.is_some() || record.remaining == 0 {
            continue;
        }
        if record.timestamp > 0 {
            if record.timestamp + u64::from(rules.rebuild_delay_seconds) * 30 > game.tick {
                continue;
            }
            record.timestamp = 0;
        }
        let entry = &state.entries[index];
        if entry.template.is_empty() {
            continue;
        }
        let Some(type_index) = game
            .catalog
            .iter()
            .position(|unit| unit.name == entry.template)
        else {
            continue;
        };
        let position = entry.position;
        let angle = entry.angle;
        if let Some((building, builder)) =
            placement::build(game, state, type_index, position, angle)
        {
            let record = &mut state.records[index];
            record.object = Some(building);
            record.builder = Some(builder);
            record.timestamp = game.tick + 1;
            record.constructing = true;
            if record.remaining != u32::MAX {
                record.remaining -= 1;
            }
            state.ready = false;
            state.structure_timer = crate::skirmish::opponent::adjusted_timer(
                rules.structure_seconds,
                game.players[state.owner].cash,
                rules,
                true,
            );
            let team = game
                .map_side_state
                .owners
                .iter()
                .find_map(|(name, &owner)| (owner == state.owner).then(|| format!("team{name}")))
                .expect("retained NPC owner");
            game.register_map_member(building, team);
            break;
        }
    }
}

fn maintain(
    game: &mut Skirmish,
    owner: usize,
    record: &mut Record,
    reserved: Option<usize>,
) -> bool {
    let Some(id) = record.object else {
        return false;
    };
    if !game.entities[id].exists() || game.entities[id].owner != owner {
        record.object = if game.entities[id].exists() {
            None
        } else {
            game.map_rebuild_hole(id)
        };
        record.timestamp = game.tick + 1;
        record.constructing = false;
        return false;
    }
    if record.constructing {
        let (builder, need_dozer) = record
            .builder
            .filter(|id| game.entities[*id].exists())
            .map_or_else(
                || placement::select_dozer(game, owner, game.entities[id].position, reserved),
                |id| (Some(id), false),
            );
        if let Some(builder) = builder {
            record.builder = Some(builder);
            if game.entities[builder].build_task_target() != Some(id) {
                let _ = game.resume_build(builder, id);
            }
        }
        return need_dozer;
    }
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn captured_authored_structure_becomes_a_delayed_rebuild_without_stealing_the_object() {
        let mut game = super::super::tests::game(Vec::new());
        let building = game.map_objects["NPC_Barracks"];
        let npc = game.entities[building].owner;
        game.entities[building].owner = 0;
        let mut states = std::mem::take(&mut game.map_scripts.npc_builds);
        let record = states[0].records.last_mut().unwrap();
        super::maintain(&mut game, npc, record, None);
        assert_eq!(record.object, None);
        assert_eq!(record.timestamp, game.tick + 1);
        assert_eq!(record.remaining, 2);
        assert_eq!(game.entities[building].owner, 0);
        assert!(game.entities[building].alive());
    }
    #[test]
    fn native_rebuild_hole_keeps_hidden_identity_then_transfers_the_authored_record() {
        use crate::skirmish::{Faction, HoleExposure, Kind, RebuildRules};
        let mut game = super::super::tests::game(Vec::new());
        let original = game.map_objects["NPC_Barracks"];
        let owner = game.entities[original].owner;
        let type_index = game.entities[original].type_index;
        let mut hole = crate::skirmish::tests::fixture_type(Faction::Gla, Kind::TechBuilding);
        hole.name = "NPCRebuildHole".into();
        hole.kind_of.push("REBUILD_HOLE".into());
        hole.maintenance.rebuild.push(RebuildRules {
            worker: game.catalog[1].name.clone(),
            respawn_frames: 2.0,
            regeneration: 0.005,
        });
        crate::skirmish::tests::register_update(&mut hole, "RebuildHoleBehavior");
        hole.behavior_modules.last_mut().unwrap().interfaces |= 2;
        game.catalog.push(hole);
        game.catalog[type_index]
            .maintenance
            .expose_hole
            .push(HoleExposure {
                conditions: crate::skirmish::death_callbacks::Conditions {
                    death_mask: u32::MAX,
                    veterancy_mask: 15,
                    exempt_status: 0,
                    required_status: 0,
                },
                template: "NPCRebuildHole".into(),
                health: 500.0,
                transfer_attackers: true,
            });
        crate::skirmish::tests::register_behavior(
            &mut game.catalog[type_index],
            "RebuildHoleExposeDie",
            2,
        );
        let mut plant = crate::skirmish::tests::fixture_type(Faction::Usa, Kind::PowerPlant);
        plant.name = "NPCPowerPlant".into();
        plant.energy = 5;
        let plant_type = game.catalog.len();
        game.catalog.push(plant);
        game.spawn(plant_type, owner, [800.0, 350.0], true);
        game.update_power();
        game.apply_native_damage(
            original,
            None,
            game.entities[original].max_health,
            crate::skirmish::DeathType::Normal,
        );
        let hole = game.map_rebuild_hole(original).unwrap();
        // RebuildHoleExposeDie retains the old corpse until its separate deletion.
        game.destroy_maintenance_object(original);
        game.process_update_destruction();
        let mut states = std::mem::take(&mut game.map_scripts.npc_builds);
        super::maintain(&mut game, owner, &mut states[0].records[0], None);
        assert_eq!(states[0].records[0].object, Some(hole));
        game.map_scripts.npc_builds = states;
        game.tick += 1;
        game.run_object_updates();
        game.tick += 1;
        game.run_object_updates();
        assert!(game.entities[hole].masked() && game.entities[hole].exists());
        let foundation = game
            .entities
            .iter()
            .enumerate()
            .find_map(|(id, _)| (game.map_rebuilder(id) == Some(hole)).then_some(id))
            .unwrap();
        let cash = game.players[owner].cash;
        game.map_scripts.npc_builds[0].build_delay = 45;
        game.map_scripts.npc_builds[0].team_delay = 30;
        for _ in 0..600 {
            game.step();
            if game.entities[foundation].complete() {
                break;
            }
        }
        assert!(game.entities[foundation].complete());
        assert_eq!(
            game.map_scripts.npc_builds[0].records[0].object,
            Some(foundation)
        );
        assert_eq!(game.map_scripts.npc_builds[0].build_delay, 0);
        assert_eq!(game.map_scripts.npc_builds[0].team_delay, 0);
        assert_eq!(game.players[owner].cash, cash);
        assert!(!game.entities[hole].exists());
    }

    #[test]
    fn authored_rebuild_waits_for_the_native_scan_then_preserves_damaged_scaffold_health() {
        use crate::skirmish::map_scripts::Action;
        let mut game =
            super::super::tests::game(vec![Action::DecrementCounter("Completions".into(), -1)]);
        let original = game.map_objects["NPC_Barracks"];
        let owner = game.entities[original].owner;
        let worker = game.spawn(1, owner, [450.0, 200.0], true);
        // Native construction timing depends on the controlling player's power.
        let mut plant = crate::skirmish::tests::fixture_type(
            crate::skirmish::Faction::Usa,
            crate::skirmish::Kind::PowerPlant,
        );
        plant.name = "NPCPowerPlant".into();
        plant.energy = 5;
        let plant_type = game.catalog.len();
        game.catalog.push(plant);
        game.spawn(plant_type, owner, [800.0, 350.0], true);
        game.update_power();
        let mut rules = crate::skirmish::opponent::tests::rules(Vec::new());
        rules.rebuild_delay_seconds = 1;
        game.opponent_rules = Some(std::sync::Arc::new(rules));
        game.destroy_maintenance_object(original);
        for _ in 0..61 {
            game.step();
        }
        assert!(game.map_scripts.npc_builds[0].records[0].object.is_none());
        assert_eq!(game.players[owner].cash, 9500);
        game.step();
        let rebuilt = game.map_scripts.npc_builds[0].records[0]
            .object
            .expect("native NPC rebuild scan");
        assert_ne!(rebuilt, original);
        assert!(!game.entities[original].exists());
        assert_eq!(game.players[owner].cash, 8000);
        assert_eq!(game.map_scripts.npc_builds[0].records[0].remaining, 1);
        assert_eq!(game.entities[worker].build_task_target(), Some(rebuilt));
        for _ in 0..450 {
            if game.entities[rebuilt].build_progress >= 0.99 {
                break;
            }
            game.step();
        }
        assert!(game.entities[rebuilt].build_progress >= 0.99);
        game.apply_native_damage(rebuilt, None, 100.0, crate::skirmish::DeathType::Normal);
        let damaged = game.entities[rebuilt].health;
        for _ in 0..10 {
            game.step();
        }
        assert!(game.entities[rebuilt].complete());
        assert!(game.entities[rebuilt].health > damaged);
        assert!(game.entities[rebuilt].health < game.entities[rebuilt].max_health);
        assert_eq!(game.map_objects["NPC_Barracks"], rebuilt);
        assert!(!game.can_sell(owner, rebuilt));
        assert_eq!(game.map_scripts.counters["Completions"].value, 2);
    }
}
