//! Selected original `AISkirmishPlayer` and SCB-script execution branches.
//! Source: pinned `GeneralsMD` `AIPlayer.cpp`, `AISkirmishPlayer.cpp` and `ScriptEngine.cpp`.
mod collectors;
mod commands;
mod construction;
mod defenses;
mod enemies;
mod garrisons;
mod lifecycle;
pub(super) mod map;
pub(super) mod priorities;
mod recruitment;
mod scripts;
mod teams;
#[cfg(test)]
pub(in crate::skirmish) mod tests;
mod transports;

use super::{Faction, Skirmish};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy)]
pub enum OpponentDifficulty {
    Easy,
    Normal,
    Hard,
}

pub struct OpponentRules {
    pub defense_extra_distance: f32,
    pub max_recruit_distance: f32,
    pub attack_priority_distance: f32,
    pub alert_range_modifier: f32,
    pub aggressive_range_modifier: f32,
    pub build_facilities: Vec<String>,
    pub map: OpponentMap,
    pub factions: Vec<OpponentFaction>,
    pub structure_seconds: f32,
    pub team_seconds: f32,
    pub wealthy: u32,
    pub poor: u32,
    pub structures_wealthy_rate: f32,
    pub structures_poor_rate: f32,
    pub teams_wealthy_rate: f32,
    pub teams_poor_rate: f32,
    pub team_resources_to_start: f32,
    pub rebuild_delay_seconds: u32,
    pub supply_center_safe_radius: f32,
}

pub struct OpponentMap {
    pub areas: Vec<AiArea>,
    pub waypoints: Vec<AiWaypoint>,
}
pub struct AiArea {
    pub name: String,
    pub points: Vec<[f32; 2]>,
}
pub struct AiWaypoint {
    pub id: i32,
    pub position: [f32; 2],
    pub labels: Vec<String>,
    pub links: Vec<i32>,
    pub bidirectional: bool,
}

pub struct OpponentFaction {
    pub default_team: String,
    pub defense_template: String,
    pub faction: Faction,
    pub buildings: Vec<AiBuilding>,
    pub gatherers: [u32; 3],
    pub scripts: Vec<AiScript>,
    pub groups: Vec<AiScriptGroup>,
    pub teams: Vec<AiTeam>,
}

pub struct AiBuilding {
    pub bounding_radius: f32,
    pub name: String,
    pub position: [f32; 2],
    pub angle: f32,
    pub rebuilds: i32,
    pub automatic: bool,
}

pub struct AiScript {
    pub group: Option<usize>,
    pub name: String,
    /// Bits in SCB order: active, one-shot, easy, normal, hard, subroutine.
    pub flags: u8,
    pub delay_frames: u32,
    pub conditions: Vec<Vec<AiCall>>,
    pub actions: Vec<AiCall>,
    pub false_actions: Vec<AiCall>,
}

pub struct AiCall {
    pub name: String,
    pub parameters: Vec<AiParameter>,
}
pub struct AiParameter {
    pub integer: i32,
    pub real: f32,
    pub text: String,
    pub position: Option<[f32; 3]>,
}
pub struct AiScriptGroup {
    pub name: String,
    pub active: bool,
    pub subroutine: bool,
}

pub struct AiTeam {
    /// Bits: recruitable by other teams, automatically reinforce casualties.
    pub recruitment: u8,
    pub home: Option<[f32; 2]>,
    pub singleton: bool,
    pub attack_common_target: bool,
    pub priority_success: i32,
    pub priority_failure: i32,
    pub execute_actions: bool,
    pub destroyed_threshold: f32,
    pub aggression: i32,
    pub name: String,
    pub units: Vec<AiTeamUnit>,
    pub priority: i32,
    pub max_instances: u32,
    pub condition: String,
    pub on_create: String,
    pub on_destroyed: String,
    pub on_idle: String,
    pub generic_scripts: Vec<String>,
    pub initial_idle_frames: u32,
}
pub struct AiTeamUnit {
    pub name: String,
    pub minimum: u32,
    pub maximum: u32,
}

