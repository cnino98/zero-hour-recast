//! Installed ordinary `BattleMaster` and Overlord nuclear-tank research consumers.
use super::{
    advance_until, friendly_entity, prepare_launcher, produce_type, require, research, template,
};
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

#[ignore = "requires original game installation"]
#[test]
fn paid_china_nuclear_tanks_change_ground_motion_and_actual_death_weapon() {
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    verify(&mut battle.simulation).unwrap();
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (launcher, _, factory) = prepare_launcher(game)?;
    let enemy = friendly_entity(game, Kind::CommandCenter, 1)?;
    let goal = game.entities[enemy].position;
    let master = template(game, "ChinaTankBattleMaster")?;
    let overlord = template(game, "ChinaTankOverlord")?;
    let gunner = produce_type(game, factory, master, 0)?;
    let initial = produce_type(game, factory, master, 0)?;
    let heavy = produce_type(game, factory, overlord, 0)?;
    verify_motion(game, initial, goal, 25.0)?;
    verify_motion(game, heavy, goal, 20.0)?;
    verify_death(game, gunner, initial, false)?;

    research(game, launcher, "Upgrade_ChinaNuclearTanks")?;
    verify_motion(game, gunner, goal, 35.0)?;
    verify_motion(game, heavy, goal, 30.0)?;
    let future_master = produce_type(game, factory, master, 0)?;
    let future_heavy = produce_type(game, factory, overlord, 0)?;
    verify_motion(game, future_master, goal, 35.0)?;
    verify_motion(game, future_heavy, goal, 30.0)?;

    verify_death(game, gunner, heavy, true)?;
    verify_death(game, gunner, future_master, true)?;

    // BattleMasterTankGun's original 60 armor-piercing damage becomes 75.
    // Both hits use this same paid actor and a single real projectile impact.
    verify_uranium_hit(game, gunner, enemy, 60.0)?;
    research(game, launcher, "Upgrade_ChinaUraniumShells")?;
    verify_uranium_hit(game, gunner, enemy, 75.0)
}

fn verify_uranium_hit(
    game: &mut Skirmish,
    tank: usize,
    target: usize,
    damage: f32,
) -> Result<(), Box<dyn Error>> {
    let health = game.entities[target].health;
    game.attack(tank, target)?;
    advance_until(
        game,
        5400,
        "BattleMaster launches an original shell",
        |game| {
            game.projectiles.iter().any(|shot| {
                shot.source == tank
                    && shot.object.is_some_and(|id| {
                        game.catalog[game.entities[id].type_index].name == "BattleMasterTankShell"
                    })
            })
        },
    )?;
    game.stop(tank)?;
    advance_until(
        game,
        150,
        "BattleMaster shell impacts enemy Command Center",
        |game| game.entities[target].health < health,
    )?;
    require(
        game.entities[target].health.to_bits() == (health - damage).to_bits(),
        "BattleMaster's actual single hit did not use the original paid Uranium damage",
    )
}

fn verify_motion(
    game: &mut Skirmish,
    tank: usize,
    goal: [f32; 2],
    speed: f32,
) -> Result<(), Box<dyn Error>> {
    let start = game.entities[tank].position;
    game.move_to(tank, goal)?;
    advance_until(
        game,
        1800,
        "paid tank reaching its authored tread speed",
        |game| {
            let position = game.entities[tank].position;
            let traveled = (position[0] - start[0]).hypot(position[1] - start[1]);
            game.tread_draw_motion(tank)
                .is_some_and(|(actual, maximum, motive, _)| {
                    maximum.mul_add(30.0, -speed).abs() < 0.01
                        && motive
                        && actual * 30.0 >= speed * 0.95
                        && traveled > 24.0
                })
        },
    )
    .map_err(|error| {
        format!(
            "{error}: requested {speed} units/sec; {}",
            generals_sim::skirmish::diagnostics::movement_details(game, tank)
        )
    })?;
    game.stop(tank)?;
    Ok(())
}

fn verify_death(
    game: &mut Skirmish,
    gunner: usize,
    target: usize,
    nuclear: bool,
) -> Result<(), Box<dyn Error>> {
    let first = game.entities.len();
    game.force_attack(gunner, target)?;
    advance_until(
        game,
        5400,
        "ordinary forced tank combat destroys the paid target",
        |game| !game.entities[target].alive(),
    )?;
    let origin = game.entities[target].position;
    let field = game.entities[first..].iter().any(|actor| {
        game.catalog[actor.type_index].name == "RadiationFieldSmall"
            && (actor.position[0] - origin[0]).hypot(actor.position[1] - origin[1]) < 1.0
    });
    require(
        field == nuclear,
        "tank death at its actual position used the wrong original radiation OCL",
    )?;
    game.stop(gunner)?;
    Ok(())
}
