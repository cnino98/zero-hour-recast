//! Installed SCB checks: passive opponents, followed by a normal-command human army.
use crate::{playtest::MAP_NAME, skirmish_data, terrain::Terrain, world_data};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, OpponentDifficulty, Order, Skirmish};
use std::{error::Error, path::Path};
mod allied;
#[cfg(test)]
mod eight_players;
mod income;
mod performance;

pub fn benchmark(root: &Path) -> Result<(), Box<dyn Error>> {
    performance::run(root)
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut game = load_game(root, MAP_NAME)?;
    for (name, faction, enemy) in [
        ("USA", Faction::Usa, Faction::China),
        ("China", Faction::China, Faction::Gla),
        ("GLA", Faction::Gla, Faction::Usa),
    ] {
        game.reset_with_difficulty(faction, enemy, OpponentDifficulty::Easy)?;
        if let Err(error) = human_victory(&mut game) {
            report_live_computer(&game);
            return Err(format!(
                "{name} human versus active Easy computer at {}: {error}",
                game.tick
            )
            .into());
        }
        println!(
            "{name}: commanded human defeated active original Easy opponent at {}",
            game.tick
        );
    }
    for difficulty in 0..3 {
        for (name, faction) in [
            ("USA", Faction::Usa),
            ("China", Faction::China),
            ("GLA", Faction::Gla),
        ] {
            let (label, level) = difficulty_level(difficulty);
            game.reset_with_difficulty(Faction::Usa, faction, level)?;
            let [supply, income, firing] = verify(&mut game).map_err(|error| {
                format!(
                    "Original {name} {label} computer at tick {}: {error}",
                    game.tick
                )
            })?;
            println!(
                "{name} {label}: autonomous victory at {}; supply building {supply}, deposited income {income}, first firing {firing} (normal 30 Hz frames)",
                game.tick
            );
            println!(
                "{name} {label} omitted source calls: {}",
                game.opponent_unsupported().join(", ")
            );
        }
    }
    Ok(())
}

const fn difficulty_level(index: usize) -> (&'static str, OpponentDifficulty) {
    match index {
        0 => ("Easy", OpponentDifficulty::Easy),
        1 => ("Medium", OpponentDifficulty::Normal),
        _ => ("Hard", OpponentDifficulty::Hard),
    }
}

pub fn load_game(root: &Path, map_name: &str) -> Result<Skirmish, Box<dyn Error>> {
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let map = MapData::parse(&assets.read(map_name)?.bytes)?;
    let mut data = skirmish_data::load(&assets, &map.objects)?;
    let settings = world_data::camera_settings(&assets, root)?;
    let opponent = crate::opponent_data::load(&assets, &map)?;
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )?;
    let (layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )?;
    let mut animations = crate::model_animation::load(&assets, &mut data.models, &data.types)?;
    animations.bind_logical_bones(&mut data.types);
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations)?;
    let movement_terrain = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement_terrain, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    game.configure_opponent(opponent, 1, OpponentDifficulty::Easy)?;
    Ok(game)
}

pub fn multiple(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut game = load_game(root, crate::playtest::FOUR_PLAYER_MAP)?;
    multiple_case(
        &mut game,
        "mixed Easy/Medium/Hard",
        [
            OpponentDifficulty::Easy,
            OpponentDifficulty::Normal,
            OpponentDifficulty::Hard,
        ],
        true,
    )?;
    multiple_case(
        &mut game,
        "three Hard opponents",
        [OpponentDifficulty::Hard; 3],
        false,
    )?;
    allied::run(&mut game)?;
    income::run(&mut game)
}

