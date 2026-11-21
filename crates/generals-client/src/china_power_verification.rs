//! Paid ordinary China research and power acceptance against installed objects.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::{research, template},
};
#[cfg(test)]
use generals_sim::skirmish::PowerTarget;
use generals_sim::skirmish::{Faction, Kind, Skirmish, visibility::Status};
use std::error::Error;

#[cfg(test)]
mod tanks;

pub const MISSILE_POWER: &str = "SuperweaponNeutronMissile";

pub fn prepare_launcher(game: &mut Skirmish) -> Result<(usize, usize, usize), Box<dyn Error>> {
    game.set_match_options(50_000, 1);
    game.reset(Faction::China, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    for _ in 0..3 {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let launcher = template(game, "ChinaNuclearMissileLauncher")?;
    let candidate = game
        .placement_near(launcher, game.entities[builder].position)
        .ok_or("no candidate nuclear launcher location")?;
    let spent = game.players[0].statistics.money_spent;
    require(
        game.build(builder, launcher, candidate).is_err()
            && game.players[0].statistics.money_spent == spent,
        "nuclear launcher was accepted before a Propaganda Center",
    )?;
    construct_named(game, builder, "ChinaPropagandaCenter")?;
    let launcher = construct_type(game, builder, launcher, 0)?;
    require(
        game.power_cooldown_remaining(launcher, MISSILE_POWER) > 10_790,
        "completed nuclear launcher skipped the original six-minute countdown",
    )?;
    let launcher_type = game.entities[launcher].type_index;
    let candidate = game
        .placement_near(launcher_type, game.entities[builder].position)
        .ok_or("no candidate second nuclear launcher location")?;
    let spent = game.players[0].statistics.money_spent;
    require(
        game.build(builder, launcher_type, candidate).is_err()
            && game.players[0].statistics.money_spent == spent,
        "nuclear launcher bypassed the selected superweapon limit",
    )?;
    Ok((launcher, builder, factory))
}

pub fn scout_launcher_target(
    game: &mut Skirmish,
    builder: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    scout_target_point(game, builder, game.entities[target].position)
}

pub fn cluster_target_point(game: &Skirmish, target: usize) -> Result<[f32; 2], Box<dyn Error>> {
    let barracks = template(game, "ChinaBarracks")?;
    game.placement_near(barracks, game.entities[target].position)
        .ok_or_else(|| "no clear land beside the occupied Cluster Mines target".into())
}

pub fn scout_target_point(
    game: &mut Skirmish,
    builder: usize,
    point: [f32; 2],
) -> Result<(), Box<dyn Error>> {
    let start = game.entities[builder].position;
    let delta = [start[0] - point[0], start[1] - point[1]];
    let length = delta[0].hypot(delta[1]);
    let destination = std::array::from_fn(|axis| delta[axis].mul_add(110.0 / length, point[axis]));
    game.move_to(builder, destination)?;
    advance_until(
        game,
        5400,
        "dozer scouting the original power target",
        |game| matches!(game.visibility_cell(0, point), Status::Clear),
    )?;
    game.stop(builder)?;
    Ok(())
}

pub fn prepare_cannon(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let (launcher, _, factory) = prepare_launcher(game)?;
    game.purchase_science(0, "SCIENCE_NukeLauncher")?;
    let kind = template(game, "ChinaVehicleNukeLauncher")?;
    let cannon = produce_type(game, factory, kind, 0)?;
    research(game, launcher, "Upgrade_ChinaNeutronShells")?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    approach_cannon_target(game, cannon, target)?;
    Ok((cannon, target))
}

fn construct_named(
    game: &mut Skirmish,
    builder: usize,
    name: &str,
) -> Result<usize, Box<dyn Error>> {
    let kind = template(game, name)?;
    construct_type(game, builder, kind, 0)
}

#[cfg(test)]
fn verify_research(
    game: &mut Skirmish,
    launcher: usize,
    builder: usize,
    factory: usize,
) -> Result<(), Box<dyn Error>> {
    let airfield = construct_named(game, builder, "ChinaAirfield")?;
    let internet = construct_named(game, builder, "ChinaInternetCenter")?;
    for (building, before, after) in [
        (
            airfield,
            "ChinaAirfieldCommandSetUpgrade",
            "ChinaAirfieldCommandSetUpgrade",
        ),
        (
            internet,
            "ChinaInternetCenterCommandSetOneUpgrade",
            "ChinaInternetCenterCommandSetTwoUpgrade",
        ),
        (
            launcher,
            "ChinaNuclearMissileCommandSetUpgrade",
            "ChinaNuclearMissileCommandSetUpgrade",
        ),
    ] {
        let normal = live_count(game, "ChinaStandardMine");
        research(game, building, "Upgrade_ChinaMines")?;
        advance_until(
            game,
            120,
            "authored standard minefield generation",
            |game| {
                game.entities[building].command_set_override() == Some(before)
                    && live_count(game, "ChinaStandardMine") > normal
            },
        )?;
        if building == internet {
            research(game, internet, "Upgrade_ChinaSatelliteHackOne")?;
            research(game, internet, "Upgrade_ChinaSatelliteHackTwo")?;
            require(
                game.entities[internet]
                    .upgrade_conditions(game.tick)
                    .contains(&"USER_2"),
                "mine command state detached the second dish model",
            )?;
        }
        let emp = live_count(game, "ChinaEMPMine");
        research(game, building, "Upgrade_ChinaEMPMines")?;
        advance_until(game, 120, "authored EMP minefield replacement", |game| {
            game.entities[building].command_set_override() == Some(after)
                && live_count(game, "ChinaEMPMine") > emp
                && live_count(game, "ChinaStandardMine") == normal
        })?;
    }
    let cannon = template(game, "ChinaVehicleNukeLauncher")?;
    let funds = game.players[0].cash;
    require(
        game.train(factory, cannon).is_err() && game.players[0].cash == funds,
        "Nuke Cannon bypassed unpaid original science",
    )?;
    let science_before = game.players[0].science_points;
    game.purchase_science(0, "SCIENCE_NukeLauncher")?;
    require(
        game.players[0].science_points + 1 == science_before,
        "paid Nuke Cannon did not debit a promotion point",
    )?;
    let cannon = produce_type(game, factory, cannon, 0)?;
    let tank_type = template(game, "ChinaTankBattleMaster")?;
    let existing = produce_type(game, factory, tank_type, 0)?;
    research(game, launcher, "Upgrade_ChinaUraniumShells")?;
    research(game, launcher, "Upgrade_ChinaNuclearTanks")?;
    research(game, launcher, "Upgrade_ChinaNeutronShells")?;
    let later = produce_type(game, factory, tank_type, 0)?;
    require(
        game.has_upgrade(0, "Upgrade_ChinaUraniumShells")
            && game.has_upgrade(0, "Upgrade_ChinaNuclearTanks")
            && game.has_upgrade(0, "Upgrade_ChinaNeutronShells")
            && game.entities[existing].alive()
            && game.entities[later].alive(),
        "launcher research did not persist at player scope across production",
    )?;
    let (primary, secondary) = ("NukeCannonGun", "NukeCannonNeutronWeapon");
    let kind = &game.catalog[game.entities[cannon].type_index];
    require(
        kind.weapon_set_rules.share_reload
            && kind.weapon.name == primary
            && kind
                .secondary_weapon
                .as_ref()
                .is_some_and(|weapon| weapon.name == secondary),
        "Nuke Cannon lost authored warheads or shared reload",
    )?;
    verify_cannon_shells(game, cannon, factory)
}

#[cfg(test)]
fn verify_cannon_shells(
    game: &mut Skirmish,
    cannon: usize,
    factory: usize,
) -> Result<(), Box<dyn Error>> {
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    approach_cannon_target(game, cannon, target)?;
    game.switch_weapon(cannon, 0)?;
    game.attack(cannon, target)?;
    advance_until(
        game,
        1600,
        "deployed Nuke Cannon's first nuclear shell",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == cannon && shot.weapon_slot == 0)
                && game.entities[cannon].deployment_condition() == Some("DEPLOYED")
        },
    )?;
    let fired = game.tick;
    game.switch_weapon(cannon, 1)?;
    game.attack(cannon, target)?;
    advance_until(
        game,
        300,
        "nuclear shell impact and medium radiation",
        |game| live_count(game, "RadiationFieldMedium") > 0,
    )?;
    let shell = wait_for_neutron_shell(game, cannon, target)?;
    require(
        game.tick - fired >= 300 && live_count(game, "NeutronCannonShell") > 0,
        "neutron selection bypassed the shared weapon reload or fired another projectile",
    )?;
    let center = game.entities[target].position;
    let position = game.entities[cannon].position;
    let outward = [position[0] - center[0], position[1] - center[1]];
    let gap = outward[0].hypot(outward[1]);
    let retreat = std::array::from_fn(|axis| outward[axis].mul_add(370.0 / gap, position[axis]));
    // Keep the cannon on the same open approach lane and retreat before
    // funding and moving the Outpost while the defender is still active.
    verify_neutron_impact(game, cannon, shell, center)?;
    game.move_to(cannon, retreat)?;
    advance_until(
        game,
        3600,
        "Cannon withdraws to friendly crew-test lane",
        |game| {
            let position = game.entities[cannon].position;
            !game.entities[cannon].alive()
                || (position[0] - retreat[0]).hypot(position[1] - retreat[1]) < 20.0
        },
    )?;
    game.stop(cannon).map_err(|error| {
        format!(
            "Cannon withdrawal: {error}; {}",
            cannon_damage(game, cannon)
        )
    })?;
    verify_neutron_crew(game, cannon, factory, target)
}

