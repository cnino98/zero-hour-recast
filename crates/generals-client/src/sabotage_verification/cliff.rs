//! Original-map elevated cliff crossing by the already paid reserve Saboteur.
use super::{MAP_NAME, Skirmish, require};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::terrain::TerrainMap;
use std::{error::Error, path::Path};

// Tournament Desert's clear plateaus straddle this authored cliff. The
// native pathfinder accepts the plateaus, but rejects a final goal on a cliff.
pub(super) const LOW: [f32; 2] = [525.0, 1775.0];
pub(super) const HIGH: [f32; 2] = [405.0, 1775.0];

pub(super) fn verify(root: &Path, game: &mut Skirmish, actor: usize) -> Result<(), Box<dyn Error>> {
    let classified = terrain(root)?;
    reach_foot(game, actor, &classified)?;
    let initial_height = game.ground_height(game.entities[actor].position);
    let ascent = cross(game, actor, HIGH, &classified)?;
    let summit_height = game.ground_height(game.entities[actor].position);
    require(
        summit_height - initial_height > 10.0,
        "Saboteur did not gain the original cliff elevation",
    )?;
    let descent = cross(game, actor, LOW, &classified)?;
    require(
        summit_height - game.ground_height(game.entities[actor].position) > 10.0,
        "Saboteur did not descend the original cliff elevation",
    )?;
    require(
        ascent > 0 && descent > 0,
        "Saboteur route avoided the authored cliff cells",
    )?;
    require(
        game.entities[actor].alive(),
        "paid Saboteur failed to survive cliff traversal",
    )?;
    println!(
        "SABOTAGE_CLIFF map={MAP_NAME} actor={actor} low={LOW:?} high={HIGH:?} elevation={} actual_heights={:?} ascent_cliff_frames={ascent} descent_cliff_frames={descent}",
        game.ground_height(HIGH) - game.ground_height(LOW),
        [
            initial_height,
            summit_height,
            game.ground_height(game.entities[actor].position)
        ]
    );
    Ok(())
}

pub(super) fn terrain(root: &Path) -> Result<TerrainMap, Box<dyn Error>> {
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let map = MapData::parse(&assets.read(MAP_NAME)?.bytes)?;
    let settings = crate::world_data::camera_settings(&assets, root)?;
    let terrain = crate::terrain::Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )?;
    terrain.classify_movement(&map.triggers, &map.objects)
}

pub(super) fn reach_foot(
    game: &mut Skirmish,
    actor: usize,
    terrain: &TerrainMap,
) -> Result<(), Box<dyn Error>> {
    let start = game.entities[actor].position;
    require(
        terrain.valid_movement(LOW, 1)
            && terrain.valid_movement(HIGH, 1)
            && game.ground_height(HIGH) - game.ground_height(LOW) > 10.0,
        "Original Tournament Desert cliff fixture lacks its clear, elevated plateaus",
    )?;
    game.move_to(actor, LOW)?;
    crate::skirmish_verification::advance_until(
        game,
        12_000,
        "paid Saboteur reaching original cliff foot",
        |game| arrived(game, actor, LOW, terrain),
    )
    .map_err(|error| {
        format!(
            "{error}; start={start:?} low={LOW:?} high={HIGH:?}; {}",
            generals_sim::skirmish::diagnostics::movement_details(game, actor)
        )
    })
    .map_err(Into::into)
}

pub(super) fn on_cliff(terrain: &TerrainMap, position: [f32; 2]) -> bool {
    terrain.valid_movement(position, 4) && !terrain.valid_movement(position, 1)
}

fn cross(
    game: &mut Skirmish,
    actor: usize,
    goal: [f32; 2],
    terrain: &TerrainMap,
) -> Result<u32, Box<dyn Error>> {
    game.move_to(actor, goal)?;
    let mut frames = 0;
    for _ in 0..12_000 {
        game.step();
        let position = game.entities[actor].position;
        if on_cliff(terrain, position) {
            frames += 1;
        }
        if arrived(game, actor, goal, terrain) {
            require(
                frames > 0,
                "Saboteur route avoided the authored cliff cells",
            )?;
            return Ok(frames);
        }
        require(
            game.entities[actor].alive(),
            "Saboteur died on original cliff route",
        )?;
    }
    Err(format!(
        "Saboteur cliff crossing timed out: {:?} -> {goal:?}; {}",
        game.entities[actor].position,
        generals_sim::skirmish::diagnostics::movement_details(game, actor)
    )
    .into())
}

pub(super) fn arrived(game: &Skirmish, actor: usize, goal: [f32; 2], terrain: &TerrainMap) -> bool {
    let entity = &game.entities[actor];
    let close = game.catalog[entity.type_index]
        .locomotor
        .ground
        .as_ref()
        .expect("installed Saboteur ground locomotor")
        .close_enough;
    // adjustCoordToCell snaps centered/even footprints by at most 5.5 units
    // per axis. Require actual command completion and a clear ground cell,
    // allowing that native snap plus the authored arrival distance.
    let radius = 5.5_f32.mul_add(std::f32::consts::SQRT_2, close);
    matches!(entity.ai.order, generals_sim::skirmish::Order::Idle)
        && squared(entity.position, goal) < radius * radius
        && terrain.valid_movement(entity.position, 1)
}

fn squared(left: [f32; 2], right: [f32; 2]) -> f32 {
    let x = left[0] - right[0];
    let y = left[1] - right[1];
    x.mul_add(x, y * y)
}
