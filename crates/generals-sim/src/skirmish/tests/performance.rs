//! Repeatable simulation workload, independent of asset loading and rendering.
//! Uses the existing test catalog and native ground-motion rules; installed-map
//! acceptance is measured separately by the client benchmark.

use std::hash::{DefaultHasher, Hasher};
use std::time::{Duration, Instant};

use super::{Faction, GroundAppearance, GroundRules, Kind, MapLayout, PlayerStart, Skirmish};

enum Workload {
    Idle,
    Movement,
    Contacts,
    Casualties,
}

impl Workload {
    const fn name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Movement => "movement",
            Self::Contacts => "contacts",
            Self::Casualties => "casualties",
        }
    }
}

#[test]
#[ignore = "Explicit performance measurement; timings are not CI assertions"]
fn simulation_scaling() {
    let frames = std::env::var("GENERALS_BENCH_FRAMES")
        .map_or(Ok(600), |value| value.parse::<usize>())
        .unwrap();
    assert!(frames > 0);
    println!(
        "SIMULATION_BENCHMARK fixture_catalog grid=400x400 frames={frames} warmup=30 trace_schema=weapons3"
    );
    for count in [64, 128, 256, 512] {
        for workload in [
            Workload::Idle,
            Workload::Movement,
            Workload::Contacts,
            Workload::Casualties,
        ] {
            measure(count, &workload, frames);
        }
    }
}

fn scenario(count: usize, workload: &Workload) -> (Skirmish, Vec<usize>) {
    let fixture = super::game();
    let mut game = Skirmish::new(
        crate::terrain::TerrainMap::classify(400, 400, [], []).unwrap(),
        fixture.catalog,
        Faction::Gla,
        Faction::Gla,
        fixture.rules,
        fixture.ai_rules,
        MapLayout {
            height_map: None,
            map_sides: Vec::new(),
            map_teams: Vec::new(),
            map_build_lists: Vec::new(),
            map_template_order: Vec::new(),
            scripts: Vec::new(),
            fences: Vec::new(),
            obstacle_teams: Vec::new(),
            world_objects: Vec::new(),
            world_object_order: Vec::new(),
            obstacle_objects: Vec::new(),
            obstacle_order: Vec::new(),
            partition_cell_size: 40.0,
            random_seed: 1,
            regular_bridges: Vec::new(),
            neutral_objects: Vec::new(),
            starts: vec![
                PlayerStart {
                    position: [300.0; 2],
                    rally: None,
                },
                PlayerStart {
                    position: [3700.0; 2],
                    rally: None,
                },
            ],
            supplies: Vec::new(),
            obstacles: Vec::new(),
        },
    )
    .unwrap();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
        if matches!(unit.kind, Kind::Infantry | Kind::Tank) {
            unit.locomotor.ground = Some(GroundRules {
                wander: None,
                appearance: if unit.kind == Kind::Infantry {
                    GroundAppearance::Legs
                } else {
                    GroundAppearance::Treads
                },
                is_dozer: false,
                downhill_only: false,
                acceleration: 100.0 / 900.0,
                acceleration_damaged: 100.0 / 900.0,
                braking: 100.0 / 900.0,
                min_speed: 0.0,
                close_enough: 1.0,
                mass: 50.0,
                forward_friction: 0.1,
                lateral_friction: 0.2,
                extra_friction: 0.0,
            });
        }
    }
    let retired = if matches!(workload, Workload::Casualties) {
        count * 4
    } else {
        0
    };
    for index in 0..retired {
        let id = game.spawn(5 + index % 2, 0, position(index, 18.0), true);
        game.entities[id].health = 0.0;
        game.observe_ground_partition(id);
    }
    let spacing = if matches!(workload, Workload::Contacts) {
        4.0
    } else {
        18.0
    };
    let ids = (0..count)
        .map(|index| game.spawn(5 + index % 2, 0, position(index, spacing), true))
        .collect();
    (game, ids)
}

fn position(index: usize, spacing: f32) -> [f32; 2] {
    let column = f32::from(u16::try_from(index % 32).unwrap());
    let row = f32::from(u16::try_from(index / 32).unwrap());
    [1500.0 + column * spacing, 1500.0 + row * spacing]
}

fn measure(count: usize, workload: &Workload, frames: usize) {
    let (mut game, ids) = scenario(count, workload);
    for _ in 0..30 {
        game.step();
    }
    let started = Instant::now();
    if !matches!(workload, Workload::Idle) {
        game.group_move_to(&ids, [2500.0, 1800.0], false).unwrap();
    }
    let command = started.elapsed();
    let mut times = Vec::with_capacity(frames);
    let mut trace = DefaultHasher::new();
    let mut shots = 0;
    for _ in 0..frames {
        let started = Instant::now();
        game.step();
        times.push(started.elapsed());
        super::super::diagnostics::record(&game, &mut trace);
        shots += game.presentation.shots.len();
    }
    times.sort_unstable();
    println!(
        "SIMULATION_PERF case={} units={count} retired={} command_us={} p50_us={} p95_us={} max_us={} trace={:016x} shots={shots} rng={:?}",
        workload.name(),
        game.entities.len() - ids.len() - 4,
        command.as_micros(),
        percentile(&times, 50).as_micros(),
        percentile(&times, 95).as_micros(),
        times.last().unwrap().as_micros(),
        trace.finish(),
        game.random,
    );
}

fn percentile(times: &[Duration], percent: usize) -> Duration {
    times[(times.len() - 1) * percent / 100]
}
