use crate::{
    playtest_view::{WorldObject, WorldObjects},
    skirmish_data,
    supply_presentation::SupplyDraw,
    terrain::Terrain,
};
use generals_assets::{
    content::{self, Declaration},
    map::MapObject,
    virtual_assets::VirtualAssets,
    w3d::Model,
};
use generals_sim::collision_geometry::Shape;
use generals_sim::skirmish::{Footprint, MapLayout, MapObstacle, PlayerStart};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::Arc,
};

mod admission;
mod bridges;
pub use bridges::radar_colors as bridge_radar_colors;
mod map_content;
mod map_text;
mod neutral;
mod obstacles;
#[cfg(test)]
mod tests;

pub struct CameraSettings {
    pub pitch: f32,
    pub yaw: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub viewport_height: f32,
    pub adjust_cliffs: bool,
    pub three_way_blends: bool,
    pub time_of_day: String,
    pub horizontal_scroll: f32,
    pub vertical_scroll: f32,
    pub scroll_factor: f32,
}

pub use map_content::MapContent;

/// Produces effective INI files and preserves ordered original override sources.
pub fn map_content(assets: &VirtualAssets, map_path: &str) -> Result<MapContent, Box<dyn Error>> {
    map_content::load(assets, map_path)
}

/// Binds bridge deck dimensions from the original left W3D mesh to native map endpoints.
pub fn install_bridges(
    assets: &VirtualAssets,
    records: &[MapObject],
    terrain: &Terrain,
    movement: &mut generals_sim::terrain::TerrainMap,
) -> Result<Vec<generals_sim::skirmish::RegularBridge>, Box<dyn Error>> {
    bridges::install(assets, records, terrain, movement)
}

/// Adds the selected map's local labels only when global `GameText` has no label.
pub fn map_labels(
    assets: &VirtualAssets,
    map_path: &str,
    labels: &mut BTreeMap<String, String>,
) -> Result<(), Box<dyn Error>> {
    map_text::load(assets, map_path, labels)
}

