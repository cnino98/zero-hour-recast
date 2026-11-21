//! Installed paid GLA power, research and physical payload regressions.
use crate::{
    cleanup_verification::tests::installed,
    p03_capture::gla::{SCUD, build_scud, gla_technology, scout_scud_target},
    p03_gla_capture::{Kind as CaptureKind, paid_radar},
    progression_verification::{construction, earn_combat_rank},
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{
    Faction, Kind, PowerTarget, ProductionItem, ProjectileFxStage, Skirmish, WeaponFxEvent, stealth,
};
use std::{collections::BTreeSet, error::Error};

const AMBUSH: &str = CaptureKind::Ambush.power();
const GPS: &str = CaptureKind::Gps.power();
const SNEAK: &str = CaptureKind::Sneak.power();
const ANTHRAX: &str = CaptureKind::Anthrax.power();
const BOOBY: &str = CaptureKind::Booby.power();

#[test]
#[ignore = "requires original game installation"]
fn paid_scud_storm_fires_nine_physical_missiles_and_original_toxin() -> Result<(), Box<dyn Error>> {
    let (_, mut battle) = installed();
    let game = &mut battle.simulation;
    game.reset(Faction::Gla, Faction::China)?;
    let (storm, worker) = build_scud(game)?;
    require(
        game.power_cooldown_remaining(storm, SCUD) == 9000,
        "Scud Storm completion did not begin its original five-minute public countdown",
    )?;
    let point = scout_scud_target(game, worker)?;
    require(
        game.cast_power(storm, SCUD, PowerTarget::Position3D(point))
            .is_err(),
        "charging Scud fired early",
    )?;
    advance_until(game, 9001, "Scud recharging", |game| {
        game.can_use_power(storm, SCUD)
    })?;
    game.cast_power(storm, SCUD, PowerTarget::Position3D(point))?;
    require(
        game.cast_power(storm, SCUD, PowerTarget::Position3D(point))
            .is_err(),
        "Scud restarted an active volley",
    )?;
    let decal = game.entities[storm]
        .radius_decal()
        .ok_or("missing active Scud targeting decal")?;
    require(
        decal.style.only_owner
            && decal.style.texture == "SCCScudStorm_GLA"
            && decal.radius.to_bits() == 200.0_f32.to_bits(),
        "Scud used different original targeting decal data",
    )?;

    let missile_type = template(game, "ScudStormMissile")?;
    let mut launched = BTreeSet::new();
    let mut ignited = BTreeSet::new();
    let mut impacted = BTreeSet::new();
    for _ in 0..900 {
        game.step();
        for projectile in game
            .projectiles
            .iter()
            .filter(|projectile| projectile.source == storm)
        {
            let object = projectile
                .object
                .ok_or("Scud projectile has no physical object")?;
            assert_eq!(game.entities[object].type_index, missile_type);
            assert_eq!(projectile.owner, 0);
            launched.insert(projectile.id);
        }
        for event in &game.presentation.weapon_fx {
            if let WeaponFxEvent::Projectile(event) = event
                && event.source == storm
                && matches!(event.stage, ProjectileFxStage::Ignition { .. })
            {
                ignited.insert(event.projectile);
            }
        }
        for shot in &game.presentation.shots {
            if game.entities[shot.source].type_index == missile_type {
                let weapon = game.catalog[shot.weapon_type]
                    .weapon_template(shot.weapon_index)
                    .unwrap();
                assert_eq!(weapon.name, "ScudStormDamageWeapon");
                assert_eq!(shot.owner, 0);
                assert!(
                    game.hazards
                        .iter()
                        .any(|field| field.source == shot.source && field.owner == 0)
                );
                impacted.insert(shot.source);
            }
        }
        if launched.len() == 9
            && impacted.len() == 9
            && game.entities[storm].radius_decal().is_none()
        {
            break;
        }
    }
    assert_eq!(launched.len(), 9);
    assert_eq!(ignited, launched);
    assert_eq!(impacted.len(), 9);
    verify_scud_research_during_flight_and_capture(game, storm, point)
}

fn verify_scud_research_during_flight_and_capture(
    game: &mut Skirmish,
    storm: usize,
    point: [f32; 3],
) -> Result<(), Box<dyn Error>> {
    const BETA: &str = "Upgrade_GLAAnthraxBeta";
    let palace = friendly_entity_named(game, "GLAPalace")?;
    let upgrade = game.catalog[game.entities[palace].type_index]
        .maintenance
        .research
        .iter()
        .position(|entry| entry.name == BETA)
        .ok_or("Palace lacks original Anthrax Beta research")?;
    let cost = game.catalog[game.entities[palace].type_index]
        .maintenance
        .research[upgrade]
        .cost;
    let captor = prepare_scud_captor(game, storm)?;
    advance_until(
        game,
        18_000,
        "ordinary funds for Anthrax Beta and Scud recharge",
        |game| game.players[0].cash >= cost && game.can_use_power(storm, SCUD),
    )?;
    let money = game.players[0].cash;
    game.queue_upgrade(palace, upgrade)?;
    assert_eq!(game.players[0].cash, money - cost);
    advance_until(
        game,
        900,
        "Anthrax Beta approaching its paid completion",
        |game| {
            game.entities[palace].queue.iter().any(|entry| {
                matches!(entry.item, ProductionItem::Upgrade(index) if index == upgrade)
                // The native Scud prefire is 90 frames and HeightDie
                // refuses to detonate until it has been airborne for 30.
                && entry.remaining_seconds <= 3.5
            })
        },
    )?;
    game.cast_power(storm, SCUD, PowerTarget::Position3D(point))?;
    advance_until(
        game,
        150,
        "a physical Scud missile leaves before research completes",
        |game| {
            game.projectiles
                .iter()
                .any(|projectile| projectile.source == storm)
        },
    )?;
    assert!(!game.has_upgrade(0, BETA));
    let missile = game
        .projectiles
        .iter()
        .find(|missile| missile.source == storm)
        .and_then(|missile| missile.object)
        .ok_or("missile has no native body")?;
    advance_until(
        game,
        500,
        "in-flight missile detonates under completed Anthrax Beta",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == missile)
        },
    )?;
    assert!(game.has_upgrade(0, BETA));
    let upgraded = game
        .presentation
        .shots
        .iter()
        .find(|shot| shot.source == missile)
        .unwrap();
    assert_eq!(
        game.catalog[upgraded.weapon_type]
            .weapon_template(upgraded.weapon_index)
            .unwrap()
            .name,
        "ScudStormDamageWeaponUpgraded"
    );
    assert_eq!(upgraded.owner, 0);
    assert!(
        game.hazards
            .iter()
            .any(|field| field.source == missile && field.owner == 0)
    );

    verify_scud_capture_in_flight(game, storm, point, captor)
}

