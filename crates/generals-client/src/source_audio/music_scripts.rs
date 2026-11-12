//! Original skirmish result and civilian music scripts in native insertion order.
mod verification;
use std::{collections::BTreeMap, error::Error};
pub use verification::verify;

use generals_assets::{
    scripts::{Script, ScriptBundle, ScriptCall},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{Faction, Skirmish};

pub(super) enum Change {
    Track { name: String, fade: bool },
    Speech { name: String, allow_overlap: bool },
    Result(crate::playtest_ui::ResultAction),
}

struct ResultState {
    defeated: bool,
    ended: bool,
    victorious: bool,
}

pub(super) struct Program {
    rules: Vec<Rule>,
    active: Vec<bool>,
    counters: BTreeMap<String, Counter>,
    object_counts: BTreeMap<String, i32>,
}

struct Rule {
    competitive: bool,
    one_shot: bool,
    conditions: Vec<Vec<Condition>>,
    actions: Vec<Action>,
    otherwise: Vec<Action>,
}

enum Condition {
    True,
    Faction(String),
    Counter(String, i32, i32),
    Timer(String),
    Completed(String, i32),
    Lost(String),
    LocalDefeat,
    AlliedDefeat,
    AlliedVictory,
}

enum Action {
    Counter(String, i32),
    Timer(String, i32),
    Track(String, bool),
    Speech(String, bool),
    Result(crate::playtest_ui::ResultAction),
}

struct Counter {
    value: i32,
    countdown: bool,
}

pub(super) fn load(assets: &VirtualAssets) -> Result<Program, Box<dyn Error>> {
    let mut bundle = ScriptBundle::parse(&assets.read("data/scripts/skirmishscripts.scb")?.bytes)?;
    let index = bundle
        .players
        .iter()
        .position(|player| player.name == "PlyrCivilian")
        .ok_or("SkirmishScripts has no civilian music scripts")?;
    let list = &mut bundle.lists[index];
    let mut scripts = std::mem::take(&mut list.scripts);
    for group in std::mem::take(&mut list.groups) {
        if group.active && !group.subroutine {
            scripts.extend(group.scripts);
        }
    }
    // These authored scripts never enable other scripts. Preserve disabled event
    // branches by leaving them disabled, including the superweapon music hooks.
    let civilian = scripts
        .into_iter()
        .filter(|script| script.active && !script.subroutine)
        .map(|script| rule(script, false))
        .collect::<Result<Vec<_>, _>>()?;
    let mut result =
        ScriptBundle::parse(&assets.read("data/scripts/multiplayerscripts.scb")?.bytes)?;
    let list = result
        .lists
        .first_mut()
        .ok_or("result scripts have no list")?;
    if !list.groups.is_empty() {
        return Err("grouped result scripts are unrepresented".into());
    }
    // GameLogic duplicates the first list with addScript(script, 0): reverse
    // its stored order and prepend it before the existing civilian scripts.
    let mut rules = std::mem::take(&mut list.scripts)
        .into_iter()
        .rev()
        .filter(|script| script.active && !script.subroutine)
        .map(|script| rule(script, true))
        .collect::<Result<Vec<_>, _>>()?;
    rules.extend(civilian);
    Ok(Program {
        active: vec![true; rules.len()],
        rules,
        counters: BTreeMap::new(),
        object_counts: BTreeMap::new(),
    })
}

fn rule(script: Script, competitive: bool) -> Result<Rule, Box<dyn Error>> {
    if script.delay_seconds != 0 || script.difficulty != [true; 3] {
        return Err(format!(
            "{}: unsupported music script interval/difficulty",
            script.name
        )
        .into());
    }
    Ok(Rule {
        competitive,
        one_shot: script.one_shot,
        conditions: script
            .conditions
            .into_iter()
            .map(|branch| branch.iter().map(condition).collect())
            .collect::<Result<_, _>>()?,
        actions: script
            .actions
            .iter()
            .map(action)
            .collect::<Result<_, _>>()?,
        otherwise: script
            .false_actions
            .iter()
            .map(action)
            .collect::<Result<_, _>>()?,
    })
}

fn condition(call: &ScriptCall) -> Result<Condition, Box<dyn Error>> {
    let p = &call.parameters;
    let name = call
        .name
        .as_deref()
        .ok_or("unnamed music script condition")?;
    let text = |index: usize| {
        p.get(index)
            .map(|value| value.text.clone())
            .ok_or("missing music condition parameter")
    };
    let integer = |index: usize| {
        p.get(index)
            .map(|value| value.integer)
            .ok_or("missing music condition parameter")
    };
    Ok(match name {
        "CONDITION_TRUE" => Condition::True,
        "MULTIPLAYER_PLAYER_DEFEAT" => Condition::LocalDefeat,
        "MULTIPLAYER_ALLIED_DEFEAT" => Condition::AlliedDefeat,
        "MULTIPLAYER_ALLIED_VICTORY" => Condition::AlliedVictory,
        "SKIRMISH_PLAYER_FACTION" | "PLAYER_LOST_OBJECT_TYPE" => {
            if text(0)? != "<Local Player>" {
                return Err("music condition references a nonlocal player".into());
            }
            if name == "SKIRMISH_PLAYER_FACTION" {
                Condition::Faction(text(1)?)
            } else {
                Condition::Lost(text(1)?)
            }
        }
        "COUNTER" => Condition::Counter(text(0)?, integer(1)?, integer(2)?),
        "TIMER_EXPIRED" => Condition::Timer(text(0)?),
        "MUSIC_TRACK_HAS_COMPLETED" => Condition::Completed(text(0)?, integer(1)?),
        _ => return Err(format!("unsupported active civilian music condition {name}").into()),
    })
}

fn action(call: &ScriptCall) -> Result<Action, Box<dyn Error>> {
    let p = &call.parameters;
    let name = call.name.as_deref().ok_or("unnamed music script action")?;
    let result = match name {
        "VICTORY" => Some(crate::playtest_ui::ResultAction::Victory),
        "DEFEAT" => Some(crate::playtest_ui::ResultAction::Defeat),
        "LOCALDEFEAT" => Some(crate::playtest_ui::ResultAction::LocalDefeat),
        _ => None,
    };
    if let Some(result) = result {
        if !p.is_empty() {
            return Err("result window action has unexpected parameters".into());
        }
        return Ok(Action::Result(result));
    }
    let key = p.first().ok_or("missing music action name")?.text.clone();
    let value = p.get(1).ok_or("missing music action value")?;
    Ok(match name {
        "SET_COUNTER" => Action::Counter(key, value.integer),
        "SPEECH_PLAY" => Action::Speech(key, value.integer != 0),
        "SET_MILLISECOND_TIMER" => {
            // ScriptEngine::setTimer stores seconds in the editor's REAL slot.
            let frames = ((value.real * 1000.0) * (30.0 / 1000.0)).ceil();
            if !frames.is_finite() || !(0.0..=16_777_216.0).contains(&frames) {
                return Err("music timer outside exact integer range".into());
            }
            Action::Timer(key, format!("{frames:.0}").parse()?)
        }
        "MUSIC_SET_TRACK" => {
            p.get(2).ok_or("missing source music fade-in flag")?;
            Action::Track(key, value.integer != 0)
        }
        _ => return Err(format!("unsupported active civilian music action {name}").into()),
    })
}

impl Program {
    pub fn tracks(&self) -> impl Iterator<Item = &str> {
        self.rules
            .iter()
            .flat_map(|rule| rule.actions.iter().chain(&rule.otherwise))
            .filter_map(|action| match action {
                Action::Track(name, _) => Some(name.as_str()),
                _ => None,
            })
    }

    pub fn speeches(&self) -> impl Iterator<Item = &str> {
        self.rules
            .iter()
            .flat_map(|rule| rule.actions.iter().chain(&rule.otherwise))
            .filter_map(|action| match action {
                Action::Speech(name, _) => Some(name.as_str()),
                _ => None,
            })
    }

    pub fn reset(&mut self) {
        self.active.fill(true);
        self.counters.clear();
        self.object_counts.clear();
    }

    pub fn advance(&mut self, game: &Skirmish, completed: Option<(&str, u32)>) -> Vec<Change> {
        let faction = match game.players[0].faction {
            Faction::Usa => "America",
            Faction::China => "China",
            Faction::Gla => "GLA",
            Faction::Civilian => unreachable!("Neutral cannot own the local music player"),
        };
        let result = (0..game.slots.len())
            .any(|owner| game.enemies(0, owner))
            .then_some(ResultState {
                defeated: game.defeated[0],
                ended: game.ended_at.is_some(),
                victorious: game.victorious(0),
            });
        self.evaluate(
            faction,
            completed,
            |name| {
                i32::try_from(
                    game.entities
                        .iter()
                        .filter(|entity| {
                            entity.owner == 0
                                && entity.complete()
                                && game.catalog[entity.type_index].name == name
                        })
                        .count(),
                )
                .unwrap_or(i32::MAX)
            },
            result.as_ref(),
        )
    }

    fn evaluate(
        &mut self,
        faction: &str,
        completed: Option<(&str, u32)>,
        count: impl Fn(&str) -> i32,
        result: Option<&ResultState>,
    ) -> Vec<Change> {
        for counter in self.counters.values_mut() {
            if counter.countdown && counter.value >= 0 {
                counter.value -= 1;
            }
        }
        let mut changes = Vec::new();
        for (index, rule) in self.rules.iter().enumerate() {
            if !self.active[index] || (rule.competitive && result.is_none()) {
                continue;
            }
            // Preserve OR/AND short circuiting: Lost updates its count only when evaluated.
            let accepted = rule.conditions.iter().any(|branch| {
                !branch.is_empty()
                    && branch.iter().all(|condition| {
                        matches_condition(
                            condition,
                            faction,
                            completed,
                            &self.counters,
                            &mut self.object_counts,
                            &count,
                            result,
                        )
                    })
            });
            let actions = if accepted {
                &rule.actions
            } else {
                &rule.otherwise
            };
            for action in actions {
                match action {
                    Action::Counter(name, value) => {
                        self.counters
                            .entry(name.clone())
                            .or_insert(Counter {
                                value: 0,
                                countdown: false,
                            })
                            .value = *value;
                    }
                    Action::Timer(name, value) => {
                        self.counters.insert(
                            name.clone(),
                            Counter {
                                value: *value,
                                countdown: true,
                            },
                        );
                    }
                    Action::Track(name, fade) => changes.push(Change::Track {
                        name: name.clone(),
                        fade: *fade,
                    }),
                    Action::Speech(name, allow_overlap) => changes.push(Change::Speech {
                        name: name.clone(),
                        allow_overlap: *allow_overlap,
                    }),
                    Action::Result(action) => changes.push(Change::Result(*action)),
                }
            }
            if rule.one_shot && (accepted || !actions.is_empty()) {
                self.active[index] = false;
            }
        }
        changes
    }
}

fn matches_condition(
    condition: &Condition,
    faction: &str,
    completed: Option<(&str, u32)>,
    counters: &BTreeMap<String, Counter>,
    counts: &mut BTreeMap<String, i32>,
    count: &impl Fn(&str) -> i32,
    result: Option<&ResultState>,
) -> bool {
    match condition {
        Condition::True => true,
        Condition::LocalDefeat => result.is_some_and(|state| state.defeated && !state.ended),
        Condition::AlliedDefeat => result.is_some_and(|state| state.defeated && state.ended),
        Condition::AlliedVictory => result.is_some_and(|state| state.victorious),
        Condition::Faction(name) => name == faction,
        Condition::Timer(name) => counters
            .get(name)
            .is_some_and(|counter| counter.countdown && counter.value < 1),
        Condition::Completed(name, times) => completed.is_some_and(|(playing, loops)| {
            playing == name && i64::from(loops) >= i64::from(*times)
        }),
        Condition::Counter(name, op, value) => {
            let actual = counters.get(name).map_or(0, |counter| counter.value);
            match op {
                0 => actual < *value,
                1 => actual <= *value,
                2 => actual == *value,
                3 => actual >= *value,
                4 => actual > *value,
                5 => actual != *value,
                _ => false,
            }
        }
        Condition::Lost(name) => {
            let actual = count(name);
            let prior = counts.insert(name.clone(), actual).unwrap_or(0);
            actual < prior
        }
    }
}

#[cfg(test)]
impl Program {
    pub(super) fn test_fixture() -> Self {
        Self {
            rules: Vec::new(),
            active: Vec::new(),
            counters: BTreeMap::new(),
            object_counts: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Change, Condition, Program, Rule};
    use std::collections::BTreeMap;

    fn program(rules: Vec<Rule>) -> Program {
        Program {
            active: vec![true; rules.len()],
            rules,
            counters: BTreeMap::new(),
            object_counts: BTreeMap::new(),
        }
    }

    #[test]
    fn timer_waits_full_native_interval_and_completion_keeps_the_next_timer() {
        let mut scripts = program(vec![
            Rule {
                competitive: false,
                one_shot: true,
                conditions: vec![vec![Condition::Faction("America".into())]],
                actions: vec![Action::Timer("start".into(), 300)],
                otherwise: Vec::new(),
            },
            Rule {
                competitive: false,
                one_shot: true,
                conditions: vec![vec![Condition::Timer("start".into())]],
                actions: vec![
                    Action::Track("Game_USA_01".into(), true),
                    Action::Timer("next".into(), 6_300),
                ],
                otherwise: Vec::new(),
            },
            Rule {
                competitive: false,
                one_shot: false,
                conditions: vec![vec![Condition::Completed("Game_USA_01".into(), 1)]],
                actions: vec![Action::Track("Game_Silent".into(), true)],
                otherwise: Vec::new(),
            },
        ]);
        for _ in 0..300 {
            assert!(scripts.evaluate("America", None, |_| 0, None).is_empty());
        }
        let change = scripts.evaluate("America", None, |_| 0, None);
        assert_eq!(change.len(), 1);
        assert!(matches!(&change[0], Change::Track {name, ..} if name == "Game_USA_01"));
        assert!(
            scripts
                .evaluate("America", Some(("Game_USA_01", 0)), |_| 0, None)
                .is_empty()
        );
        let change = scripts.evaluate("America", Some(("Game_USA_01", 1)), |_| 0, None);
        assert!(matches!(&change[0], Change::Track {name, fade: true} if name == "Game_Silent"));
        assert_eq!(scripts.counters["next"].value, 6_298);
        scripts.reset();
        assert!(scripts.evaluate("America", None, |_| 0, None).is_empty());
        assert_eq!(scripts.counters["start"].value, 300);
    }

    #[test]
    fn lost_building_condition_consumes_counts_only_when_its_branch_runs() {
        let mut scripts = program(vec![Rule {
            competitive: false,
            one_shot: false,
            conditions: vec![vec![
                Condition::Faction("China".into()),
                Condition::Lost("ChinaSupplyCenter".into()),
            ]],
            actions: vec![Action::Track("End_Chi_Failure".into(), true)],
            otherwise: Vec::new(),
        }]);
        assert!(scripts.evaluate("America", None, |_| 5, None).is_empty());
        assert!(scripts.object_counts.is_empty());
        assert!(scripts.evaluate("China", None, |_| 2, None).is_empty());
        let change = scripts.evaluate("China", None, |_| 1, None);
        assert!(matches!(&change[0], Change::Track {name, ..} if name == "End_Chi_Failure"));
        assert!(scripts.evaluate("China", None, |_| 1, None).is_empty());
    }

    #[test]
    fn local_defeat_waits_for_global_outcome_and_sandbox_omits_result_scripts() {
        use super::ResultState;
        use crate::playtest_ui::ResultAction;
        let mut scripts = program(
            [
                (Condition::LocalDefeat, ResultAction::LocalDefeat),
                (Condition::AlliedDefeat, ResultAction::Defeat),
                (Condition::AlliedVictory, ResultAction::Victory),
            ]
            .into_iter()
            .map(|(condition, result)| Rule {
                competitive: true,
                one_shot: true,
                conditions: vec![vec![condition]],
                actions: vec![Action::Result(result)],
                otherwise: Vec::new(),
            })
            .collect(),
        );
        assert!(scripts.evaluate("America", None, |_| 0, None).is_empty());
        let mut state = ResultState {
            defeated: true,
            ended: false,
            victorious: false,
        };
        let changes = scripts.evaluate("America", None, |_| 0, Some(&state));
        assert!(matches!(
            changes.as_slice(),
            [Change::Result(ResultAction::LocalDefeat)]
        ));
        assert!(
            scripts
                .evaluate("America", None, |_| 0, Some(&state))
                .is_empty()
        );
        // A defeated ally can share the latched victory. Native predicates are
        // independent, so both final rules execute in authored insertion order.
        state.ended = true;
        state.victorious = true;
        let changes = scripts.evaluate("America", None, |_| 0, Some(&state));
        assert!(matches!(
            changes.as_slice(),
            [
                Change::Result(ResultAction::Defeat),
                Change::Result(ResultAction::Victory)
            ]
        ));
        scripts.reset();
        assert!(scripts.evaluate("America", None, |_| 0, None).is_empty());
    }
}