pub fn camera_settings(
    assets: &VirtualAssets,
    root: &std::path::Path,
) -> Result<CameraSettings, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let global = file
        .declarations
        .iter()
        .find(|item| item.kind == "GameData")
        .ok_or("GameData missing")?;
    // Defaults from GlobalData::GlobalData; all supplied overrides are read verbatim.
    let mut scroll_factor = number(global, "KeyboardDefaultScrollSpeedFactor", 0.5)?;
    // OptionPreferences::getScrollFactor replaces the INI keyboard factor.
    match std::fs::read_to_string(root.join("UserData/Options.ini")) {
        Ok(options) => {
            scroll_factor = crate::options_preferences::scroll_factor(&options, scroll_factor)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(CameraSettings {
        pitch: number(global, "CameraPitch", 0.0)?.to_radians(),
        yaw: number(global, "CameraYaw", 0.0)?.to_radians(),
        min_height: number(global, "MinCameraHeight", 0.0)? * 0.1,
        max_height: number(global, "MaxCameraHeight", 0.0)? * 0.1,
        viewport_height: number(global, "ViewportHeightScale", 0.8)?,
        adjust_cliffs: value(global, "AdjustCliffTextures")
            .is_some_and(|text| text.eq_ignore_ascii_case("Yes")),
        three_way_blends: number(global, "Use3WayTerrainBlends", 1.0)? != 0.0,
        time_of_day: value(global, "TimeOfDay")
            .ok_or("GameData TimeOfDay missing")?
            .to_owned(),
        horizontal_scroll: number(global, "HorizontalScrollSpeedFactor", 1.0)?,
        vertical_scroll: number(global, "VerticalScrollSpeedFactor", 1.0)?,
        scroll_factor,
    })
}

pub fn load(
    assets: &VirtualAssets,
    records: &[MapObject],
    terrain: &Terrain,
    types: &[generals_sim::skirmish::UnitType],
    models: &mut Vec<Model>,
) -> Result<(MapLayout, WorldObjects), Box<dyn Error>> {
    let settings = admission::installed(assets, &generals_assets::game_root()?)?;
    load_with_admission(assets, records, terrain, types, models, &settings)
}

fn load_with_admission(
    assets: &VirtualAssets,
    records: &[MapObject],
    terrain: &Terrain,
    types: &[generals_sim::skirmish::UnitType],
    models: &mut Vec<Model>,
    settings: &admission::Settings,
) -> Result<(MapLayout, WorldObjects), Box<dyn Error>> {
    let definitions = object_definitions(assets)?;
    let mut layout = map_layout(assets, records)?;
    layout.height_map = Some(terrain.height_map());
    let mut placed = bridges::draw(assets, records, terrain, models)?;
    let mut model_indices = BTreeMap::new();
    let mut omitted = BTreeSet::new();
    for record in records {
        if record.waypoint_id().is_some() || record.flags & 0x36 != 0 {
            continue;
        }
        if register_catalog_object(&mut layout, record, types, settings)? {
            continue;
        }
        let Some(object) = definitions.get(&record.template) else {
            omitted.insert(format!(
                "{}: no Object template (road/bridge rendering pending)",
                record.template
            ));
            continue;
        };
        let chain = reskin_chain(object, &definitions)?;
        let base = chain.last().copied().ok_or("Empty object ancestry")?;
        let kind = field(base, "KindOf").unwrap_or(&[]);
        if !settings.use_trees && kind.iter().any(|kind| kind == "SHRUBBERY") {
            continue;
        }
        let xy = [record.position[0], record.position[1]];
        let ground = terrain.sample(bevy::prelude::Vec2::from_array(xy) * 0.1) * 10.0;
        let supply = place_simulation_object(&mut layout, base, &chain, record, ground, settings)?;
        let visual_template = chain
            .iter()
            .copied()
            .find(|object| field(object, "Draw").is_some())
            .unwrap_or(base);
        if !model_indices.contains_key(&record.template) {
            match map_models(assets, visual_template, base, supply.is_some(), models) {
                Ok(draws) => {
                    model_indices.insert(record.template.clone(), draws);
                }
                Err(error) => {
                    if supply.is_some()
                        || skirmish_data::blocks(&visual_template.lines, 0, "Draw")?
                            .iter()
                            .any(|draw| {
                                draw[0]
                                    .tokens
                                    .get(1)
                                    .is_some_and(|name| name == "W3DTreeDraw")
                            })
                    {
                        return Err(format!("{} supply draw: {error}", record.template).into());
                    }
                    omitted.insert(format!("{}: {error}", record.template));
                    continue;
                }
            }
        }
        // GameLogic's placed-object loader adds stored Z to the actual terrain height.
        let z = ground + record.position[2];
        for draw in &model_indices[&record.template] {
            placed.push(WorldObject {
                animation: draw.animation.as_ref().map(Arc::clone),
                model_index: draw.index,
                position: [xy[0], xy[1], z],
                angle: record.angle,
                scale: number(base, "Scale", 1.0)?,
                tree: draw.tree,
                tree_data: draw.tree_data.as_ref().map(|data| {
                    let index = layout
                        .world_object_order
                        .iter()
                        .filter(|placement| {
                            matches!(
                                placement,
                                generals_sim::skirmish::MapWorldPlacement::Tree(_)
                            )
                        })
                        .count();
                    layout
                        .world_object_order
                        .push(generals_sim::skirmish::MapWorldPlacement::Tree(index));
                    (index, Arc::clone(data))
                }),
                prop_radius: draw.prop_radius,
                supply,
                supply_draw: draw.supply.clone(),
            });
        }
    }
    if layout.supplies.is_empty() {
        return Err("Map has no resolved supply sources".into());
    }
    for reason in omitted {
        eprintln!("Unported map draw: {reason}");
    }
    Ok((layout, WorldObjects(placed)))
}

fn register_catalog_object(
    layout: &mut MapLayout,
    record: &MapObject,
    types: &[generals_sim::skirmish::UnitType],
    settings: &admission::Settings,
) -> Result<bool, Box<dyn Error>> {
    let Some(unit) = types.iter().find(|unit| unit.name == record.template) else {
        return Ok(false);
    };
    if !settings.use_trees && unit.kind_of.iter().any(|kind| kind == "SHRUBBERY") {
        return Ok(true);
    }
    if !admission::creates_object(&unit.kind_of, unit.fence_width, settings) {
        return Ok(false);
    }
    let index = layout.neutral_objects.len();
    neutral::register(layout, record)?;
    if unit.supply.warehouse.is_some() {
        layout.supplies.push(index);
    }
    if layout.neutral_objects.len() != index {
        layout
            .world_object_order
            .push(generals_sim::skirmish::MapWorldPlacement::Neutral(index));
    }
    Ok(true)
}

fn place_simulation_object(
    layout: &mut MapLayout,
    base: &Declaration,
    chain: &[&Declaration],
    record: &MapObject,
    ground: f32,
    settings: &admission::Settings,
) -> Result<Option<usize>, Box<dyn Error>> {
    let kind = field(base, "KindOf").unwrap_or(&[]);
    let xy = [record.position[0], record.position[1]];
    let footprint = map_footprint(chain, xy, record.angle)
        .map_err(|error| format!("{} map geometry: {error}", record.template))?;
    // GameLogic does not create Objects for optimized trees or props.
    let world_object =
        if admission::creates_object(kind, number(base, "FenceWidth", 0.0)?, settings) {
            let geometry_value = |key| chain.iter().find_map(|object| value(object, key));
            let index = layout.world_objects.len();
            layout
                .world_objects
                .push(generals_sim::skirmish::MapWorldObject {
                    template: record.template.clone(),
                    kind_of: kind.to_vec(),
                    position: [xy[0], xy[1], ground + record.position[2]],
                    angle: record.angle,
                    geometry: generals_sim::collision_geometry::Geometry {
                        shape: footprint.shape,
                        is_small: geometry_value("GeometryIsSmall")
                            .is_some_and(|text| text.eq_ignore_ascii_case("Yes")),
                        major: footprint.major,
                        minor: footprint.minor,
                        height: scalar(geometry_value("GeometryHeight"), "GeometryHeight", 1.0)?,
                    },
                    original_team: original_team(record)?,
                });
            layout
                .world_object_order
                .push(generals_sim::skirmish::MapWorldPlacement::Object(index));
            Some(index)
        } else {
            None
        };
    if let Some(fence) = obstacles::fence(base, record)? {
        layout.obstacle_order.push(MapObstacle::Fence {
            index: layout.fences.len(),
            transparent: kind.iter().any(|flag| flag == "CAN_SEE_THROUGH_STRUCTURE"),
        });
        layout.fences.push(fence);
    }
    let obstacle = obstacles::admitted(chain, record, ground)
        .map_err(|error| format!("{} map placement: {error}", record.template))?
        .then_some(layout.obstacles.len())
        .filter(|_| world_object.is_some());
    if kind.iter().any(|flag| flag == "SUPPLY_SOURCE") {
        return Err("Supply source must have an actual catalog Object and Dock module".into());
    }
    if obstacle.is_some() {
        layout.obstacle_order.push(MapObstacle::Static {
            index: layout.obstacles.len(),
            transparent: kind.iter().any(|flag| flag == "CAN_SEE_THROUGH_STRUCTURE"),
        });
        layout.obstacle_teams.push(original_team(record)?);
        layout.obstacle_objects.push(world_object);
        layout.obstacles.push(footprint);
    }
    Ok(None)
}

fn map_footprint(
    chain: &[&Declaration],
    position: [f32; 2],
    angle: f32,
) -> Result<Footprint, Box<dyn Error>> {
    // ThingFactory copies the parent before applying the restricted reskin table,
    // which includes geometry overrides as well as Draw modules.
    let geometry_value = |key| chain.iter().find_map(|object| value(object, key));
    Ok(Footprint {
        position,
        angle,
        major: scalar(
            geometry_value("GeometryMajorRadius"),
            "GeometryMajorRadius",
            1.0,
        )?,
        minor: scalar(
            geometry_value("GeometryMinorRadius"),
            "GeometryMinorRadius",
            1.0,
        )?,
        shape: match geometry_value("Geometry").unwrap_or("SPHERE") {
            "BOX" => Shape::Box,
            "CYLINDER" => Shape::Cylinder,
            "SPHERE" => Shape::Sphere,
            name => return Err(format!("Unsupported original map geometry {name}").into()),
        },
    })
}

struct MapDraw {
    animation: Option<Arc<crate::model_animation::WorldDraw>>,
    index: usize,
    tree: bool,
    tree_data: Option<Arc<crate::tree_buffer::data::Data>>,
    prop_radius: Option<f32>,
    supply: Option<SupplyDraw>,
}

fn map_models(
    assets: &VirtualAssets,
    object: &Declaration,
    base: &Declaration,
    is_supply: bool,
    models: &mut Vec<Model>,
) -> Result<Vec<MapDraw>, Box<dyn Error>> {
    let draws = skirmish_data::blocks(&object.lines, 0, "Draw")?;
    let mut result = Vec::new();
    // Supply sources own multiple draw modules on one Drawable: omitting the
    // secondary W3DSupplyDraw makes the full warehouse look empty.
    for (draw_index, draw) in draws.into_iter().enumerate() {
        let module = draw[0].tokens.get(1).map(String::as_str);
        if draw_index > 0
            && (!is_supply || !matches!(module, Some("W3DSupplyDraw" | "W3DModelDraw")))
        {
            continue;
        }
        let model = skirmish_data::normal_draw_model(assets, draw)?;
        let supply = if is_supply {
            let prefix = if module == Some("W3DSupplyDraw") {
                Some(skirmish_data::text(draw, 1, "SupplyBonePrefix")?)
            } else {
                None
            };
            Some(SupplyDraw::new(
                &model,
                prefix,
                supply_boxes(base)?,
                delete_when_empty(base)?,
            )?)
        } else {
            None
        };
        let index = models.len();
        let prop_radius = if module == Some("W3DPropDraw") {
            Some(
                model
                    .scene_sphere
                    .as_ref()
                    .ok_or("W3DPropDraw native bounding sphere has unresolved subobjects")?
                    .radius,
            )
        } else {
            None
        };
        let tree_data = (module == Some("W3DTreeDraw"))
            .then(|| crate::tree_buffer::data::Data::load(assets, draw, &model).map(Arc::new))
            .transpose()?;
        models.push(model);
        result.push(MapDraw {
            animation: (is_supply && draw_index > 0 && module == Some("W3DModelDraw")).then(|| {
                Arc::new(crate::model_animation::WorldDraw {
                    template: object.name.clone(),
                    module: draw[0].tokens.get(2).cloned().unwrap_or_default(),
                })
            }),
            index,
            tree: module == Some("W3DTreeDraw"),
            tree_data,
            prop_radius,
            supply,
        });
    }
    Ok(result)
}

fn delete_when_empty(object: &Declaration) -> Result<bool, Box<dyn Error>> {
    for module in skirmish_data::blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "SupplyWarehouseDockUpdate")
        {
            return Ok(skirmish_data::field(module, 1, "DeleteWhenEmpty")
                .and_then(|line| line.tokens.get(1))
                .is_some_and(|value| value.eq_ignore_ascii_case("Yes")));
        }
    }
    Err("Supply source lacks its warehouse dock module".into())
}

