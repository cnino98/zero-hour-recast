//! Local crash and slow-frame context; observes without changing game commands.
use crate::playtest::Match;
use bevy::{prelude::*, window::PrimaryWindow};
use generals_sim::skirmish::{Faction, OpponentDifficulty};
use std::time::{Duration, Instant};

pub struct FrameClock {
    previous: Instant,
    reported: Option<Instant>,
}

pub fn frame_time(
    battle: Res<Match>,
    windows: Query<&Window, With<PrimaryWindow>>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<crate::source_material::SourceMaterial>>,
    sounds: Query<(), With<AudioSink>>,
    mut clock: Local<Option<FrameClock>>,
) {
    let battle = battle.into_inner();
    let meshes = meshes.into_inner();
    let materials = materials.into_inner();
    let now = Instant::now();
    let clock = clock.get_or_insert(FrameClock {
        previous: now,
        reported: None,
    });
    let interval = now.duration_since(clock.previous);
    clock.previous = now;
    if battle.started
        && !battle.paused
        && should_report(interval, Duration::from_millis(100), now, clock.reported)
    {
        clock.reported = Some(now);
        let game = &battle.simulation;
        eprintln!(
            "Slow display interval: frame={} wall_ms={:.3} focused={} objects={} live={} projectiles={} mesh_assets={} material_assets={} audio_sinks={}",
            game.tick,
            interval.as_secs_f64() * 1000.0,
            windows.single().is_ok_and(|window| window.focused),
            game.entities.len(),
            game.entities.iter().filter(|actor| actor.alive()).count(),
            game.projectiles.len(),
            meshes.len(),
            materials.len(),
            sounds.iter().len()
        );
    }
}

pub fn simulation_time(
    game: &generals_sim::skirmish::Skirmish,
    elapsed: Duration,
    reported: &mut Option<Instant>,
) {
    let now = Instant::now();
    if !should_report(elapsed, Duration::from_millis(50), now, *reported) {
        return;
    }
    *reported = Some(now);
    let mut defenses = [0; 3];
    for actor in game.entities.iter().filter(|actor| actor.alive()) {
        match game.catalog[actor.type_index].name.as_str() {
            "AmericaFireBase" => defenses[0] += 1,
            "AmericaPatriotBattery" => defenses[1] += 1,
            _ if actor.stationed() => defenses[2] += 1,
            _ => {}
        }
    }
    eprintln!(
        "Slow simulation tick: frame={} cpu_ms={:.3} objects={} live={} projectiles={} shots={} fire_bases={} patriots={} stationed={}",
        game.tick,
        elapsed.as_secs_f64() * 1000.0,
        game.entities.len(),
        game.entities.iter().filter(|actor| actor.alive()).count(),
        game.projectiles.len(),
        game.presentation.shots.len(),
        defenses[0],
        defenses[1],
        defenses[2]
    );
}

fn should_report(
    elapsed: Duration,
    threshold: Duration,
    now: Instant,
    previous: Option<Instant>,
) -> bool {
    elapsed >= threshold
        && previous.is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
}

pub struct LastReport {
    generation: u32,
    started: bool,
    paused: bool,
    tick: u64,
    selected: Vec<usize>,
}

pub fn update(
    battle: Res<Match>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut previous: Local<Option<LastReport>>,
) {
    let battle = battle.into_inner();
    let buttons = buttons.into_inner();
    let keys = keys.into_inner();
    let game = &battle.simulation;
    let changed_match = previous
        .as_ref()
        .is_none_or(|old| old.generation != battle.generation || old.started != battle.started);
    let changed = previous.as_ref().is_none_or(|old| {
        old.paused != battle.paused
            || old.selected != battle.selected
            || game.tick.saturating_sub(old.tick) >= 900
    });
    let mouse_pressed: Vec<_> = buttons.get_just_pressed().collect();
    let mouse_released: Vec<_> = buttons.get_just_released().collect();
    let key_pressed: Vec<_> = keys.get_just_pressed().collect();
    if !changed_match
        && !changed
        && mouse_pressed.is_empty()
        && mouse_released.is_empty()
        && key_pressed.is_empty()
    {
        return;
    }
    eprintln!(
        "Match context: generation={} frame={} started={} paused={} selected={:?} ended={:?}",
        battle.generation, game.tick, battle.started, battle.paused, battle.selected, game.ended_at
    );
    if changed_match || changed {
        for (owner, slot) in game
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.occupied)
        {
            let player = &game.players[owner];
            let faction = match player.faction {
                Faction::Usa => "USA",
                Faction::China => "China",
                Faction::Gla => "GLA",
                Faction::Civilian => "Civilian",
            };
            let controller = match slot.difficulty {
                None => "human",
                Some(OpponentDifficulty::Easy) => "Easy",
                Some(OpponentDifficulty::Normal) => "Medium",
                Some(OpponentDifficulty::Hard) => "Hard",
            };
            eprintln!(
                "  player={owner} faction={faction} controller={controller} team={:?} start={:?} active={} cash={} units_built={} buildings_built={}",
                slot.team,
                game.start_positions[owner],
                game.player_active(owner),
                player.cash,
                player.statistics.units_built,
                player.statistics.buildings_built
            );
        }
    }
    for &id in battle.selected.iter().take(8) {
        if let Some(entity) = game.entities.get(id) {
            eprintln!(
                "  selected={id} template={} owner={} health={} position={:?}",
                game.catalog[entity.type_index].name, entity.owner, entity.health, entity.position
            );
        }
    }
    if !mouse_pressed.is_empty() || !mouse_released.is_empty() || !key_pressed.is_empty() {
        eprintln!(
            "  game input: cursor={:?} mouse_pressed={mouse_pressed:?} mouse_released={mouse_released:?} keys_pressed={key_pressed:?}",
            windows.single().ok().and_then(Window::cursor_position)
        );
    }
    *previous = Some(LastReport {
        generation: battle.generation,
        started: battle.started,
        paused: battle.paused,
        tick: game.tick,
        selected: battle.selected.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_slow_frames_are_rate_limited_without_hiding_the_first_spike() {
        let start = Instant::now();
        let limit = Duration::from_millis(50);
        assert!(!should_report(
            Duration::from_millis(49),
            limit,
            start,
            None
        ));
        assert!(should_report(limit, limit, start, None));
        assert!(!should_report(
            Duration::from_secs(2),
            limit,
            start + Duration::from_millis(999),
            Some(start)
        ));
        assert!(should_report(
            limit,
            limit,
            start + Duration::from_secs(1),
            Some(start)
        ));
    }
}