pub(super) struct Opponent {
    rules: Arc<OpponentRules>,
    state: State,
}
struct State {
    owner: usize,
    enemy: Option<usize>,
    base_center: [f32; 2],
    defenses: defenses::Placement,
    next_enemy_check: u64,
    start: usize,
    enemy_start: usize,
    difficulty: OpponentDifficulty,
    faction: usize,
    scripts: Vec<ScriptState>,
    groups: Vec<bool>,
    counters: BTreeMap<String, Counter>,
    flags: BTreeMap<String, bool>,
    object_lists: BTreeMap<String, Vec<String>>,
    buildings: Vec<Building>,
    teams: Vec<Team>,
    team_prototypes: Vec<usize>,
    requests: Vec<BuildRequest>,
    build_queue: Vec<usize>,
    ready_queue: Vec<usize>,
    priorities: Vec<i32>,
    attack_priorities: Vec<Option<usize>>,
    structure_timer: f32,
    build_delay: i32,
    team_timer: f32,
    team_delay: i32,
    enabled: u8,
    unsupported: Vec<String>,
}
struct ScriptState {
    active: bool,
    next: u64,
}
struct Counter {
    value: i32,
    countdown: bool,
}
struct Building {
    name: String,
    position: [f32; 2],
    angle: f32,
    remaining: i32,
    automatic: bool,
    priority: bool,
    object: Option<usize>,
    destroyed_at: Option<u64>,
    supply: Option<SupplyQuota>,
}
struct SupplyQuota {
    desired: u32,
    current: i32,
}
enum RequestPhase {
    Building,
    Ready,
    Finished,
}
struct BuildRequest {
    reinforcement: bool,
    reinforcement_unit: Option<usize>,
    team: usize,
    orders: Vec<WorkOrder>,
    created_at: u64,
    phase: RequestPhase,
    priority: bool,
}
struct Team {
    native_instance: Option<usize>,
    recruitable: Option<bool>,
    template: Option<usize>,
    members: Vec<usize>,
    deleted: bool,
    callbacks: lifecycle::Callbacks,
    scripts: Vec<ScriptState>,
    sequence: Option<(usize, usize)>,
    follow: Option<(usize, Option<usize>)>,
}
struct WorkOrder {
    type_index: usize,
    remaining: u32,
    producer: Option<usize>,
    required: bool,
    resource_gatherer: bool,
}

impl Team {
    fn new(template: Option<usize>, tick: u64, script_count: usize) -> Self {
        Self {
            native_instance: None,
            recruitable: None,
            template,
            members: Vec::new(),
            deleted: false,
            callbacks: lifecycle::Callbacks {
                was_idle: false,
                previous_units: 0,
                destroy_threshold: None,
            },
            scripts: (0..script_count)
                .map(|_| ScriptState {
                    active: true,
                    next: tick,
                })
                .collect(),
            sequence: None,
            follow: None,
        }
    }
}

