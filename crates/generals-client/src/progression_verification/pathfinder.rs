//! Original Pathfinder unlock, paid production and sniper/stealth after combat-earned ranks.
use crate::{
    skirmish_verification::{
        advance_until, construct, friendly_entity, produce, produce_type, require,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{DamageType, Kind, Order, Skirmish, stealth};
use std::error::Error;

pub(super) fn verify(
    game: &mut Skirmish,
    army: &[usize],
    retreat: &[[f32; 2]],
) -> Result<(), Box<dyn Error>> {
    let index = template(game, "AmericaInfantryPathfinder")?;
    verify_rules(game, index)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let barracks_type = game
        .type_for(0, Kind::Barracks)
        .ok_or("Missing USA Barracks")?;
    fund(game, barracks_type, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let cash = game.players[0].cash;
    require(
        game.train(barracks, index).is_err() && game.players[0].cash == cash,
        "Pathfinder bypassed its unpaid original science",
    )?;
    // The unchanged repair-chain assertions spent the rank-three points. Earn
    // another normally; never grant XP, ranks or science points to this fixture.
    for _ in 0..12 {
        if game.can_purchase_science(0, "SCIENCE_Pathfinder") {
            break;
        }
        let target = super::target_supply_center(game)?;
        super::destroy_target(game, army, target)?;
        super::retreat_army(game, army, retreat, target)?;
    }
    require(
        game.players[0].rank >= 3 && game.can_purchase_science(0, "SCIENCE_Pathfinder"),
        "ordinary combat did not earn Pathfinder science eligibility",
    )?;
    let points = game.players[0].science_points;
    let experience = game.players[0].skill_points;
    game.purchase_science(0, "SCIENCE_Pathfinder")?;
    require(
        game.players[0].science_points + 1 == points && game.players[0].skill_points == experience,
        "Pathfinder science did not spend exactly its original point",
    )?;
    fund(game, index, 0)?;
    let actor = produce_type(game, barracks, index, 0)?;
    verify_concealment(game, actor).map_err(|error| {
        format!(
            "Pathfinder {actor} concealment at tick {}: {error}",
            game.tick
        )
    })?;
    let target = prepare_target(game).map_err(|error| {
        format!(
            "Pathfinder {actor} paid infantry victim preparation at tick {}: {error}",
            game.tick
        )
    })?;
    attack(game, actor, target)?;
    println!(
        "Paid Pathfinder: combat-earned rank {}, one-point science, $600 Barracks production, moving reveal/stationary concealment, original sniper kill and loss/kill credit passed",
        game.players[0].rank,
    );
    Ok(())
}

/// Reuse combat-earned science and paid production, then stage another paid victim.
pub fn prepare_capture(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    game.reset(
        generals_sim::skirmish::Faction::Usa,
        generals_sim::skirmish::Faction::China,
    )?;
    super::initial(game)?;
    super::exercise(game)?;
    let actor = game
        .entities
        .iter()
        .position(|actor| {
            actor.alive()
                && actor.owner == 0
                && game.catalog[actor.type_index].name == "AmericaInfantryPathfinder"
        })
        .ok_or("paid Pathfinder missing after progression")?;
    // Return through a real Move before producing the next infantry; a concealed
    // idle sniper otherwise legitimately shoots that victim during preparation.
    let home = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    game.move_to(actor, home)?;
    advance_until(
        game,
        18_000,
        "Pathfinder returning before next paid victim",
        |game| {
            matches!(game.entities[actor].ai.order, Order::Idle)
                && !game.entities[actor].moving()
                && distance(game.entities[actor].position, home) < 150.0
        },
    )?;
    let target = prepare_target(game)?;
    let origin = game.entities[actor].position;
    let victim = game.entities[target].position;
    let length = distance(origin, victim);
    require(
        length > 400.0,
        "Pathfinder staging lacks safe initial separation",
    )?;
    let point = [
        ((origin[0] - victim[0]) / length).mul_add(380.0, victim[0]),
        ((origin[1] - victim[1]) / length).mul_add(380.0, victim[1]),
    ];
    game.move_to(actor, point)?;
    advance_until(
        game,
        18_000,
        "Pathfinder approaching visible native staging",
        |game| {
            matches!(game.entities[actor].ai.order, Order::Idle)
                && !game.entities[actor].moving()
                && distance(game.entities[actor].position, point) < 20.0
        },
    )?;
    require(
        game.entities[target].alive() && game.entities[actor].stealthed(),
        "Pathfinder staging lost the paid victim or stationary concealment",
    )?;
    Ok((actor, target))
}

fn verify_rules(game: &Skirmish, index: usize) -> Result<(), Box<dyn Error>> {
    let unit = &game.catalog[index];
    require(
        unit.build_rules.sciences == ["SCIENCE_Pathfinder"]
            && unit.cost == 600
            && unit.weapon.name == "USAPathfinderSniperRifle"
            && matches!(unit.weapon.damage_type, DamageType::Sniper)
            && unit.weapon.damage.to_bits() == 100.0_f32.to_bits()
            && unit.weapon.range.to_bits() == 300.0_f32.to_bits()
            && unit.behavior.stealth.as_ref().is_some_and(|rules| {
                rules.delay == 0
                    && rules.forbidden == stealth::MOVING
                    && rules.threshold_speed.to_bits() == (3.0_f32 * (1.0 / 30.0)).to_bits()
            }),
        "Pathfinder source science, weapon or movement-only stealth rules differ",
    )
}

fn verify_concealment(game: &mut Skirmish, actor: usize) -> Result<(), Box<dyn Error>> {
    game.stop(actor)?;
    advance_until(game, 300, "Pathfinder stationary concealment", |game| {
        !game.entities[actor].moving() && game.entities[actor].stealthed()
    })?;
    require(
        !game.logically_visible(1, actor),
        "undetected Pathfinder was visible to enemy",
    )?;
    let home = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    let enemy = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    let destination = [
        (enemy[0] - home[0]).mul_add(0.25, home[0]),
        (enemy[1] - home[1]).mul_add(0.25, home[1]),
    ];
    game.move_to(actor, destination)?;
    advance_until(
        game,
        3600,
        "moving Pathfinder reveals through native speed threshold",
        |game| game.entities[actor].moving() && !game.entities[actor].stealthed(),
    )?;
    game.stop(actor)?;
    advance_until(game, 300, "stopped Pathfinder conceals again", |game| {
        !game.entities[actor].moving() && game.entities[actor].stealthed()
    })?;
    require(
        !game.logically_visible(1, actor),
        "stationary Pathfinder failed to conceal again",
    )
}

fn prepare_target(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    let builder = super::construction::builder(game)?;
    let barracks_type = game
        .type_for(1, Kind::Barracks)
        .ok_or("Missing enemy Barracks")?;
    fund(game, barracks_type, 1)?;
    let barracks = construct(game, builder, Kind::Barracks, 1)?;
    let infantry_type = game
        .type_for(1, Kind::Infantry)
        .ok_or("Missing enemy infantry")?;
    fund(game, infantry_type, 1)?;
    let target = produce(game, barracks, Kind::Infantry, 1)?;
    let home = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    let enemy = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    let destination = [(home[0] + enemy[0]) * 0.5, (home[1] + enemy[1]) * 0.5];
    game.move_to(target, destination)?;
    advance_until(
        game,
        18_000,
        "paid sniper victim arriving clear of promotion army",
        |game| {
            matches!(game.entities[target].ai.order, Order::Idle) && !game.entities[target].moving()
        },
    )?;
    require(
        game.entities[target].alive()
            && distance(game.entities[target].position, destination) < 20.0,
        "paid sniper victim did not reach the combat staging point alive",
    )?;
    Ok(target)
}

fn attack(game: &mut Skirmish, actor: usize, target: usize) -> Result<(), Box<dyn Error>> {
    game.move_to(actor, game.entities[target].position)?;
    advance_until(
        game,
        18_000,
        "Pathfinder revealing its paid infantry target",
        |game| {
            game.visible_to(0, target)
                && distance(
                    game.entities[actor].position,
                    game.entities[target].position,
                ) < 350.0
        },
    )?;
    require(
        game.weapon_target_reachable(actor, target),
        "native sniper cannot target paid infantry",
    )?;
    let killed = game.players[0].statistics.units_destroyed;
    let lost = game.players[1].statistics.units_lost;
    let experience = game.entities[actor].experience;
    game.attack(actor, target)?;
    let mut shots = 0;
    let mut concealed_shot = false;
    for _ in 0..3600 {
        game.step();
        for shot in game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == actor && shot.target == Some(target))
        {
            require(
                shot.weapon_slot == 0,
                "Pathfinder used a non-native sniper slot",
            )?;
            shots += 1;
            concealed_shot |= game.entities[actor].stealthed();
        }
        if !game.entities[target].alive() {
            require(
                game.entities[actor].alive()
                    && shots == 1
                    && concealed_shot
                    && game.entities[actor].experience > experience
                    && game.players[0].statistics.units_destroyed == killed + 1
                    && game.players[1].statistics.units_lost == lost + 1,
                &format!(
                    "Pathfinder source kill/stealth/credit failed: shots={shots} concealed={concealed_shot} actor_hp={} victim_hp={}",
                    game.entities[actor].health, game.entities[target].health
                ),
            )?;
            return Ok(());
        }
    }
    Err(format!(
        "Pathfinder sniper combat timed out; actor={}; victim={}",
        generals_sim::skirmish::diagnostics::movement_details(game, actor),
        generals_sim::skirmish::diagnostics::movement_details(game, target),
    )
    .into())
}

fn fund(game: &mut Skirmish, index: usize, owner: usize) -> Result<(), Box<dyn Error>> {
    let cost = game.catalog[index].cost;
    advance_until(
        game,
        18_000,
        "ordinary supply income funding Pathfinder scenario",
        |game| game.players[owner].cash >= cost,
    )
}

fn distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    (left[0] - right[0]).hypot(left[1] - right[1])
}

#[cfg(test)]
mod tests {
    use crate::{model_animation, projectile_data, skirmish_data, terrain::Terrain, world_data};
    use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
    use generals_sim::skirmish::Faction;

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_pathfinder_earns_science_then_snipes_with_native_stationary_stealth() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let map = MapData::parse(&assets.read(crate::playtest::MAP_NAME).unwrap().bytes).unwrap();
        let mut data = skirmish_data::load(&assets, &map.objects).unwrap();
        let settings = world_data::camera_settings(&assets, root).unwrap();
        let terrain = Terrain::new(
            map.heightmap,
            map.tiles,
            settings.adjust_cliffs,
            settings.three_way_blends,
        )
        .unwrap();
        let (layout, _) = world_data::load(
            &assets,
            &map.objects,
            &terrain,
            &data.types,
            &mut data.models,
        )
        .unwrap();
        let animations = model_animation::load(&assets, &mut data.models, &data.types).unwrap();
        let projectiles =
            projectile_data::load(&assets, &mut data.types, &data.models, &animations).unwrap();
        let movement = terrain
            .classify_movement(&map.triggers, &map.objects)
            .unwrap();
        let mut game = data.into_simulation(movement, layout).unwrap();
        terrain
            .configure_projectiles(&mut game, projectiles.partition_cell_size)
            .unwrap();
        game.reset(Faction::Usa, Faction::China).unwrap();
        super::super::initial(&game).unwrap();
        super::super::exercise(&mut game).unwrap();
    }
}
