//! Original sabotage/hijack feedback and their victim EVA checks, SH 2bfaea60.
use super::{
    SourceAudio,
    data::{ST_PLAYER, ST_SHROUDED},
};
use crate::{
    playtest::Match,
    playtest_view,
    skirmish_data::{blocks, definition, duration_frames, field, scalar},
};
use bevy::prelude::Vec3;
use generals_assets::{
    content::{self, ContentLine},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{Faction, sabotage::Effect};
use std::{collections::BTreeSet, error::Error, path::Path};

struct Eva {
    sounds: [Vec<String>; 3],
    priority: u32,
    checks: u32,
    expires: u32,
}
struct Check {
    kind: usize,
    triggered: u64,
    next: u64,
    played: bool,
}
const EVA_NAMES: [&str; 29] = [
    "CashStolen",
    "BuildingSabotaged",
    "VehicleStolen",
    "BuildingStolen",
    "BuildingBeingStolen",
    "SuperweaponReady_Own_ParticleCannon",
    "SuperweaponReady_Own_Nuke",
    "SuperweaponReady_Own_ScudStorm",
    "SuperweaponReady_Ally_ParticleCannon",
    "SuperweaponReady_Ally_Nuke",
    "SuperweaponReady_Ally_ScudStorm",
    "SuperweaponReady_Enemy_ParticleCannon",
    "SuperweaponReady_Enemy_Nuke",
    "SuperweaponReady_Enemy_ScudStorm",
    "SuperweaponLaunched_Own_ParticleCannon",
    "SuperweaponLaunched_Own_Nuke",
    "SuperweaponLaunched_Own_ScudStorm",
    "SuperweaponLaunched_Ally_ParticleCannon",
    "SuperweaponLaunched_Ally_Nuke",
    "SuperweaponLaunched_Ally_ScudStorm",
    "SuperweaponLaunched_Enemy_ParticleCannon",
    "SuperweaponLaunched_Enemy_Nuke",
    "SuperweaponLaunched_Enemy_ScudStorm",
    "SuperweaponLaunched_Own_GPS_Scrambler",
    "SuperweaponLaunched_Ally_GPS_Scrambler",
    "SuperweaponLaunched_Enemy_GPS_Scrambler",
    "SuperweaponLaunched_Own_Sneak_Attack",
    "SuperweaponLaunched_Ally_Sneak_Attack",
    "SuperweaponLaunched_Enemy_Sneak_Attack",
];
pub(super) struct State {
    sounds: [String; 5],
    eva: [Option<Eva>; EVA_NAMES.len()],
    money_volume: f32,
    seen: BTreeSet<(u64, usize)>,
    checks: Vec<Check>,
    should_play: [bool; EVA_NAMES.len()],
    frame: Option<u64>,
}
impl State {
    pub(super) fn load(assets: &VirtualAssets, options: &Path) -> Result<Self, Box<dyn Error>> {
        let misc = [content::scan(assets.read("data/ini/miscaudio.ini")?)?];
        let lines = &definition(&misc, "MiscAudio", "")?.lines;
        let mut sounds = std::array::from_fn(|_| String::new());
        for (sound, key) in sounds.iter_mut().zip([
            "MoneyWithdrawSound",
            "SabotageResetTimeBuilding",
            "SabotageShutDownBuilding",
            "MoneyDepositSound",
            "RadarInfiltrationSound",
        ]) {
            *sound = field(lines, 0, key)
                .map(scalar)
                .transpose()?
                .unwrap_or_default()
                .into();
        }
        let files = [content::scan(assets.read("data/ini/eva.ini")?)?];
        let eva = EVA_NAMES
            .into_iter()
            .map(|name| {
                if files[0]
                    .declarations
                    .iter()
                    .any(|event| event.kind == "EvaEvent" && event.name == name)
                {
                    load_eva(&definition(&files, "EvaEvent", name)?.lines).map(Some)
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap_or_else(|_| unreachable!("loaded original EVA slots"));
        let settings = [content::scan(assets.read("data/ini/audiosettings.ini")?)?];
        let settings = &definition(&settings, "AudioSettings", "")?.lines;
        // AudioSettings defaults this volume to zero for RTS_ZEROHOUR.
        let mut money_volume =
            super::data::percent(settings, "DefaultMoneyTransactionVolume", 0.0)?;
        if options.is_file() {
            for line in std::fs::read_to_string(options)?.lines() {
                if let Some((key, value)) = line.split_once('=')
                    && key.trim() == "MoneyTransactionVolume"
                {
                    money_volume = super::data::signed_number(value.trim())?.max(0.0) / 100.0;
                }
            }
        }
        Ok(Self {
            sounds,
            eva,
            money_volume,
            seen: BTreeSet::new(),
            checks: Vec::new(),
            should_play: [false; EVA_NAMES.len()],
            frame: None,
        })
    }
    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.sounds
            .iter()
            .map(String::as_str)
            .chain(std::iter::once("RadarEvent"))
            .chain(
                self.eva
                    .iter()
                    .flatten()
                    .flat_map(|eva| eva.sounds.iter().flatten().map(String::as_str)),
            )
    }
    pub(super) fn eva_name(&self, name: &str) -> bool {
        self.eva
            .iter()
            .flatten()
            .any(|eva| eva.sounds.iter().flatten().any(|sound| sound == name))
    }
    pub(super) fn reset(&mut self) {
        self.seen.clear();
        self.checks.clear();
        self.should_play = [false; EVA_NAMES.len()];
        self.frame = None;
    }
    fn next_eva(&mut self, frame: u64, playing: bool) -> Option<usize> {
        if frame < 2 || self.frame == Some(frame) {
            return None;
        }
        self.frame = Some(frame);
        // Native EVA enum order, retaining the existing storage indices.
        for kind in (14..23).chain(5..14).chain([2, 3, 0, 4, 1]).chain(23..29) {
            if self.should_play[kind]
                && let Some(eva) = &self.eva[kind]
                && !self.checks.iter().any(|check| check.kind == kind)
            {
                self.checks.push(Check {
                    kind,
                    triggered: frame,
                    next: frame + u64::from(eva.checks),
                    played: false,
                });
            }
        }
        self.should_play = [false; EVA_NAMES.len()];
        self.checks.retain(|check| {
            !(check.played && check.next <= frame + 1
                || !check.played
                    && check.triggered + u64::from(self.eva[check.kind].as_ref().unwrap().expires)
                        <= frame)
        });
        if playing {
            return None;
        }
        let mut chosen = None;
        let mut priority = 0;
        for (index, check) in self.checks.iter().enumerate() {
            let eva = self.eva[check.kind].as_ref().unwrap();
            if !check.played && eva.priority > priority {
                priority = eva.priority;
                chosen = Some(index);
            }
        }
        let check = &mut self.checks[chosen?];
        check.played = true;
        check.next = frame + u64::from(self.eva[check.kind].as_ref().unwrap().checks);
        Some(check.kind)
    }
    #[cfg(test)]
    pub(super) fn fixture() -> Self {
        Self {
            sounds: std::array::from_fn(|_| String::new()),
            eva: std::array::from_fn(|_| {
                Some(Eva {
                    sounds: std::array::from_fn(|_| Vec::new()),
                    priority: 4,
                    checks: 150,
                    expires: 150,
                })
            }),
            money_volume: 0.0,
            seen: BTreeSet::new(),
            checks: Vec::new(),
            should_play: [false; EVA_NAMES.len()],
            frame: None,
        }
    }
}
fn load_eva(lines: &[ContentLine]) -> Result<Eva, Box<dyn Error>> {
    let mut sounds = std::array::from_fn(|_| Vec::new());
    for side in blocks(lines, 0, "SideSounds")? {
        let index = match field(side, 1, "Side").map(scalar).transpose()? {
            Some("America") => 0,
            Some("China") => 1,
            Some("GLA") => 2,
            _ => continue,
        };
        sounds[index] =
            field(side, 1, "Sounds").map_or_else(Vec::new, |line| line.tokens[1..].to_vec());
    }
    Ok(Eva {
        sounds,
        priority: field(lines, 0, "Priority")
            .map_or(Ok(1), |line| -> Result<u32, Box<dyn Error>> {
                Ok(scalar(line)?.parse()?)
            })?,
        checks: field(lines, 0, "TimeBetweenChecksMS").map_or(Ok(900), duration_frames)?,
        expires: field(lines, 0, "ExpirationTimeMS").map_or(Ok(150), duration_frames)?,
    })
}
impl SourceAudio {
    pub(crate) fn ready_superweapon(&mut self, relation: usize, power_type: &str) {
        if let Some(kind) = superweapon_kind(power_type) {
            self.sabotage.should_play[5 + relation * 3 + kind] = true;
        }
    }

    pub(super) fn launch_superweapon(&mut self, relation: usize, power_type: &str) {
        let index = match power_type {
            "SPECIAL_GPS_SCRAMBLER" | "SLTH_SPECIAL_GPS_SCRAMBLER" => 23 + relation,
            "SPECIAL_SNEAK_ATTACK" => 26 + relation,
            name => {
                let Some(kind) = superweapon_kind(name) else {
                    return;
                };
                14 + relation * 3 + kind
            }
        };
        self.sabotage.should_play[index] = true;
    }

    pub(super) fn ability_feedback(
        &mut self,
        event: &generals_sim::skirmish::special_ability::FeedbackEvent,
        viewer: usize,
    ) {
        use generals_sim::skirmish::special_ability::FeedbackEffect;
        if event.victim != viewer {
            return;
        }
        match &event.effect {
            FeedbackEffect::Infiltration {
                building_stolen_warning,
            } => {
                if *building_stolen_warning {
                    self.sabotage.should_play[4] = true;
                }
                self.infiltration_warning();
            }
            FeedbackEffect::BuildingStolen => self.sabotage.should_play[3] = true,
            FeedbackEffect::CashStolen(_) => self.sabotage.should_play[0] = true,
            FeedbackEffect::PowerCashStolen(_) => {
                let name = self.sabotage.sounds[0].clone();
                self.sabotage_sound(
                    &name,
                    None,
                    None,
                    Some(true),
                    true,
                    self.sabotage.money_volume,
                );
            }
            FeedbackEffect::CaptureFlash | FeedbackEffect::DisableParticles { .. } => {}
        }
    }

    pub(crate) fn money_deposit(&mut self, local: bool) {
        let name = self.sabotage.sounds[3].clone();
        self.sabotage_sound(
            &name,
            None,
            None,
            Some(local),
            true,
            self.sabotage.money_volume,
        );
    }

    pub const fn apply_money_transaction_volume(&mut self, volume: f32) {
        self.sabotage.money_volume = volume;
    }

    pub(crate) fn infiltration_warning(&mut self) {
        let sound = self.sabotage.sounds[4].clone();
        self.ui(&sound);
    }

    pub(super) fn update_sabotage(&mut self, battle: &Match, playing: bool) {
        if !battle.started {
            return;
        }
        let game = &battle.simulation;
        let viewed = battle.observer_player.unwrap_or(0);
        self.sabotage.seen.retain(|&(frame, source)| {
            game.presentation
                .sabotage_events
                .iter()
                .any(|event| event.frame == frame && event.source == source)
                || game
                    .presentation
                    .hijacker_events
                    .iter()
                    .any(|event| event.frame == frame && event.source == source)
        });
        for event in &game.presentation.hijacker_events {
            if self.sabotage.seen.insert((event.frame, event.source)) && event.victim == viewed {
                self.sabotage.should_play[2] = true;
            }
        }
        for event in &game.presentation.sabotage_events {
            if !self.sabotage.seen.insert((event.frame, event.source)) {
                continue;
            }
            let source = &game.entities[event.source];
            let target = &game.entities[event.target];
            let effect = &game.catalog[source.type_index].behavior.sabotage[event.module].effect;
            let sound = match effect {
                Effect::FakeBuilding => None,
                Effect::CommandCenter | Effect::Superweapon => Some(1),
                Effect::SupplyCenter { .. } => Some(0),
                _ => Some(2),
            };
            if let Some(index) = sound {
                let name = self.sabotage.sounds[index].clone();
                let point = playtest_view::unit_position(game, event.target) * 10.0;
                self.sabotage_sound(
                    &name,
                    None,
                    Some(point),
                    None,
                    matches!(
                        game.visibility_cell(viewed, target.position),
                        generals_sim::skirmish::visibility::Status::Clear
                    ),
                    1.0,
                );
            }
            if matches!(effect, Effect::SupplyCenter { .. }) && event.cash > 0 {
                for (index, owner) in [(0, target.owner), (3, source.owner)] {
                    let name = self.sabotage.sounds[index].clone();
                    self.sabotage_sound(
                        &name,
                        None,
                        None,
                        Some(owner == viewed),
                        true,
                        self.sabotage.money_volume,
                    );
                }
            }
            if target.owner == viewed {
                self.sabotage.should_play[usize::from(
                    !matches!(effect, Effect::SupplyCenter { .. }) || event.cash == 0,
                )] = true;
            }
        }
        if let Some(kind) = self.sabotage.next_eva(game.tick, playing) {
            let faction = match game.players[viewed].faction {
                Faction::Usa => 0,
                Faction::China => 1,
                Faction::Gla => 2,
                Faction::Civilian => return,
            };
            let sounds = &self.sabotage.eva[kind].as_ref().unwrap().sounds[faction];
            if !sounds.is_empty() {
                let index = self
                    .random
                    .lock()
                    .expect("audio RNG lock")
                    .index(sounds.len());
                let name = sounds[index].clone();
                let _ = self.queue(&name, None, None, None, false);
            }
        }
    }
    fn sabotage_sound(
        &mut self,
        name: &str,
        object: Option<usize>,
        point: Option<Vec3>,
        local: Option<bool>,
        clear: bool,
        volume: f32,
    ) {
        let Some(event) = self.events.get(name) else {
            return;
        };
        if volume <= 0.0 || event.restrictions & ST_PLAYER != 0 && !local.unwrap_or(!event.world) {
            return;
        }
        let hidden =
            event.world && event.restrictions & ST_SHROUDED != 0 && event.priority != 4 && !clear;
        let count = self.requests.len();
        let _ = self.queue(name, object, point, None, false);
        if hidden {
            self.requests.truncate(count);
        } else if let Some(request) = self.requests.get_mut(count) {
            request.volume *= volume;
        }
    }
}

fn superweapon_kind(power_type: &str) -> Option<usize> {
    Some(match power_type {
        "SPECIAL_PARTICLE_UPLINK_CANNON"
        | "SUPW_SPECIAL_PARTICLE_UPLINK_CANNON"
        | "LAZR_SPECIAL_PARTICLE_UPLINK_CANNON" => 0,
        "SPECIAL_NEUTRON_MISSILE"
        | "NUKE_SPECIAL_NEUTRON_MISSILE"
        | "SUPW_SPECIAL_NEUTRON_MISSILE" => 1,
        "SPECIAL_SCUD_STORM" => 2,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_verification::{construct, friendly_entity, produce_type};
    use generals_sim::skirmish::Kind;
    #[test]
    fn simultaneous_native_ability_warnings_keep_eva_enum_order() {
        let mut state = State::fixture();
        state.should_play[..5].fill(true);
        state.next_eva(2, true);
        assert_eq!(
            state
                .checks
                .iter()
                .map(|check| check.kind)
                .collect::<Vec<_>>(),
            [2, 3, 0, 4, 1]
        );
        assert!(state.checks.iter().all(|check| !check.played));
        state.next_eva(2, true);
        assert_eq!(state.checks.len(), 5);
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_all_control_plays_finite_original_attack_then_body() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let mut audio =
            super::super::load(&assets, &battle.simulation.catalog, Path::new("")).unwrap();
        let event = &audio.events["SabotageBuilding"];
        assert_eq!(event.control, super::super::data::ALL);
        assert_eq!(event.attack, ["isabotag"]);
        assert_eq!(event.sounds, ["isabotab"]);
        assert!(event.decay.is_empty());
        audio.ui("SabotageBuilding");
        let request = audio.requests.pop().unwrap();
        assert_eq!(request.sample_names, ["isabotag", "isabotab"]);
        assert!(request.looping.is_none());
        let expected: Vec<_> = request.samples.iter().flat_map(Clone::clone).collect();
        assert!(!expected.is_empty());
        let sequence = super::super::Sequence {
            samples: request.samples,
            looping: None,
        };
        let decoded: Vec<_> = bevy::audio::Decodable::decoder(&sequence).collect();
        assert_eq!(decoded.len(), expected.len());
        assert_eq!(
            decoded
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>()
        );
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_supply_and_fake_feedback_queue_original_events_once_for_the_victim() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        battle.simulation.reset(Faction::Gla, Faction::Usa).unwrap();
        let worker = friendly_entity(&battle.simulation, Kind::Builder, 0).unwrap();
        construct(&mut battle.simulation, worker, Kind::SupplyCenter, 0).unwrap();
        let barracks = construct(&mut battle.simulation, worker, Kind::Barracks, 0).unwrap();
        construct(&mut battle.simulation, worker, Kind::Factory, 0).unwrap();
        let saboteur =
            crate::strategy_verification::template(&battle.simulation, "GLAInfantrySaboteur")
                .unwrap();
        let source = produce_type(&mut battle.simulation, barracks, saboteur, 0).unwrap();
        let target = friendly_entity(&battle.simulation, Kind::CommandCenter, 1).unwrap();
        battle.observer_player = Some(1);
        let module = battle.simulation.catalog[saboteur]
            .behavior
            .sabotage
            .iter()
            .position(|rules| matches!(rules.effect, Effect::SupplyCenter { .. }))
            .unwrap();
        let mut audio =
            super::super::load(&assets, &battle.simulation.catalog, Path::new("")).unwrap();
        battle.started = true;
        let frame = battle.simulation.tick;
        battle.simulation.presentation.sabotage_events.push(
            generals_sim::skirmish::sabotage::Event {
                frame,
                source,
                target,
                module,
                cash: 700,
            },
        );
        audio.update_sabotage(&battle, false);
        // The original installed archives omit the three CashStolen recordings.
        // Native playback consumes the EVA check even when its file cannot open.
        assert!(audio.samples["euscashs.wav"].is_none());
        assert!(audio.samples["echcashs.wav"].is_none());
        assert!(audio.samples["eglcashs.wav"].is_none());
        assert!(
            audio
                .sabotage
                .checks
                .iter()
                .any(|check| check.kind == 0 && check.played)
        );
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].event, audio.sabotage.sounds[0]);
        assert_eq!(audio.requests[0].object, None);
        assert!(audio.requests[0].position.is_some());
        assert!(!audio.requests.iter().any(|request| {
            audio.sabotage.eva[0].as_ref().unwrap().sounds[0].contains(&request.event)
        }));
        audio.update_sabotage(&battle, false);
        assert_eq!(audio.requests.len(), 1);
        for _ in 0..3 {
            battle.simulation.step();
            audio.update_sabotage(&battle, false);
            assert_eq!(audio.requests.len(), 1);
        }
        assert!(battle.simulation.presentation.sabotage_events.is_empty());
        assert!(audio.sabotage.seen.is_empty());
        audio.requests.clear();
        battle.simulation.step();
        let module = battle.simulation.catalog[saboteur]
            .behavior
            .sabotage
            .iter()
            .position(|rules| matches!(rules.effect, Effect::FakeBuilding))
            .unwrap();
        let frame = battle.simulation.tick;
        battle.simulation.presentation.sabotage_events.clear();
        battle.simulation.presentation.sabotage_events.push(
            generals_sim::skirmish::sabotage::Event {
                frame,
                source,
                target,
                module,
                cash: 0,
            },
        );
        audio.update_sabotage(&battle, false);
        assert_eq!(audio.requests.len(), 1);
        assert!(
            audio.sabotage.eva[1].as_ref().unwrap().sounds[0].contains(&audio.requests[0].event)
        );
        assert!(audio.requests[0].position.is_none());
    }

    #[test]
    fn victim_eva_coalesces_and_preserves_priority_order_cooldown_and_expiration() {
        let mut state = State::fixture();
        state.should_play[..2].fill(true);
        assert_eq!(state.next_eva(2, false), Some(0));
        assert_eq!(state.next_eva(2, false), None);
        assert_eq!(state.next_eva(3, true), None);
        assert_eq!(state.next_eva(4, false), Some(1));
        state.should_play[0] = true;
        assert_eq!(state.next_eva(5, false), None);
        state.should_play[0] = true;
        assert_eq!(state.next_eva(152, false), None);
        state.should_play[0] = true;
        assert_eq!(state.next_eva(153, false), Some(0));
        let mut state = State::fixture();
        state.should_play[0] = true;
        assert_eq!(state.next_eva(2, true), None);
        assert_eq!(state.next_eva(152, false), None);
    }

    #[test]
    fn simultaneous_victim_checks_keep_native_vehicle_cash_building_enum_order() {
        let mut state = State::fixture();
        state.should_play[..3].fill(true);
        assert_eq!(state.next_eva(2, false), Some(2));
        assert_eq!(state.next_eva(2, false), None);
        assert_eq!(state.next_eva(3, false), Some(0));
        assert_eq!(state.next_eva(4, false), Some(1));
        state.should_play[2] = true;
        assert_eq!(state.next_eva(5, false), None);
    }

    #[test]
    fn unauthored_launch_announcement_does_not_enter_eva_queue() {
        let mut state = State::fixture();
        state.eva[23] = None;
        state.should_play[23] = true;
        state.should_play[25] = true;
        assert_eq!(state.next_eva(2, false), Some(25));
        assert_eq!(state.checks.len(), 1);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn real_hijack_vehicle_stolen_eva_uses_previous_owner_and_delivers_once() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let setup = crate::hijacker_verification::prepare(&mut battle.simulation).unwrap();
        battle.simulation.hijack(setup.actor, setup.host).unwrap();
        crate::skirmish_verification::advance_until(
            &mut battle.simulation,
            3000,
            "real hijack audio contact",
            |game| game.entities[setup.host].hijacked(),
        )
        .unwrap_or_else(|error| {
            let game = &battle.simulation;
            let actor = &game.entities[setup.actor];
            let host = &game.entities[setup.host];
            panic!(
                "{error}; driver health={} position={:?} enter={} moving={} masked={}; host health={} position={:?} owner={}; attacker position={:?}; eligible={}",
                actor.health,
                actor.position,
                matches!(actor.ai.order, generals_sim::skirmish::Order::Enter(target) if target == setup.host),
                actor.moving(),
                actor.masked(),
                host.health,
                host.position,
                host.owner,
                game.entities[setup.attacker].position,
                game.can_hijack(setup.actor, setup.host)
            );
        });
        assert_eq!(battle.simulation.entities[setup.host].owner, 0);
        assert_eq!(battle.simulation.presentation.hijacker_events[0].victim, 1);
        battle.started = true;
        let mut audio =
            super::super::load(&assets, &battle.simulation.catalog, Path::new("")).unwrap();
        battle.observer_player = Some(0);
        audio.update_sabotage(&battle, false);
        assert!(audio.sabotage.checks.is_empty());
        audio.sabotage.reset();
        battle.observer_player = Some(1);
        audio.update_sabotage(&battle, false);
        assert_eq!(audio.sabotage.checks.len(), 1);
        assert_eq!(audio.sabotage.checks[0].kind, 2);
        assert!(audio.sabotage.checks[0].played);
        let requests = audio.requests.len();
        for _ in 0..3 {
            battle.simulation.step();
            audio.update_sabotage(&battle, false);
            assert_eq!(audio.requests.len(), requests);
        }
        assert!(battle.simulation.presentation.hijacker_events.is_empty());
        assert!(audio.sabotage.seen.is_empty());
    }
    #[test]
    fn effect_sound_keeps_native_position_and_applies_player_and_shroud_before_output() {
        let mut audio = super::super::speech_tests::audio();
        let event = audio.events.get_mut("click").unwrap();
        event.world = true;
        event.restrictions = ST_PLAYER | ST_SHROUDED;
        let point = Vec3::new(300.0, 20.0, -400.0);
        audio.sabotage_sound("click", Some(7), Some(point), Some(false), true, 1.0);
        audio.sabotage_sound("click", Some(7), Some(point), Some(true), false, 1.0);
        assert!(audio.requests.is_empty());
        audio.sabotage_sound("click", Some(7), Some(point), Some(true), true, 0.5);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].position, Some(point));
        assert_eq!(audio.requests[0].object, Some(7));
        assert_eq!(audio.requests[0].volume.to_bits(), 0.5_f32.to_bits());
    }
}
