//! SH `ScriptEngine::executeScript` and the admitted civilian map operations.
use super::{Skirmish, startup};

mod actions;
mod breeze;
mod toppling;
pub use breeze::Breeze;
mod build_lists;
mod sides;
pub(in crate::skirmish) use sides::SideState;
mod teams;
mod wander;

/// Raw `ThingFactory` registration metadata before per-map final overrides.
pub struct TemplateDefinition {
    pub name: String,
    pub dozer: bool,
    pub harvester: bool,
}

pub struct SideDefinition {
    pub gatherers: [i32; 3],
    pub name: String,
    pub faction: String,
    pub human: bool,
    pub template_money: u32,
    pub start_money: u32,
    pub allies: Vec<String>,
    pub enemies: Vec<String>,
}

/// Authored `TeamPrototype` and its named controlling side.
pub struct TeamDefinition {
    pub name: String,
    pub owner: String,
    pub singleton: bool,
    pub attack_common_target: bool,
    pub initial_idle_frames: u32,
    pub owner_faction: String,
    pub owner_allies: Vec<String>,
    pub owner_enemies: Vec<String>,
    pub owner_human: bool,
}

pub struct BuildList {
    pub owner: String,
    pub entries: Vec<BuildEntry>,
}

#[derive(Clone)]
pub struct BuildEntry {
    pub name: String,
    pub template: String,
    pub position: [f32; 2],
    pub angle: f32,
    pub initially_built: bool,
    pub rebuilds: i32,
    pub script: String,
    pub health: i32,
    /// Authored whiner, unsellable and repairable property bits.
    pub properties: u8,
}

impl BuildEntry {
    pub const WHINER: u8 = 1;
    pub const UNSELLABLE: u8 = 2;
    pub const REPAIRABLE: u8 = 4;
}

pub struct Script {
    pub owner: String,
    pub name: String,
    pub active: bool,
    pub one_shot: bool,
    pub difficulty: [bool; 3],
    pub delay_frames: u32,
    pub subroutine: bool,
    pub group: Option<Group>,
    pub conditions: Vec<Vec<Condition>>,
    pub actions: Vec<Action>,
    pub false_actions: Vec<Action>,
}

pub struct Group {
    pub name: String,
    pub active: bool,
    pub subroutine: bool,
}

pub struct Fade {
    pub start_frame: u64,
    pub minimum: f32,
    pub maximum: f32,
    pub increase: u32,
    pub hold: u32,
    pub decrease: u32,
    pub additive: bool,
    pub amount: f32,
    frame: u32,
}

impl Fade {
    fn advance(&mut self) -> bool {
        self.frame += 1;
        if self.frame <= self.increase {
            let factor = native_real(self.frame) / native_real(self.increase);
            let change = factor * (self.maximum - self.minimum);
            self.amount = self.minimum + change;
        } else if self.frame - self.increase <= self.hold {
            self.amount = self.maximum;
        } else if self.frame - self.increase - self.hold <= self.decrease {
            let factor = native_real(self.frame - self.increase - self.hold)
                / native_real(self.decrease + 1);
            let change = factor * (self.minimum - self.maximum);
            self.amount = self.maximum + change;
        } else {
            return false;
        }
        true
    }
}

pub enum AudioAction {
    Play(String),
    OverrideVolume(String, f32),
}

struct Counter {
    value: i32,
    countdown: bool,
}

pub enum Condition {
    True,
    False,
    PlayerUnitsInArea(String, super::AiArea),
    ScienceAcquired(String, String),
    NamedDestroyed(String),
    TimerExpired(String),
    Counter(String, u8, i32),
}

pub enum Action {
    TeamHuntCommand(String, String),
    ToppleDirection(String, [f32; 3]),
    TreeSway([f32; 3], i32, f32),
    DrawIconUi(bool),
    PlayerBuild(String, bool, bool),
    PlayerRepair(String, String),
    SetCaveIndex(String, usize),
    NamedKill(String),
    NamedDelete(String),
    EnableScript(String, bool),
    SetCounter(String, i32),
    DecrementCounter(String, i32),
    RandomTimer(String, i32, i32),
    RandomSecondsTimer(String, f32, f32),
    StopTimer(String),
    FadeAdd(f32, f32, [u32; 3]),
    Sound(String),
    AudioVolume(String, f32),
    TeamAttackArea(String, Vec<[f32; 2]>),
    TeamWander(String, String, Vec<super::AiWaypoint>),
    TeamPanic(String, String, Vec<super::AiWaypoint>),
    TeamAttitude(String, i32),
    TeamRelation(String, String, u8),
    NamedDamage(String, u32),
    WarehouseValue(usize, u32),
}