#[cfg(test)]
fn wait_for_neutron_shell(
    game: &mut Skirmish,
    cannon: usize,
    target: usize,
) -> Result<u64, Box<dyn Error>> {
    advance_until(
        game,
        900,
        "paid neutron shell after shared reload",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == cannon && shot.weapon_slot == 1 && shot.target == Some(target))
        },
    )
    .map_err(|error| {
        format!(
            "{error}; cannon {} at {:?}, deployment {:?}; target {} health {}, active {}, owner {}; {}; {}",
            cannon,
            game.entities[cannon].position,
            game.entities[cannon].deployment_condition(),
            target,
            game.entities[target].health,
            game.entities[target].alive(),
            game.entities[target].owner,
            generals_sim::skirmish::diagnostics::weapon_details(game, cannon, target),
            cannon_damage(game, cannon),
        )
    })?;
    game.projectiles
        .iter()
        .find(|shot| shot.source == cannon && shot.weapon_slot == 1)
        .map(|shot| shot.id)
        .ok_or_else(|| "second Cannon shot did not create an actual projectile".into())
}

#[cfg(test)]
fn cannon_damage(game: &Skirmish, cannon: usize) -> String {
    let actor = &game.entities[cannon];
    let attacker = actor.last_attacker().map(|id| {
        let unit = &game.entities[id];
        (
            id,
            &game.catalog[unit.type_index].name,
            unit.owner,
            game.object_position(id),
        )
    });
    format!(
        "Cannon health {}, death type {}, last attacker {attacker:?}",
        actor.health,
        actor.death_type() as u8
    )
}

