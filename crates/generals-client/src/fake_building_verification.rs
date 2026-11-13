//! Paid original fake structures and `ReplaceObjectUpgrade` destruction semantics.
use crate::{
    playtest::MAP_NAME,
    skirmish_verification::{advance_until, construct, construct_type, friendly_entity, require},
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::{error::Error, path::Path};

pub const NAMES: [&str; 5] = [
    "FakeGLACommandCenter",
    "FakeGLABarracks",
    "FakeGLASupplyStash",
    "FakeGLAArmsDealer",
    "FakeGLABlackMarket",
];

pub struct Setup {
    pub worker: usize,
    pub fakes: [usize; 5],
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset_sandbox(Faction::Gla)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    construct(game, worker, Kind::Barracks, 0)?;
    construct(game, worker, Kind::Factory, 0)?;
    let palace = template(game, "GLAPalace")?;
    construct_type(game, worker, palace, 0)?;
    switch(game, worker, true)?;
    require(
        game.entities[worker].command_set_override() == Some("GLAWorkerFakeBuildingsCommandSet"),
        "Worker did not enter its original fake command set",
    )?;
    let mut fakes = [0; 5];
    for (slot, name) in NAMES.iter().enumerate() {
        let kind = template(game, name)?;
        let cost = game.catalog[kind].cost;
        advance_until(
            game,
            18_000,
            "supply income funding fake construction",
            |game| game.players[0].cash >= cost,
        )?;
        fakes[slot] = construct_type(game, worker, kind, 0)?;
        require(
            game.catalog[kind].fake_building() && game.can_detonate_fake(0, fakes[slot]),
            "Paid original fake building was unavailable",
        )?;
    }
    Ok(Setup { worker, fakes })
}

pub fn convert(game: &mut Skirmish, old: usize) -> Result<usize, Box<dyn Error>> {
    let kind = game.entities[old].type_index;
    let rule = game.catalog[kind]
        .maintenance
        .replacement
        .as_ref()
        .ok_or("missing source replacement")?;
    let name = rule.upgrade.clone();
    let target = template(game, &rule.object)?;
    let position = game.entities[old].position.map(f32::to_bits);
    let facing = game.entities[old].facing.to_bits();
    let index = game.catalog[kind]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == name)
        .ok_or("missing conversion upgrade")?;
    let cost = game.catalog[kind].maintenance.research[index].cost;
    advance_until(game, 18_000, "income funding conversion", |game| {
        game.players[0].cash >= cost
    })?;
    let cash = game.players[0].cash;
    game.queue_upgrade(old, index)?;
    require(
        game.players[0].cash == cash - cost,
        "Replacement upgrade did not pay original cost",
    )?;
    advance_until(game, 9000, "actual replacement callback", |game| {
        game.presentation
            .replacement_events
            .iter()
            .any(|event| event.previous == old)
    })?;
    let next = game
        .presentation
        .replacement_events
        .iter()
        .find(|event| event.previous == old)
        .ok_or("missing actual replacement callback")?
        .next;
    require(
        game.entities[old].masked()
            && game.entities[next].type_index == target
            && game.entities[next].owner == 0
            && game.entities[next].complete(),
        "Replacement did not destroy old ID and create completed current-owner real template",
    )?;
    require(
        game.entities[next].position.map(f32::to_bits) == position
            && game.entities[next].facing.to_bits() == facing
            && game.entities[next].health.to_bits()
                == game.catalog[target].initial_health.to_bits(),
        "Replacement lost transform or original full health",
    )?;
    Ok(next)
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut game = crate::opponent_verification::load_game(root, MAP_NAME)?;
    let setup = prepare(&mut game)?;
    let fake = setup.fakes[1];
    let kind = game.entities[fake].type_index;
    let index = 0;
    let cost = game.catalog[kind].maintenance.research[index].cost;
    advance_until(
        &mut game,
        18_000,
        "income funding cancellable conversion",
        |game| game.players[0].cash >= cost,
    )?;
    let cash = game.players[0].cash;
    game.queue_upgrade(fake, index)?;
    require(
        game.players[0].cash == cash - cost,
        "Conversion did not charge original cost",
    )?;
    game.cancel_upgrade(0, fake, index)?;
    require(
        game.players[0].cash == cash,
        "Cancelled conversion failed exact refund",
    )?;
    for old in setup.fakes {
        convert(&mut game, old).map_err(|error| {
            format!(
                "paid conversion {}: {error}; oldhealth={} masked={} complete={} attacker={:?}",
                game.catalog[game.entities[old].type_index].name,
                game.entities[old].health,
                game.entities[old].masked(),
                game.entities[old].complete(),
                game.entities[old].last_attacker().map(|id| (
                    id,
                    game.catalog[game.entities[id].type_index].name.as_str(),
                    game.entities[id].owner
                ))
            )
        })?;
    }
    switch(&mut game, setup.worker, false)?;
    require(
        game.entities[setup.worker].command_set_override() == Some("GLAWorkerCommandSet"),
        "Worker did not restore original real command set",
    )?;
    switch(&mut game, setup.worker, true)?;
    for name in NAMES {
        let kind = template(&game, name)?;
        let cost = game.catalog[kind].cost;
        advance_until(&mut game, 18_000, "income funding detonation", |game| {
            game.players[0].cash >= cost
        })?;
        let fake = isolated_fake(&mut game, setup.worker, kind)?;
        game.detonate_fake(0, fake)?;
        require(
            game.entities[fake].alive()
                && matches!(
                    game.entities[fake].ai.order,
                    generals_sim::skirmish::Order::FireAt { .. }
                ),
            "original detonation button did not enter the ordinary attack state",
        )?;
        advance_until(&mut game, 30, "SuicideWeapon self damage", |game| {
            matches!(
                game.entities[fake].death_type(),
                generals_sim::skirmish::DeathType::Suicided
            )
        })?;
        advance_until(
            &mut game,
            30,
            "original final blast and deferred destruction",
            |game| game.entities[fake].masked(),
        )?;
        require(
            game.detonate_fake(0, fake).is_err(),
            "Destroyed fake accepted repeated detonation",
        )?;
    }
    println!(
        "FAKE_BUILDINGS paid_all_five=true worker_switch=true cancelled_conversion_refund=true fresh_real_ids=true full_health_and_transform=true actual_detonation=true"
    );
    Ok(())
}