pub(super) struct State {
    topple_directions: Vec<(String, [f32; 3])>,
    breeze: Breeze,
    scripts: Vec<Script>,
    active: Vec<bool>,
    next_evaluation: Vec<u64>,
    groups: std::collections::BTreeMap<String, bool>,
    counters: std::collections::BTreeMap<String, Counter>,
    fade: Option<Fade>,
    audio: Vec<AudioAction>,
    npc_builds: Vec<build_lists::State>,
    current_player: Option<usize>,
    condition_players: std::collections::BTreeMap<(usize, usize, usize), Option<usize>>,
}

impl State {
    pub(super) fn new(scripts: Vec<Script>) -> Self {
        let active = scripts.iter().map(|script| script.active).collect();
        let next_evaluation = vec![0; scripts.len()];
        let groups = scripts
            .iter()
            .filter_map(|script| script.group.as_ref())
            .map(|group| (group.name.clone(), group.active))
            .collect();
        Self {
            topple_directions: Vec::new(),
            breeze: Breeze::native(),
            scripts,
            active,
            next_evaluation,
            groups,
            counters: std::collections::BTreeMap::new(),
            fade: None,
            audio: Vec::new(),
            npc_builds: Vec::new(),
            current_player: None,
            condition_players: std::collections::BTreeMap::new(),
        }
    }

    pub(super) fn reset(&mut self, random: &mut [u32; 6]) {
        self.groups = self
            .scripts
            .iter()
            .filter_map(|script| script.group.as_ref())
            .map(|group| (group.name.clone(), group.active))
            .collect();
        self.topple_directions.clear();
        self.breeze = Breeze::native();
        self.counters.clear();
        self.fade = None;
        self.audio.clear();
        self.npc_builds.clear();
        self.current_player = None;
        self.condition_players.clear();
        for ((active, next), script) in self
            .active
            .iter_mut()
            .zip(&mut self.next_evaluation)
            .zip(&self.scripts)
        {
            *active = script.active;
            *next = if script.delay_frames > 0 {
                u64::from(startup::random_between(random, 60))
            } else {
                0
            };
        }
    }
}

impl Skirmish {
    /// Installs admitted original map-authored civilian scripts for this map.
    /// Definitions survive restart; live one-shot state resets.
    pub fn install_map_scripts(&mut self, scripts: Vec<Script>) {
        self.map_scripts = State::new(scripts);
        self.map_scripts.reset(&mut self.random);
    }

    #[must_use]
    pub const fn map_script_fade(&self) -> Option<&Fade> {
        self.map_scripts.fade.as_ref()
    }

    #[must_use]
    pub fn take_map_script_audio(&mut self) -> Vec<AudioAction> {
        std::mem::take(&mut self.map_scripts.audio)
    }