impl Skirmish {
    /// Starts a fresh match using the selected original computer difficulty.
    /// # Errors
    /// Returns the same map/start validation errors as `reset`.
    pub fn reset_with_difficulty(
        &mut self,
        player: Faction,
        enemy: Faction,
        difficulty: OpponentDifficulty,
    ) -> Result<(), &'static str> {
        let mut slots = self.legacy_slots(player, Some(enemy));
        slots[1].difficulty = Some(difficulty);
        self.reset_slots(slots)
    }
    /// Enables the original installed script/build-list slice for one computer slot.
    /// # Errors
    /// Rejects invalid slot, missing faction data or invalid AI timing values.
    pub fn configure_opponent(
        &mut self,
        rules: OpponentRules,
        owner: usize,
        difficulty: OpponentDifficulty,
    ) -> Result<(), &'static str> {
        if owner == 0
            || owner >= self.slots.len()
            || rules.factions.len() != 3
            || [
                rules.structure_seconds,
                rules.team_seconds,
                rules.structures_wealthy_rate,
                rules.structures_poor_rate,
                rules.teams_wealthy_rate,
                rules.teams_poor_rate,
                rules.team_resources_to_start,
                rules.max_recruit_distance,
                rules.alert_range_modifier,
                rules.aggressive_range_modifier,
                rules.defense_extra_distance,
            ]
            .into_iter()
            .any(|n| !n.is_finite() || n < 0.0)
            || [
                rules.structures_wealthy_rate,
                rules.structures_poor_rate,
                rules.teams_wealthy_rate,
                rules.teams_poor_rate,
            ]
            .contains(&0.0)
            || rules.factions.iter().any(|faction| {
                faction.buildings.iter().any(|building| {
                    !building.bounding_radius.is_finite() || building.bounding_radius < 0.0
                }) || faction
                    .gatherers
                    .iter()
                    .any(|&count| count > i32::MAX.unsigned_abs() - 1)
                    || faction.teams.iter().any(|team| {
                        !team.destroyed_threshold.is_finite()
                            || team.units.iter().any(|unit| unit.minimum > unit.maximum)
                    })
            })
            || !rules.attack_priority_distance.is_finite()
            || rules.attack_priority_distance <= 0.0
        {
            return Err("Invalid computer-player rules");
        }
        let faction = rules
            .factions
            .iter()
            .position(|f| f.faction == self.players[owner].faction)
            .ok_or("Missing computer faction")?;
        let rules = Arc::new(rules);
        self.opponent_rules = Some(Arc::clone(&rules));
        self.slots[owner].difficulty = Some(difficulty);
        let state = State::new(self, &rules, owner, difficulty, faction);
        self.opponents[owner] = Some(Opponent { rules, state });
        for id in 0..self.entities.len() {
            self.observe_guard_triggers(id);
        }
        Ok(())
    }

    /// Unsupported source script calls seen across active AI slots.
    #[must_use]
    pub fn opponent_unsupported(&self) -> Vec<String> {
        let mut calls = Vec::new();
        for opponent in self.opponents.iter().flatten() {
            for call in &opponent.state.unsupported {
                if !calls.contains(call) {
                    calls.push(call.clone());
                }
            }
        }
        calls
    }

    // Player::getAiBaseCenter returns false without the actual AI instance or
    // an initialized, nonempty build-list center (AIPlayer.h189/cpp3119).
    pub(in crate::skirmish) fn pilot_ai_base_center(&self, owner: usize) -> Option<[f32; 3]> {
        let state = &self.opponents.get(owner)?.as_ref()?.state;
        state
            .buildings
            .iter()
            .any(|building| !building.name.is_empty())
            .then_some([state.base_center[0], state.base_center[1], 0.0])
    }

    pub(super) fn reset_opponent(&mut self) {
        self.attack_priorities.clear();
        self.trigger_state = map::TriggerState::new();
        self.opponents = (0..self.players.len()).map(|_| None).collect();
        let Some(rules) = self.opponent_rules.as_ref().map(Arc::clone) else {
            return;
        };
        for owner in 1..self.slots.len() {
            if !self.player_active(owner) {
                continue;
            }
            let Some(difficulty) = self.slots[owner].difficulty else {
                continue;
            };
            if let Some(faction) = rules
                .factions
                .iter()
                .position(|f| f.faction == self.players[owner].faction)
            {
                let state = State::new(self, &rules, owner, difficulty, faction);
                self.opponents[owner] = Some(Opponent {
                    rules: Arc::clone(&rules),
                    state,
                });
            }
        }
        for id in 0..self.entities.len() {
            self.observe_guard_triggers(id);
        }
    }

    pub(super) fn update_opponent_scripts(&mut self) {
        if self.end_game_at.is_some() {
            return;
        }
        for owner in 1..self.opponents.len() {
            if !self.player_active(owner) {
                continue;
            }
            self.ai_enemy(owner);
            if let Some(mut opponent) = self.opponents[owner].take() {
                scripts::update(self, &opponent.rules, &mut opponent.state);
                lifecycle::update_states(self, &opponent.rules, &mut opponent.state);
                self.opponents[owner] = Some(opponent);
            }
        }
    }

    pub(super) fn update_opponent(&mut self) {
        for owner in 1..self.opponents.len() {
            if !self.player_active(owner) {
                continue;
            }
            if let Some(mut opponent) = self.opponents[owner].take() {
                construction::update(self, &opponent.rules, &mut opponent.state);
                teams::update(self, &opponent.rules, &mut opponent.state);
                self.opponents[owner] = Some(opponent);
            }
        }
    }

    pub(super) fn opponent_unit_produced(&mut self, producer: usize, unit: usize) {
        let owner = self.entities[unit].owner;
        let Some(mut opponent) = self.opponents[owner].take() else {
            return;
        };
        if self.entities[unit].owner != opponent.state.owner || !self.occupied[opponent.state.owner]
        {
            self.opponents[owner] = Some(opponent);
            return;
        }
        let mut resource_gatherer = false;
        let mut destination = None;
        for &index in &opponent.state.build_queue {
            let request = &mut opponent.state.requests[index];
            if let Some(order) = request.orders.iter_mut().find(|order| {
                order.producer == Some(producer)
                    && self.catalog[self.entities[unit].type_index]
                        .equivalent_to(&self.catalog[order.type_index])
                    && order.remaining > 0
            }) {
                order.remaining -= 1;
                order.producer = None;
                destination = Some(request.team);
                if request.reinforcement {
                    request.reinforcement_unit = Some(unit);
                }
                resource_gatherer = order.resource_gatherer;
                self.entities[unit].ai.supply.force_wanting = resource_gatherer;
                break;
            }
        }
        if let Some(team) = destination {
            recruitment::transfer(self, &opponent.rules, &mut opponent.state, unit, team);
        }
        if resource_gatherer {
            collectors::assign_produced(self, &mut opponent.state, unit);
        } else if self.catalog[self.entities[unit].type_index].kind == super::Kind::Builder {
            opponent.state.build_delay = 0;
            opponent.state.structure_timer = 1.0;
        }
        opponent.state.team_delay = 0;
        self.opponents[owner] = Some(opponent);
    }

    pub(super) fn opponent_structure_produced(&mut self, building: usize) {
        let owner = self.entities[building].owner;
        if let Some(opponent) = self.opponents[owner].as_mut()
            && self.occupied[opponent.state.owner]
            && self.entities[building].owner == opponent.state.owner
        {
            opponent.state.build_delay = 0;
            opponent.state.team_delay = 0;
            let desired = opponent.rules.factions[opponent.state.faction].gatherers
                [opponent.state.difficulty_index()]
                + 1;
            if self.catalog[self.entities[building].type_index]
                .supply
                .dock
                .is_some()
                && let Some(entry) = opponent
                    .state
                    .buildings
                    .iter_mut()
                    .find(|entry| entry.object == Some(building))
            {
                entry.supply = Some(SupplyQuota {
                    desired,
                    current: -1,
                });
            }
        }
    }
}