fn multiple_case(
    game: &mut Skirmish,
    label: &str,
    difficulties: [OpponentDifficulty; 3],
    allow_defeated: bool,
) -> Result<(), Box<dyn Error>> {
    use generals_sim::skirmish::PlayerSlot;
    let factions = [Faction::Usa, Faction::China, Faction::Gla, Faction::Usa];
    let slots = factions
        .into_iter()
        .enumerate()
        .map(|(owner, faction)| PlayerSlot {
            faction: Some(faction),
            occupied: true,
            difficulty: owner.checked_sub(1).map(|index| difficulties[index]),
            team: None,
            start: None,
            color: Some(owner),
        })
        .collect();
    game.reset_slots(slots)?;
    println!("Fallen Empire verification: {label}");
    let mut fired = [false; 4];
    for _ in 0..18_000 {
        game.step();
        for shot in &game.presentation.shots {
            if let Some(fired) = fired.get_mut(shot.owner) {
                *fired = true;
            }
        }
        if game.tick == 1 || game.tick.is_multiple_of(3000) {
            multiple_progress(game, fired);
        }
        if multiple_complete(game, fired, allow_defeated) {
            break;
        }
        if game.ended_at.is_some() {
            break;
        }
    }
    for (owner, &did_fire) in fired.iter().enumerate().skip(1) {
        let stats = &game.players[owner].statistics;
        println!(
            "Fallen Empire {label}, AI slot {owner}: start {:?}, {} buildings, {} units, ${} gathered, fired {} at frame {}",
            game.start_positions[owner],
            stats.buildings_built,
            stats.units_built,
            stats.money_earned,
            did_fire,
            game.tick
        );
        if !did_fire && allow_defeated && game.defeated[owner] {
            println!(
                "  AI slot {owner} was eliminated before firing; mixed-difficulty elimination evidence only"
            );
        }
        if !(did_fire || (allow_defeated && game.defeated[owner]))
            || stats.money_earned == 0
            || stats.buildings_built < 3
        {
            return Err(format!(
                "Fallen Empire {label}, AI slot {owner} did not complete the required economy/build/combat or elimination activity"
            )
            .into());
        }
    }
    if fired.into_iter().skip(1).filter(|&value| value).count() < 2 {
        return Err(format!("Fallen Empire {label}: fewer than two AI players fired").into());
    }
    let omitted = game.opponent_unsupported().join(", ");
    let starts = game.start_positions.clone();
    game.reset_slots(game.slots.clone())?;
    if game.start_positions != starts
        || game.ended_at.is_some()
        || game
            .entities
            .iter()
            .filter(|entity| entity.owner != game.neutral_owner())
            .count()
            != 8
    {
        return Err("Four-player restart failed".into());
    }
    println!(
        "Fallen Empire {label}: independent economy/build activity, required combat or elimination evidence, and restart passed. Unsupported remaining calls: {omitted}"
    );
    Ok(())
}

fn multiple_complete(game: &Skirmish, fired: [bool; 4], allow_defeated: bool) -> bool {
    fired.into_iter().skip(1).filter(|&value| value).count() >= 2
        && (1..4).all(|owner| {
            (fired[owner] || (allow_defeated && game.defeated[owner]))
                && game.players[owner].statistics.money_earned > 0
                && game.players[owner].statistics.buildings_built >= 3
        })
}

fn multiple_progress(game: &Skirmish, fired: [bool; 4]) {
    for (owner, did_fire) in fired.iter().enumerate().skip(1) {
        let stats = &game.players[owner].statistics;
        let live = game
            .entities
            .iter()
            .filter(|entity| entity.owner == owner && entity.alive())
            .count();
        let queued: usize = game
            .entities
            .iter()
            .filter(|entity| entity.owner == owner && entity.alive())
            .map(|entity| entity.queue.len())
            .sum();
        println!(
            "Fallen Empire progress frame {}: AI {owner}, start {:?}, active {}, {live} live objects, {} buildings, {} units, ${} gathered, ${} cash, {queued} queued, fired {did_fire}",
            game.tick,
            game.start_positions[owner],
            game.player_active(owner),
            stats.buildings_built,
            stats.units_built,
            stats.money_earned,
            game.players[owner].cash
        );
        multiple_orders(game, owner);
    }
}

