//! Authored bridge endpoints and original W3D left-mesh deck width.
//! `W3DBridgeBuffer::loadBridges`, `W3DBridge::{load,getBridgeInfo}`.
use super::{Declaration, Error, MapObject, Terrain, number, value};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::{
    skirmish::{NeutralObject, RegularBridge},
    terrain::{BridgeGeometry, TerrainMap},
};
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

pub(super) fn install(
    assets: &VirtualAssets,
    records: &[MapObject],
    terrain: &Terrain,
    movement: &mut TerrainMap,
) -> Result<Vec<RegularBridge>, Box<dyn Error>> {
    let objects = super::object_definitions(assets)?;
    let has_landmark = |record: &MapObject| {
        objects.get(&record.template).is_some_and(|object| {
            super::field(object, "KindOf")
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "LANDMARK_BRIDGE"))
        })
    };
    if !records
        .iter()
        .any(|record| record.flags & 0x10 != 0 || has_landmark(record))
    {
        return Ok(Vec::new());
    }
    let definitions = definitions(assets)?;
    let default = definitions.get("DefaultBridge");
    let mut bridges = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let first = &records[index];
        index += 1;
        if first.flags & 0x10 == 0 {
            continue;
        }
        let Some(second) = records.get(index) else {
            break;
        };
        if second.flags & 0x20 == 0 {
            continue;
        }
        index += 1;
        let definition = definitions
            .get(&first.template)
            .ok_or("Missing authored Bridge template")?;
        let geometry = endpoint_geometry(assets, first, second, definition, default, terrain)?;
        let object = regular_object(first, &geometry)?;
        let layer = movement.add_bridge(geometry, |position| {
            terrain.sample(bevy::prelude::Vec2::from_array(position) * 0.1) * 10.0
        })?;
        bridges.push(RegularBridge {
            layer,
            template_name: first.template.clone(),
            object,
        });
    }
    install_landmarks(records, &objects, terrain, movement)?;
    Ok(bridges)
}

fn endpoint_geometry(
    assets: &VirtualAssets,
    first: &MapObject,
    second: &MapObject,
    definition: &Declaration,
    default: Option<&Declaration>,
    terrain: &Terrain,
) -> Result<BridgeGeometry, Box<dyn Error>> {
    let model_name = value(definition, "BridgeModelName")
        .or_else(|| default.and_then(|default| value(default, "BridgeModelName")))
        .ok_or("BridgeModelName missing")?;
    let default_scale = default
        .map(|default| number(default, "BridgeScale", 0.0))
        .transpose()?
        .unwrap_or(0.0);
    let scale = number(definition, "BridgeScale", default_scale)?;
    if scale <= 0.0 {
        return Err("BridgeScale must be positive".into());
    }
    let model = crate::skirmish_data::state_model(assets, model_name, &[])?;
    let left_prefix = format!("{model_name}.BRIDGE_LEFT");
    let left = model
        .meshes
        .iter()
        .find(|mesh| {
            mesh.name
                .to_ascii_lowercase()
                .starts_with(&left_prefix.to_ascii_lowercase())
        })
        .ok_or("Bridge has no native left mesh")?;
    let minimum = left
        .positions
        .iter()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min);
    let maximum = left
        .positions
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max);
    if !minimum.is_finite() || !maximum.is_finite() {
        return Err("Bridge left mesh is empty".into());
    }
    let ground =
        |position: [f32; 2]| terrain.sample(bevy::prelude::Vec2::from_array(position) * 0.1) * 10.0;
    let from = [
        first.position[0],
        first.position[1],
        ground([first.position[0], first.position[1]]) + 0.25,
    ];
    let to = [
        second.position[0],
        second.position[1],
        ground([second.position[0], second.position[1]]) + 0.25,
    ];
    geometry(from, to, minimum, maximum, scale)
}

