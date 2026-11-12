//! Device-free fixtures use the production map, navigation and art loaders.
mod bridge_tests;
mod texture_tests;

use super::*;

pub struct Fixture {
    pub data: skirmish_data::SkirmishData,
    pub layout: generals_sim::skirmish::MapLayout,
    terrain: Terrain,
    movement: generals_sim::terrain::TerrainMap,
    partition_cell_size: f32,
}

impl Fixture {
    pub fn load(root: &Path, path: &str) -> Result<Self, Box<dyn Error>> {
        let world = World::new();
        let (assets, map, metadata, _) = load_map_inputs(&world, root, path)?;
        let mut data = skirmish_data::load(&assets, &map.objects)?;
        let settings = world_data::camera_settings(&assets, root)?;
        let graph = crate::opponent_data::map_data(&map)?;
        let terrain = Terrain::new(
            map.heightmap,
            map.tiles,
            settings.adjust_cliffs,
            settings.three_way_blends,
        )?;
        let (movement, bridges) = load_navigation(&assets, &terrain, &map.triggers, &map.objects)?;
        let (layout, objects) = load_map_world(
            &assets,
            &metadata,
            &map.objects,
            &terrain,
            &mut data,
            bridges,
            (&graph, false),
        )?;
        let (_, projectiles) = load_unit_art(&assets, &mut data, &objects)?;
        Ok(Self {
            data,
            layout,
            terrain,
            movement,
            partition_cell_size: projectiles.partition_cell_size,
        })
    }

    #[must_use]
    pub fn draw_region(&self) -> [[f32; 2]; 2] {
        self.terrain.draw_region()
    }

    pub fn into_simulation(self) -> Result<Skirmish, Box<dyn Error>> {
        let mut game =
            prepare_simulation(&World::new(), self.data, self.movement, self.layout, None)?;
        self.terrain
            .configure_projectiles(&mut game, self.partition_cell_size)?;
        Ok(game)
    }
}