#[cfg(test)]
fn verify_neutron_impact(
    game: &mut Skirmish,
    cannon: usize,
    shell: u64,
    target: [f32; 2],
) -> Result<(), Box<dyn Error>> {
    use generals_sim::skirmish::{ProjectileFxStage, WeaponFxEvent};

    let mut impact = None;
    let health = game.entities[cannon].health;
    advance_until(
        game,
        210,
        "paid neutron shell impacts enemy Command Center",
        |game| {
            impact = game
                .presentation
                .weapon_fx
                .iter()
                .find_map(|event| match event {
                    WeaponFxEvent::Projectile(event)
                        if event.projectile == shell
                            && matches!(event.stage, ProjectileFxStage::Detonation) =>
                    {
                        Some(event.position)
                    }
                    _ => None,
                });
            impact.is_some()
        },
    )?;
    let point = impact.expect("verified neutron detonation");
    let distance = |other: [f32; 2]| (point[0] - other[0]).hypot(point[1] - other[1]);
    require(
        distance(target) < 70.0
            && distance(game.entities[cannon].position) > 70.0
            && game.entities[cannon].alive()
            && !game.entities[cannon].unmanned(),
        &format!(
            "unexpected neutron detonation at {point:?} for {target:?}; {}",
            cannon_damage(game, cannon)
        ),
    )?;
    if game.entities[cannon].health.to_bits() != health.to_bits() {
        println!(
            "Cannon health changed before its Command Center neutron hit from {health}; {}",
            cannon_damage(game, cannon)
        );
    }
    Ok(())
}