fn regular_object(
    first: &MapObject,
    geometry: &BridgeGeometry,
) -> Result<NeutralObject, Box<dyn Error>> {
    for (key, expected) in [
        ("objectEnabled", true),
        ("objectPowered", true),
        ("objectSelectable", false),
        ("objectUnsellable", false),
    ] {
        if first.properties.get(key).is_some_and(|value| !matches!(value, generals_assets::map::MapValue::Bool(value) if *value == expected)) {
            return Err(format!("Regular bridge has unsupported map property {key}").into());
        }
    }
    if first
        .properties
        .get("objectInitialHealth")
        .is_some_and(|value| !matches!(value, generals_assets::map::MapValue::Int(100)))
    {
        return Err(
            "Regular bridge initial health override requires its source body property consumer"
                .into(),
        );
    }
    for key in first.properties.keys() {
        if matches!(
            key.as_str(),
            "objectMaxHPs" | "objectVisualRange" | "objectShroudClearingDistance"
        ) || key.starts_with("objectGrantUpgrade")
        {
            return Err(format!("Regular bridge has unrepresented map property {key}").into());
        }
    }
    let script_name = match first.properties.get("objectName") {
        None => None,
        Some(generals_assets::map::MapValue::Ascii(name)) => {
            (!name.is_empty()).then(|| name.clone())
        }
        Some(_) => return Err("Regular bridge objectName is not an ASCII string".into()),
    };
    let position = [0, 1, 2].map(|axis| {
        let sum = geometry.corners[0][axis] + geometry.corners[3][axis];
        sum / 2.0
    });
    let direction = [
        geometry.corners[2][0] - geometry.corners[0][0],
        geometry.corners[2][1] - geometry.corners[0][1],
    ];
    Ok(NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        selectable: None,
        script_targetable: super::neutral::property_bool(first, "objectTargetable")?,
        indestructible: super::neutral::property_bool(first, "objectIndestructible")?,
        original_team: "team".into(),
        script_name,
        name: "GenericBridge".into(),
        position,
        angle: direction_angle(direction),
    })
}

// Coord2D::toAngle uses normalized X and signed ACos in Zero Hour.
fn direction_angle(direction: [f32; 2]) -> f32 {
    let square_x = direction[0] * direction[0];
    let square_y = direction[1] * direction[1];
    let length = (square_x + square_y).sqrt();
    if length.partial_cmp(&0.0) == Some(std::cmp::Ordering::Equal) {
        return 0.0;
    }
    let cosine = (direction[0] / length).clamp(-1.0, 1.0);
    let angle = cosine.acos();
    if direction[1] < 0.0 { -angle } else { angle }
}

fn definitions(assets: &VirtualAssets) -> Result<BTreeMap<String, Declaration>, Box<dyn Error>> {
    let mut definitions = BTreeMap::new();
    for prefix in ["data/ini/default/roads", "data/ini/roads"] {
        let mut declarations = match assets.read(&format!("{prefix}.ini")) {
            Ok(asset) => content::scan(asset)?.declarations,
            Err(generals_assets::AssetError::Missing(_)) => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        for path in assets.list(prefix)?.into_iter().filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
        }) {
            declarations.extend(content::scan(assets.read(&path)?)?.declarations);
        }
        for declaration in declarations {
            if declaration.kind == "Bridge"
                && definitions
                    .insert(declaration.name.clone(), declaration)
                    .is_some()
            {
                return Err("Repeated native Bridge definition".into());
            }
        }
    }
    Ok(definitions)
}

fn geometry(
    from: [f32; 3],
    to: [f32; 3],
    minimum: f32,
    maximum: f32,
    scale: f32,
) -> Result<BridgeGeometry, Box<dyn Error>> {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let horizontal_squared = dx * dx;
    let vertical_squared = dy * dy;
    let length = (horizontal_squared + vertical_squared).sqrt();
    if length == 0.0 {
        return Err("Bridge endpoints coincide".into());
    }
    let normal = [-dy / length, dx / length];
    let corner = |point: [f32; 3], side| {
        let x_offset = normal[0] * side * scale;
        let y_offset = normal[1] * side * scale;
        [point[0] + x_offset, point[1] + y_offset, point[2]]
    };
    Ok(BridgeGeometry {
        from,
        to,
        corners: [
            corner(from, maximum),
            corner(from, minimum),
            corner(to, maximum),
            corner(to, minimum),
        ],
    })
}