    pub(super) fn update_map_scripts(&mut self) {
        let mut state = std::mem::replace(&mut self.map_scripts, State::new(Vec::new()));
        if state.fade.as_mut().is_some_and(|fade| !fade.advance()) {
            state.fade = None;
        }
        for counter in state.counters.values_mut() {
            if counter.countdown && counter.value >= 0 {
                counter.value -= 1;
            }
        }
        let definitions = std::mem::take(&mut state.scripts);
        for (index, script) in definitions.iter().enumerate() {
            if script.subroutine
                || script
                    .group
                    .as_ref()
                    .is_some_and(|group| group.subroutine || !state.groups[&group.name])
            {
                continue;
            }
            state.current_player = self.map_side_state.owners.get(&script.owner).copied();
            self.execute_map_script(&mut state, &definitions, index);
        }
        state.current_player = None;
        state.scripts = definitions;
        self.map_scripts = state;
    }
    pub(super) fn run_attached_map_script(&mut self, name: &str, object: usize) {
        if name.is_empty() || name == "None" {
            return;
        }
        let previous = self.map_objects.insert("<This Object>".into(), object);
        let mut state = std::mem::replace(&mut self.map_scripts, State::new(Vec::new()));
        let definitions = std::mem::take(&mut state.scripts);
        let group = definitions
            .iter()
            .filter_map(|script| script.group.as_ref())
            .find(|group| group.name == name);
        let indices: Vec<_> = if let Some(group) = group {
            if group.subroutine && state.groups.get(name).copied().unwrap_or(false) {
                definitions
                    .iter()
                    .enumerate()
                    .filter_map(|(index, script)| {
                        script
                            .group
                            .as_ref()
                            .is_some_and(|group| group.name == name)
                            .then_some(index)
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            definitions
                .iter()
                .enumerate()
                .find_map(|(index, script)| {
                    (script.name == name && script.subroutine).then_some(index)
                })
                .into_iter()
                .collect()
        };
        for index in indices {
            self.execute_map_script(&mut state, &definitions, index);
        }
        state.scripts = definitions;
        self.map_scripts = state;
        if let Some(previous) = previous {
            self.map_objects.insert("<This Object>".into(), previous);
        } else {
            self.map_objects.remove("<This Object>");
        }
    }

    fn map_script_player(&self, state: &State, name: &str) -> Option<usize> {
        match name {
            "<Local Player>" => Some(0),
            "<This Player>" => state.current_player,
            _ => self.map_side_state.owners.get(name).copied(),
        }
    }

    fn execute_map_script(&mut self, state: &mut State, definitions: &[Script], index: usize) {
        let script = &definitions[index];
        if !state.active[index] || !script.difficulty[1] || self.tick < state.next_evaluation[index]
        {
            return;
        }
        if script.delay_frames > 0 {
            state.next_evaluation[index] = self.tick + u64::from(script.delay_frames);
        }
        let matched = script
            .conditions
            .iter()
            .enumerate()
            .any(|(branch_index, branch)| {
                !branch.is_empty()
                    && branch
                        .iter()
                        .enumerate()
                        .all(|(condition_index, condition)| match condition {
                            Condition::True => true,
                            Condition::False => false,
                            Condition::PlayerUnitsInArea(name, area) => {
                                let key = (index, branch_index, condition_index);
                                let owner = if let Some(owner) = state.condition_players.get(&key) {
                                    *owner
                                } else {
                                    let owner = self.map_script_player(state, name);
                                    state.condition_players.insert(key, owner);
                                    owner
                                };
                                teams::side_units_in_area(self, owner, area)
                            }
                            Condition::ScienceAcquired(player, science) => {
                                self.map_script_player(state, player).is_some_and(|owner| {
                                    let pending = &mut self.players[owner].acquired_sciences;
                                    pending.iter().position(|name| name == science).is_some_and(
                                        |index| {
                                            pending.remove(index);
                                            true
                                        },
                                    )
                                })
                            }
                            Condition::NamedDestroyed(name) => {
                                self.named_map_object_destroyed(name)
                            }
                            Condition::TimerExpired(name) => state
                                .counters
                                .get(name)
                                .is_some_and(|counter| counter.countdown && counter.value < 1),
                            Condition::Counter(name, comparison, value) => {
                                let current =
                                    state.counters.get(name).map_or(0, |counter| counter.value);
                                match comparison {
                                    0 => current < *value,
                                    1 => current <= *value,
                                    2 => current == *value,
                                    3 => current >= *value,
                                    4 => current > *value,
                                    5 => current != *value,
                                    _ => unreachable!("validated counter comparison"),
                                }
                            }
                        })
            });
        let actions = if matched {
            &script.actions
        } else {
            &script.false_actions
        };
        for action in actions {
            self.execute_map_action(state, definitions, action);
        }
        if script.one_shot && (matched || !actions.is_empty()) {
            state.active[index] = false;
        }
    }
}

fn native_real(value: u32) -> f32 {
    let bytes = value.to_le_bytes();
    f32::from(u16::from_le_bytes([bytes[2], bytes[3]])).mul_add(
        65_536.0,
        f32::from(u16::from_le_bytes([bytes[0], bytes[1]])),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::Kind;
    use num_traits::ToPrimitive;

    fn script(conditions: Vec<Vec<Condition>>, actions: Vec<Action>) -> Script {
        Script {
            owner: String::new(),
            name: "Original map callback".into(),
            active: true,
            one_shot: true,
            difficulty: [true; 3],
            delay_frames: 0,
            subroutine: false,
            group: None,
            conditions,
            actions,
            false_actions: Vec::new(),
        }
    }

    #[test]
    fn acquired_science_condition_consumes_one_notification_in_script_order() {
        let mut game = crate::skirmish::tests::game();
        let condition = || {
            vec![vec![Condition::ScienceAcquired(
                "<Local Player>".into(),
                "SCIENCE_MOAB".into(),
            )]]
        };
        let first = script(condition(), vec![Action::DrawIconUi(false)]);
        let mut second = script(condition(), vec![Action::DrawIconUi(true)]);
        second.name = "Second science consumer".into();
        game.install_map_scripts(vec![first, second]);
        game.grant_native_science(0, "SCIENCE_MOAB");
        game.update_map_scripts();
        assert!(!game.draw_icon_ui);
        assert!(game.players[0].acquired_sciences.is_empty());
        game.grant_native_science(0, "SCIENCE_MOAB");
        game.update_map_scripts();
        assert!(!game.draw_icon_ui);
    }

    #[test]
    fn icon_ui_action_runs_in_script_order_and_resets_for_next_match() {
        let mut game = crate::skirmish::tests::game();
        game.install_map_scripts(vec![script(
            vec![vec![Condition::True]],
            vec![Action::DrawIconUi(false)],
        )]);
        assert!(game.draw_icon_ui);
        game.update_map_scripts();
        assert!(!game.draw_icon_ui);
        game.reset(crate::skirmish::Faction::Gla, crate::skirmish::Faction::Gla)
            .unwrap();
        assert!(game.draw_icon_ui);
    }

    #[test]
    fn player_area_condition_resolves_current_and_local_players_before_named_lookup() {
        let mut game = crate::skirmish::tests::game();
        let actor = game.spawn(6, 1, [900.0, 900.0], true);
        assert_eq!(
            game.team_membership.team_name(
                game.team_membership
                    .team_of(actor)
                    .expect("object native team")
            ),
            "teamplayer1"
        );
        let area = || super::super::AiArea {
            name: "Owner one unit".into(),
            points: vec![
                [890.0, 890.0],
                [910.0, 890.0],
                [910.0, 910.0],
                [890.0, 910.0],
            ],
        };
        let mut current = script(
            vec![vec![Condition::PlayerUnitsInArea(
                "<This Player>".into(),
                area(),
            )]],
            vec![Action::SetCounter("Current matched".into(), 1)],
        );
        current.owner = "player1".into();
        let mut local = script(
            vec![vec![Condition::PlayerUnitsInArea(
                "<Local Player>".into(),
                area(),
            )]],
            vec![Action::SetCounter("Local matched".into(), 1)],
        );
        local.owner = "player1".into();
        local.name = "Local area condition".into();
        game.install_map_scripts(vec![current, local]);
        game.update_map_scripts();
        assert_eq!(game.map_scripts.counters["Current matched"].value, 1);
        assert!(!game.map_scripts.counters.contains_key("Local matched"));
    }

    #[test]
    fn attached_player_area_condition_caches_first_context_and_restart_clears_it() {
        let mut game = crate::skirmish::tests::game();
        let actor = game.spawn(6, 1, [900.0, 900.0], true);
        let mut attached = script(
            vec![vec![Condition::PlayerUnitsInArea(
                "<This Player>".into(),
                super::super::AiArea {
                    name: "Owner one unit".into(),
                    points: vec![
                        [890.0, 890.0],
                        [910.0, 890.0],
                        [910.0, 910.0],
                        [890.0, 910.0],
                    ],
                },
            )]],
            vec![Action::SetCounter("Cached owner matched".into(), 1)],
        );
        attached.subroutine = true;
        attached.one_shot = false;
        game.install_map_scripts(vec![attached]);
        game.map_scripts.current_player = Some(1);
        game.run_attached_map_script("Original map callback", actor);
        assert_eq!(game.map_scripts.counters["Cached owner matched"].value, 1);
        assert_eq!(game.map_scripts.current_player, Some(1));
        game.map_scripts.current_player = Some(0);
        game.map_scripts.counters.remove("Cached owner matched");
        game.run_attached_map_script("Original map callback", actor);
        assert_eq!(game.map_scripts.counters["Cached owner matched"].value, 1);
        assert_eq!(game.map_scripts.current_player, Some(0));
        game.map_scripts.reset(&mut game.random);
        game.map_scripts.current_player = Some(0);
        game.run_attached_map_script("Original map callback", actor);
        assert!(
            !game
                .map_scripts
                .counters
                .contains_key("Cached owner matched")
        );
    }

    #[test]
    fn rain_timer_stop_counter_script_enable_and_flash_follow_native_order_and_rng() {
        let mut game = crate::skirmish::tests::game();
        let mut waiting = script(
            vec![vec![Condition::TimerExpired("Lightning".into())]],
            vec![
                Action::RandomTimer("Flash_Times".into(), 4, 4),
                Action::StopTimer("Flash_Times".into()),
                Action::EnableScript("Reset Lightning".into(), true),
            ],
        );
        waiting.name = "Time Between Lightning".into();
        waiting.active = false;
        let mut reset = script(
            vec![vec![Condition::True]],
            vec![
                Action::RandomSecondsTimer("Lightning".into(), 15.0, 30.0),
                Action::EnableScript("Time Between Lightning".into(), true),
            ],
        );
        reset.name = "Reset Lightning".into();
        reset.active = false;
        let start = script(
            vec![vec![Condition::True]],
            vec![
                Action::RandomSecondsTimer("Lightning".into(), 10.0, 20.0),
                Action::EnableScript("Time Between Lightning".into(), true),
            ],
        );
        game.install_map_scripts(vec![waiting, reset, start]);
        let mut random = game.random;
        let initial = startup::random_real(&mut random, 10.0, 20.0);
        let frames = ((initial * 1000.0) * (30.0 / 1000.0))
            .ceil()
            .to_i32()
            .unwrap();
        game.update_map_scripts();
        assert_eq!(game.map_scripts.counters["Lightning"].value, frames);
        assert_eq!(game.random, random);
        for _ in 0..frames - 1 {
            game.update_map_scripts();
        }
        assert!(!game.map_scripts.active[1]);
        let next = startup::random_real(&mut random, 15.0, 30.0);
        game.update_map_scripts();
        assert_eq!(game.map_scripts.counters["Flash_Times"].value, 4);
        assert!(!game.map_scripts.counters["Flash_Times"].countdown);
        assert_eq!(
            game.map_scripts.counters["Lightning"].value,
            ((next * 1000.0) * (30.0 / 1000.0)).ceil().to_i32().unwrap()
        );
        assert_eq!(game.random, random);
        let flash = script(
            vec![vec![Condition::Counter("Flash_Times".into(), 4, 0)]],
            vec![
                Action::FadeAdd(0.6, 0.0, [1, 2, 2]),
                Action::Sound("ThunderStrikesStereo".into()),
            ],
        );
        game.install_map_scripts(vec![flash]);
        game.map_scripts.counters.insert(
            "Flash_Times".into(),
            Counter {
                value: 1,
                countdown: false,
            },
        );
        game.update_map_scripts();
        assert_eq!(
            game.map_script_fade().unwrap().amount.to_bits(),
            0.6_f32.to_bits()
        );
        assert!(
            matches!(&game.take_map_script_audio()[0], AudioAction::Play(name) if name == "ThunderStrikesStereo")
        );
        game.update_map_scripts();
        assert_eq!(
            game.map_script_fade().unwrap().amount.to_bits(),
            0.0_f32.to_bits()
        );
        assert!(game.take_map_script_audio().is_empty());
        for _ in 0..5 {
            game.update_map_scripts();
        }
        assert!(game.map_script_fade().is_none());
    }

    #[test]
    fn attached_startup_audio_survives_logic_until_consumer_drains_it() {
        let mut game = crate::skirmish::tests::game();
        let mut attached = script(
            vec![vec![Condition::True]],
            vec![Action::Sound("Startup".into())],
        );
        attached.name = "Attached".into();
        attached.subroutine = true;
        let periodic = script(
            vec![vec![Condition::True]],
            vec![Action::AudioVolume("Rain".into(), 0.4)],
        );
        game.install_map_scripts(vec![attached, periodic]);
        game.run_attached_map_script("Attached", 0);
        game.update_map_scripts();
        game.update_map_scripts();
        let audio = game.take_map_script_audio();
        assert_eq!(audio.len(), 2);
        assert!(matches!(&audio[0], AudioAction::Play(name) if name == "Startup"));
        assert!(
            matches!(&audio[1], AudioAction::OverrideVolume(name, volume)
            if name == "Rain" && volume.to_bits() == 0.4_f32.to_bits())
        );
        assert!(game.take_map_script_audio().is_empty());
    }

    #[test]
    fn dormant_script_startup_draws_precede_active_periodic_callbacks() {
        let mut game = crate::skirmish::tests::game();
        let mut dormant = script(vec![vec![Condition::True]], Vec::new());
        dormant.active = false;
        dormant.delay_frames = 90;
        let mut periodic = script(
            vec![vec![Condition::True]],
            vec![Action::WarehouseValue(0, 151)],
        );
        periodic.one_shot = false;
        periodic.delay_frames = 30;
        let mut expected_random = game.random;
        let dormant_start = u64::from(startup::random_between(&mut expected_random, 60));
        let active_start = u64::from(startup::random_between(&mut expected_random, 60));
        game.install_map_scripts(vec![dormant, periodic]);
        assert_eq!(game.random, expected_random);
        assert_eq!(
            game.map_scripts.next_evaluation,
            [dormant_start, active_start]
        );
        game.supplies[0].boxes = 7;
        for frame in 0..active_start {
            game.tick = frame;
            game.update_map_scripts();
            assert_eq!(game.supplies[0].boxes, 7);
        }
        game.tick = active_start;
        game.update_map_scripts();
        assert_eq!(game.supplies[0].boxes, 3);
        game.supplies[0].boxes = 6;
        game.tick = active_start + 29;
        game.update_map_scripts();
        assert_eq!(game.supplies[0].boxes, 6);
        game.tick = active_start + 30;
        game.update_map_scripts();
        assert_eq!(game.supplies[0].boxes, 3);
        assert_eq!(game.random, expected_random);
        game.map_scripts.reset(&mut game.random);
        assert!(game.map_scripts.active[1]);
        assert!(!game.map_scripts.active[0]);
        let expected_dormant = u64::from(startup::random_between(&mut expected_random, 60));
        let expected_active = u64::from(startup::random_between(&mut expected_random, 60));
        assert_eq!(
            game.map_scripts.next_evaluation,
            [expected_dormant, expected_active]
        );
        assert_eq!(game.random, expected_random);
    }

    #[test]
    fn named_death_actions_execute_in_authored_order_once_without_a_killer() {
        let mut game = crate::skirmish::tests::game();
        let actor = game
            .entities
            .iter()
            .position(|entity| {
                entity.owner == 0 && game.catalog[entity.type_index].kind == Kind::Builder
            })
            .unwrap();
        let target = game
            .entities
            .iter()
            .position(|entity| {
                entity.owner == 1 && game.catalog[entity.type_index].kind == Kind::CommandCenter
            })
            .unwrap();
        game.map_objects.insert("Gate".into(), actor);
        game.map_objects.insert("Neighbor".into(), target);
        game.install_map_scripts(vec![
            script(
                vec![vec![Condition::NamedDestroyed("Never existed".into())]],
                vec![Action::NamedKill("Neighbor".into())],
            ),
            script(
                vec![vec![Condition::True]],
                vec![Action::NamedKill("Gate".into())],
            ),
            script(
                vec![vec![Condition::NamedDestroyed("Gate".into())]],
                vec![Action::NamedDamage("Neighbor".into(), 600)],
            ),
        ]);
        let health = game.entities[target].health;
        game.step();
        assert!(!game.entities[actor].alive());
        assert_eq!(
            game.entities[target].health.to_bits(),
            (health - 600.0).to_bits()
        );
        assert!(game.entities[target].damage_state.last_attacker.is_none());
        game.step();
        assert_eq!(
            game.entities[target].health.to_bits(),
            (health - 600.0).to_bits()
        );
        assert!(game.map_scripts.active[0]);
        assert!(!game.map_scripts.active[1]);
        assert!(!game.map_scripts.active[2]);
    }

    #[test]
    fn zero_delay_warehouse_partial_box_else_one_shot_and_restart_follow_source() {
        let mut game = crate::skirmish::tests::game();
        let mut repeating = script(vec![vec![Condition::False]], Vec::new());
        repeating.one_shot = false;
        repeating.false_actions = vec![Action::WarehouseValue(0, 151)];
        let mut one_shot_else = script(vec![vec![Condition::False]], Vec::new());
        one_shot_else.false_actions = vec![Action::WarehouseValue(1, 0)];
        game.install_map_scripts(vec![repeating, one_shot_else]);
        game.step();
        assert_eq!(game.supplies[0].boxes, 3);
        assert_eq!(game.supplies[1].boxes, 0);
        assert!(!game.map_scripts.active[1]);
        game.supplies[0].boxes = 2;
        game.step();
        assert_eq!(game.supplies[0].boxes, 3);
        game.reset(super::super::Faction::Gla, super::super::Faction::Gla)
            .unwrap();
        assert!(game.map_scripts.active[1]);
        game.step();
        assert_eq!(game.supplies[0].boxes, 3);
        assert_eq!(game.supplies[1].boxes, 0);
    }
}