fn verify_scud_capture_in_flight(
    game: &mut Skirmish,
    storm: usize,
    point: [f32; 3],
    captor: usize,
) -> Result<(), Box<dyn Error>> {
    advance_until(game, 9001, "post-Beta Scud recharge", |game| {
        game.can_use_power(storm, SCUD)
    })?;
    game.capture(captor, storm)?;
    advance_until(
        game,
        3600,
        "Red Guard starts its authored Scud capture preparation",
        |game| game.entities[captor].capture_condition() == Some("RAISING_FLAG"),
    )?;
    // Red Guard preparation is 600 frames. Fire 470 frames into that genuine
    // capture action to allow the 90-frame Scud prefire before ownership flips.
    for _ in 0..470 {
        game.step();
    }
    game.cast_power(storm, SCUD, PowerTarget::Position3D(point))?;
    advance_until(
        game,
        151,
        "Scud Storm captured during the launched volley",
        |game| game.entities[storm].owner == 1,
    )?;
    let missile = game
        .projectiles
        .iter()
        .find(|missile| missile.source == storm && missile.owner == 0)
        .and_then(|missile| missile.object)
        .ok_or("capture lost all previously launched native missiles")?;
    assert!(game.power_cooldown_remaining(storm, SCUD) > 0);
    advance_until(
        game,
        800,
        "old-owner Scud rocket lands after provider capture",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == missile)
        },
    )?;
    let old_owner = game
        .presentation
        .shots
        .iter()
        .find(|shot| shot.source == missile)
        .unwrap();
    assert_eq!(old_owner.owner, 0);
    assert_eq!(
        game.catalog[old_owner.weapon_type]
            .weapon_template(old_owner.weapon_index)
            .unwrap()
            .name,
        "ScudStormDamageWeaponUpgraded"
    );
    assert!(!game.has_upgrade(1, "Upgrade_GLAAnthraxBeta"));
    Ok(())
}