impl Opponent {
    pub(super) const fn difficulty(&self) -> OpponentDifficulty {
        self.state.difficulty
    }
}

impl State {
    fn new(
        game: &mut Skirmish,
        rules: &OpponentRules,
        owner: usize,
        difficulty: OpponentDifficulty,
        faction: usize,
    ) -> Self {
        let data = &rules.factions[faction];
        let team_prototypes = data
            .teams
            .iter()
            .map(|team| {
                let prototype = game.team_membership.add_prototype(owner, team.name.clone());
                game.team_membership
                    .set_prototype_common_target(prototype, team.attack_common_target);
                game.team_membership
                    .set_prototype_singleton(prototype, team.singleton);
                if team.singleton {
                    game.team_membership.create_team(prototype);
                }
                prototype
            })
            .collect();
        let scripts = data
            .scripts
            .iter()
            .map(|s| ScriptState {
                active: s.flags & 1 != 0,
                next: if s.delay_frames > 0 {
                    u64::from(game.random_between(60))
                } else {
                    0
                },
            })
            .collect();
        let buildings = construction::initial_buildings(game, owner, data);
        let mut base_center = [0.0; 2];
        let mut count = 0_u16;
        for building in &buildings {
            if building.name.is_empty() {
                continue;
            }
            base_center[0] += building.position[0];
            base_center[1] += building.position[1];
            count += 1;
        }
        if count > 0 {
            base_center[0] /= f32::from(count);
            base_center[1] /= f32::from(count);
        }
        let defenses = defenses::Placement::new(&buildings, data, base_center);
        Self {
            owner,
            base_center,
            defenses,
            enemy: None,
            next_enemy_check: 0,
            start: game.start_positions[owner].unwrap_or(owner),
            enemy_start: game.start_positions[0].unwrap_or(0),
            difficulty,
            faction,
            scripts,
            groups: data.groups.iter().map(|group| group.active).collect(),
            counters: BTreeMap::new(),
            flags: BTreeMap::new(),
            object_lists: BTreeMap::new(),
            buildings,
            teams: Vec::new(),
            team_prototypes,
            requests: Vec::new(),
            build_queue: Vec::new(),
            ready_queue: Vec::new(),
            priorities: data.teams.iter().map(|team| team.priority).collect(),
            attack_priorities: data.teams.iter().map(|_| None).collect(),
            structure_timer: 2.0,
            build_delay: 0,
            team_timer: 2.0,
            team_delay: 0,
            // Player starts with base construction enabled; AISkirmishPlayer also
            // enables unit construction after the base AIPlayer disabled it.
            enabled: 3,
            unsupported: Vec::new(),
        }
    }
    fn report(&mut self, name: &str) {
        if !self.unsupported.iter().any(|item| item == name) {
            self.unsupported.push(name.to_owned());
        }
    }
    const fn difficulty_index(&self) -> usize {
        match self.difficulty {
            OpponentDifficulty::Easy => 0,
            OpponentDifficulty::Normal => 1,
            OpponentDifficulty::Hard => 2,
        }
    }

    fn queue_request(&mut self, team: usize, orders: Vec<WorkOrder>, tick: u64, priority: bool) {
        let request = self.requests.len();
        self.requests.push(BuildRequest {
            reinforcement: false,
            reinforcement_unit: None,
            team,
            orders,
            created_at: tick,
            priority,
            phase: RequestPhase::Building,
        });
        if priority {
            self.build_queue.insert(0, request);
        } else {
            self.build_queue.push(request);
        }
        self.team_delay = 0;
    }
}

pub(super) fn adjusted_timer(
    seconds: f32,
    cash: u32,
    rules: &OpponentRules,
    structure: bool,
) -> f32 {
    let mut timer = (seconds * 30.0).trunc();
    if cash < rules.poor {
        timer = (timer
            / if structure {
                rules.structures_poor_rate
            } else {
                rules.teams_poor_rate
            })
        .trunc();
    }
    if cash > rules.wealthy {
        timer = (timer
            / if structure {
                rules.structures_wealthy_rate
            } else {
                rules.teams_wealthy_rate
            })
        .trunc();
    }
    timer
}
