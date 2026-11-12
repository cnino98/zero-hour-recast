//! Installed-data checks run by the ordinary-command skirmish diagnostic.
use generals_sim::skirmish::{Faction, Skirmish};
use std::error::Error;
mod construction;
pub mod pathfinder;

pub fn initial(game: &Skirmish) -> Result<(), Box<dyn Error>> {
    for player in &game.players {
        let faction = match player.faction {
            Faction::Usa => "SCIENCE_AMERICA",
            Faction::China => "SCIENCE_CHINA",
            Faction::Gla => "SCIENCE_GLA",
            Faction::Civilian => continue,
        };
        if player.rank != 1
            || player.skill_points != 0
            || player.science_points != 1
            || !player.has_science("SCIENCE_Rank1")
            || !player.has_science(faction)
        {
            return Err("Installed starting rank, point grant or faction science differs from the pinned source data".into());
        }
    }
    if game.can_purchase_science(0, "SCIENCE_EmergencyRepair1") {
        return Err("Rank-three repair was purchasable at the starting rank".into());
    }
    if game.players[0].faction == Faction::Usa && game.can_purchase_science(0, "SCIENCE_Pathfinder")
    {
        return Err(
            "Pathfinder science was purchasable before its original rank-three prerequisite".into(),
        );
    }
    Ok(())
}

pub fn earned(game: &Skirmish, winner: usize) -> Result<(), Box<dyn Error>> {
    // Object::scoreTheKill credits enemy kills. Player::killPlayer clears
    // survivors without an attacker, including the ten Rangers from the
    // USA command center's OCL_AmericanRangerDebris10. Check kill counts
    // against the losing player's losses before applying installed XP values.
    let player = &game.players[winner];
    let stats = &player.statistics;
    let (loser, defeated) = game
        .players
        .iter()
        .enumerate()
        .find(|(owner, enemy)| enemy.playable_side && game.enemies(winner, *owner))
        .ok_or("missing losing playable faction")?;
    let mut earned = 0;
    let mut credited_buildings = 0;
    let mut command_center = false;
    for victim in game.entities.iter().filter(|victim| victim.owner == loser) {
        let unit = &game.catalog[victim.type_index];
        if unit.scoring.victory {
            if victim.alive() || victim.build_progress < 1.0 {
                return Err("surviving or unfinished victory building in the defeated army".into());
            }
            // The GLA command center earns XP but its reconstruction hole owns the
            // SCORE_DESTROY flag. Count building kills and XP separately.
            command_center |= unit.kind == generals_sim::skirmish::Kind::CommandCenter;
            credited_buildings += u32::from(unit.scoring.destroy);
            earned += unit.progression.skill_point_value[usize::from(victim.veterancy)];
        }
    }
    let builder = game
        .entities
        .iter()
        .find(|victim| {
            victim.owner == loser
                && game.catalog[victim.type_index].kind == generals_sim::skirmish::Kind::Builder
        })
        .ok_or("missing defeated starting builder")?;
    if !command_center
        || credited_buildings != stats.buildings_destroyed
        || stats.units_destroyed > 1
        || stats.buildings_destroyed != defeated.statistics.buildings_lost
        || stats.units_destroyed != defeated.statistics.units_lost
    {
        return Err(
            "combat credit differs from the losing command center, hole and starting builder"
                .into(),
        );
    }
    earned += stats.units_destroyed
        * game.catalog[builder.type_index]
            .progression
            .skill_point_value[usize::from(builder.veterancy)];
    let expected = earned.min(5000);
    let rank = 1 + [800, 1500, 2500, 5000]
        .into_iter()
        .filter(|threshold| expected >= *threshold)
        .count();
    let rank = u32::try_from(rank)?;
    let granted = if rank == 5 { 7 } else { rank };
    // The roster check purchases the installed rank-one Marauder unlock (cost 1).
    let points = granted - u32::from(player.has_science("SCIENCE_MarauderTank"));
    if earned == 0
        || player.skill_points != expected
        || player.rank != rank
        || player.science_points != points
    {
        return Err(format!("Installed combat progression mismatch: awarded {} XP/rank {}/{} points; expected {expected}/{rank}/{points} from {} buildings and {} units", player.skill_points, player.rank, player.science_points, stats.buildings_destroyed, stats.units_destroyed).into());
    }
    println!(
        "Source progression: {} kill XP, rank {}, {} available promotion points",
        player.skill_points, player.rank, player.science_points
    );
    Ok(())
}

