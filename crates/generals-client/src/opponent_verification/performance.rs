//! Installed-map growth through paid production and ordinary army commands.
//! Headless timings exclude assets, setup, trace hashing and console output.

use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::time::{Duration, Instant};

use super::{Kind, Skirmish, allied, load_game};
use generals_sim::skirmish::{ProductionItem, diagnostics};

mod slow_ticks;

struct Measurement {
    ticks: Vec<Duration>,
    commands: Vec<Duration>,
    population: [Vec<Duration>; 5],
    peak_live: usize,
    peak_moving: usize,
    shots: [u64; 4],
    paid_units: usize,
    paid_cost: u64,
    trace: DefaultHasher,
    random: [u32; 6],
    slow: slow_ticks::Samples,
}

pub(super) fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let loading = Instant::now();
    let mut game = load_game(root, crate::playtest::FOUR_PLAYER_MAP)?;
    println!(
        "SKIRMISH_BENCHMARK assets_ms={}",
        loading.elapsed().as_millis()
    );
    let setup = Instant::now();
    allied::reset(&mut game)?;
    allied::prepare(&mut game)?;
    println!(
        "SKIRMISH_BENCHMARK map=FallenEmpire seed=1 teams=USA+HardChina_vs_2HardGLA setup_frames={} setup_ms={} measured_limit=18000 production_interval=30 attack_interval=120 trace_schema=weapons3",
        game.tick,
        setup.elapsed().as_millis()
    );
    let mut measured = Measurement {
        ticks: Vec::with_capacity(18_000),
        commands: Vec::new(),
        population: std::array::from_fn(|_| Vec::new()),
        peak_live: 0,
        peak_moving: 0,
        shots: [0; 4],
        paid_units: 0,
        paid_cost: 0,
        trace: DefaultHasher::new(),
        random: [0; 6],
        slow: slow_ticks::Samples::new(),
    };
    for iteration in 0_u32..18_000 {
        if game.ended_at.is_some() {
            break;
        }
        if iteration.is_multiple_of(30) {
            let command = Instant::now();
            let (units, cost) = queue_ready(&mut game)?;
            if iteration.is_multiple_of(120) {
                allied::command_army(&mut game);
            }
            measured.commands.push(command.elapsed());
            measured.paid_units += units;
            measured.paid_cost += cost;
        }
        let navigation_before = diagnostics::navigation_tick_context(&game);
        let started = Instant::now();
        game.step();
        let duration = started.elapsed();
        measured.record(&game, duration, navigation_before);
        if (iteration + 1).is_multiple_of(3000) {
            println!(
                "SKIRMISH_GROWTH frame={} records={} peak_mobile={} trained={} shots={:?} trace={:016x} rng={:?}",
                game.tick,
                game.entities.len(),
                measured.peak_live,
                measured.paid_units,
                measured.shots,
                measured.trace.finish(),
                measured.random
            );
        }
    }
    measured.report(&game);
    if measured.ticks.is_empty()
        || measured.paid_units == 0
        || measured.shots.iter().sum::<u64>() == 0
    {
        return Err("Growing-army workload did not exercise paid production and combat".into());
    }
    Ok(())
}

pub(super) fn queue_ready(game: &mut Skirmish) -> Result<(usize, u64), Box<dyn Error>> {
    let producers = game
        .entities
        .iter()
        .enumerate()
        .filter(|(_, actor)| {
            actor.owner == 0
                && actor.complete()
                && actor.queue.len() < 9
                && matches!(
                    game.catalog[actor.type_index].kind,
                    Kind::Barracks | Kind::Factory
                )
        })
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let mut count = 0;
    let mut spent = 0;
    for producer in producers {
        let kind = if game.catalog[game.entities[producer].type_index].kind == Kind::Barracks {
            Kind::Infantry
        } else {
            Kind::Tank
        };
        let unit = game
            .type_for(0, kind)
            .ok_or("Missing ordinary production type")?;
        let cost = game.catalog[unit].cost;
        let cash = game.players[0].cash;
        if cash < cost {
            continue;
        }
        game.train(producer, unit)?;
        if cash.checked_sub(cost) != Some(game.players[0].cash) {
            return Err("Growing-army production failed its exact payment check".into());
        }
        count += 1;
        spent += u64::from(cost);
    }
    Ok((count, spent))
}