pub fn object_definitions(
    assets: &VirtualAssets,
) -> Result<BTreeMap<String, Declaration>, Box<dyn Error>> {
    let mut definitions = BTreeMap::new();
    for path in assets.list("data/ini/object/")? {
        if !path.to_ascii_lowercase().ends_with(".ini") {
            continue;
        }
        for object in content::scan(assets.read(&path)?)?.declarations {
            if matches!(object.kind.as_str(), "Object" | "ObjectReskin") {
                definitions.insert(object.name.clone(), object);
            }
        }
    }
    for object in content::scan(assets.read("data/ini/crate.ini")?)?.declarations {
        if matches!(object.kind.as_str(), "Object" | "ObjectReskin") {
            definitions.insert(object.name.clone(), object);
        }
    }
    Ok(definitions)
}

pub fn validate_map_objects(
    records: &[MapObject],
    definitions: &BTreeMap<String, Declaration>,
    types: &[generals_sim::skirmish::UnitType],
) -> Result<(), Box<dyn Error>> {
    for record in records {
        // WorldHeightMap identifies these records by typed metadata, not by
        // an Object template. Static scorch/light rendering remains separate.
        if record.waypoint_id().is_some()
            || record.flags & 0x36 != 0
            || matches!(
                record.properties.get("scorchType"),
                Some(generals_assets::map::MapValue::Int(_))
            )
            || matches!(
                record.properties.get("lightHeightAboveTerrain"),
                Some(generals_assets::map::MapValue::Real(_))
            )
        {
            continue;
        }
        let object = definitions
            .get(&record.template)
            .ok_or_else(|| format!("Missing placed Object template {}", record.template))?;
        let chain = reskin_chain(object, definitions)?;
        let base = chain.last().copied().ok_or("Empty object ancestry")?;
        let kind = field(base, "KindOf").unwrap_or(&[]);
        if kind.iter().any(|flag| {
            matches!(
                flag.as_str(),
                "CAPTURABLE" | "CAN_ATTACK" | "MINE" | "CRATE"
            )
        }) && !types.iter().any(|unit| unit.name == record.template)
        {
            return Err(format!(
                "Placed gameplay object {} is not translated",
                record.template
            )
            .into());
        }
    }
    Ok(())
}