fn landmark_geometry(position: [f32; 3], angle: f32, major: f32, minor: f32) -> BridgeGeometry {
    let (sine, cosine) = angle.sin_cos();
    let major_cosine = major * cosine;
    let major_sine = major * sine;
    let minor_cosine = minor * cosine;
    let minor_sine = minor * sine;
    let from_left = [
        position[0] - major_cosine - minor_sine,
        position[1] + minor_cosine - major_sine,
        position[2],
    ];
    let to_left = [
        position[0] + major_cosine - minor_sine,
        position[1] + minor_cosine + major_sine,
        position[2],
    ];
    let from_right = [
        position[0] - major_cosine + minor_sine,
        position[1] - minor_cosine - major_sine,
        position[2],
    ];
    let to_right = [
        position[0] + major_cosine + minor_sine,
        position[1] - minor_cosine + major_sine,
        position[2],
    ];
    let midpoint = |first: [f32; 3], second: [f32; 3]| {
        std::array::from_fn(|axis| {
            let sum = first[axis] + second[axis];
            sum / 2.0
        })
    };
    BridgeGeometry {
        from: midpoint(from_left, from_right),
        to: midpoint(to_left, to_right),
        corners: [from_left, from_right, to_left, to_right],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_bridge_geometry_uses_asymmetric_mesh_width_and_endpoint_heights() {
        let geometry = geometry(
            [100.0, 200.0, 10.25],
            [200.0, 200.0, 20.25],
            -10.0,
            20.0,
            0.7,
        )
        .unwrap();
        assert_eq!(
            geometry.corners,
            [
                [100.0, 214.0, 10.25],
                [100.0, 193.0, 10.25],
                [200.0, 214.0, 20.25],
                [200.0, 193.0, 20.25]
            ]
        );
        assert_eq!(
            geometry.height([150.0, 205.0]).to_bits(),
            15.25_f32.to_bits()
        );
        assert!(geometry.contains([100.0, 193.0]));
        assert!(!geometry.contains([150.0, 192.999]));
    }
}

pub(super) fn draw(
    assets: &VirtualAssets,
    records: &[MapObject],
    terrain: &Terrain,
    models: &mut Vec<generals_assets::w3d::Model>,
) -> Result<Vec<crate::playtest_view::WorldObject>, Box<dyn Error>> {
    if !records.iter().any(|record| record.flags & 0x10 != 0) {
        return Ok(Vec::new());
    }
    let definitions = definitions(assets)?;
    let default = definitions.get("DefaultBridge");
    let mut placed = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let first = &records[index];
        index += 1;
        if first.flags & 0x10 == 0 {
            continue;
        }
        let Some(second) = records.get(index) else {
            break;
        };
        if second.flags & 0x20 == 0 {
            continue;
        }
        index += 1;
        let definition = definitions
            .get(&first.template)
            .ok_or("Missing authored Bridge template")?;
        let lookup = |key| {
            value(definition, key).or_else(|| default.and_then(|default| value(default, key)))
        };
        let name = lookup("BridgeModelName").ok_or("BridgeModelName missing")?;
        let texture = lookup("Texture").ok_or("Bridge Texture missing")?;
        let scale: f32 = lookup("BridgeScale")
            .ok_or("BridgeScale missing")?
            .parse()?;
        let ground = |record: &MapObject| {
            let height = terrain
                .sample(bevy::prelude::Vec2::new(record.position[0], record.position[1]) * 0.1)
                * 10.0;
            height + 0.25
        };
        let from = [first.position[0], first.position[1], ground(first)];
        let to = [second.position[0], second.position[1], ground(second)];
        let pieces = draw_models(assets, name, texture, from, to, scale)?;
        for model in pieces {
            let model_index = models.len();
            models.push(model);
            placed.push(crate::playtest_view::WorldObject {
                terrain_bridge: true,
                animation: None,
                model_index,
                position: [0.0; 3],
                angle: 0.0,
                scale: 1.0,
                tree: false,
                tree_data: None,
                prop_radius: None,
                supply: None,
                supply_draw: None,
            });
        }
    }
    Ok(placed)
}

