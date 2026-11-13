//! Paid installed Countermeasures activation and original flare object lifecycle.
use super::{advance_until, comanche_trial, prepare};
use generals_sim::skirmish::{DamageType, Skirmish};

#[test]
#[ignore = "requires original game installation"]
fn paid_raptor_countermeasures_emit_three_three_flare_volleys_and_retire() {
    verify(false);
}

#[test]
#[ignore = "requires original game installation"]
fn paid_comanche_countermeasures_emit_three_two_flare_volleys_and_retire() {
    verify(true);
}

#[test]
#[ignore = "requires original game installation"]
fn paid_raptor_returns_from_airborne_guard_without_touchdown_damage() {
    let (_, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    animations.bind_logical_bones(&mut battle.simulation.catalog);
    let game = &mut battle.simulation;
    let setup = prepare(game).unwrap();
    verify_runway_bones(game, setup.airfield);
    super::approach(game, setup.raptor, setup.target).unwrap();
    let home = game.entities[setup.airfield].position;
    game.guard(setup.raptor, home).unwrap();
    for _ in 0..180 {
        game.step();
    }
    assert!(game.height_above_ground(setup.raptor) > 10.0);
    let health = game.entities[setup.raptor].health;
    assert_eq!(
        health.to_bits(),
        game.entities[setup.raptor].max_health.to_bits()
    );
    let previous_source = game.entities[setup.raptor].last_attacker();
    game.land_aircraft(setup.raptor, setup.airfield).unwrap();
    let approach = game.entities[setup.raptor].ai_goal_position();
    assert!((approach[0] - home[0]).hypot(approach[1] - home[1]) > 1.0);
    for _ in 0..7200 {
        game.step();
        assert_eq!(
            game.entities[setup.raptor].health.to_bits(),
            health.to_bits(),
            "ordinary runway return inflicted damage: {}",
            super::details(game, setup.raptor)
        );
        // ActiveBody retains ParkingPlace healing as its last DamageInfo even
        // when the health was already full (ActiveBody.cpp844, ParkingPlace.cpp743).
        let source = game.entities[setup.raptor].last_attacker();
        assert!(
            source == previous_source || source == Some(setup.airfield),
            "unexpected return damage source: before={previous_source:?} after={source:?}; {}",
            super::details(game, setup.raptor)
        );
        if game.entities[setup.raptor].aircraft_phase() == Some("idle")
            && game.height_above_ground(setup.raptor) < 10.0
        {
            return;
        }
    }
    panic!(
        "runway return timed out: {}",
        super::details(game, setup.raptor)
    );
}

fn verify_runway_bones(game: &Skirmish, airfield: usize) {
    let start = game
        .logical_bone_position(airfield, "RunwayStart1")
        .unwrap();
    let end = game.logical_bone_position(airfield, "RunwayEnd1").unwrap();
    let parking = game
        .logical_bone_position(airfield, "Runway1Parking1")
        .unwrap();
    eprintln!("Original airfield bones: start={start:?} end={end:?} parking={parking:?}");
    assert!((end[0] - start[0]).hypot(end[1] - start[1]) > 0.0);
    assert!(parking.iter().all(|coordinate| coordinate.is_finite()));
    let rules = game.catalog[game.entities[airfield].type_index]
        .parking
        .as_ref()
        .unwrap();
    assert!(rules.approach_height > 0.0);
}

fn verify(helicopter: bool) {
    let (_, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    // ParkingPlace reads pristine Draw bones when it first caches runway geometry.
    animations.bind_logical_bones(&mut battle.simulation.catalog);
    let game = &mut battle.simulation;
    let setup = prepare(game).unwrap();
    verify_runway_bones(game, setup.airfield);
    verify_original_armor(game, setup.raptor, false);
    verify_original_armor(game, setup.comanche, true);
    assert!(!game.has_upgrade(0, "Upgrade_AmericaCountermeasures"));
    crate::strategy_verification::research(game, setup.airfield, "Upgrade_AmericaCountermeasures")
        .unwrap();
    assert!(game.has_upgrade(0, "Upgrade_AmericaCountermeasures"));
    let defender = comanche_trial::produce_mig(game).unwrap();
    let weapon = &game.catalog[game.entities[defender].type_index].weapon;
    assert_eq!(weapon.damage.to_bits(), 75.0_f32.to_bits());
    assert!(matches!(weapon.damage_type, DamageType::JetMissiles));
    assert_eq!(weapon.clip_size, 2);
    let actor = if helicopter {
        setup.comanche
    } else {
        setup.raptor
    };
    if helicopter {
        verify_comanche_volley(game, actor, defender, setup.airfield);
        for _ in 0..120 {
            game.step();
        }
        verify_comanche_volley(game, actor, defender, setup.airfield);
    } else {
        verify_raptor_volley(game, actor, defender, setup.airfield);
        verify_exhausted_in_flight(game, actor, defender, setup.airfield);
        game.land_aircraft(actor, setup.airfield).unwrap();
        advance_until(
            game,
            7200,
            "Raptor landing reloads Countermeasures",
            |game| {
                game.entities[actor].aircraft_phase() == Some("idle")
                    && game.height_above_ground(actor) < 10.0
                    && game.entities[actor].health.to_bits()
                        == game.entities[actor].max_health.to_bits()
            },
        )
        .unwrap_or_else(|error| {
            panic!(
                "{error}; last_attacker={:?}; {}",
                game.entities[actor]
                    .last_attacker()
                    .map(|id| (id, game.catalog[game.entities[id].type_index].name.as_str())),
                super::details(game, actor)
            )
        });
        verify_raptor_volley(game, actor, defender, setup.airfield);
    }
}

fn verify_comanche_volley(game: &mut Skirmish, actor: usize, defender: usize, airfield: usize) {
    let mut trial = comanche_trial::Exposure::new(actor, defender, airfield);
    let first = game.entities.len();
    let mut flares = Vec::new();
    let mut next = first;
    let mut moved = false;
    for _ in 0..36_000 {
        game.step();
        trial.observe(game).unwrap();
        assert!(game.entities[actor].alive());
        for id in next..game.entities.len() {
            if game.catalog[game.entities[id].type_index].name == "CountermeasureFlare" {
                flares.push((id, game.tick, game.object_position(id)));
            }
        }
        next = game.entities.len();
        for &(id, born, initial) in &flares {
            if game.entities[id].exists() {
                moved |= game.object_position(id).map(f32::to_bits) != initial.map(f32::to_bits);
                assert!(
                    game.tick - born <= 90,
                    "Comanche flare exceeded its original lifetime"
                );
            }
        }
        if trial.diverted
            && flares.len() == 6
            && flares.iter().all(|&(id, _, _)| !game.entities[id].exists())
        {
            assert!(moved);
            for volley in flares.chunks_exact(2) {
                assert_eq!(volley[0].1, volley[1].1);
            }
            assert_eq!(flares[2].1 - flares[0].1, 30);
            assert_eq!(flares[4].1 - flares[2].1, 30);
            return;
        }
    }
    panic!(
        "Comanche lifecycle timed out: diverted={} flares={} moved={moved}; {}",
        trial.diverted,
        flares.len(),
        super::details(game, actor)
    );
}

fn verify_raptor_volley(game: &mut Skirmish, actor: usize, defender: usize, airfield: usize) {
    for attempt in 1..=12 {
        eprintln!(
            "Countermeasures sequence: aircraft={actor} expected=9 attempt={attempt} tick={} health={}",
            game.tick, game.entities[actor].health
        );
        if verify_volley_attempt(game, actor, defender, airfield) {
            return;
        }
        // A 30% evasion roll need not succeed on the first incoming missile.
        // Refit the same paid Raptor between single-missile exposures.
        game.land_aircraft(actor, airfield).unwrap();
        advance_until(
            game,
            7200,
            "Raptor repairs after one unsuccessful flare trigger",
            |game| {
                game.entities[actor].aircraft_phase() == Some("idle")
                    && game.height_above_ground(actor) < 10.0
                    && game.entities[actor].health.to_bits()
                        == game.entities[actor].max_health.to_bits()
            },
        )
        .unwrap_or_else(|error| {
            panic!(
                "{error}; last_attacker={:?}; {}",
                game.entities[actor]
                    .last_attacker()
                    .map(|id| (id, game.catalog[game.entities[id].type_index].name.as_str())),
                super::details(game, actor)
            )
        });
    }
    panic!("no successful Countermeasures roll during twelve repaired Raptor sorties");
}

fn verify_volley_attempt(
    game: &mut Skirmish,
    actor: usize,
    defender: usize,
    airfield: usize,
) -> bool {
    ready_missile_provider(game, defender);
    let target = game.entities[defender].position;
    let approach = [target[0] + 100.0, target[1]];
    game.move_to(actor, approach).unwrap();
    game.attack(defender, actor).unwrap();
    let first = game.entities.len();
    let mut flares = Vec::new();
    let mut moved = false;
    let mut missile = false;
    let mut retreated = false;
    let mut incoming_at = None;
    for _ in 0..5400 {
        game.step();
        missile |= game.projectiles.iter().any(|projectile| {
            projectile.source == defender && projectile.object.is_some_and(|id| id >= first)
        });
        if missile && incoming_at.is_none() {
            incoming_at = Some(game.tick);
            eprintln!(
                "Countermeasures incoming: aircraft={actor} tick={} health={}",
                game.tick, game.entities[actor].health
            );
        }
        for id in first..game.entities.len() {
            let entity = &game.entities[id];
            if game.catalog[entity.type_index].name == "CountermeasureFlare"
                && !flares.iter().any(|&(known, _, _)| known == id)
            {
                flares.push((id, game.tick, game.object_position(id)));
            }
        }
        for &(id, born, initial) in &flares {
            if game.entities[id].exists() {
                moved |= game.object_position(id).map(f32::to_bits) != initial.map(f32::to_bits);
                assert!(
                    game.tick - born <= 90,
                    "original flare exceeded its 90-frame lifetime"
                );
            }
        }
        if missile && !retreated {
            // End a Raptor's exposure after one missile, even if evasion fails;
            // already scheduled volleys and diversion continue independently.
            separate_aircraft_from_defender(game, actor, defender, airfield);
            retreated = true;
        }
        assert!(
            game.entities[actor].alive(),
            "aircraft died before Countermeasures proof: flares={} incoming_at={incoming_at:?}; {}",
            flares.len(),
            super::details(game, actor)
        );
        if flares.len() == 9 && flares.iter().all(|&(id, _, _)| !game.entities[id].exists()) {
            assert!(
                missile && moved,
                "actual incoming missile and moving flare required"
            );
            let births: Vec<_> = flares.iter().map(|&(_, tick, _)| tick).collect();
            let size = 3;
            for volley in births.chunks_exact(size) {
                assert!(volley.iter().all(|tick| *tick == volley[0]));
            }
            assert_eq!(births[size] - births[0], 30);
            assert_eq!(births[size * 2] - births[size], 30);
            return true;
        }
        if flares.is_empty() && incoming_at.is_some_and(|frame| game.tick >= frame + 180) {
            return false;
        }
    }
    panic!(
        "Countermeasures lifecycle timed out: missile={missile} flares={} expected=9 moved={moved}; {}",
        flares.len(),
        super::details(game, actor)
    );
}

fn verify_original_armor(game: &Skirmish, actor: usize, helicopter: bool) {
    let unit = &game.catalog[game.entities[actor].type_index];
    let upgrade = unit.maintenance.armor_upgrade.as_ref().unwrap();
    assert_eq!(upgrade.trigger, "Upgrade_AmericaCountermeasures");
    for (damage, normal, upgraded) in [
        (DamageType::SmallArms, 1.2_f32, 0.75_f32),
        (DamageType::Gattling, 1.2, 0.75),
        (DamageType::InfantryMissile, 1.2, 1.0),
        (
            DamageType::Explosion,
            if helicopter { 1.3 } else { 1.0 },
            if helicopter { 1.1 } else { 0.9 },
        ),
    ] {
        assert_eq!(unit.armor[damage as usize].to_bits(), normal.to_bits());
        assert_eq!(
            upgrade.coefficients[damage as usize].to_bits(),
            upgraded.to_bits()
        );
    }
    assert_eq!(
        unit.behavior.countermeasures[0].reload,
        if helicopter { 120 } else { 0 }
    );
}

fn verify_exhausted_in_flight(game: &mut Skirmish, actor: usize, defender: usize, airfield: usize) {
    ready_missile_provider(game, defender);
    let target = game.entities[defender].position;
    game.move_to(actor, [target[0] + 100.0, target[1]]).unwrap();
    game.attack(defender, actor).unwrap();
    let first = game.entities.len();
    let mut incoming_at = None;
    for _ in 0..5400 {
        game.step();
        assert!(
            game.entities[actor].alive(),
            "exhausted Raptor died before the second missile check"
        );
        assert!(
            game.height_above_ground(actor) > 10.0,
            "Raptor landed before its no-reload proof"
        );
        assert!(
            !game.entities[first..]
                .iter()
                .any(|entity| { game.catalog[entity.type_index].name == "CountermeasureFlare" }),
            "Raptor reloaded Countermeasures without landing"
        );
        if incoming_at.is_none()
            && game.projectiles.iter().any(|projectile| {
                projectile.source == defender && projectile.object.is_some_and(|id| id >= first)
            })
        {
            incoming_at = Some(game.tick);
            separate_aircraft_from_defender(game, actor, defender, airfield);
        }
        if incoming_at.is_some_and(|frame| game.tick >= frame + 60) {
            eprintln!(
                "Countermeasures exhausted check: aircraft={actor} tick={} health={} last_attacker={:?} defender_position={:?} airfield_position={:?}",
                game.tick,
                game.entities[actor].health,
                game.entities[actor].last_attacker(),
                game.entities[defender].position,
                game.entities[airfield].position
            );
            return;
        }
    }
    panic!("exhausted Raptor never received another incoming missile");
}

fn separate_aircraft_from_defender(
    game: &mut Skirmish,
    actor: usize,
    defender: usize,
    airfield: usize,
) {
    // A paid MiG carries only two original 75-damage JET_MISSILES; Raptor's
    // original 25% coefficient makes even an undiverted clip survivable.
    // Stop returns that provider to its own runway and cancels the attack.
    if game.entities[defender].aircraft_phase().is_some() {
        game.stop(defender).unwrap();
    } else {
        let home = game.entities[airfield].position;
        let target = game.entities[defender].position;
        let direction = [target[0] - home[0], target[1] - home[1]];
        let distance = direction[0].hypot(direction[1]);
        assert!(distance > 0.0);
        let offset = direction.map(|value| value / distance * 600.0);
        game.move_to(defender, [target[0] + offset[0], target[1] + offset[1]])
            .unwrap();
    }
    // Guard near the home Airfield keeps the exhausted Raptor airborne while
    // the missile provider rearms, without idle-return resupplying its flares.
    game.guard(actor, game.entities[airfield].position).unwrap();
}

fn ready_missile_provider(game: &mut Skirmish, defender: usize) {
    if game.entities[defender].aircraft_phase().is_none() {
        return;
    }
    advance_until(
        game,
        7200,
        "paid MiG parked and rearmed before missile exposure",
        |game| {
            game.entities[defender].aircraft_phase() == Some("idle")
                && game.height_above_ground(defender) < 10.0
                && game.entities[defender].weapon_rounds_remaining(0) == 2
        },
    )
    .unwrap_or_else(|error| panic!("{error}; {}", super::details(game, defender)));
}