#[cfg(test)]
fn verify_neutron_crew(
    game: &mut Skirmish,
    cannon: usize,
    factory: usize,
    enemy: usize,
) -> Result<(), Box<dyn Error>> {
    let outpost_type = template(game, "ChinaVehicleListeningOutpost")?;
    let outpost = produce_type(game, factory, outpost_type, 0)
        .map_err(|error| format!("producing occupied neutron target: {error}"))?;
    let crew = game.passengers(outpost).to_vec();
    require(
        crew.len() == 2,
        "paid Outpost lacks its original two passengers",
    )?;
    let position = game.entities[cannon].position;
    let center = game.entities[enemy].position;
    let delta = [position[0] - center[0], position[1] - center[1]];
    let gap = delta[0].hypot(delta[1]);
    // Reuse the original cannon approach lane on the friendly side. Clear the
    // 150-unit minimum after subtracting both bodies' bounding-circle radii.
    let staging = std::array::from_fn(|axis| delta[axis].mul_add(245.0 / gap, position[axis]));
    game.move_to(outpost, staging)
        .map_err(|error| format!("moving occupied neutron target: {error}"))?;
    advance_until(
        game,
        18_000,
        "paid Outpost reaching the cannon lane",
        |game| {
            let position = game.entities[outpost].position;
            (position[0] - staging[0]).hypot(position[1] - staging[1]) < 12.0
        },
    )?;
    game.stop(outpost)
        .map_err(|error| format!("parking occupied neutron target: {error}"))?;
    game.force_attack(cannon, outpost).map_err(|error| {
        format!(
            "force-attacking occupied neutron target: {error}; {}; target health {}, unmanned {}",
            cannon_damage(game, cannon),
            game.entities[outpost].health,
            game.entities[outpost].unmanned(),
        )
    })?;
    let _ = wait_for_neutron_shell(game, cannon, outpost)?;
    advance_until(
        game,
        210,
        "paid neutron shell neutralizes its occupied target",
        |game| game.entities[outpost].unmanned(),
    )?;
    require(
        game.entities[outpost].alive()
            && game.entities[outpost].owner == game.neutral_owner()
            && game.passengers(outpost).is_empty()
            && crew.iter().all(|&id| !game.entities[id].alive()),
        "authored neutron blast failed to kill the crew and leave the vehicle recoverable",
    )
}

fn approach_cannon_target(
    game: &mut Skirmish,
    cannon: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    let center = game.entities[target].position;
    let from = game.entities[cannon].position;
    let delta = [from[0] - center[0], from[1] - center[1]];
    let length = delta[0].hypot(delta[1]);
    let firing_point =
        std::array::from_fn(|axis| delta[axis].mul_add(300.0 / length, center[axis]));
    game.move_to(cannon, firing_point)?;
    advance_until(game, 18_000, "ordinary Nuke Cannon approach", |game| {
        let position = game.entities[cannon].position;
        (position[0] - firing_point[0]).hypot(position[1] - firing_point[1]) < 25.0
    })
}

#[cfg(test)]
fn verify_launcher(
    game: &mut Skirmish,
    launcher: usize,
    builder: usize,
) -> Result<(), Box<dyn Error>> {
    let enemy = friendly_entity(game, Kind::CommandCenter, 1)?;
    scout_launcher_target(game, builder, enemy)?;
    let half = game.entities[enemy].health;
    let target = PowerTarget::Position(game.entities[enemy].position);
    advance_until(game, 11_000, "nuclear launcher door opening", |game| {
        game.model_flags(&game.entities[launcher], [0.5, 0.25])
            .contains("DOOR_1_OPENING")
            || game.can_use_power(launcher, MISSILE_POWER)
    })?;
    advance_until(game, 250, "ready nuclear launcher", |game| {
        game.can_use_power(launcher, MISSILE_POWER)
    })?;
    require(
        game.model_flags(&game.entities[launcher], [0.5, 0.25])
            .contains("DOOR_1_WAITING_OPEN"),
        "ready launcher did not expose the original raised missile",
    )?;
    game.cast_power(launcher, MISSILE_POWER, target)?;
    require(
        game.model_flags(&game.entities[launcher], [0.5, 0.25])
            .contains("DOOR_1_WAITING_TO_CLOSE")
            && game.power_cooldown_remaining(launcher, MISSILE_POWER) == 10_800,
        "launcher firing lost the original door state or countdown reset",
    )?;
    advance_until(
        game,
        90,
        "native neutron missile leaving the silo",
        |game| {
            live_count(game, "NeutronMissile") == 1
                && game.projectiles.iter().any(|projectile| {
                    projectile.source == launcher
                        && projectile.render_transform(&game.entities).is_some()
                })
        },
    )?;
    advance_until(game, 1600, "damage wave and midpoint radiation", |game| {
        game.entities[enemy].health < half && live_count(game, "NukeRadiationFieldWeapon") > 0
    })?;
    require(
        game.completed_special_powers.iter().any(|event| {
            event.object == launcher && event.owner == 0 && event.power == MISSILE_POWER
        }),
        "nuclear missile failed to report completion with the original launcher identity",
    )
}