fn multiple_orders(game: &Skirmish, owner: usize) {
    let mut counts = [0; 5];
    let mut closest = f32::INFINITY;
    let mut attack = None;
    for (id, actor) in game.entities.iter().enumerate().filter(|(_, actor)| {
        actor.owner == owner && actor.alive() && game.catalog[actor.type_index].weapon.damage > 0.0
    }) {
        let category = match actor.ai.order {
            Order::Idle => 0,
            Order::Move(_) | Order::Exit(_) | Order::FollowPath => 1,
            Order::Attack(target) => {
                attack.get_or_insert((id, target));
                2
            }
            Order::AttackMove(_) | Order::Guard(_) => 3,
            _ => 4,
        };
        counts[category] += 1;
        for other in game
            .entities
            .iter()
            .filter(|other| other.alive() && game.enemies(owner, other.owner))
        {
            let dx = actor.position[0] - other.position[0];
            let dy = actor.position[1] - other.position[1];
            closest = closest.min(dx.hypot(dy));
        }
    }
    println!(
        "  armed orders [idle, moving, attack, attack-move/guard, other] {counts:?}; nearest live enemy center {closest:.1}"
    );
    if let Some((id, target)) = attack {
        let actor = &game.entities[id];
        if let Some(victim) = game.entities.get(target) {
            println!(
                "  attack {id} {} at {:?} -> {target} {} at {:?}, range {}",
                game.catalog[actor.type_index].name,
                actor.position,
                game.catalog[victim.type_index].name,
                victim.position,
                game.catalog[actor.type_index].weapon.range
            );
        }
    }
}

fn verify(game: &mut Skirmish) -> Result<[u64; 3], Box<dyn Error>> {
    let initial_boxes = game
        .supplies
        .iter()
        .map(|s| u64::from(s.boxes))
        .sum::<u64>();
    let mut milestones = [0; 3];
    let mut static_activity = [false; 4];
    for _ in 0..90_000 {
        game.step();
        for (id, actor) in game.entities.iter().enumerate() {
            if actor.owner == 1 && actor.complete() {
                let defense = game.catalog[actor.type_index].kind == Kind::Defense;
                let occupied = game.is_garrison(id) && !game.passengers(id).is_empty();
                let transported = actor.loaded()
                    && game.catalog[actor.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "TRANSPORT");
                let hacking = game.passengers(id).iter().any(|&passenger| {
                    game.entities[passenger].hacking_condition() == Some("FIRING_A")
                });
                for (slot, active) in [defense, occupied, transported, hacking]
                    .into_iter()
                    .enumerate()
                {
                    if active && !static_activity[slot] {
                        println!(
                            "Original AI completed {} activity {} at frame {}",
                            game.catalog[actor.type_index].name,
                            ["defense", "garrison", "transport", "contained Hacker"][slot],
                            game.tick
                        );
                        static_activity[slot] = true;
                    }
                }
            }
        }
        if milestones[2] == 0 && game.presentation.shots.iter().any(|shot| shot.owner == 1) {
            milestones[2] = game.tick;
        }
        let has = |kind| {
            game.entities.iter().any(|entity| {
                entity.owner == 1
                    && entity.complete()
                    && game.catalog[entity.type_index].kind == kind
            })
        };
        let collected = game
            .supplies
            .iter()
            .map(|s| u64::from(s.boxes))
            .sum::<u64>()
            < initial_boxes;
        if milestones[0] == 0 && has(Kind::SupplyCenter) {
            milestones[0] = game.tick;
        }
        if milestones[1] == 0 && game.players[1].statistics.money_earned > 0 {
            milestones[1] = game.tick;
        }
        if milestones[2] > 0
            && collected
            && game.players[1].statistics.money_earned > 0
            && has(Kind::SupplyCenter)
            && has(Kind::Barracks)
            && has(Kind::Factory)
            && game.outcome == Some(1)
        {
            return Ok(milestones);
        }
        if game.ended_at.is_some() {
            break;
        }
    }
    report_live_computer(game);
    eprintln!("Computer milestones {milestones:?}");
    Err(
        "original-script computer did not complete its economy/production/attack/victory chain"
            .into(),
    )
}

