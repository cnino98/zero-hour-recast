//! Paid Scout revelation and original Dozer custom destruction of a Demo Trap.
use crate::{
    skirmish_verification::{advance_until, construct, friendly_entity, produce_type, require},
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;
mod trace;
pub use trace::Trace;

pub struct Setup {
    pub builder: usize,
    pub trap: usize,
    pub scout: usize,
    pub position: [f32; 3],
    losses: u32,
    kills: u32,
    experience: i32,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::Gla)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let parent = produce_type(game, factory, template(game, "AmericaVehicleHumvee")?, 0)?;
    research(game, parent, "Upgrade_AmericaScoutDrone")?;
    let scout = game
        .produced_drone(parent)
        .ok_or("paid disarm detector lacks original Scout child")?;
    let trap = build_trap(game)?;
    require(
        game.entities[trap].stealthed() && !game.weapon_can_target(builder, trap),
        "unrevealed Demo Trap was already admitted to an ordinary disarm attack",
    )?;
    let center = game.entities[trap].position;
    let origin = game.entities[builder].position;
    let distance = (origin[0] - center[0]).hypot(origin[1] - center[1]);
    require(
        distance > 400.0,
        "disarm bases do not provide detector staging room",
    )?;
    let direction = [
        (origin[0] - center[0]) / distance,
        (origin[1] - center[1]) / distance,
    ];
    // Stage the builder before the Scout's finite revelation window. The
    // Dozer detects MINE kinds; this original target is a DEMOTRAP instead.
    let shots = move_near(game, builder, point(center, direction, 60.0))?;
    let station = point(center, direction, 100.0);
    game.move_to(parent, station)?;
    // Detector feedback precedes StealthUpdate's detected status. Remember the
    // real Scout callback until that target becomes visible and attackable.
    let mut revealed_by_scout = false;
    advance_until(
        game,
        18_000,
        "paid Scout revealing the original Demo Trap",
        |game| {
            revealed_by_scout |= game.presentation.stealth_events.iter().any(|event| {
                matches!(event,
            generals_sim::skirmish::stealth::Event::Detection { detector, target }
                if *detector == scout && *target == trap)
            });
            revealed_by_scout
                && game.entities[trap].alive()
                && game.entities[trap].detected()
                && game.logically_visible(0, trap)
                && game.weapon_can_target(builder, trap)
        },
    )?;
    game.stop(parent)?;
    // Return on the real Scout revelation. The native capture pauses here
    // and issues its context order before advancing the simulation again.
    require(
        game.entities[trap].health.to_bits() == game.entities[trap].max_health.to_bits()
            && game.entities[trap].detected()
            && game.weapon_can_target(builder, trap),
        "paid detector staging lost the untouched, attackable manual trap",
    ).map_err(|error| format!(
        "{error}; frame={} trap={}/{} masked={} detected={} visible={} targetable={} reachable={} builder_weapon={} scout={}; shots={shots:?}; trap_state={}; builder_state={}; parent_state={}",
        game.tick, game.entities[trap].health, game.entities[trap].max_health,
        game.entities[trap].masked(), game.entities[trap].detected(), game.logically_visible(0, trap),
        game.weapon_can_target(builder, trap), game.weapon_target_reachable(builder, trap),
        game.entities[builder].active_weapon(&game.catalog[game.entities[builder].type_index]).name,
        scout,
        generals_sim::skirmish::diagnostics::movement_details(game, trap),
        generals_sim::skirmish::diagnostics::movement_details(game, builder),
        generals_sim::skirmish::diagnostics::movement_details(game, parent)
    ))?;
    Ok(Setup {
        builder,
        trap,
        scout,
        position: game.object_position(trap),
        losses: game.players[1].statistics.units_lost,
        kills: game.players[0].statistics.units_destroyed,
        experience: game.entities[builder].experience,
    })
}