fn player_starts(records: &[MapObject]) -> Result<Vec<PlayerStart>, Box<dyn Error>> {
    let mut starts = Vec::new();
    for player in 1..=8 {
        let name = format!("Player_{player}_Start");
        if records
            .iter()
            .any(|object| object.waypoint_name() == Some(name.as_str()))
        {
            starts.push(player_start(records, player)?);
        } else {
            break;
        }
    }
    Ok(starts)
}

fn player_start(objects: &[MapObject], player: u8) -> Result<PlayerStart, Box<dyn Error>> {
    let named = |suffix| {
        let name = format!("Player_{player}_{suffix}");
        objects
            .iter()
            .find(|object| object.waypoint_name() == Some(name.as_str()))
            .map(|object| [object.position[0], object.position[1]])
    };
    Ok(PlayerStart {
        position: named("Start")
            .ok_or_else(|| format!("Map has no Player_{player}_Start waypoint"))?,
        rally: named("Rally"),
    })
}

fn supply_boxes(object: &Declaration) -> Result<u32, Box<dyn Error>> {
    for module in skirmish_data::blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "SupplyWarehouseDockUpdate")
        {
            continue;
        }
        let approaches = skirmish_data::field(module, 1, "NumberApproachPositions")
            .map(skirmish_data::scalar)
            .transpose()?
            .unwrap_or("0");
        if approaches != "-1" {
            return Err(format!("{} uses an unported fixed warehouse dock", object.name).into());
        }
        return Ok(skirmish_data::field(module, 1, "StartingBoxes")
            .map(skirmish_data::scalar)
            .transpose()?
            .unwrap_or("1")
            .parse()?);
    }
    Err(format!("{} supply source lacks its warehouse dock", object.name).into())
}

