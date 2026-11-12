//! Saboteur crate client callbacks: selection flash, radar warning and cash text.
use crate::{
    control_bar::UiData, income, notifications::Notifications, playtest::Match, radar::Radar,
    selection_feedback::Feedback, source_audio::SourceAudio,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use generals_sim::skirmish::sabotage::Effect;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct State {
    generation: Option<u32>,
    seen: BTreeSet<(u64, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_verification::{advance_until, construct, friendly_entity, produce_type};
    use generals_sim::skirmish::{Faction, Kind};

    fn paid_supply_contact(battle: &mut Match) {
        let game = &mut battle.simulation;
        game.reset(Faction::Gla, Faction::Usa).unwrap();
        let worker = friendly_entity(game, Kind::Builder, 0).unwrap();
        construct(game, worker, Kind::SupplyCenter, 0).unwrap();
        let barracks = construct(game, worker, Kind::Barracks, 0).unwrap();
        construct(game, worker, Kind::Factory, 0).unwrap();
        let kind = crate::strategy_verification::template(game, "GLAInfantrySaboteur").unwrap();
        let source = produce_type(game, barracks, kind, 0).unwrap();
        let worker = friendly_entity(game, Kind::Builder, 1).unwrap();
        construct(game, worker, Kind::PowerPlant, 1).unwrap();
        let target = construct(game, worker, Kind::SupplyCenter, 1).unwrap();
        let point = game.entities[target].position;
        let radius = game.catalog[game.entities[target].type_index]
            .geometry_bounds
            .sphere_radius;
        // Isolate callback lifetime from cross-map navigation. The actual enter
        // order, physical contact, original cash transfer and consumption run.
        game.entities[source].position = [point[0] + radius + 20.0, point[1]];
        game.stop(source).unwrap();
        game.step();
        game.enter(source, target).unwrap();
        advance_until(game, 3000, "real supply sabotage contact", |game| {
            game.presentation
                .sabotage_events
                .iter()
                .any(|event| event.source == source && event.target == target && event.cash > 0)
        })
        .unwrap();
        battle.started = true;
        battle.paused = false;
        battle.observer_player = Some(1);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn real_cash_callback_delivers_once_through_retained_event_and_match_reset() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        paid_supply_contact(&mut battle);
        let map = generals_assets::map::MapData::parse(
            &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
        )
        .unwrap();
        let terrain = crate::terrain::Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let ui = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let radar = Radar::load(&assets, &terrain, &map.triggers).unwrap();
        let feedback = crate::selection_feedback::load(&assets).unwrap();
        let messages = crate::notifications::load(&assets).unwrap();
        let audio =
            crate::source_audio::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let mut app = App::new();
        app.insert_resource(battle)
            .insert_resource(ui)
            .insert_resource(radar)
            .insert_resource(feedback)
            .insert_resource(messages)
            .insert_resource(audio)
            .add_systems(FixedUpdate, record);
        app.world_mut().run_schedule(FixedUpdate);
        for _ in 0..2 {
            app.world_mut().resource_mut::<Match>().simulation.step();
            app.world_mut().run_schedule(FixedUpdate);
            assert_eq!(
                app.world_mut()
                    .query::<&crate::income::Floating>()
                    .iter(app.world())
                    .count(),
                2
            );
        }
        assert_eq!(
            app.world()
                .resource::<Match>()
                .simulation
                .presentation
                .sabotage_events
                .len(),
            1
        );
        let event = &app
            .world()
            .resource::<Match>()
            .simulation
            .presentation
            .sabotage_events[0];
        let retained_key = (event.frame, event.source);
        {
            let mut battle = app.world_mut().resource_mut::<Match>();
            battle.generation = battle.generation.wrapping_add(1);
            paid_supply_contact(&mut battle);
            let event = &battle.simulation.presentation.sabotage_events[0];
            assert_eq!((event.frame, event.source), retained_key);
        }
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world_mut()
                .query::<&crate::income::Floating>()
                .iter(app.world())
                .count(),
            4
        );
        for _ in 0..3 {
            app.world_mut().resource_mut::<Match>().simulation.step();
            app.world_mut().run_schedule(FixedUpdate);
            assert_eq!(
                app.world_mut()
                    .query::<&crate::income::Floating>()
                    .iter(app.world())
                    .count(),
                4
            );
        }
        assert!(
            app.world()
                .resource::<Match>()
                .simulation
                .presentation
                .sabotage_events
                .is_empty()
        );
    }
}

#[derive(SystemParam)]
pub struct Resources<'w> {
    ui: Res<'w, UiData>,
    feedback: ResMut<'w, Feedback>,
    radar: ResMut<'w, Radar>,
    messages: ResMut<'w, Notifications>,
    audio: ResMut<'w, SourceAudio>,
}

pub fn record(
    mut commands: Commands,
    battle: Res<Match>,
    resources: Resources,
    mut state: Local<State>,
) {
    let Resources {
        ui,
        mut feedback,
        mut radar,
        mut messages,
        mut audio,
    } = resources;
    let battle = battle.into_inner();
    let ui = ui.into_inner();
    if state.generation != Some(battle.generation) {
        state.seen.clear();
        state.generation = Some(battle.generation);
    }
    if !battle.started || battle.paused {
        return;
    }
    let game = &battle.simulation;
    state.seen.retain(|&(frame, source)| {
        game.presentation
            .sabotage_events
            .iter()
            .any(|event| event.frame == frame && event.source == source)
    });
    let viewed = battle.observer_player.unwrap_or(0);
    for event in &game.presentation.sabotage_events {
        if !state.seen.insert((event.frame, event.source)) {
            continue;
        }
        let target = &game.entities[event.target];
        let source = &game.entities[event.source];
        let effect = &game.catalog[source.type_index].behavior.sabotage[event.module].effect;
        // tryInfiltrationEvent's warning/message occur even without functioning radar.
        if target.owner == viewed {
            radar.infiltration(
                game.object_position(event.target),
                event.frame,
                battle.generation,
            );
            messages.add(ui.label("RADAR:Infiltration").to_owned(), event.frame);
            audio.infiltration_warning();
        }
        // Fake buildings return before doSabotageFeedbackFX's selection flash.
        if !matches!(effect, Effect::FakeBuilding) {
            feedback.flash_as_selected(battle, event.target);
        }
        if matches!(effect, Effect::SupplyCenter { .. }) && event.cash > 0 {
            income::cash_theft(
                &mut commands,
                battle,
                ui,
                event.frame,
                event.source,
                event.target,
                event.cash,
            );
        }
    }
}
