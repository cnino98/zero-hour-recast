//! The original pre-defection radar warning uses the captured victim owner.
use crate::{
    control_bar::UiData, notifications::Notifications, playtest::Match, radar::Radar,
    source_audio::SourceAudio,
};
use bevy::prelude::*;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct State {
    generation: Option<u32>,
    seen: BTreeSet<(u64, usize, bool)>,
    ability_seen: BTreeSet<(u64, usize)>,
}

pub fn record(
    battle: Res<Match>,
    ui: Res<UiData>,
    mut radar: ResMut<Radar>,
    mut messages: ResMut<Notifications>,
    mut audio: ResMut<SourceAudio>,
    mut state: Local<State>,
) {
    let battle = battle.into_inner();
    let ui = ui.into_inner();
    if state.generation != Some(battle.generation) {
        state.generation = Some(battle.generation);
        state.seen.clear();
        state.ability_seen.clear();
    }
    if !battle.started || battle.paused {
        return;
    }
    let game = &battle.simulation;
    state.seen.retain(|&(frame, _, _)| frame + 1 >= game.tick);
    let viewed = battle.observer_player.unwrap_or(0);
    state
        .ability_seen
        .retain(|&(frame, _)| frame + 1 >= game.tick);
    for (index, event) in game.presentation.ability_feedback.iter().enumerate() {
        if matches!(
            event.effect,
            generals_sim::skirmish::special_ability::FeedbackEffect::Infiltration { .. }
        ) && event.victim == viewed
            && state.ability_seen.insert((event.frame, index))
        {
            radar.infiltration(event.target_position, event.frame, battle.generation);
            messages.add(ui.label("RADAR:Infiltration").to_owned(), event.frame);
        }
    }
    for event in &game.presentation.hijacker_events {
        if state.seen.insert((event.frame, event.source, false)) && event.victim == viewed {
            radar.infiltration(event.position, event.frame, battle.generation);
            messages.add(ui.label("RADAR:Infiltration").to_owned(), event.frame);
            audio.infiltration_warning();
        }
    }
    for event in &game.presentation.capture_events {
        if let generals_sim::skirmish::CaptureEffect::Infiltrated(position) = event.effect
            && event.owner == viewed
            && state.seen.insert((event.frame, event.target, true))
        {
            radar.infiltration(position, event.frame, battle.generation);
            messages.add(ui.label("RADAR:Infiltration").to_owned(), event.frame);
            audio.infiltration_warning();
        }
    }
}