fn mesh_index(model: &generals_assets::w3d::Model, name: &str, part: &str) -> Option<usize> {
    let prefix = format!("{name}.{part}").to_ascii_lowercase();
    model
        .meshes
        .iter()
        .position(|mesh| mesh.name.to_ascii_lowercase().starts_with(&prefix))
}

fn x_bounds(mesh: &generals_assets::w3d::Mesh) -> [f32; 2] {
    [
        mesh.positions
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min),
        mesh.positions
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max),
    ]
}

fn draw_models(
    assets: &VirtualAssets,
    name: &str,
    texture: &str,
    from: [f32; 3],
    to: [f32; 3],
    scale: f32,
) -> Result<Vec<generals_assets::w3d::Model>, Box<dyn Error>> {
    let source = crate::skirmish_data::state_model(assets, name, &[])?;
    let mut direction = std::array::from_fn::<_, 3, _>(|axis| to[axis] - from[axis]);
    let squared = direction.iter().map(|value| value * value).sum::<f32>();
    if squared < 1.0 {
        let length = squared.sqrt();
        if length == 0.0 {
            return Err("Bridge endpoints coincide".into());
        }
        direction = direction.map(|component| component / length);
    }
    let desired = direction
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    let x_squared = direction[0] * direction[0];
    let y_squared = direction[1] * direction[1];
    let normal_length = (x_squared + y_squared).sqrt();
    let normal = [
        -direction[1] / normal_length * scale,
        direction[0] / normal_length * scale,
        0.0,
    ];
    let delta_z = (to[2] - from[2]) / desired;
    let z_squared = delta_z * delta_z;
    let vertical = [-delta_z * scale, 0.0, (1.0 - z_squared).sqrt() * scale];
    let mut pieces = Vec::new();
    let Sections { length, offsets } = sections(&source, name, desired)?;
    let direction = direction.map(|value| value / length);
    for (mesh_index, offset) in offsets {
        // Each repeated source span owns its model data independently.
        let mut model = crate::skirmish_data::state_model(assets, name, &[])?;
        let mut mesh = model.meshes.remove(mesh_index);
        for point in &mut mesh.positions {
            *point = std::array::from_fn(|axis| {
                let along = (point[0] + offset) * direction[axis];
                let across = point[1] * normal[axis];
                let up = point[2] * vertical[axis];
                along + across + up + from[axis]
            });
        }
        if let Some(normals) = &mut mesh.normals {
            for source_normal in normals {
                let transformed: [f32; 3] = std::array::from_fn(|axis| {
                    let along = source_normal[0] * direction[axis];
                    let across = source_normal[1] * normal[axis];
                    let up = source_normal[2] * vertical[axis];
                    along + across + up
                });
                let length = transformed
                    .iter()
                    .map(|component| component * component)
                    .sum::<f32>()
                    .sqrt();
                *source_normal = transformed.map(|component| component / length);
            }
        }
        if let Some(materials) = &mut mesh.materials {
            for source_texture in &mut materials.textures {
                texture.clone_into(&mut source_texture.name);
            }
        }
        model.bounds.min = [0, 1, 2].map(|axis| {
            mesh.positions
                .iter()
                .map(|point| point[axis])
                .fold(f32::INFINITY, f32::min)
        });
        model.bounds.max = [0, 1, 2].map(|axis| {
            mesh.positions
                .iter()
                .map(|point| point[axis])
                .fold(f32::NEG_INFINITY, f32::max)
        });
        model.meshes = vec![mesh];
        pieces.push(model);
    }
    Ok(pieces)
}