pub fn switch(game: &mut Skirmish, worker: usize, fake: bool) -> Result<(), Box<dyn Error>> {
    let name = if fake {
        "Upgrade_GLAWorkerFakeCommandSet"
    } else {
        "Upgrade_GLAWorkerRealCommandSet"
    };
    let command = if fake {
        "GLAWorkerFakeBuildingsCommandSet"
    } else {
        "GLAWorkerCommandSet"
    };
    let index = game.catalog[game.entities[worker].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == name)
        .ok_or("missing Worker source upgrade")?;
    game.queue_upgrade(worker, index).map_err(|error| {
        format!(
            "Worker switch {name}: {error}; health={} masked={} complete={}",
            game.entities[worker].health,
            game.entities[worker].masked(),
            game.entities[worker].complete()
        )
    })?;
    advance_until(game, 2, "zero-time Worker command switch", |game| {
        game.entities[worker].command_set_override() == Some(command)
    })
}

pub fn isolated_fake(
    game: &mut Skirmish,
    worker: usize,
    kind: usize,
) -> Result<usize, Box<dyn Error>> {
    let owner = game.entities[worker].owner;
    let base = game.entities[friendly_entity(game, Kind::CommandCenter, owner)?].position;
    let point = (0_u8..16)
        .find_map(|index| {
            let angle = f32::from(index) * std::f32::consts::TAU / 16.0;
            let wanted = [
                angle.cos().mul_add(650.0, base[0]),
                angle.sin().mul_add(650.0, base[1]),
            ];
            game.placement_near(kind, wanted).filter(|point| {
                game.entities
                    .iter()
                    .filter(|actor| {
                        !actor.masked()
                            && game.catalog[actor.type_index]
                                .kind_of
                                .iter()
                                .any(|flag| flag == "STRUCTURE")
                    })
                    .all(|actor| {
                        (actor.position[0] - point[0]).hypot(actor.position[1] - point[1]) > 350.0
                    })
            })
        })
        .ok_or("no isolated legal source fake detonation site")?;
    let fake = crate::skirmish_verification::construct_type_at(game, worker, kind, owner, point)?;
    game.move_to(worker, base)?;
    advance_until(
        game,
        18000,
        "Worker leaving authored blast radius",
        |game| {
            (game.entities[worker].position[0] - point[0])
                .hypot(game.entities[worker].position[1] - point[1])
                > 300.0
        },
    )?;
    game.stop(worker).map_err(|error| {
        format!(
            "Worker clearing isolated detonation: {error}; health={} masked={}",
            game.entities[worker].health,
            game.entities[worker].masked()
        )
    })?;
    Ok(fake)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_verification::produce_type;
    use generals_sim::skirmish::structure_death::VisualAction;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_all_five_build_convert_and_detonate() {
        super::run(&generals_assets::game_root().unwrap()).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_fake_damage_repair_and_conversion_keep_original_transition_particles() {
        let root = generals_assets::game_root().unwrap();
        let mut game = crate::opponent_verification::load_game(&root, MAP_NAME).unwrap();
        game.reset(Faction::Gla, Faction::Usa).unwrap();
        let worker = friendly_entity(&game, Kind::Builder, 0).unwrap();
        switch(&mut game, worker, true).unwrap();
        let fake_type = template(&game, NAMES[1]).unwrap();
        let fake = isolated_fake(&mut game, worker, fake_type).unwrap();
        let enemy = friendly_entity(&game, Kind::Builder, 1).unwrap();
        construct(&mut game, enemy, Kind::PowerPlant, 1).unwrap();
        let barracks = construct(&mut game, enemy, Kind::Barracks, 1).unwrap();
        let ranger_type = template(&game, "AmericaInfantryRanger").unwrap();
        let ranger = produce_type(&mut game, barracks, ranger_type, 1).unwrap();
        let base = game.entities[friendly_entity(&game, Kind::CommandCenter, 1).unwrap()].position;
        damage_and_repair(&mut game, ranger, fake, worker, base);
        let real = convert(&mut game, fake).unwrap();
        assert!(!game.entities[fake].exists());
        let worker_base =
            game.entities[friendly_entity(&game, Kind::CommandCenter, 0).unwrap()].position;
        game.move_to(worker, worker_base).unwrap();
        advance_until(
            &mut game,
            18_000,
            "Worker leaves converted building",
            |game| {
                let [x, y] = game.entities[worker].position;
                let [target_x, target_y] = game.entities[real].position;
                (x - target_x).hypot(y - target_y) > 300.0
            },
        )
        .unwrap();
        damage_and_repair(&mut game, ranger, real, worker, base);
    }

    fn damage_and_repair(
        game: &mut Skirmish,
        attacker: usize,
        target: usize,
        worker: usize,
        retreat: [f32; 2],
    ) {
        game.attack(attacker, target).unwrap();
        advance_until(game, 18_000, "paid Ranger triggers damaged smoke", |game| {
            game.presentation.structure_events.iter().any(|event| {
                event.object == target
                    && matches!(&event.action,
                    VisualAction::TransitionParticle { template, .. }
                    if template == "StructureTransitionSmallSmoke")
            })
        })
        .unwrap_or_else(|error| {
            let actor = &game.entities[attacker];
            let building = &game.entities[target];
            panic!(
                "{error}; attacker position={:?}, health={}; target {} position={:?}, health={}/{}",
                actor.position,
                actor.health,
                game.catalog[building.type_index].name,
                building.position,
                building.health,
                building.max_health
            );
        });
        game.move_to(attacker, retreat).unwrap();
        game.repair(worker, target).unwrap();
        let mut cleared = false;
        advance_until(
            game,
            18_000,
            "paid structure repair retires damage smoke",
            |game| {
                cleared |= game.presentation.structure_events.iter().any(|event| {
                    event.object == target
                        && matches!(event.action, VisualAction::TransitionClear { .. })
                });
                let building = &game.entities[target];
                building.health.to_bits() == building.max_health.to_bits() && cleared
            },
        )
        .unwrap();
    }
}
