use super::{Setup, details, require};
use generals_sim::skirmish::Skirmish;
use std::error::Error;

pub(super) fn verify(
    game: &mut Skirmish,
    setup: &Setup,
    upgraded: bool,
) -> Result<(), Box<dyn Error>> {
    let first = game.entities.len();
    let health = game.entities[setup.target].health;
    let mut minimum_health = health;
    for &id in &setup.migs {
        game.attack(id, setup.target)?;
    }
    let mut shots = [0; 4];
    let mut missile = false;
    let mut field = false;
    let mut storm = false;
    let mut returned = [false; 4];
    let mut shot_frames = Vec::new();
    let mut impacts = Vec::new();
    let mut observed = first;
    let names = if upgraded {
        ["FireFieldUpgradedSmall", "BlackNapalmFirestormSmall"]
    } else {
        ["FireFieldSmall", "FirestormSmall"]
    };
    for _ in 0..9000 {
        advance_sortie_frame(game, setup)?;
        for shot in &game.presentation.shots {
            if setup.migs.contains(&shot.source) {
                shot_frames.push((game.tick, shot.source));
            }
        }
        for actor in &game.entities[observed..] {
            let name = &game.catalog[actor.type_index].name;
            if name.contains("Fire") {
                impacts.push((game.tick, name.clone(), actor.position));
            }
        }
        observed = game.entities.len();
        minimum_health = minimum_health.min(game.entities[setup.target].health);
        for (index, &id) in setup.migs.iter().enumerate() {
            shots[index] += game
                .presentation
                .shots
                .iter()
                .filter(|shot| shot.source == id)
                .count();
            returned[index] |= matches!(
                game.entities[id].aircraft_phase(),
                Some("return for landing" | "landing" | "reload ammunition")
            );
        }
        missile |= game
            .projectiles
            .iter()
            .any(|projectile| setup.migs.contains(&projectile.source));
        field |= game.entities[first..]
            .iter()
            .any(|actor| game.catalog[actor.type_index].name == names[0]);
        storm |= game.entities[first..]
            .iter()
            .any(|actor| game.catalog[actor.type_index].name == names[1]);
        if shots.iter().all(|&count| count >= 2)
            && setup.migs.iter().all(|&id| {
                game.entities[id].aircraft_phase() == Some("idle")
                    && game.entities[id].weapon_rounds_remaining(0) == 2
                    && game.height_above_ground(id) < 10.0
            })
        {
            require(
                shots == [2; 4],
                "MiG sortie did not consume exactly two missiles per aircraft",
            )?;
            if !(missile && field && storm) {
                return Err(format!("MiG shared firestorm failed upgraded={upgraded} missile={missile} field={field} storm={storm}; shots={shot_frames:?}; impacts={impacts:?}; {}", details(game, setup)).into());
            }
            require(
                returned.into_iter().all(|value| value),
                "MiG did not return through its original landing/reload state",
            )?;
            require(
                minimum_health < health,
                "MiG payload did not damage its real target",
            )?;
            return Ok(());
        }
    }
    Err(format!("MiG sortie timed out upgraded={upgraded} shots={shots:?} field={field} storm={storm} returned={returned:?}; {}", details(game, setup)).into())
}

fn advance_sortie_frame(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let before = setup.migs.map(|id| {
        let actor = &game.entities[id];
        (
            actor.aircraft_phase(),
            game.object_position(id),
            actor.health,
        )
    });
    game.step();
    for (&id, previous) in setup.migs.iter().zip(before) {
        let actor = &game.entities[id];
        if !actor.alive() {
            let attacker = actor.last_attacker().map(|source| {
                let source = &game.entities[source];
                (&game.catalog[source.type_index].name, source.position)
            });
            return Err(format!(
                "MiG {id} died during the sortie at tick {}, previous={previous:?}, attacker={attacker:?}, death type={}; {}",
                game.tick, actor.death_type() as u32, details(game, setup)
            ).into());
        }
    }
    Ok(())
}