fn friendly_entity_named(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    let kind = template(game, name)?;
    game.entities
        .iter()
        .position(|actor| actor.owner == 0 && actor.type_index == kind && actor.complete())
        .ok_or_else(|| format!("missing completed {name}").into())
}

fn prepare_scud_captor(game: &mut Skirmish, storm: usize) -> Result<usize, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    construct(game, builder, Kind::SupplyCenter, 1)?;
    let barracks = construct(game, builder, Kind::Barracks, 1)?;
    research(game, barracks, "Upgrade_InfantryCaptureBuilding")?;
    let guard = produce_type(game, barracks, template(game, "ChinaInfantryRedguard")?, 1)?;
    let target = game.entities[storm].position;
    let from = game.entities[guard].position;
    let delta = [from[0] - target[0], from[1] - target[1]];
    let distance = delta[0].hypot(delta[1]);
    let offset = delta.map(|axis| axis * (90.0 / distance));
    let destination = [target[0] + offset[0], target[1] + offset[1]];
    game.move_to(guard, destination)?;
    advance_until(
        game,
        18_000,
        "Red Guard scouting the original Scud capture target",
        |game| game.can_capture(guard, storm),
    )?;
    game.stop(guard)?;
    Ok(guard)
}

#[test]
#[ignore = "requires original game installation"]
fn paid_radar_research_unpauses_existing_and_future_vans_independently()
-> Result<(), Box<dyn Error>> {
    const POWER: &str = CaptureKind::Radar.power();
    let (_, mut battle) = installed();
    let game = &mut battle.simulation;
    game.reset(Faction::Gla, Faction::China)?;
    let (van, future) = paid_radar(game)?;
    advance_until(game, 901, "each Radar Van charging independently", |game| {
        game.can_use_power(van, POWER) && game.can_use_power(future, POWER)
    })?;
    let point = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    let marker_type = template(game, "RadarVanPing")?;
    game.cast_power(van, POWER, PowerTarget::Position(point))?;
    assert!(!game.can_use_power(van, POWER));
    assert!(game.can_use_power(future, POWER));
    let marker = game
        .entities
        .iter()
        .position(|actor| actor.type_index == marker_type && actor.alive())
        .unwrap();
    assert_eq!(game.entities[marker].owner, 0);
    advance_until(game, 2, "Radar Van's ordinary clear vision", |game| {
        matches!(
            game.visibility_cell(0, point),
            generals_sim::skirmish::visibility::Status::Clear
        )
    })?;
    advance_until(game, 302, "original radar marker deleting", |game| {
        !game.entities[marker].exists()
    })?;
    Ok(())
}

#[test]
#[ignore = "requires original game installation"]
fn earned_cash_bounty_ranks_pay_their_original_percentages() -> Result<(), Box<dyn Error>> {
    let (_, mut battle) = installed();
    let game = &mut battle.simulation;
    game.set_match_options(50_000, 1);
    game.reset(Faction::Gla, Faction::China)?;
    crate::progression_verification::verify_cash_bounty_ranks(game)
}