#[cfg(test)]
fn live_count(game: &Skirmish, name: &str) -> usize {
    game.entities
        .iter()
        .filter(|actor| actor.alive() && game.catalog[actor.type_index].name == name)
        .count()
}

#[cfg(test)]
fn verify_ranked_powers(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    game.set_match_options(50_000, 1);
    game.reset(Faction::China, Faction::China)?;
    let (army, _) = crate::progression_verification::earn_combat_rank(game, 5)?;
    let caster = friendly_entity(game, Kind::CommandCenter, 0)?;
    let enemy = crate::progression_verification::target_supply_center(game)?;
    let scout = friendly_entity(game, Kind::Builder, 0)?;
    scout_launcher_target(game, scout, enemy)?;
    let point = game.entities[enemy].position;
    for science in [
        "SCIENCE_Frenzy1",
        "SCIENCE_ClusterMines",
        "SCIENCE_ArtilleryBarrage1",
        "Early_SCIENCE_ChinaCarpetBomb",
        "SCIENCE_EMPPulse",
        "SCIENCE_CashHack1",
    ] {
        game.purchase_science(0, science)?;
    }
    let fighter = army[0];
    let friendly = game.entities[fighter].position;
    game.cast_power(caster, "SuperweaponFrenzy", PowerTarget::Position(friendly))?;
    advance_until(game, 30, "ranked Frenzy buff and original tint", |game| {
        game.entities[fighter].frenzy_tint()
    })?;
    let own = game.players[0].cash;
    let enemy_cash = game.players[1].cash;
    game.cast_power(caster, "SuperweaponCashHack", PowerTarget::Object(enemy))?;
    require(
        game.players[0].cash == own + enemy_cash.min(1000)
            && game.players[1].cash == enemy_cash.saturating_sub(1000),
        "China ranked Cash Hack did not transfer its source amount",
    )?;
    let target_health = game.entities[enemy].health;
    game.cast_power(caster, "SuperweaponEMPPulse", PowerTarget::Position(point))?;
    advance_until(game, 3600, "delivered EMP spheroid disable", |game| {
        game.entities[enemy].drawable_disabled_tint()
            && game.entities.iter().any(|actor| {
                actor.exists() && game.catalog[actor.type_index].name == "EMPPulseEffectSpheroid"
            })
    })?;
    let cluster = cluster_target_point(game, enemy)?;
    scout_target_point(game, scout, cluster)?;
    let cluster_birth = game.entities.len();
    game.cast_power(
        caster,
        "SuperweaponClusterMines",
        PowerTarget::Position(cluster),
    )?;
    let mut traced = None;
    let mut impact = None;
    advance_until(game, 3600, "delivered cluster mines", |game| {
        let actors = &game.entities[cluster_birth..];
        if let Some(offset) = actors
            .iter()
            .position(|actor| game.catalog[actor.type_index].name == "ClusterMinesBomb")
        {
            let id = cluster_birth + offset;
            let actor = &game.entities[id];
            traced = Some((id, game.object_position(id), actor.alive(), actor.contained_by()));
            if let Some(event) = game.presentation.object_fx_events.iter().find(|event| {
                event.object == id && event.effect == "WeaponFX_ClusterMineImpact"
            }) {
                impact = Some((event.frame, event.position));
            }
        }
        live_count(game, "ChinaClusterMine") > 0
    })
    .map_err(|error| {
        format!("{error}; aimed at {cluster:?}, bomb pose/alive/container {traced:?}, generation FX {impact:?}")
    })?;
    game.cast_power(
        caster,
        "Early_SuperweaponChinaCarpetBomb",
        PowerTarget::Position(point),
    )?;
    advance_until(game, 3600, "ordinary China carpet bomb impacts", |game| {
        game.entities[enemy].health < target_health
            && game
                .entities
                .iter()
                .any(|actor| game.catalog[actor.type_index].name == "ChinaCarpetBomb")
    })?;
    verify_artillery_deliveries(game, caster, point, 12)?;
    advance_until(
        game,
        3600,
        "native Artillery Barrage shells in flight",
        |game| live_count(game, "ChinaArtilleryBarrageShell") > 0,
    )
}

