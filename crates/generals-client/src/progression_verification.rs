//! Installed-data checks run by the ordinary-command skirmish diagnostic.
use generals_sim::skirmish::{Faction, Skirmish};
use std::error::Error;
pub mod construction;
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
    use crate::skirmish_verification::{friendly_entity, require};
    use generals_sim::skirmish::{Kind, PowerTarget};

    println!("Checking earned promotions through ordinary construction and combat commands");
    let junk_repair = game.players[0].faction == Faction::Gla;
    let (factory, mut army) = prepare_army(game, junk_repair)?;
    let retreat: Vec<_> = army.iter().map(|&id| game.entities[id].position).collect();
    for _ in 0..12 {
        if game.players[0].rank >= 3 {
            break;
        }
        if junk_repair {
            wait_for_junk_repair(game, &army)?;
        }
        let target = target_supply_center(game)?;
        destroy_target(game, &army, target)?;
        retreat_and_replenish(game, factory, &mut army, &retreat, target)?;
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
        Faction::Gla => {
            wait_for_junk_repair(game, &army)?;
            verify_cash_bounty(game, &army)?;
        }
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

fn prepare_army(
    game: &mut Skirmish,
    junk_repair: bool,
) -> Result<(usize, [usize; 4]), Box<dyn Error>> {
    use crate::skirmish_verification::{
        advance_until, construct, friendly_entity, produce_type, producer_unit,
    };
    use generals_sim::skirmish::Kind;

    for owner in 0..2 {
        let builder = friendly_entity(game, Kind::Builder, owner)?;
        if game.players[owner].faction != Faction::Gla {
            construct(game, builder, Kind::PowerPlant, owner)?;
        }
        construct(game, builder, Kind::SupplyCenter, owner)?;
    }
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    if junk_repair {
        let center = friendly_entity(game, Kind::SupplyCenter, 0)?;
        research_junk_repair(game, builder, center)?;
    }
    let mut army = [0; 4];
    for actor in &mut army {
        let tank = producer_unit(game, factory, Kind::Tank)?;
        if junk_repair {
            let price = game.build_cost(0, tank);
            advance_until(
                game,
                18_000,
                "ordinary income for a promotion tank",
                |game| game.players[0].cash >= price,
            )?;
        }
        *actor = produce_type(game, factory, tank, 0)?;
    }
    for &actor in &army {
        game.stop(actor)?;
    }
    advance_until(
        game,
        1800,
        "promotion army settling into its individual return positions",
        |game| army.iter().all(|&id| !game.entities[id].moving()),
    )?;
    Ok((factory, army))
}

fn research_junk_repair(
    game: &mut Skirmish,
    builder: usize,
    supply_center: usize,
) -> Result<(), Box<dyn Error>> {
    use crate::{
        skirmish_verification::{advance_until, construct_type, require},
        strategy_verification::{research, template},
    };

    // The Supply Center releases armed defenders on death. Pay for the
    // Scorpions' own AutoHealBehavior between sorties against those defenders.
    let origin = game.entities[supply_center].position;
    let distance = |index| {
        let [x, y] = game.supply_position(index);
        (x - origin[0]).hypot(y - origin[1])
    };
    let supply = game
        .supplies
        .iter()
        .enumerate()
        .filter(|(_, stock)| stock.boxes > 0)
        .min_by(|(left, _), (right, _)| distance(*left).total_cmp(&distance(*right)))
        .map(|(index, _)| index)
        .ok_or("missing nonempty original supply stock for GLA progression")?;
    game.harvest(builder, supply)?;
    let palace = template(game, "GLAPalace")?;
    let price = game.build_cost(0, palace);
    advance_until(
        game,
        18_000,
        "ordinary supplies funding the Palace",
        |game| game.players[0].cash >= price,
    )?;
    construct_type(game, builder, palace, 0)?;
    game.harvest(builder, supply)?;
    let market = template(game, "GLABlackMarket")?;
    let price = game.build_cost(0, market);
    advance_until(
        game,
        18_000,
        "ordinary supplies funding the Black Market",
        |game| game.players[0].cash >= price,
    )?;
    let market = construct_type(game, builder, market, 0)?;
    game.harvest(builder, supply)?;
    research(game, market, "Upgrade_GLAJunkRepair")?;
    // The player upgrade survives sale. End passive market income before the
    // exact cash-bounty credit is measured later in the exercise.
    game.sell(0, market)?;
    advance_until(game, 900, "ordinary Black Market sale", |game| {
        !game.entities[market].exists()
    })?;
    require(
        game.has_upgrade(0, "Upgrade_GLAJunkRepair"),
        "paid player Junk Repair did not persist after Black Market sale",
    )
}

fn wait_for_junk_repair(game: &mut Skirmish, army: &[usize]) -> Result<(), Box<dyn Error>> {
    crate::skirmish_verification::advance_until(
        game,
        18_000,
        "paid Junk Repair restoring the full promotion army before its next sortie",
        |game| {
            army.iter().all(|&id| {
                let tank = &game.entities[id];
                tank.complete() && tank.health >= tank.max_health
            })
        },
    )
    .map_err(|error| format!("{error}; army={:?}", combat_details(game, army)).into())
}

pub fn retreat_army(
    game: &mut Skirmish,
    army: &[usize],
    retreat: &[[f32; 2]],
    target: usize,
) -> Result<(), Box<dyn Error>> {
    retreat_to_positions(game, army, retreat, target, false)
}

fn retreat_and_replenish(
    game: &mut Skirmish,
    factory: usize,
    army: &mut [usize],
    retreat: &[[f32; 2]],
    target: usize,
) -> Result<(), Box<dyn Error>> {
    use crate::skirmish_verification::{advance_until, produce_type};
    use generals_sim::skirmish::Order;

    retreat_to_positions(game, army, retreat, target, true)?;
    for (id, &destination) in army.iter_mut().zip(retreat) {
        if game.entities[*id].alive() {
            continue;
        }
        let type_index = game.entities[*id].type_index;
        let price = game.build_cost(0, type_index);
        advance_until(
            game,
            18_000,
            "ordinary income for a replacement tank",
            |game| game.players[0].cash >= price,
        )?;
        let replacement = produce_type(game, factory, type_index, 0)?;
        game.move_to(replacement, destination)?;
        let Order::Move(goal) = game.entities[replacement].ai.order else {
            return Err("Replacement tank did not retain its ordinary move order".into());
        };
        advance_until(
            game,
            1_800,
            "paid replacement reaching its army return position",
            |game| {
                let actor = &game.entities[replacement];
                actor.complete()
                    && matches!(actor.ai.order, Order::Idle)
                    && !actor.moving()
                    && (actor.position[0] - goal[0]).hypot(actor.position[1] - goal[1]) <= 10.0
            },
        )?;
        println!(
            "  Paid replacement {} {replacement} after real combat loss of {}",
            game.catalog[type_index].name, *id
        );
        *id = replacement;
    }
    Ok(())
}

fn retreat_to_positions(
    game: &mut Skirmish,
    army: &[usize],
    retreat: &[[f32; 2]],
    target: usize,
    replenish_losses: bool,
) -> Result<(), Box<dyn Error>> {
    use generals_sim::skirmish::Order;

    let mut goals = Vec::new();
    crate::skirmish_verification::require(
        army.len() == retreat.len(),
        "Each promotion army member needs its own observed return position",
    )?;
    for (&actor, &position) in army.iter().zip(retreat) {
        if replenish_losses && !game.entities[actor].alive() {
            continue;
        }
        game.move_to(actor, position)?;
        let Order::Move(goal) = game.entities[actor].ai.order else {
            return Err("Promotion army retreat did not retain its ordinary move order".into());
        };
        goals.push((actor, goal));
    }
    crate::skirmish_verification::advance_until(
        game,
        18_000,
        "army retreat and outstanding shells before rebuilding",
        |game| {
            if replenish_losses && !army.iter().any(|&id| game.entities[id].alive()) {
                return true;
            }
            game.projectiles.is_empty()
                && goals.iter().all(|&(actor, _)| {
                    let entity = &game.entities[actor];
                    if replenish_losses && !entity.alive() {
                        return true;
                    }
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
                        && matches!(entity.ai.order, Order::Idle)
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
    if replenish_losses {
        crate::skirmish_verification::require(
            army.iter().any(|&id| game.entities[id].alive()),
            "No promotion army tank survived to verify its retreat",
        )?;
    }
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
    destroy_target_observing(game, army, target, |_| {})
}

fn destroy_target_observing(
    game: &mut Skirmish,
    army: &[usize],
    target: usize,
    mut observe: impl FnMut(&Skirmish),
) -> Result<(), Box<dyn Error>> {
    use crate::skirmish_verification::{advance_until, require};
    require(
        army.iter().all(|&id| game.entities[id].complete()),
        "promotion diagnostic lost a commanded attacker",
    )
    .map_err(|error| format!("{error}; army={:?}", combat_details(game, army)))?;
    for &actor in army {
        game.attack(actor, target)?;
    }
    advance_until(
        game,
        18_000,
        "completed enemy structure destroyed through combat",
        |game| {
            observe(game);
            !game.entities[target].alive()
                || (army.iter().all(|&id| !game.entities[id].alive())
                    && game.projectiles.is_empty())
        },
    )
    .and_then(|()| {
        require(
            !game.entities[target].alive(),
            "promotion army was destroyed before killing the enemy structure",
        )
    })
    .map_err(|error| {
        format!(
            "{error}; target={:?}; army={:?}",
            combat_details(game, &[target]),
            combat_details(game, army)
        )
        .into()
    })
}

fn combat_details(game: &Skirmish, ids: &[usize]) -> Vec<String> {
    ids.iter()
        .map(|&id| {
            let actor = &game.entities[id];
            let unit = &game.catalog[actor.type_index];
            let attacker = actor
                .last_attacker()
                .map(|id| (id, game.catalog[game.entities[id].type_index].name.as_str()));
            format!(
                "{} {id}: health={}/{} exists={} death_type={} last_attacker={attacker:?}; {}",
                unit.name,
                actor.health,
                actor.max_health,
                actor.exists(),
                actor.death_type() as u32,
                generals_sim::skirmish::diagnostics::movement_details(game, id)
            )
        })
        .collect()
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
    let earned = game.players[0].statistics.money_earned;
    let first_deposit = game.presentation.deposit_events.len();
    let mut alive: Vec<_> = game
        .entities
        .iter()
        .map(generals_sim::skirmish::Entity::alive)
        .collect();
    let mut credited = Vec::new();
    destroy_target_observing(game, army, target, |game| {
        alive.resize(game.entities.len(), true);
        for (id, was_alive) in alive.iter_mut().enumerate() {
            let actor = &game.entities[id];
            if *was_alive
                && !actor.alive()
                && game.enemies(0, actor.owner)
                && actor.build_progress >= 1.0
                && !game.catalog[actor.type_index].progression.ignored_in_gui
                && let Some(attacker) = actor
                    .last_attacker()
                    .filter(|&id| game.entities[id].owner == 0)
            {
                // Player::doBountyForKill prices the victim for its own owner.
                // Integer costs at 5% give exact ceil(cost / 20) in this roster.
                let amount = game.build_cost(actor.owner, actor.type_index).div_ceil(20);
                credited.push((
                    id,
                    game.catalog[actor.type_index].name.clone(),
                    attacker,
                    amount,
                ));
            }
            *was_alive = actor.alive();
        }
    })?;
    let deposits: i64 = game.presentation.deposit_events[first_deposit..]
        .iter()
        .filter(|event| event.owner == 0)
        .map(|event| i64::from(event.amount))
        .sum();
    let expected = credited
        .iter()
        .map(|&(_, _, _, amount)| i64::from(amount))
        .sum::<i64>();
    let cash_delta = i64::from(game.players[0].cash) - i64::from(cash);
    let earned_delta = i64::from(game.players[0].statistics.money_earned) - i64::from(earned);
    if !credited
        .iter()
        .any(|&(id, _, _, amount)| id == target && amount > 0)
        || cash_delta != expected + deposits
        || earned_delta != expected + deposits
    {
        return Err(format!("installed five-percent bounty mismatch: target={target} cash={cash_delta} earned={earned_delta} noncombat_deposits={deposits} expected_bounty={expected} credited={credited:?}; target_state={:?}", combat_details(game, &[target])).into());
    }
    println!(
        "  Paid combat bounty: {expected} across {} credited kills; {deposits} in separately observed deposits",
        credited.len()
    );
    game.purchase_science(0, "SCIENCE_CashBounty2")?;
    Ok(())
}