#[test]
#[ignore = "requires original game installation"]
fn earned_gla_powers_create_their_original_ambush_cloak_tunnel_and_anthrax_payload()
-> Result<(), Box<dyn Error>> {
    let (assets, mut battle) = installed();
    let game = &mut battle.simulation;
    game.set_match_options(50_000, 1);
    game.reset(Faction::Gla, Faction::China)?;
    assert!(!game.can_purchase_science(0, "SCIENCE_AnthraxBomb"));
    let (army, _) = earn_combat_rank(game, 5)?;
    for science in [
        "SCIENCE_GPSScrambler",
        "SCIENCE_SneakAttack",
        "SCIENCE_AnthraxBomb",
    ] {
        let points = game.players[0].science_points;
        game.purchase_science(0, science)?;
        assert_eq!(game.players[0].science_points, points - 1);
    }
    let command = friendly_entity(game, Kind::CommandCenter, 0)?;
    verify_ambush(game, command, army[0])?;
    verify_gps(game, command, army[0])?;
    verify_sneak(&mut battle, &assets, command, army[0])?;
    verify_anthrax(&mut battle.simulation, command)?;
    verify_anthrax_interception(&mut battle.simulation, command, &army)
}

fn verify_ambush(game: &mut Skirmish, command: usize, tank: usize) -> Result<(), Box<dyn Error>> {
    let rebel_type = template(game, "GLAInfantryRebel")?;
    assert!(!game.can_use_power(command, AMBUSH));
    for (science, count) in [
        ("SCIENCE_RebelAmbush1", 4),
        ("SCIENCE_RebelAmbush2", 8),
        ("SCIENCE_RebelAmbush3", 16),
    ] {
        let points = game.players[0].science_points;
        game.purchase_science(0, science)?;
        assert_eq!(game.players[0].science_points, points - 1);
        if count > 4 {
            assert_eq!(game.power_cooldown_remaining(command, AMBUSH), 7200);
        }
        advance_until(game, 7201, "earned Ambush recharged", |game| {
            game.can_use_power(command, AMBUSH)
        })?;
        let point = game.entities[tank].position;
        let first = game.entities.len();
        game.cast_power(command, AMBUSH, PowerTarget::Position(point))?;
        let rebels: Vec<_> = game.entities[first..]
            .iter()
            .filter(|actor| actor.alive() && actor.type_index == rebel_type && actor.owner == 0)
            .collect();
        assert_eq!(rebels.len(), count);
        assert!(
            rebels
                .iter()
                .all(|actor| { actor.ocl_fade_opacity(game.tick).to_bits() == 0.0_f32.to_bits() })
        );
        assert_eq!(game.power_cooldown_remaining(command, AMBUSH), 7200);
    }
    Ok(())
}

fn verify_gps(game: &mut Skirmish, command: usize, tank: usize) -> Result<(), Box<dyn Error>> {
    advance_until(game, 7201, "earned GPS ready", |game| {
        game.can_use_power(command, GPS)
    })?;
    let point = game.entities[tank].position;
    let marker_type = template(game, "GPSScrambler_InvisibleMarker")?;
    assert!(!game.entities[tank].stealthed());
    game.cast_power(command, GPS, PowerTarget::Position(point))?;
    let marker = game
        .entities
        .iter()
        .position(|actor| actor.alive() && actor.type_index == marker_type)
        .ok_or("GPS emitted no original marker")?;
    advance_until(
        game,
        3,
        "GPS permanently grants the actual tank stealth",
        |game| {
            game.entities[tank].stealthed() && game.presentation.stealth_events.iter().any(|event| {
            matches!(event, stealth::Event::GrantFlash { object } if *object == tank)
        })
        },
    )?;
    advance_until(game, 8, "GPS marker retires after final scan", |game| {
        !game.entities[marker].exists()
    })?;
    Ok(())
}