#[cfg(test)]
mod installed_tests {
    use super::*;
    use generals_assets::map::MapData;

    #[test]
    fn regular_bridge_facing_uses_zero_hour_signed_acos_and_first_marker_controls() {
        assert_eq!(
            direction_angle([-1.0, -0.0]).to_bits(),
            std::f32::consts::PI.to_bits()
        );
        assert_eq!(direction_angle([0.0, 0.0]).to_bits(), 0.0_f32.to_bits());
        let span = geometry([0.0, 0.0, 7.0], [20.0, 0.0, 9.0], -4.0, 4.0, 1.0).unwrap();
        let mut record = MapObject {
            template: "AuthoredBridge".into(),
            position: [0.0; 3],
            angle: 2.0,
            flags: 0x10,
            properties: BTreeMap::from([
                (
                    "objectName".into(),
                    generals_assets::map::MapValue::Ascii("Span".into()),
                ),
                (
                    "originalOwner".into(),
                    generals_assets::map::MapValue::Ascii("teamplayer0".into()),
                ),
                (
                    "objectSelectable".into(),
                    generals_assets::map::MapValue::Bool(false),
                ),
            ]),
        };
        let object = regular_object(&record, &span).unwrap();
        assert_eq!(object.name, "GenericBridge");
        assert_eq!(object.original_team, "team");
        assert_eq!(object.script_name.as_deref(), Some("Span"));
        assert_eq!(object.position[2].to_bits(), 8.0_f32.to_bits());
        record.properties.insert(
            "objectTargetable".into(),
            generals_assets::map::MapValue::Bool(true),
        );
        assert!(regular_object(&record, &span).unwrap().script_targetable);
        record.properties.insert(
            "objectIndestructible".into(),
            generals_assets::map::MapValue::Bool(true),
        );
        assert!(regular_object(&record, &span).unwrap().indestructible);
        record.properties.insert(
            "objectTargetable".into(),
            generals_assets::map::MapValue::Int(1),
        );
        assert!(regular_object(&record, &span).is_err());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_regular_bridge_keeps_native_generic_body_position_names_and_restart() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let mut map = MapData::parse(
            &assets
                .read("maps/golden oasis/golden oasis.map")
                .unwrap()
                .bytes,
        )
        .unwrap();
        let first = map
            .objects
            .iter_mut()
            .find(|record| record.flags & 0x10 != 0)
            .unwrap();
        first.properties.insert(
            "objectName".into(),
            generals_assets::map::MapValue::Ascii("Original bridge".into()),
        );
        first.properties.insert(
            "originalOwner".into(),
            generals_assets::map::MapValue::Ascii("teamplayer0".into()),
        );
        let settings = crate::world_data::camera_settings(&assets, root).unwrap();
        let terrain = Terrain::new(
            map.heightmap,
            map.tiles,
            settings.adjust_cliffs,
            settings.three_way_blends,
        )
        .unwrap();
        let mut movement = terrain
            .classify_movement(&map.triggers, &map.objects)
            .unwrap();
        let bridges = install(&assets, &map.objects, &terrain, &mut movement).unwrap();
        assert_eq!(bridges.len(), 3);
        let expected = bridges[0].object.position;
        assert_eq!(bridges[0].object.original_team, "team");
        let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
        let index = data
            .types
            .iter()
            .position(|unit| unit.name == "GenericBridge")
            .unwrap();
        assert_original_generic_template(&data.types[index], &data.models[index]);
        let (mut layout, _) = crate::world_data::load(
            &assets,
            &map.objects,
            &terrain,
            &data.types,
            &mut data.models,
        )
        .unwrap();
        layout.regular_bridges = bridges;
        let mut game = data.into_simulation(movement, layout).unwrap();
        for restart in [false, true] {
            if restart {
                game.reset(
                    generals_sim::skirmish::Faction::Usa,
                    generals_sim::skirmish::Faction::Gla,
                )
                .unwrap();
            }
            assert_original_golden_map_controls(&game);
            assert_eq!(
                game.entities
                    .iter()
                    .filter(|actor| actor.type_index == index)
                    .count(),
                3
            );
            let actor = &game.entities[0];
            assert_eq!(actor.type_index, index);
            assert_eq!(actor.owner, game.neutral_owner());
            assert_eq!(actor.layer, 1);
            assert_eq!(
                game.object_position(0).map(f32::to_bits),
                expected.map(f32::to_bits)
            );
            let hit = game.pick_bridge(
                [expected[0], expected[1], expected[2] + 100.0],
                [expected[0], expected[1], expected[2] - 100.0],
            );
            assert!(hit.is_some());
        }
    }

