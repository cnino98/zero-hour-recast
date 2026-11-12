//! Paid installed Strategy Center and army-research command paths.
use crate::skirmish_verification::{
    advance_until, construct, construct_type, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{
    Faction, FlightRules, Kind, ProjectileFxStage, Skirmish, WeaponFxEvent, battle_plan::Plan,
};
use std::error::Error;

pub struct Setup {
    pub center: usize,
    pub tank: usize,
    pub factory: usize,
}

pub fn template(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    game.catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or_else(|| format!("missing original {name}").into())
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let tank_type = template(game, "AmericaTankCrusader")?;
    let tank = produce_type(game, factory, tank_type, 0)?;
    let index = template(game, "AmericaStrategyCenter")?;
    let center = construct_type(game, builder, index, 0)?;
    Ok(Setup {
        center,
        tank,
        factory,
    })
}

pub fn research(game: &mut Skirmish, producer: usize, name: &str) -> Result<(), Box<dyn Error>> {
    let owner = game.entities[producer].owner;
    let index = game.catalog[game.entities[producer].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == name)
        .ok_or_else(|| format!("missing original research {name}"))?;
    let upgrade = &game.catalog[game.entities[producer].type_index]
        .maintenance
        .research[index];
    let cost = upgrade.cost;
    let player_upgrade = upgrade.player;
    advance_until(game, 18_000, "ordinary supplies funding research", |game| {
        game.players[owner].cash >= cost
    })?;
    let before = game.players[owner].cash;
    game.queue_upgrade(producer, index)?;
    require(
        game.players[owner].cash == before - cost,
        "research did not charge original cost",
    )?;
    advance_until(game, 9_000, "paid original research completion", |game| {
        if player_upgrade {
            game.has_upgrade(owner, name)
        } else {
            game.object_has_upgrade(producer, name)
        }
    })
}

pub fn activate(game: &mut Skirmish, center: usize, plan: Plan) -> Result<(), Box<dyn Error>> {
    require(
        game.choose_battle_plan(center, plan),
        "original battle-plan command rejected",
    )?;
    advance_until(game, 1500, "source battle-plan transition", |game| {
        game.entities[center].battle_plan() == Some(plan)
    })
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let Setup {
        center,
        tank,
        factory,
    } = prepare(game)?;
    let health = game.entities[center].max_health;
    let tank_health = game.entities[tank].max_health;
    let unit = &game.catalog[game.entities[tank].type_index];
    let sight = game.entities[tank].vision_range(unit);
    verify_bombardment(game, center)?;
    activate(game, center, Plan::HoldTheLine)?;
    require(
        game.entities[center].max_health.to_bits() == (health * 2.0).to_bits(),
        "Hold the Line center health did not double",
    )?;
    activate(game, center, Plan::SearchAndDestroy)?;
    require(
        game.entities[center].max_health.to_bits() == health.to_bits(),
        "leaving Hold the Line retained center health bonus",
    )?;
    require(
        game.entities[tank]
            .vision_range(&game.catalog[game.entities[tank].type_index])
            .to_bits()
            == (sight * 1.2).to_bits(),
        "Search and Destroy did not apply source sight bonus",
    )?;
    research(game, center, "Upgrade_AmericaCompositeArmor")?;
    require(
        game.entities[tank].max_health.to_bits() == (tank_health + 200.0).to_bits(),
        "original two Composite Armor modules were not applied",
    )?;
    research(game, center, "Upgrade_AmericaAdvancedTraining")?;
    let tank_type = game.entities[tank].type_index;
    let future = produce_type(game, factory, tank_type, 0)?;
    require(
        game.entities[future].max_health.to_bits() == game.entities[tank].max_health.to_bits(),
        "future Crusader lost Composite Armor",
    )?;
    require(
        game.entities[future]
            .vision_range(&game.catalog[tank_type])
            .to_bits()
            == game.entities[tank]
                .vision_range(&game.catalog[tank_type])
                .to_bits(),
        "future Crusader lost active army sight bonus",
    )?;
    game.sell(0, center)?;
    advance_until(game, 3000, "selling Strategy Center", |game| {
        !game.entities[center].alive()
    })?;
    require(
        game.entities[tank]
            .vision_range(&game.catalog[tank_type])
            .to_bits()
            == sight.to_bits(),
        "Strategy Center sale retained army sight bonus",
    )?;
    println!(
        "Installed Strategy Center: paid prerequisites/production, original artillery shell launch/impact, three source battle plans, center health/sight transitions, Composite Armor, Advanced Training, future-unit effects and sale cleanup passed"
    );
    Ok(())
}

fn verify_bombardment(game: &mut Skirmish, center: usize) -> Result<(), Box<dyn Error>> {
    let target = friendly_entity(game, Kind::Builder, 1)?;
    let position = game.entities[center].position;
    game.move_to(target, position)?;
    advance_until(
        game,
        18_000,
        "enemy dozer approaching Strategy Center",
        |game| {
            let [x, y] = game.entities[target].position;
            (x - position[0]).hypot(y - position[1]) < 290.0
        },
    )?;
    game.stop(target)?;
    activate(game, center, Plan::Bombardment)?;
    game.attack(center, target)?;
    let mut shell = None;
    for _ in 0..1500 {
        let health = game.entities[target].health;
        game.step();
        if shell.is_none()
            && let Some(projectile) = game.projectiles.iter().find(|shot| shot.source == center)
        {
            let weapon = game.catalog[game.entities[center].type_index]
                .weapon_template(projectile.weapon_index)
                .ok_or("Strategy Center shell weapon missing")?;
            let flight = projectile
                .object
                .and_then(|id| {
                    game.catalog[game.entities[id].type_index]
                        .projectile_rules
                        .first()
                })
                .map_or("missing", |rules| match rules {
                    FlightRules::Shell(_) => "shell",
                    FlightRules::Missile(_) => "missile",
                });
            // The authored ArtilleryShell object uses MissileAIUpdate/THRUST;
            // its name does not imply DumbProjectileBehavior's Bezier flight.
            if flight != "missile" || projectile.render_transform(&game.entities).is_none() {
                return Err(format!(
                    "Strategy Center shell {} slot {} template {} {}: motion={flight} render={} positions {:?}/{:?}",
                    projectile.id, projectile.weapon_slot, projectile.weapon_index, weapon.name,
                    projectile.render_transform(&game.entities).is_some(), game.object_position(center), game.object_position(target)
                ).into());
            }
            shell = Some(projectile.id);
        }
        if game.presentation.weapon_fx.iter().any(|event| {
            matches!(event, WeaponFxEvent::Projectile(event)
                if Some(event.projectile) == shell
                    && event.source == center
                    && matches!(event.stage, ProjectileFxStage::Detonation))
        }) && game.entities[target].health < health
        {
            return Ok(());
        }
        if shell.is_some_and(|id| game.projectiles.iter().all(|shot| shot.id != id)) {
            shell = None;
        }
    }
    Err("Strategy Center shell did not detonate and damage the commanded enemy dozer".into())
}