const fn point(center: [f32; 2], direction: [f32; 2], distance: f32) -> [f32; 2] {
    [
        direction[0].mul_add(distance, center[0]),
        direction[1].mul_add(distance, center[1]),
    ]
}

fn move_near(
    game: &mut Skirmish,
    actor: usize,
    point: [f32; 2],
) -> Result<Vec<String>, Box<dyn Error>> {
    game.move_to(actor, point)?;
    let mut shots = Vec::new();
    for _ in 0..18_000 {
        let unit = &game.entities[actor];
        if matches!(unit.ai.order, Order::Idle)
            && !unit.moving()
            && (unit.position[0] - point[0]).hypot(unit.position[1] - point[1]) < 20.0
        {
            return Ok(shots);
        }
        require(
            game.ended_at.is_none(),
            "match ended during ordinary disarm staging",
        )?;
        game.step();
        for shot in &game.presentation.shots {
            if shots.len() < 16 {
                let unit = &game.catalog[game.entities[shot.source].type_index];
                shots.push(format!(
                    "{}:{}:{}->{:?}",
                    game.tick,
                    shot.source,
                    unit.weapon_template(shot.weapon_index)
                        .map_or("missing", |weapon| &weapon.name),
                    shot.target
                ));
            }
        }
    }
    Err(format!(
        "ordinary disarm staging move timed out; shots={shots:?}; {}",
        generals_sim::skirmish::diagnostics::movement_details(game, actor)
    )
    .into())
}

fn build_trap(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, worker, Kind::SupplyCenter, 1)?;
    construct(game, worker, Kind::Barracks, 1)?;
    construct(game, worker, Kind::Factory, 1)?;
    let base = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    let index = template(game, "GLADemoTrap")?;
    let position = game
        .placement_near(index, [base[0] + 320.0, base[1] + 320.0])
        .ok_or("no legal open disarm trap site")?;
    crate::skirmish_verification::scout_construction_site(game, worker, index, position)?;
    let cash = game.players[1].cash;
    let trap = game.build(worker, index, position)?;
    require(
        game.players[1].cash + 400 == cash,
        "disarm target did not charge original trap cost",
    )?;
    advance_until(game, 18_000, "paid disarm target construction", |game| {
        game.entities[trap].complete()
    })?;
    game.switch_weapon(trap, 2)?;
    advance_until(game, 180, "manual trap concealing before disarm", |game| {
        game.entities[trap].stealthed()
    })?;
    // A normal return order avoids an unrelated Humvee kill of the worker
    // who constructed the trap during the later mine-clearing encounter.
    game.move_to(worker, base)?;
    Ok(trap)
}

pub fn verify_removed(game: &Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let trap = &game.entities[setup.trap];
    require(
        trap.masked()
            && !trap.corpse_present()
            && game.entities[setup.builder].alive()
            && game.players[1].statistics.units_lost == setup.losses
            && game.players[0].statistics.units_destroyed == setup.kills
            && game.entities[setup.builder].experience == setup.experience,
        "DISARM must custom-destroy the trap without a corpse, death weapon or kill credit",
    )
}

#[cfg(test)]
fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    game.attack(setup.builder, setup.trap)?;
    let mut trace = Trace::new();
    for _ in 0..1800 {
        game.step();
        trace.record(game, &setup)?;
        if trace.reloaded {
            break;
        }
    }
    require(
        trace.reloaded,
        "paid disarm did not complete its original 120-frame reload",
    )?;
    verify_removed(game, &setup)?;
    println!(
        "Paid DISARM: actual Scout revelation, Dozer conditional weapon, 36-frame dig, 120-frame reload and one custom-destruction shot without explosion or credit passed"
    );
    Ok(())
}

#[cfg(test)]
mod area_tests;

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_scout_reveals_manual_trap_then_dozer_disarms_without_detonation_or_credit() {
        let root = generals_assets::game_root().unwrap();
        let mut game = crate::infiltrator_verification::load_game(&root).unwrap();
        super::verify(&mut game).unwrap();
    }
}