impl Measurement {
    fn record(
        &mut self,
        game: &Skirmish,
        duration: Duration,
        navigation_before: diagnostics::NavigationTickContext,
    ) {
        let mut live = 0;
        let mut moving = 0;
        for actor in &game.entities {
            if actor.owner != game.neutral_owner()
                && actor.complete()
                && !game.catalog[actor.type_index].kind.is_building()
            {
                live += 1;
                moving += usize::from(actor.moving());
            }
        }
        self.peak_live = self.peak_live.max(live);
        self.peak_moving = self.peak_moving.max(moving);
        self.ticks.push(duration);
        let bin = [64, 128, 256, 512].partition_point(|&limit| live >= limit);
        self.population[bin].push(duration);
        self.slow.record(slow_ticks::Sample {
            tick: game.tick,
            iteration: self.ticks.len() - 1,
            elapsed: duration,
            records: game.entities.len(),
            live,
            moving,
            structure_events: game.presentation.structure_events.len(),
            before: navigation_before,
            after: diagnostics::navigation_tick_context(game),
        });
        for shot in &game.presentation.shots {
            if let Some(count) = self.shots.get_mut(shot.owner) {
                *count += 1;
            }
        }
        self.random = diagnostics::record(game, &mut self.trace);
        record_production(game, &mut self.trace);
    }

    fn report(&mut self, game: &Skirmish) {
        self.slow.report();
        report_times("logic", &mut self.ticks);
        report_times("commands", &mut self.commands);
        for (label, times) in [
            "mobile_0_63",
            "mobile_64_127",
            "mobile_128_255",
            "mobile_256_511",
            "mobile_512_plus",
        ]
        .into_iter()
        .zip(&mut self.population)
        {
            report_times(label, times);
        }
        println!(
            "SKIRMISH_RESULT frame={} outcome={:?} records={} retired={} peak_mobile={} peak_moving={} paid_units={} paid_cost={} shots={:?} trace={:016x} rng={:?}",
            game.tick,
            game.outcome,
            game.entities.len(),
            game.entities.iter().filter(|actor| !actor.alive()).count(),
            self.peak_live,
            self.peak_moving,
            self.paid_units,
            self.paid_cost,
            self.shots,
            self.trace.finish(),
            self.random
        );
        for (owner, player) in game.players.iter().take(4).enumerate() {
            let stats = &player.statistics;
            println!(
                "SKIRMISH_PLAYER owner={owner} built={} lost={} destroyed={} buildings={} earned={} cash={}",
                stats.units_built,
                stats.units_lost,
                stats.units_destroyed,
                stats.buildings_built,
                stats.money_earned,
                player.cash
            );
        }
        println!(
            "SKIRMISH_OMITTED {}",
            game.opponent_unsupported().join(", ")
        );
    }
}

fn report_times(label: &str, times: &mut [Duration]) {
    if times.is_empty() {
        println!("SKIRMISH_PERF scope={label} frames=0");
        return;
    }
    times.sort_unstable();
    let at = |percent: usize| times[(times.len() - 1) * percent / 100].as_micros();
    println!(
        "SKIRMISH_PERF scope={label} frames={} p50_us={} p95_us={} max_us={}",
        times.len(),
        at(50),
        at(95),
        times.last().unwrap().as_micros()
    );
}

fn record_production(game: &Skirmish, trace: &mut DefaultHasher) {
    for actor in &game.entities {
        actor.queue.len().hash(trace);
        for production in &actor.queue {
            production.id.hash(trace);
            std::mem::discriminant(&production.item).hash(trace);
            match production.item {
                ProductionItem::Unit(index) | ProductionItem::Upgrade(index) => index.hash(trace),
            }
            production.remaining_seconds.to_bits().hash(trace);
        }
    }
}