    fn assert_original_generic_template(
        unit: &generals_sim::skirmish::UnitType,
        model: &generals_assets::w3d::Model,
    ) {
        assert!(!unit.selectable);
        assert_eq!(unit.max_health.to_bits(), 1.0_f32.to_bits());
        assert_eq!(unit.minimum_health.to_bits(), 1.0_f32.to_bits());
        assert!(matches!(
            unit.geometry.shape,
            generals_sim::collision_geometry::Shape::Box
        ));
        assert_eq!(
            [
                unit.geometry.major,
                unit.geometry.minor,
                unit.geometry.height
            ]
            .map(f32::to_bits),
            [1.0_f32.to_bits(); 3]
        );
        assert!(model.meshes.is_empty());
    }

    fn assert_original_golden_map_controls(game: &generals_sim::skirmish::Skirmish) {
        assert_eq!(
            game.entities
                .iter()
                .filter(|actor| actor.script_targetable())
                .count(),
            14
        );
        for name in ["MarketVender01", "JunkDealer"] {
            let actors: Vec<_> = game
                .entities
                .iter()
                .filter(|actor| game.catalog[actor.type_index].name == name)
                .collect();
            assert_eq!(actors.len(), 1, "{name}");
            assert!(game.catalog[actors[0].type_index].selectable, "{name}");
            assert!(
                !actors[0].selectable(&game.catalog[actors[0].type_index]),
                "{name}"
            );
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_killing_fields_under_bridge_terrain() {
        use generals_sim::{Cell, terrain::CellType};
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let map = MapData::parse(
            &assets
                .read("maps/killing fields/killing fields.map")
                .unwrap()
                .bytes,
        )
        .unwrap();
        let settings = crate::world_data::camera_settings(&assets, root).unwrap();
        let terrain = Terrain::new(
            map.heightmap,
            map.tiles,
            settings.adjust_cliffs,
            settings.three_way_blends,
        )
        .unwrap();
        let mut movement = terrain
            .classify_movement(&map.triggers, &map.objects)
            .unwrap();
        install(&assets, &map.objects, &terrain, &mut movement).unwrap();
        let landmark = map
            .objects
            .iter()
            .find(|object| object.template == "MonumentTrainBridge")
            .unwrap();
        let center = [landmark.position[0], landmark.position[1]];
        let layer = movement
            .bridge_layers()
            .find(|&layer| movement.bridge_contains(layer, center))
            .unwrap();
        let mut below = 0;
        let mut water = 0;
        let mut cliff = 0;
        let mut legal = Vec::new();
        for y in 0..movement.height() {
            for x in 0..movement.width() {
                let point = [f32::from(x), f32::from(y)].map(|value| {
                    let edge = value * 10.0;
                    edge + 5.0
                });
                let Some(deck) = movement.bridge_height(layer, point) else {
                    continue;
                };
                if !movement.bridge_contains(layer, point) {
                    continue;
                }
                let ground = terrain.sample(bevy::prelude::Vec2::from_array(point) * 0.1) * 10.0;
                if ground >= deck {
                    continue;
                }
                below += 1;
                let cell = movement.cell(Cell { x, y }).unwrap();
                water += usize::from(matches!(cell.kind, CellType::Water));
                cliff += usize::from(matches!(cell.kind, CellType::Cliff));
                if movement.valid_movement(point, 1) {
                    legal.push([point[0], point[1], ground]);
                }
            }
        }
        assert!(below > 0 && water > 0 && cliff > 0);
        assert!(
            legal.is_empty(),
            "original river admits ground routes: {legal:?}"
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_bridge_pairs_landmarks_and_orphan_markers_follow_native_loading() {
        let (assets, _) = crate::cleanup_verification::tests::installed();
        let root = &generals_assets::game_root().unwrap();
        let settings = crate::world_data::camera_settings(&assets, root).unwrap();
        for (path, count, has_landmark) in [
            ("maps/tournament island/tournament island.map", 0, false),
            ("maps/winter wolf/winter wolf.map", 0, false),
            ("maps/alpine assault/alpine assault.map", 1, true),
            ("maps/final crusade/final crusade.map", 2, false),
        ] {
            let map = MapData::parse(&assets.read(path).unwrap().bytes).unwrap();
            let terrain = Terrain::new(
                map.heightmap,
                map.tiles,
                settings.adjust_cliffs,
                settings.three_way_blends,
            )
            .unwrap();
            let mut movement = terrain
                .classify_movement(&map.triggers, &map.objects)
                .unwrap();
            install(&assets, &map.objects, &terrain, &mut movement).unwrap();
            assert_eq!(movement.bridge_layers().count(), count, "{path}");
            if count == 0 {
                // Native W3DBridgeBuffer::loadBridges ignores POINT1 unless the
                // next serialized object is POINT2; these maps retain orphans.
                assert!(map.objects.iter().any(|object| object.flags & 0x10 != 0));
                assert!(
                    !map.objects
                        .windows(2)
                        .any(|pair| pair[0].flags & 0x10 != 0 && pair[1].flags & 0x20 != 0)
                );
                assert!(
                    draw(&assets, &map.objects, &terrain, &mut Vec::new())
                        .unwrap()
                        .is_empty()
                );
                continue;
            }
            if has_landmark {
                let record = map
                    .objects
                    .iter()
                    .find(|record| record.template == "MonumentTrainBridge")
                    .unwrap();
                assert!(movement.bridge_contains(2, [record.position[0], record.position[1]]));
            } else {
                let first = map
                    .objects
                    .iter()
                    .find(|object| object.flags & 0x10 != 0)
                    .unwrap();
                let geometry = movement.bridge_geometry(2).unwrap();
                assert_eq!(
                    [geometry.from[0], geometry.from[1]].map(f32::to_bits),
                    [first.position[0], first.position[1]].map(f32::to_bits)
                );
                let original_ground = terrain
                    .sample(bevy::prelude::Vec2::new(first.position[0], first.position[1]) * 0.1)
                    * 10.0;
                assert_eq!(
                    geometry.from[2].to_bits(),
                    (original_ground + 0.25).to_bits()
                );
                let mut models = Vec::new();
                let draws = draw(&assets, &map.objects, &terrain, &mut models).unwrap();
                assert!(!draws.is_empty());
                assert!(models.iter().all(|model| model.meshes.len() == 1));
                assert!(
                    models
                        .iter()
                        .flat_map(|model| &model.meshes)
                        .all(|mesh| mesh.name.to_ascii_lowercase().contains("bridge_"))
                );
            }
        }
    }
}

fn sections(
    source: &generals_assets::w3d::Model,
    name: &str,
    desired: f32,
) -> Result<Sections, Box<dyn Error>> {
    let left = mesh_index(source, name, "BRIDGE_LEFT").ok_or("Bridge has no native left mesh")?;
    let span = mesh_index(source, name, "BRIDGE_SPAN");
    let right = mesh_index(source, name, "BRIDGE_RIGHT");
    let left_bounds = x_bounds(&source.meshes[left]);
    let section_bounds = span.map(|index| x_bounds(&source.meshes[index]));
    let right_bounds = right.map(|index| x_bounds(&source.meshes[index]));
    let (length, offsets) =
        if let (Some(span), Some(right), Some(section_bounds), Some(right_bounds)) =
            (span, right, section_bounds, right_bounds)
        {
            let length = (right_bounds[1] - left_bounds[0]).max(1.0);
            let span_length = right_bounds[0] - left_bounds[1];
            if span_length <= 0.0 {
                return Err("Bridge span has nonpositive source length".into());
            }
            let margin = 0.05 * length;
            let fixed = left_bounds[1] > section_bounds[0] + margin
                || right_bounds[0] < section_bounds[1] - margin;
            let count = if fixed {
                1.0
            } else {
                ((desired - (length - span_length) + span_length / 2.0) / span_length)
                    .floor()
                    .max(0.0)
            };
            let count = count
                .to_i32()
                .ok_or("Bridge span count exceeds native integer")?;
            let mut offsets = vec![(left, -left_bounds[0])];
            for index in 0..count {
                let displacement =
                    index.to_f32().expect("native span count fits Real") * span_length;
                offsets.push((span, -left_bounds[0] + displacement));
            }
            let displacement =
                (count - 1).to_f32().expect("native span count fits Real") * span_length;
            offsets.push((right, -left_bounds[0] + displacement));
            (length + displacement, offsets)
        } else {
            (
                (left_bounds[1] - left_bounds[0]).max(1.0),
                vec![(left, -left_bounds[0])],
            )
        };
    Ok(Sections { length, offsets })
}

fn install_landmarks(
    records: &[MapObject],
    objects: &std::collections::BTreeMap<String, Declaration>,
    terrain: &Terrain,
    movement: &mut TerrainMap,
) -> Result<(), Box<dyn Error>> {
    for record in records.iter().filter(|record| {
        objects.get(&record.template).is_some_and(|object| {
            super::field(object, "KindOf")
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "LANDMARK_BRIDGE"))
        })
    }) {
        let object = &objects[&record.template];
        if value(object, "Geometry") != Some("BOX") {
            return Err("Native landmark bridge must have BOX geometry".into());
        }
        let major = number(object, "GeometryMajorRadius", 0.0)?;
        let minor = number(object, "GeometryMinorRadius", 0.0)?;
        let ground = |position: [f32; 2]| {
            terrain.sample(bevy::prelude::Vec2::from_array(position) * 0.1) * 10.0
        };
        let position = [
            record.position[0],
            record.position[1],
            ground([record.position[0], record.position[1]]) + record.position[2],
        ];
        movement.add_bridge(
            landmark_geometry(position, record.angle, major, minor),
            ground,
        )?;
    }
    Ok(())
}

struct Sections {
    length: f32,
    offsets: Vec<(usize, f32)>,
}

/// Native `TerrainRoadType` constructor initializes `RadarColor` to black; `newBridge`
/// does not inherit that field from `DefaultBridge`.
pub fn radar_colors(assets: &VirtualAssets) -> Result<BTreeMap<String, [f32; 3]>, Box<dyn Error>> {
    definitions(assets)?
        .into_iter()
        .map(|(name, definition)| {
            let mut color = [0.0; 3];
            if let Some(values) = super::field(&definition, "RadarColor") {
                for (channel, prefix) in color.iter_mut().zip(["R:", "G:", "B:"]) {
                    let value = values
                        .iter()
                        .find_map(|value| value.strip_prefix(prefix))
                        .ok_or("invalid bridge RadarColor")?
                        .parse::<u8>()?;
                    *channel = f32::from(value) / 255.0;
                }
            }
            Ok((name, color))
        })
        .collect()
}
