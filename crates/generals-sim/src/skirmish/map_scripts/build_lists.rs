//! `AIPlayer::newMap` authored NPC initial structures; `AISkirmishPlayer` replaces
//! playable-side lists with its source `AIData` layout before building.
use super::BuildEntry;
use crate::skirmish::{Skirmish, ai_repair};

mod collectors;
mod instant;
pub(super) mod repair;
mod runtime;
#[cfg(test)]
mod tests;
mod work;

pub(super) struct State {
    owner: usize,
    entries: Vec<BuildEntry>,
    records: Vec<Record>,
    structure_timer: f32,
    build_delay: i32,
    ready: bool,
    work: Vec<work::WorkOrder>,
    team_delay: i32,
    repair: ai_repair::State,
    center: Option<[f32; 2]>,
}

struct Record {
    object: Option<usize>,
    timestamp: u64,
    remaining: u32,
    builder: Option<usize>,
    constructing: bool,
    supply: Option<collectors::Quota>,
}

impl Skirmish {
    pub(in crate::skirmish) fn initialize_map_build_lists(&mut self) -> Result<(), &'static str> {
        let lists = std::mem::take(&mut self.layout.map_build_lists);
        let mut result = Ok(());
        for list in &lists {
            if let Err(error) = self.initialize_npc_build_list(list) {
                result = Err(error);
                break;
            }
        }
        self.layout.map_build_lists = lists;
        self.rebuild_navigation();
        result
    }

    fn initialize_npc_build_list(&mut self, list: &super::BuildList) -> Result<(), &'static str> {
        let Some(side) = self
            .layout
            .map_sides
            .iter()
            .find(|side| side.name == list.owner)
        else {
            return Ok(());
        };
        if list.owner.is_empty() || side.human || side.faction != "FactionCivilian" {
            return Ok(());
        }
        let Some(&owner) = self.map_side_state.owners.get(&list.owner) else {
            return Ok(());
        };
        let mut entries = list.entries.clone();
        let mut records = Vec::new();
        append_factories(self, owner, &mut entries, &mut records);
        let authored = records.len();
        for entry in &list.entries {
            let allowance = u32::from_ne_bytes(entry.rebuilds.to_ne_bytes());
            records.push(Record {
                object: None,
                timestamp: 0,
                builder: None,
                constructing: false,
                supply: None,
                remaining: if !entry.initially_built && allowance != u32::MAX {
                    allowance.wrapping_add(1)
                } else {
                    allowance
                },
            });
        }
        let index = self.map_scripts.npc_builds.len();
        let center = base_center(self, &entries);
        self.map_scripts.npc_builds.push(State {
            owner,
            entries,
            records,
            structure_timer: 2.0,
            build_delay: 0,
            ready: false,
            work: Vec::new(),
            team_delay: 0,
            repair: ai_repair::State::new(),
            center,
        });
        for entry in authored..self.map_scripts.npc_builds[index].entries.len() {
            self.initialize_npc_structure(index, entry)?;
        }
        Ok(())
    }

    fn initialize_npc_structure(&mut self, state: usize, index: usize) -> Result<(), &'static str> {
        let entry = self.map_scripts.npc_builds[state].entries[index].clone();
        if !entry.initially_built || entry.template.is_empty() {
            return Ok(());
        }
        let type_index = self
            .catalog
            .iter()
            .position(|unit| unit.name == entry.template)
            .ok_or("Authored NPC build template has no gameplay consumer")?;
        let owner = self.map_scripts.npc_builds[state].owner;
        let position = [entry.position[0], entry.position[1], 0.0];
        self.clear_removable_for_construction(type_index, position, entry.angle);
        self.move_objects_for_construction(owner, type_index, position, entry.angle);
        let id = self.spawn(type_index, owner, entry.position, true);
        self.set_object_orientation(id, entry.angle);
        let team = self
            .map_side_state
            .owners
            .iter()
            .find_map(|(name, &value)| (value == owner).then(|| format!("team{name}")))
            .expect("retained NPC side");
        self.register_map_member(id, team);
        if !entry.name.is_empty() {
            self.map_objects.insert(entry.name.clone(), id);
        }
        self.set_map_unsellable(id, entry.properties & BuildEntry::UNSELLABLE != 0);
        // CRC=0 preserves scaffold health rather than applying authored percentage.
        let record = &mut self.map_scripts.npc_builds[state].records[index];
        record.object = Some(id);
        record.timestamp = self.tick + 1;
        self.run_attached_map_script(&entry.script, id);
        self.map_scripts.npc_builds[state].records[index].supply = collectors::quota(self, id);
        Ok(())
    }
}

// AIPlayer::newMap snapshots authored entries before Player::addToBuildList
// prepends live factories; those additions never run instant-build callbacks.
fn append_factories(
    game: &Skirmish,
    owner: usize,
    entries: &mut Vec<BuildEntry>,
    records: &mut Vec<Record>,
) {
    for (id, actor) in game.entities.iter().enumerate().rev() {
        let unit = &game.catalog[actor.type_index];
        if actor.owner != owner || !actor.exists() || unit.production.is_none() {
            continue;
        }
        entries.insert(
            0,
            BuildEntry {
                name: String::new(),
                template: unit.name.clone(),
                position: actor.position,
                angle: actor.facing,
                initially_built: false,
                rebuilds: 0,
                script: String::new(),
                health: 100,
                properties: BuildEntry::WHINER | BuildEntry::REPAIRABLE,
            },
        );
        records.insert(
            0,
            Record {
                object: Some(id),
                timestamp: 0,
                remaining: 0,
                builder: None,
                constructing: false,
                supply: None,
            },
        );
    }
}

fn base_center(game: &Skirmish, entries: &[BuildEntry]) -> Option<[f32; 2]> {
    use num_traits::ToPrimitive;
    let mut position = [0.0; 2];
    let mut count = 0_u32;
    for entry in entries {
        if entry.template.is_empty() || !game.catalog.iter().any(|unit| unit.name == entry.template)
        {
            continue;
        }
        position[0] += entry.position[0];
        position[1] += entry.position[1];
        count += 1;
    }
    if count == 0 {
        return None;
    }
    let count = count.to_f32().expect("native build list count");
    Some(position.map(|value| value / count))
}