#[cfg(test)]
fn verify_artillery_deliveries(
    game: &mut Skirmish,
    provider: usize,
    point: [f32; 2],
    expected: usize,
) -> Result<(), Box<dyn Error>> {
    let first = game.entities.len();
    game.cast_power(
        provider,
        "SuperweaponArtilleryBarrage",
        PowerTarget::Position(point),
    )?;
    let delivered = game.entities[first..]
        .iter()
        .filter(|actor| game.catalog[actor.type_index].name == "ChinaArtilleryCannon")
        .count();
    require(
        delivered == expected,
        &format!("Artillery Barrage formed {delivered} original carriers, expected {expected}"),
    )
}

#[cfg(test)]
fn prepare_ranked_china(game: &mut Skirmish) -> Result<([usize; 4], usize), Box<dyn Error>> {
    game.set_match_options(20_000, 1);
    game.reset(Faction::China, Faction::China)?;
    let (army, _) = crate::progression_verification::earn_combat_rank(game, 3)?;
    Ok((army, friendly_entity(game, Kind::CommandCenter, 0)?))
}

#[cfg(test)]
fn verify_ranked_artillery(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (_, provider) = prepare_ranked_china(game)?;
    // The actual barrage may level target buildings. Keep the repeat aim in
    // clear ground beside the scouted former construction site.
    let target = crate::progression_verification::target_supply_center(game)?;
    let scout = friendly_entity(game, Kind::Builder, 0)?;
    scout_launcher_target(game, scout, target)?;
    game.move_to(scout, game.entities[provider].position)?;
    let point = game.entities[target].position;
    let power = "SuperweaponArtilleryBarrage";
    game.purchase_science(0, "SCIENCE_ArtilleryBarrage1")?;
    for (science, expected) in [
        ("SCIENCE_ArtilleryBarrage2", 24),
        ("SCIENCE_ArtilleryBarrage3", 36),
    ] {
        game.purchase_science(0, science)?;
        advance_until(game, 9001, "original Artillery Barrage recharge", |game| {
            game.can_use_power(provider, power)
        })?;
        verify_artillery_deliveries(game, provider, point, expected)?;
    }
    Ok(())
}

#[cfg(test)]
fn verify_ranked_frenzy(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (army, provider) = prepare_ranked_china(game)?;
    let fighter = army[0];
    let power = "SuperweaponFrenzy";
    let point = game.entities[fighter].position;
    game.purchase_science(0, "SCIENCE_Frenzy1")?;
    for (science, marker, duration) in [
        ("SCIENCE_Frenzy2", "Frenzy_InvisibleMarker_Level2", 600),
        ("SCIENCE_Frenzy3", "Frenzy_InvisibleMarker_Level3", 900),
    ] {
        game.purchase_science(0, science)?;
        advance_until(game, 7201, "original Frenzy recharge", |game| {
            game.can_use_power(provider, power)
        })?;
        let first = game.entities.len();
        game.cast_power(provider, power, PowerTarget::Position(point))?;
        require(
            game.entities[first..]
                .iter()
                .any(|actor| game.catalog[actor.type_index].name == marker),
            "purchased Frenzy level did not create its original bonus marker",
        )?;
        advance_until(game, 30, "Frenzy actor receives its pulse", |game| {
            game.entities[fighter].frenzy_tint()
        })?;
        let applied = game.tick;
        advance_until(game, duration + 1, "Frenzy bonus expires", |game| {
            !game.entities[fighter].frenzy_tint()
        })?;
        require(
            game.tick - applied == u64::from(duration),
            "purchased Frenzy level changed its original bonus duration",
        )?;
    }
    Ok(())
}