pub fn exercise(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    use crate::skirmish_verification::{construct, friendly_entity, produce, require};
    use generals_sim::skirmish::{Kind, PowerTarget};

    println!("Checking earned promotions through ordinary construction and combat commands");
    for owner in 0..2 {
        let builder = friendly_entity(game, Kind::Builder, owner)?;
        if game.players[owner].faction != Faction::Gla {
            construct(game, builder, Kind::PowerPlant, owner)?;
        }
        construct(game, builder, Kind::SupplyCenter, owner)?;
    }
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let mut army = Vec::new();
    for _ in 0..4 {
        army.push(produce(game, factory, Kind::Tank, 0)?);
    }
    for &actor in &army {
        game.stop(actor)?;
    }
    crate::skirmish_verification::advance_until(
        game,
        1800,
        "promotion army settling into its individual return positions",
        |game| army.iter().all(|&id| !game.entities[id].moving()),
    )?;
    let retreat: Vec<_> = army.iter().map(|&id| game.entities[id].position).collect();
    for _ in 0..12 {
        if game.players[0].rank >= 3 {
            break;
        }
        let target = target_supply_center(game)?;
        destroy_target(game, &army, target)?;
        retreat_army(game, &army, &retreat, target)?;
    }
    require(
        game.players[0].rank >= 3,
        "ordinary combat did not earn rank-three sciences",
    )?;
    let command_center = friendly_entity(game, Kind::CommandCenter, 0)?;
    game.purchase_science(0, "SCIENCE_EmergencyRepair1")?;
    let target = army[0];
    let before = game.entities[target].health;
    let expected = (before + 100.0).min(game.entities[target].max_health);
    game.cast_power(
        command_center,
        "SuperweaponEmergencyRepair",
        PowerTarget::Position(game.entities[target].position),
    )?;
    require(
        game.power_cooldown_remaining(command_center, "SuperweaponEmergencyRepair") == 7200,
        "installed repair did not begin its four-minute shared recharge",
    )?;
    game.step();
    require(
        (game.entities[target].health - expected).abs() < 0.000_01,
        "installed repair did not respect its source amount and maximum health",
    )?;
    match game.players[0].faction {
        Faction::China => verify_cash_hack(game, command_center)?,
        Faction::Gla => verify_cash_bounty(game, &army)?,
        Faction::Civilian => return Err("Neutral has no player progression".into()),
        Faction::Usa => {
            let remaining =
                game.power_cooldown_remaining(command_center, "SuperweaponEmergencyRepair");
            game.purchase_science(0, "SCIENCE_EmergencyRepair2")?;
            game.purchase_science(0, "SCIENCE_EmergencyRepair3")?;
            require(
                game.power_cooldown_remaining(command_center, "SuperweaponEmergencyRepair")
                    == remaining,
                "repair upgrades reset the existing recharge",
            )?;
            pathfinder::verify(game, &army, &retreat)?;
        }
    }
    require(
        game.ended_at.is_none(),
        "promotion exercise unexpectedly ended the match",
    )?;
    println!(
        "Earned rank {}, purchased native promotion chain, cast source power and verified shared recharge at tick {}",
        game.players[0].rank, game.tick
    );
    Ok(())
}