fn verify_sneak(
    battle: &mut crate::playtest::Match,
    assets: &generals_assets::virtual_assets::VirtualAssets,
    command: usize,
    tank: usize,
) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::Barracks, 0)?;
    let tunnel = construct_type(game, worker, template(game, "GLATunnelNetwork")?, 0)?;
    game.enter(tank, tunnel)?;
    advance_until(game, 6000, "ordinary tank entry into paid tunnel", |game| {
        game.entities[tank].contained_by() == Some(tunnel)
    })?;
    advance_until(game, 4501, "earned Sneak Attack ready", |game| {
        game.can_use_power(command, SNEAK)
    })?;
    let tunnel_type = template(game, "GLASneakAttackTunnelNetwork")?;
    let rotation = std::f32::consts::FRAC_PI_4;
    let angle = game.catalog[tunnel_type].placement_angle + rotation;
    let desired = crate::p03_gla_capture::sneak_site(game, command, worker, tunnel, rotation)?;
    require(
        game.can_use_power_at_location(command, SNEAK, desired),
        "ordinary prepared Sneak Attack site rejected",
    )?;
    crate::input::placement::tests::verify_sneak_release(
        battle,
        assets,
        command,
        tunnel_type,
        [desired[0], desired[1]],
        angle,
    )?;
    let game = &mut battle.simulation;
    let start = game.tick;
    let start_type = template(game, "GLASneakAttackTunnelNetworkStart")?;
    let marker = game
        .entities
        .iter()
        .position(|actor| actor.alive() && actor.type_index == start_type)
        .ok_or("Sneak Attack emitted no original entrance marker")?;
    assert!(!game.is_tunnel(marker));
    assert!((game.entities[marker].facing - angle).abs() < 0.000_001);
    advance_until(
        game,
        151,
        "original five-second Sneak Attack buildout",
        |game| !game.entities[marker].exists(),
    )?;
    assert!((150..=151).contains(&(game.tick - start)));
    let exit = game
        .entities
        .iter()
        .position(|actor| actor.alive() && actor.type_index == tunnel_type)
        .ok_or("Sneak Attack did not open original remote tunnel")?;
    assert!(game.is_tunnel(exit));
    assert_eq!(game.entities[exit].owner, 0);
    assert!(game.passengers(exit).contains(&tank));
    game.exit_container_at(tank, exit)?;
    advance_until(game, 300, "original remote tank exit", |game| {
        game.entities[tank].contained_by().is_none()
    })
}

fn verify_anthrax(game: &mut Skirmish, command: usize) -> Result<(), Box<dyn Error>> {
    advance_until(game, 10_801, "earned Anthrax Bomb ready", |game| {
        game.can_use_power(command, ANTHRAX)
    })?;
    let center = game.entities[command].position;
    let point = [center[0] + 130.0, center[1] + 70.0];
    let (cargo, bomb) = launch_anthrax(game, command, point)?;
    assert!(game.entities[cargo].radius_decal().is_some());
    advance_until(
        game,
        3000,
        "physical Anthrax bomb leaves cargo plane",
        |game| game.entities[bomb].alive() && game.entities[bomb].contained_by().is_none(),
    )?;
    advance_until(
        game,
        1200,
        "physical Anthrax bomb creates actual toxin field",
        |game| {
            game.hazards
                .iter()
                .any(|field| field.source == bomb && field.owner == 0)
        },
    )?;
    assert!(!game.entities[bomb].alive());
    let field = game
        .hazards
        .iter()
        .find(|field| field.source == bomb)
        .unwrap();
    assert_eq!(field.expires - field.created, 1800);
    Ok(())
}

fn launch_anthrax(
    game: &mut Skirmish,
    command: usize,
    point: [f32; 2],
) -> Result<(usize, usize), Box<dyn Error>> {
    let first = game.entities.len();
    game.cast_power(command, ANTHRAX, PowerTarget::Position(point))?;
    let cargo_type = template(game, "GLAJetCargoPlane")?;
    let bomb_type = template(game, "AnthraxBomb")?;
    let cargo = game.entities[first..]
        .iter()
        .position(|actor| actor.type_index == cargo_type && actor.alive())
        .map(|index| first + index)
        .ok_or("Anthrax used no original plane")?;
    let bomb = game.entities[first..]
        .iter()
        .position(|actor| actor.type_index == bomb_type && actor.alive())
        .map(|index| first + index)
        .ok_or("Anthrax used no original bomb")?;
    assert_eq!(game.entities[bomb].contained_by(), Some(cargo));
    Ok((cargo, bomb))
}