fn field<'a>(object: &'a Declaration, name: &str) -> Option<&'a [String]> {
    object
        .lines
        .iter()
        .rev()
        .find(|line| line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == name))
        .map(|line| &line.tokens[1..])
}

fn value<'a>(object: &'a Declaration, name: &str) -> Option<&'a str> {
    field(object, name)?.first().map(String::as_str)
}

fn number(object: &Declaration, name: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    scalar(value(object, name), name, default)
}

fn scalar(value: Option<&str>, name: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    let result = value.map_or(Ok(default), str::parse)?;
    if !result.is_finite() {
        return Err(format!("{name} is not finite").into());
    }
    Ok(result)
}

fn reskin_chain<'a>(
    object: &'a Declaration,
    definitions: &'a BTreeMap<String, Declaration>,
) -> Result<Vec<&'a Declaration>, Box<dyn Error>> {
    let mut chain = vec![object];
    let mut current = object;
    while let Some(parent) = &current.parent {
        if chain.iter().any(|object| &object.name == parent) {
            return Err(format!("ObjectReskin cycle at {parent}").into());
        }
        current = definitions
            .get(parent)
            .ok_or_else(|| format!("Missing ObjectReskin parent {parent}"))?;
        chain.push(current);
    }
    Ok(chain)
}

fn partition_cell_size(assets: &VirtualAssets) -> Result<f32, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let global = file
        .declarations
        .iter()
        .find(|item| item.kind == "GameData")
        .ok_or("GameData missing")?;
    // GlobalData constructor is zero; PartitionManager clamps it before map initialization.
    Ok(number(global, "PartitionCellSize", 0.0)?.max(1.0))
}

fn map_layout(assets: &VirtualAssets, records: &[MapObject]) -> Result<MapLayout, Box<dyn Error>> {
    let starts = player_starts(records)?;
    Ok(MapLayout {
        partition_cell_size: partition_cell_size(assets)?,
        random_seed: 1,
        starts,
        neutral_objects: Vec::new(),
        regular_bridges: Vec::new(),
        supplies: Vec::new(),
        obstacles: Vec::new(),
        fences: Vec::new(),
        scripts: Vec::new(),
        map_template_order: Vec::new(),
        height_map: None,
        map_sides: Vec::new(),
        map_teams: Vec::new(),
        map_build_lists: Vec::new(),
        obstacle_teams: Vec::new(),
        world_objects: Vec::new(),
        world_object_order: Vec::new(),
        obstacle_objects: Vec::new(),
        obstacle_order: Vec::new(),
    })
}

fn original_team(record: &MapObject) -> Result<String, Box<dyn Error>> {
    match record.properties.get("originalOwner") {
        Some(generals_assets::map::MapValue::Ascii(name)) => Ok(name.clone()),
        None => Ok("team".into()),
        _ => Err("Static map originalOwner requires ASCII text".into()),
    }
}