pub fn retreat_army(
    game: &mut Skirmish,
    army: &[usize],
    retreat: &[[f32; 2]],
    target: usize,
) -> Result<(), Box<dyn Error>> {
    let mut goals = Vec::new();
    crate::skirmish_verification::require(
        army.len() == retreat.len(),
        "Each promotion army member needs its own observed return position",
    )?;
    for (&actor, &position) in army.iter().zip(retreat) {
        game.move_to(actor, position)?;
        let generals_sim::skirmish::Order::Move(goal) = game.entities[actor].ai.order else {
            return Err("Promotion army retreat did not retain its ordinary move order".into());
        };
        goals.push(goal);
    }
    crate::skirmish_verification::advance_until(
        game,
        18_000,
        "army retreat and outstanding shells before rebuilding",
        |game| {
            game.projectiles.is_empty()
                && army.iter().all(|&actor| {
                    let entity = &game.entities[actor];
                    let unit = &game.catalog[entity.type_index];
                    let radius = unit.ai.vision_range
                        + game.catalog[game.entities[target].type_index]
                            .geometry
                            .major;
                    let delta = [
                        entity.position[0] - game.entities[target].position[0],
                        entity.position[1] - game.entities[target].position[1],
                    ];
                    // Native turret opportunity fire continues during Move.
                    // Leaving one dead target's radius is not a completed retreat
                    // and does not protect the next paid construction site.
                    entity.alive()
                        && matches!(entity.ai.order, generals_sim::skirmish::Order::Idle)
                        && !entity.moving()
                        && delta[0].hypot(delta[1]) > radius
                })
        },
    )
    .map_err(|error| {
        format!(
            "{error}; retreat goals={goals:?}; army={:?}",
            army.iter()
                .map(|&id| generals_sim::skirmish::diagnostics::movement_details(game, id))
                .collect::<Vec<_>>()
        )
    })?;
    Ok(())
}

pub fn target_supply_center(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    use crate::skirmish_verification::{advance_until, construct};
    use generals_sim::skirmish::Kind;
    let builder = construction::builder(game)?;
    let supply = game
        .type_for(1, Kind::SupplyCenter)
        .ok_or("missing enemy supply type")?;
    let cost = game.catalog[supply].cost;
    advance_until(
        game,
        18_000,
        "collected funds for the next enemy structure",
        |game| game.players[1].cash >= cost,
    )?;
    let target = construct(game, builder, Kind::SupplyCenter, 1)?;
    construction::retreat(game, builder, target)?;
    Ok(target)
}

pub fn destroy_target(
    game: &mut Skirmish,
    army: &[usize],
    target: usize,
) -> Result<(), Box<dyn Error>> {
    use crate::skirmish_verification::{advance_until, require};
    for &actor in army {
        require(
            game.entities[actor].complete(),
            "promotion diagnostic lost a commanded attacker",
        )?;
        game.attack(actor, target)?;
    }
    advance_until(
        game,
        18_000,
        "completed enemy structure destroyed through combat",
        |game| !game.entities[target].alive(),
    )
}

fn verify_cash_hack(game: &mut Skirmish, command_center: usize) -> Result<(), Box<dyn Error>> {
    use crate::skirmish_verification::require;
    use generals_sim::skirmish::PowerTarget;
    let target = target_supply_center(game)?;
    game.purchase_science(0, "SCIENCE_CashHack1")?;
    let own = game.players[0].cash;
    let enemy = game.players[1].cash;
    let amount = enemy.min(1000);
    game.cast_power(
        command_center,
        "SuperweaponCashHack",
        PowerTarget::Object(target),
    )?;
    require(
        amount > 0
            && game.players[0].cash == own + amount
            && game.players[1].cash == enemy - amount,
        "installed cash hack did not transfer its bounded source amount",
    )?;
    game.purchase_science(0, "SCIENCE_CashHack2")?;
    require(
        game.power_cooldown_remaining(command_center, "SuperweaponCashHack") == 7200
            && game
                .cast_power(
                    command_center,
                    "SuperweaponCashHack",
                    PowerTarget::Object(target),
                )
                .is_err(),
        "cash hack upgrade bypassed recharge",
    )?;
    Ok(())
}

fn verify_cash_bounty(game: &mut Skirmish, army: &[usize]) -> Result<(), Box<dyn Error>> {
    use crate::skirmish_verification::require;
    game.purchase_science(0, "SCIENCE_CashBounty1")?;
    let target = target_supply_center(game)?;
    // Ordinary Stop orders isolate kill income from collection income for this assertion.
    for id in 0..game.entities.len() {
        let entity = &game.entities[id];
        if entity.owner == 0
            && entity.complete()
            && game.catalog[entity.type_index].supply.capacity > 0
        {
            game.stop(id)?;
        }
    }
    let cash = game.players[0].cash;
    let expected = game.catalog[game.entities[target].type_index]
        .cost
        .div_ceil(20);
    destroy_target(game, army, target)?;
    require(
        game.players[0].cash == cash + expected,
        "installed level-one five-percent bounty did not pay the actual kill cost",
    )?;
    game.purchase_science(0, "SCIENCE_CashBounty2")?;
    Ok(())
}