fn verify_anthrax_interception(
    game: &mut Skirmish,
    command: usize,
    army: &[usize],
) -> Result<(), Box<dyn Error>> {
    // Tournament Desert's original center lane is flat from the GLA start
    // through this target. The carrier approaches from the low-Y map edge.
    let lane = game.entities[command].position[0];
    let point = [lane, 1370.0];
    if matches!(
        game.visibility_cell(0, point),
        generals_sim::skirmish::visibility::Status::Shrouded
    ) {
        let scout = army
            .iter()
            .copied()
            .find(|&id| game.entities[id].complete())
            .ok_or("no surviving promotion tank to scout the Anthrax interception point")?;
        game.move_to(scout, point)?;
        advance_until(
            game,
            5400,
            "paid GLA tank scouting the ordinary Anthrax strike point",
            |game| {
                !matches!(
                    game.visibility_cell(0, point),
                    generals_sim::skirmish::visibility::Status::Shrouded
                )
            },
        )?;
        game.move_to(scout, game.entities[command].position)?;
    }
    let guns = stage_anthrax_interceptors(game, [lane, 1080.0])?;
    advance_until(game, 10_801, "intercepted Anthrax sortie ready", |game| {
        game.can_use_power_at_location(
            command,
            ANTHRAX,
            [point[0], point[1], game.ground_height(point)],
        )
    })?;
    let (cargo, bomb) = launch_anthrax(game, command, point)?;
    advance_until(
        game,
        1500,
        "visible loaded Anthrax plane entering AA range",
        |game| {
            game.visible_to(1, cargo)
                && guns
                    .iter()
                    .all(|&gun| game.player_attack_target_reachable(gun, cargo))
                || game.entities[bomb].contained_by() != Some(cargo)
        },
    )?;
    assert_eq!(game.entities[bomb].contained_by(), Some(cargo));
    for gun in guns {
        game.attack(gun, cargo)?;
    }
    for _ in 0..600 {
        require_no_anthrax_detonation(game, bomb)?;
        if !game.entities[cargo].alive() {
            require(
                game.entities[cargo]
                    .last_attacker()
                    .is_some_and(|id| guns.contains(&id)),
                "the paid Gattling Tanks did not kill the original plane",
            )?;
            return verify_intercepted_anthrax_cleanup(game, command, cargo, bomb);
        }
        assert_eq!(
            game.entities[bomb].contained_by(),
            Some(cargo),
            "AA arrived after the bomb released"
        );
        game.step();
    }
    Err("paid AA failed to intercept the Anthrax flight".into())
}

fn verify_intercepted_anthrax_cleanup(
    game: &mut Skirmish,
    command: usize,
    cargo: usize,
    bomb: usize,
) -> Result<(), Box<dyn Error>> {
    for _ in 0..600 {
        require_no_anthrax_detonation(game, bomb)?;
        if !game.entities[cargo].exists() && !game.entities[bomb].exists() {
            assert!(game.entities[cargo].radius_decal().is_none());
            assert!(game.power_cooldown_remaining(command, ANTHRAX) > 0);
            return Ok(());
        }
        game.step();
    }
    Err("intercepted carrier and its unreleased payload did not retire".into())
}

fn require_no_anthrax_detonation(game: &Skirmish, bomb: usize) -> Result<(), Box<dyn Error>> {
    require(
        !game.hazards.iter().any(|field| field.source == bomb)
            && !game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == bomb),
        "destroyed, held Anthrax payload incorrectly detonated",
    )
}

