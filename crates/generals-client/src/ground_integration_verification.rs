//! Focused installed checks for the current ground-unit and research additions.
use crate::{playtest::MAP_NAME, skirmish_data, terrain::Terrain, world_data};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use std::{error::Error, path::Path};

pub fn run(root: &Path, strategy: bool) -> Result<(), Box<dyn Error>> {
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let map = MapData::parse(&assets.read(MAP_NAME)?.bytes)?;
    let mut data = skirmish_data::load(&assets, &map.objects)?;
    let settings = world_data::camera_settings(&assets, root)?;
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
    let animations = crate::model_animation::load(&assets, &mut data.models, &data.types)?;
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations)?;
    if strategy {
        animations.verify_strategy_art(&data.models, &data.types)?;
        crate::horde_decal::load(&assets, &data.types)?;
    } else {
        animations.verify_weapon_research_art(&data.models, &data.types)?;
    }
    let options = root.join("UserData/Options.ini");
    drop(crate::control_bar::load(&assets, &data.types, &options)?);
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    crate::veterancy_feedback::load(&assets)?;
    let movement_terrain = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement_terrain, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    if strategy {
        crate::strategy_verification::verify(&mut game)?;
        crate::horde_verification::verify(&mut game)?;
        return Ok(());
    }
    crate::weapon_verification::research::verify(&mut game)?;
    crate::skirmish_verification::crawler::verify(&mut game)?;
    println!(
        "Focused ground integration passed; native controls, art and audio are checked separately"
    );
    Ok(())
}