fn report_live_computer(game: &Skirmish) {
    multiple_orders(game, 1);
    let targets: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.owner == 0 && entity.alive())
        .map(|(id, entity)| {
            (
                id,
                &game.catalog[entity.type_index].name,
                entity.position,
                entity.health,
            )
        })
        .collect();
    eprintln!("Surviving human objects {targets:?}");
    for (id, entity) in game
        .entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.owner == 1 && entity.alive())
    {
        if game.catalog[entity.type_index].weapon.damage > 0.0 {
            let target = if let Order::Attack(victim) = entity.ai.order {
                Some(victim)
            } else {
                targets.first().map(|target| target.0)
            };
            if let Some(target) = target {
                eprintln!(
                    "Computer attacker {id}: {}; {}",
                    generals_sim::skirmish::diagnostics::weapon_details(game, id, target),
                    generals_sim::skirmish::diagnostics::movement_details(game, id)
                );
            }
        }
        println!(
            "Computer entity {id} {} at {:?}, build {}, health {}, queue {}",
            game.catalog[entity.type_index].name,
            entity.position,
            entity.build_progress,
            entity.health,
            entity.queue.len()
        );
    }
    eprintln!(
        "Computer cash {}, earned {}, omitted calls {}",
        game.players[1].cash,
        game.players[1].statistics.money_earned,
        game.opponent_unsupported().join(", ")
    );
}

pub fn human_victory(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    use crate::skirmish_verification::{
        construct, friendly_entity, require, verify_combat_and_victory,
    };
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    if game.players[0].faction != Faction::Gla {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let enemy_fired = verify_combat_and_victory(game, barracks, factory, 0)?;
    require(game.outcome == Some(0), "human did not receive victory")?;
    require(
        game.players[1].statistics.money_earned > 0
            && game.players[1].statistics.units_built > 1
            && enemy_fired,
        "computer was not economically active during the human victory check",
    )?;
    require(
        !game.entities.iter().any(|entity| {
            entity.owner == 1 && entity.alive() && game.catalog[entity.type_index].scoring.victory
        }),
        "victory left an enemy victory-counting building or foundation alive",
    )?;
    Ok(())
}

/// A completed four-slot battle produced by the installed growth scenario's ordinary commands.
pub fn four_player_result(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    allied::reset(game)?;
    allied::prepare(game)?;
    for iteration in 0_u32..54_000 {
        if game.ended_at.is_some() {
            println!(
                "Four-player result: native battle finished at {} with winner {:?}",
                game.tick, game.outcome
            );
            return Ok(());
        }
        if iteration.is_multiple_of(30) {
            performance::queue_ready(game)?;
            if iteration.is_multiple_of(120) {
                allied::command_army(game);
            }
        }
        game.step();
    }
    Err("four-player paid battle did not finish within original scenario allowance".into())
}

/// Stop at ordinary local elimination while at least two alliances remain.
pub fn local_defeat_result(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    allied::reset(game)?;
    allied::prepare(game)?;
    for iteration in 0_u32..54_000 {
        if game.defeated[0] {
            crate::skirmish_verification::require(
                game.ended_at.is_none(),
                "local defeat coincided with the final outcome",
            )?;
            println!(
                "Personal defeat: original paid four-player battle reached frame {} with no global winner",
                game.tick
            );
            return Ok(());
        }
        if game.ended_at.is_some() {
            return Err("four-player battle ended before a personal defeat".into());
        }
        if iteration.is_multiple_of(30) {
            performance::queue_ready(game)?;
            if iteration.is_multiple_of(120) {
                allied::command_army(game);
            }
        }
        game.step();
    }
    Err("four-player battle did not reach personal defeat within the scenario allowance".into())
}