fn stage_anthrax_interceptors(
    game: &mut Skirmish,
    point: [f32; 2],
) -> Result<[usize; 6], Box<dyn Error>> {
    let dozer = construction::builder(game)
        .map_err(|error| format!("Anthrax interception needs a paid China dozer: {error}"))?;
    let factory = construct(game, dozer, Kind::Factory, 1)?;
    let kind = template(game, "ChinaTankGattling")?;
    let mut guns = [0; 6];
    for (gun, offset) in guns.iter_mut().zip([-70.0, -42.0, -14.0, 14.0, 42.0, 70.0]) {
        *gun = produce_type(game, factory, kind, 1)?;
        game.move_to(*gun, [point[0] + offset, point[1]])?;
    }
    advance_until(
        game,
        3600,
        "paid Gattling Tanks reaching the Anthrax flight lane",
        |game| {
            guns.iter().all(|&gun| {
                let unit = &game.entities[gun];
                unit.alive()
                    && !unit.moving()
                    && (unit.position[0] - point[0]).hypot(unit.position[1] - point[1]) <= 100.0
            })
        },
    )?;
    Ok(guns)
}

#[test]
#[ignore = "requires original game installation"]
fn paid_rebel_research_survives_cancellation_control_and_new_production()
-> Result<(), Box<dyn Error>> {
    const CAMO: &str = "Upgrade_GLACamouflage";
    const TRAP: &str = "Upgrade_GLAInfantryRebelBoobyTrapAttack";
    let (assets, mut battle) = installed();
    let game = &mut battle.simulation;
    game.reset(Faction::Gla, Faction::China)?;
    let (_, barracks, _, palace) = gla_technology(game)?;
    let rebel_type = template(game, "GLAInfantryRebel")?;
    let rebel = produce_type(game, barracks, rebel_type, 0)?;
    let command = friendly_entity(game, Kind::CommandCenter, 0)?;
    assert!(!game.entities[rebel].stealthed());
    assert!(
        game.cast_power(rebel, BOOBY, PowerTarget::Object(command))
            .is_err()
    );

    let upgrade = game.catalog[game.entities[barracks].type_index]
        .maintenance
        .research
        .iter()
        .position(|entry| entry.name == TRAP)
        .ok_or("Barracks lacks original Booby Trap research")?;
    advance_until(game, 18000, "ordinary income funding Booby Trap", |game| {
        game.players[0].cash >= 1000
    })?;
    let cash = game.players[0].cash;
    game.queue_upgrade(barracks, upgrade)?;
    assert_eq!(game.players[0].cash, cash - 1000);
    assert!(!game.can_research(barracks, upgrade));
    game.step();
    let cash = game.players[0].cash;
    game.cancel_upgrade(0, barracks, upgrade)?;
    assert_eq!(game.players[0].cash, cash + 1000);
    assert!(!game.has_upgrade(0, TRAP));
    assert!(
        game.cast_power(rebel, BOOBY, PowerTarget::Object(command))
            .is_err()
    );

    research(game, barracks, TRAP)?;
    research(game, palace, CAMO)?;
    let future = produce_type(game, barracks, rebel_type, 0)?;
    advance_until(game, 180, "existing and new paid Rebels conceal", |game| {
        game.entities[rebel].stealthed() && game.entities[future].stealthed()
    })?;
    battle.select_objects(&[rebel], true);
    let root = generals_assets::game_root()?;
    let mut mouse = crate::input::load(
        &assets,
        &root.join("UserData/Options.ini"),
        &battle.simulation.catalog,
    )?;
    crate::playtest_ui::dispatch_command(
        &mut battle,
        &mut mouse,
        crate::control_bar::CommandAction::NativePower(crate::control_bar::PowerCommand {
            name: BOOBY,
            aim: crate::control_bar::PowerAim::Object,
        }),
    );
    assert!(matches!(
        mouse.voices.as_slice(),
        [voice] if matches!(voice.voice(), crate::source_audio::Voice::BoobyTrapMode)
    ));
    assert!(!battle.simulation.entities[command].booby_trapped());
    let point = battle.simulation.entities[command].position;
    crate::input::targeted_order(&mut battle, &mut mouse, point.into(), Some(command));
    crate::input::commands::step(&mut battle, &mut mouse);
    let game = &mut battle.simulation;
    advance_until(
        game,
        1200,
        "original Rebel installs the charged Booby Trap",
        |game| game.entities[command].booby_trapped(),
    )?;
    assert!(
        game.cast_power(future, BOOBY, PowerTarget::Object(command))
            .is_err()
    );
    Ok(())
}