#[cfg(test)]
fn verify_cash_hack_transfer(
    game: &mut Skirmish,
    provider: usize,
    enemy: usize,
    expected: u32,
) -> Result<(), Box<dyn Error>> {
    use generals_sim::skirmish::special_ability::FeedbackEffect;

    let cash = [game.players[0].cash, game.players[1].cash];
    let earned = game.players[0].statistics.money_earned;
    let first = game.presentation.ability_feedback.len();
    game.cast_power(provider, "SuperweaponCashHack", PowerTarget::Object(enemy))?;
    let recorded = game.presentation.ability_feedback[first..]
        .iter()
        .filter_map(|event| match event.effect {
            FeedbackEffect::PowerCashStolen(amount) => Some(amount),
            _ => None,
        });
    require(
        game.players[0].cash - cash[0] == expected
            && cash[1] - game.players[1].cash == expected
            && game.players[0].statistics.money_earned - earned == expected
            && recorded.eq((expected != 0).then_some(expected)),
        "upgraded Cash Hack changed the original debit, credit, score or floating feedback",
    )
}

#[cfg(test)]
fn verify_ranked_cash_hack(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (_, provider) = prepare_ranked_china(game)?;
    let target = crate::progression_verification::target_supply_center(game)?;
    let scout = friendly_entity(game, Kind::Builder, 0)?;
    scout_launcher_target(game, scout, target)?;
    for id in 0..game.entities.len() {
        let actor = &game.entities[id];
        if actor.owner == 1
            && actor.complete()
            && game.catalog[actor.type_index].supply.capacity > 0
        {
            game.stop(id)?;
        }
    }
    game.purchase_science(0, "SCIENCE_CashHack1")?;
    game.purchase_science(0, "SCIENCE_CashHack2")?;
    let power = "SuperweaponCashHack";
    let command = friendly_entity(game, Kind::CommandCenter, 1)?;
    require(
        game.cast_power(provider, power, PowerTarget::Object(command))
            .is_err()
            && game.can_use_power(provider, power),
        "Cash Hack must reject the enemy Command Center without consuming its charge",
    )?;
    require(
        game.players[1].cash >= 6000,
        "ordinary defender spending left too little cash to prove both upgraded amounts",
    )?;
    verify_cash_hack_transfer(game, provider, target, 2000)?;
    game.purchase_science(0, "SCIENCE_CashHack3")?;
    while game.players[1].cash >= 4000 {
        advance_until(game, 7201, "original Cash Hack recharge", |game| {
            game.can_use_power(provider, power)
        })?;
        verify_cash_hack_transfer(game, provider, target, 4000)?;
    }
    // Defender collection was stopped above. Empty the short balance through
    // the same paid, recharged power, then issue it once against an empty ledger.
    for expected in [game.players[1].cash, 0] {
        advance_until(game, 7201, "original Cash Hack recharge", |game| {
            game.can_use_power(provider, power)
        })?;
        require(
            game.players[1].cash == expected,
            "defender income resumed during the spent-cash check",
        )?;
        verify_cash_hack_transfer(game, provider, target, expected)?;
    }
    Ok(())
}

#[ignore = "requires original game installation"]
#[cfg(test)]
#[test]
fn paid_china_nuclear_launcher_countdown_native_blast_and_radiation() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let (launcher, builder, _) = prepare_launcher(&mut battle.simulation).unwrap();
    verify_launcher(&mut battle.simulation, launcher, builder).unwrap();
}

#[ignore = "requires original game installation"]
#[cfg(test)]
#[test]
fn paid_china_mines_and_player_research_reach_authored_consumers() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let (launcher, builder, factory) = prepare_launcher(&mut battle.simulation).unwrap();
    verify_research(&mut battle.simulation, launcher, builder, factory).unwrap();
}

#[ignore = "requires original game installation"]
#[cfg(test)]
#[test]
fn earned_china_promotion_powers_spawn_original_deliveries_and_effects() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    verify_ranked_powers(&mut battle.simulation).unwrap();
}

#[ignore = "requires original game installation"]
#[cfg(test)]
#[test]
fn earned_china_ranked_artillery_frenzy_and_cash_hack_use_native_levels() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    verify_ranked_artillery(game).unwrap();
    verify_ranked_frenzy(game).unwrap();
    verify_ranked_cash_hack(game).unwrap();
}
