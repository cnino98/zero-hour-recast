use super::*;
use crate::{
    collision_geometry::Shape,
    skirmish::{
        Faction, Kind, NeutralObject,
        tests::{fixture_type, game},
    },
};

fn record(name: &str, label: &str, position: [f32; 3]) -> NeutralObject {
    NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        original_team: "team".into(),
        script_name: Some(label.into()),
        name: name.into(),
        position,
        angle: 0.0,
        script_targetable: true,
        indestructible: false,
        selectable: None,
    }
}

fn wall_map() -> (Skirmish, usize, usize, usize) {
    let mut game = game();
    game.ai_rules.wall_height = 80.0;
    let mut wall = fixture_type(Faction::Civilian, Kind::TechBuilding);
    wall.name = "FortressWall".into();
    wall.kind_of = ["STRUCTURE", "IMMOBILE", "WALK_ON_TOP_OF_WALL"]
        .map(str::to_owned)
        .to_vec();
    wall.geometry.shape = Shape::Box;
    wall.geometry.major = 60.0;
    wall.geometry.minor = 20.0;
    wall.maintenance.rubble_height = Some(5.0);
    game.catalog.push(wall);
    let infantry = game.catalog[5].name.clone();
    game.layout.neutral_objects.extend([
        record("FortressWall", "wall", [350.0, 200.0, 0.0]),
        record(&infantry, "walker", [345.0, 205.0, 80.0]),
        record(&infantry, "below", [345.0, 205.0, 0.0]),
    ]);
    game.spawn_map_bridges().unwrap();
    game.spawn_neutral_objects().unwrap();
    let wall = game.map_objects["wall"];
    let walker = game.map_objects["walker"];
    let below = game.map_objects["below"];
    (game, wall, walker, below)
}

#[test]
fn map_walls_exist_before_ordinary_placement_assigns_native_layer_and_xyz() {
    let (game, wall, walker, below) = wall_map();
    assert!(wall < walker);
    assert_eq!(game.entities[walker].layer, 15);
    assert_eq!(game.entities[below].layer, 1);
    assert_eq!(
        game.object_position(walker).map(f32::to_bits),
        [345.0_f32, 205.0, 80.0].map(f32::to_bits)
    );
    assert_eq!(game.object_position(below)[2].to_bits(), 0.0_f32.to_bits());
    assert_eq!(game.entities[wall].owner, game.neutral_owner());
}

#[test]
fn wall_rubble_kills_only_objects_on_the_removed_wall_and_reclassifies_cells() {
    let (mut game, wall, walker, below) = wall_map();
    assert!(game.movement_terrain.is_point_on_wall([345.0, 205.0]));
    game.apply_weapon_hit(
        wall,
        wall,
        999_999.0,
        DamageType::Unresistable,
        DeathType::Normal,
    );
    assert!(!game.entities[walker].alive());
    assert!(game.entities[below].alive());
    assert!(matches!(
        game.entities[walker].damage_state.death_type,
        DeathType::Splatted
    ));
    assert_eq!(
        game.entities[walker].damage_state.last_attacker,
        Some(walker)
    );
    assert!(!game.movement_terrain.is_point_on_wall([345.0, 205.0]));
}

#[test]
fn direct_wall_delete_removes_id_before_destructor_footprint_falling_damage() {
    let (mut game, wall, walker, below) = wall_map();
    game.destroy_maintenance_object(wall);
    assert!(!game.movement_terrain.wall_piece_ids().any(|id| id == wall));
    assert!(game.entities[walker].alive());
    game.process_update_destruction();
    assert!(!game.entities[walker].alive());
    assert!(game.entities[below].alive());
    assert!(!game.movement_terrain.is_point_on_wall([345.0, 205.0]));
}
