//! Admission of original map-authored civilian scripts; unsupported calls fail.
use generals_assets::{
    content::Declaration,
    map::{MapObject, MapValue},
    scripts::{MapScripts, ScriptCall},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{
    MapLayout, UnitType,
    map_scripts::{Action, Condition, Group, Script},
};
use num_traits::ToPrimitive;
use std::{collections::BTreeMap, error::Error};

mod startup;
#[cfg(test)]
mod startup_tests;
#[cfg(test)]
mod warehouse_tests;

type BoxError = Box<dyn Error>;

pub struct Startup {
    pub template_order: Vec<generals_sim::skirmish::map_scripts::TemplateDefinition>,
    pub sides: Vec<generals_sim::skirmish::map_scripts::SideDefinition>,
    pub teams: Vec<generals_sim::skirmish::map_scripts::TeamDefinition>,
    pub build_lists: Vec<generals_sim::skirmish::map_scripts::BuildList>,
}

impl Startup {
    pub fn apply(self, layout: &mut MapLayout) {
        layout.map_sides = self.sides;
        layout.map_teams = self.teams;
        layout.map_build_lists = self.build_lists;
        layout.map_template_order = self.template_order;
    }
}

pub fn load_startup(
    metadata: &MapScripts,
    catalog: &[UnitType],
    assets: &VirtualAssets,
) -> Result<Startup, BoxError> {
    startup::load(metadata, catalog, assets)
}

pub fn load(
    assets: &VirtualAssets,
    metadata: &MapScripts,
    records: &[MapObject],
    layout: &MapLayout,
    catalog: &[UnitType],
    map: &generals_sim::skirmish::OpponentMap,
) -> Result<Vec<Script>, BoxError> {
    let definitions = crate::world_data::object_definitions(assets)?;
    let warehouses = warehouses(records, layout, catalog, &definitions)?;
    let mut result = Vec::new();
    for (side, list) in metadata.script_lists.iter().enumerate() {
        let civilian = metadata.sides.get(side).is_some_and(|side| {
            matches!(side.properties.get("playerFaction"), Some(MapValue::Ascii(name)) if name == "FactionCivilian") || matches!(side.properties.get("playerName"), Some(MapValue::Ascii(name)) if name.is_empty())
        });
        for (source, group) in
            list.scripts
                .iter()
                .map(|script| (script, None))
                .chain(list.groups.iter().flat_map(|group| {
                    group
                        .scripts
                        .iter()
                        .map(move |script| (script, Some(group)))
                }))
        {
            if !civilian {
                return Err(format!(
                    "{}: map scripts bound to playable sides are not translated",
                    source.name
                )
                .into());
            }
            let mut script = translate(source, records, layout, catalog, &warehouses, map)?;
            script.owner = match metadata
                .sides
                .get(side)
                .and_then(|side| side.properties.get("playerName"))
            {
                Some(MapValue::Ascii(name)) => name.clone(),
                None => String::new(),
                _ => return Err("Map script side name requires ASCII text".into()),
            };
            script.group = group.map(|group| Group {
                name: group.name.clone(),
                active: group.active,
                subroutine: group.subroutine,
            });
            result.push(script);
        }
    }
    Ok(result)
}

fn translate(
    source: &generals_assets::scripts::Script,
    records: &[MapObject],
    layout: &MapLayout,
    catalog: &[UnitType],
    warehouses: &BTreeMap<String, usize>,
    map: &generals_sim::skirmish::OpponentMap,
) -> Result<Script, BoxError> {
    let conditions = source
        .conditions
        .iter()
        .map(|branch| {
            branch
                .iter()
                .map(|call| {
                    Ok(match operation(call)? {
                        "CONDITION_TRUE" if call.parameters.is_empty() => Condition::True,
                        "CONDITION_FALSE" if call.parameters.is_empty() => Condition::False,
                        "TIMER_EXPIRED" => Condition::TimerExpired(named(call)?.to_owned()),
                        "COUNTER" => Condition::Counter(
                            named(call)?.to_owned(),
                            comparison(call.parameters[1].integer)?,
                            call.parameters[2].integer,
                        ),
                        "SKIRMISH_PLAYER_HAS_UNITS_IN_AREA" => {
                            let area = map
                                .areas
                                .iter()
                                .find(|area| area.name == call.parameters[1].text)
                                .ok_or("Map script trigger area is absent")?;
                            Condition::PlayerUnitsInArea(
                                player_name(call)?.into(),
                                generals_sim::skirmish::AiArea {
                                    name: area.name.clone(),
                                    points: area.points.clone(),
                                },
                            )
                        }
                        "PLAYER_ACQUIRED_SCIENCE" if call.parameters.len() == 2 => {
                            Condition::ScienceAcquired(
                                player_name(call)?.into(),
                                call.parameters[1].text.clone(),
                            )
                        }
                        "NAMED_DESTROYED" if call.parameters.len() == 1 => {
                            let name = named(call)?;
                            validate_named(name, records, layout, catalog)?;
                            Condition::NamedDestroyed(name.to_owned())
                        }
                        operation => {
                            return Err(format!(
                                "{}: unsupported map condition {operation}",
                                source.name
                            )
                            .into());
                        }
                    })
                })
                .collect::<Result<Vec<_>, BoxError>>()
        })
        .collect::<Result<Vec<_>, BoxError>>()?;
    let actions = |calls: &[ScriptCall]| {
        calls
            .iter()
            .map(|call| {
                translate_action(
                    &source.name,
                    call,
                    records,
                    layout,
                    catalog,
                    warehouses,
                    map,
                )
            })
            .collect::<Result<Vec<_>, BoxError>>()
    };
    Ok(Script {
        owner: String::new(),
        name: source.name.clone(),
        active: source.active,
        one_shot: source.one_shot,
        difficulty: source.difficulty,
        delay_frames: delay_frames(source.delay_seconds)?,
        subroutine: source.subroutine,
        group: None,
        conditions,
        actions: actions(&source.actions)?,
        false_actions: actions(&source.false_actions)?,
    })
}

fn delay_frames(seconds: i32) -> Result<u32, BoxError> {
    if seconds <= 0 {
        return Ok(0);
    }
    let frames = seconds
        .checked_mul(30)
        .ok_or("Map script period overflows native frame arithmetic")?;
    Ok(u32::try_from(frames)?)
}

fn comparison(value: i32) -> Result<u8, BoxError> {
    let value = u8::try_from(value)?;
    if value > 5 {
        return Err("Invalid native counter comparison".into());
    }
    Ok(value)
}

fn operation(call: &ScriptCall) -> Result<&str, BoxError> {
    // Native action/condition templates require UNIT=14 and INT=0.
    let (name, kinds): (&str, &[i32]) = match call.id {
        253 => ("PLAYER_REPAIR_NAMED_STRUCTURE", &[11, 14]),
        278 => ("TEAM_HUNT_WITH_COMMAND_BUTTON", &[3, 41]),
        62 => ("PLAYER_DISABLE_BASE_CONSTRUCTION", &[11]),
        64 => ("PLAYER_DISABLE_UNIT_CONSTRUCTION", &[11]),
        65 => ("PLAYER_ENABLE_BASE_CONSTRUCTION", &[11]),
        67 => ("PLAYER_ENABLE_UNIT_CONSTRUCTION", &[11]),
        49 => ("TEAM_ATTACK_AREA", &[3, 9]),
        96 => ("SKIRMISH_PLAYER_HAS_UNITS_IN_AREA", &[11, 9]),
        100 => ("SET_TREE_SWAY", &[17, 17, 17, 0, 1]),
        226 => ("NAMED_SET_TOPPLE_DIRECTION", &[14, 16]),
        224 => ("SET_CAVE_INDEX", &[14, 0]),
        112 => ("TEAM_WANDER", &[3, 24]),
        113 => ("TEAM_PANIC", &[3, 24]),
        46 => ("TEAM_SET_ATTITUDE", &[3, 20]),
        178 => ("TEAM_SET_OVERRIDE_RELATION_TO_TEAM", &[3, 3, 19]),
        3 => ("CONDITION_TRUE", &[]),
        1 => ("COUNTER", &[4, 6, 0]),
        4 => ("TIMER_EXPIRED", &[4]),
        2 => ("SET_COUNTER", &[4, 0]),
        16 => ("DECREMENT_COUNTER", &[0, 4]),
        147 => ("SET_RANDOM_TIMER", &[4, 0, 0]),
        148 => ("SET_RANDOM_MSEC_TIMER", &[4, 1, 1]),
        149 => ("STOP_TIMER", &[4]),
        8 => ("ENABLE_SCRIPT", &[2]),
        9 => ("DISABLE_SCRIPT", &[2]),
        7 => ("PLAY_SOUND_EFFECT", &[12]),
        220 => ("AUDIO_OVERRIDE_VOLUME_TYPE", &[12, 1]),
        121 => ("CAMERA_FADE_ADD", &[1, 1, 0, 0, 0]),
        296 => ("OPTIONS_SET_DRAWICON_UI_MODE", &[8]),
        71 => ("NAMED_DELETE", &[14]),
        0 => ("CONDITION_FALSE", &[]),
        15 => ("NAMED_DESTROYED", &[14]),
        70 => ("NAMED_DAMAGE", &[14, 0]),
        73 => ("NAMED_KILL", &[14]),
        212 => ("WAREHOUSE_SET_VALUE", &[14, 0]),
        _ => {
            return Err(format!(
                "Unsupported map script operation {:?} ({})",
                call.name, call.id
            )
            .into());
        }
    };
    if call.parameters.len() != kinds.len()
        || !call
            .parameters
            .iter()
            .zip(kinds)
            .all(|(parameter, kind)| parameter.kind == *kind)
    {
        return Err(format!("Map script {name} has incompatible native parameter types").into());
    }
    if call
        .name
        .as_deref()
        .is_some_and(|declared| declared != name)
    {
        return Err("Map script operation name and native ID disagree".into());
    }
    Ok(name)
}

fn player_name(call: &ScriptCall) -> Result<&str, BoxError> {
    let name = named(call)?;
    if name == "<This Player's Enemy>" {
        return Err(
            "Map player alias requires the qualified skirmish enemy-player consumer".into(),
        );
    }
    Ok(name)
}

fn named(call: &ScriptCall) -> Result<&str, BoxError> {
    let name = &call
        .parameters
        .first()
        .ok_or("Missing named script parameter")?
        .text;
    if name.is_empty() {
        return Err("Empty named map script parameter".into());
    }
    Ok(name)
}

fn object_name(record: &MapObject) -> Option<&str> {
    match record.properties.get("objectName") {
        Some(MapValue::Ascii(name)) if !name.is_empty() => Some(name),
        _ => None,
    }
}

fn validate_named(
    name: &str,
    records: &[MapObject],
    layout: &MapLayout,
    catalog: &[UnitType],
) -> Result<(), BoxError> {
    if name == "<This Object>" {
        return Ok(());
    }
    if let Some(entry) = layout
        .map_build_lists
        .iter()
        .flat_map(|list| &list.entries)
        .find(|entry| entry.name == name && !entry.template.is_empty())
    {
        let unit = catalog
            .iter()
            .find(|unit| unit.name == entry.template)
            .ok_or("Named build-list template missing")?;
        if unit.fire.is_some() {
            return Err(format!("Named script damage/kill on inactive or immortal build-list body {name} is not translated").into());
        }
        return Ok(());
    }
    if !records
        .iter()
        .any(|record| object_name(record) == Some(name))
    {
        // Native getUnitNamed returns null for a never-existing name.
        return Ok(());
    }
    let object = layout
        .neutral_objects
        .iter()
        .chain(layout.regular_bridges.iter().map(|bridge| &bridge.object))
        .find(|object| object.script_name.as_deref() == Some(name))
        .ok_or_else(|| format!("Map script names unrepresented object {name}"))?;
    let unit = catalog
        .iter()
        .find(|unit| unit.name == object.name)
        .ok_or("Named object template missing")?;
    if unit.fire.is_some() {
        return Err(format!(
            "Named script damage/kill on inactive or immortal body {name} is not translated"
        )
        .into());
    }
    Ok(())
}

fn warehouses(
    records: &[MapObject],
    layout: &MapLayout,
    catalog: &[UnitType],
    definitions: &BTreeMap<String, Declaration>,
) -> Result<BTreeMap<String, usize>, BoxError> {
    let mut index = 0;
    let mut names = BTreeMap::new();
    for record in records {
        if record.waypoint_id().is_some() || record.flags & 0x36 != 0 {
            continue;
        }
        if let Some(unit) = catalog.iter().find(|unit| unit.name == record.template) {
            if unit.supply.warehouse.is_none() {
                continue;
            }
        } else {
            let Some(mut base) = definitions.get(&record.template) else {
                continue;
            };
            let mut parents = Vec::new();
            while let Some(parent) = &base.parent {
                if parents.contains(parent) {
                    return Err("ObjectReskin cycle in warehouse binding".into());
                }
                parents.push(parent.clone());
                base = definitions
                    .get(parent)
                    .ok_or("Missing warehouse ObjectReskin parent")?;
            }
            let supply =
                crate::skirmish_data::field(&base.lines, 0, "KindOf").is_some_and(|tokens| {
                    tokens.tokens[1..]
                        .iter()
                        .any(|token| token == "SUPPLY_SOURCE")
                });
            if !supply {
                continue;
            }
            let dock = crate::skirmish_data::blocks(&base.lines, 0, "Behavior")?
                .into_iter()
                .any(|module| {
                    module[0]
                        .tokens
                        .get(1)
                        .is_some_and(|name| name == "SupplyWarehouseDockUpdate")
                });
            if !dock {
                return Err("Supply source lacks native warehouse module".into());
            }
        }
        let source = layout
            .supplies
            .get(index)
            .ok_or("Warehouse source order exceeds layout")?;
        if [
            layout.neutral_objects[*source].position[0],
            layout.neutral_objects[*source].position[1],
        ]
        .map(f32::to_bits)
            != [record.position[0], record.position[1]].map(f32::to_bits)
        {
            return Err("Warehouse source order differs from layout".into());
        }
        if let Some(name) = object_name(record)
            && names.insert(name.to_owned(), index).is_some()
        {
            return Err(format!("Ambiguous named warehouse {name}").into());
        }
        index += 1;
    }
    if index != layout.supplies.len() {
        return Err("Warehouse layout source count differs".into());
    }
    Ok(names)
}

fn translate_action(
    source_name: &str,
    call: &ScriptCall,
    records: &[MapObject],
    layout: &MapLayout,
    catalog: &[UnitType],
    warehouses: &BTreeMap<String, usize>,
    map: &generals_sim::skirmish::OpponentMap,
) -> Result<Action, BoxError> {
    Ok(match operation(call)? {
        "PLAYER_REPAIR_NAMED_STRUCTURE" => {
            Action::PlayerRepair(player_name(call)?.into(), call.parameters[1].text.clone())
        }
        "TEAM_HUNT_WITH_COMMAND_BUTTON" => {
            Action::TeamHuntCommand(named(call)?.into(), call.parameters[1].text.clone())
        }
        "PLAYER_DISABLE_BASE_CONSTRUCTION"
        | "PLAYER_DISABLE_UNIT_CONSTRUCTION"
        | "PLAYER_ENABLE_BASE_CONSTRUCTION"
        | "PLAYER_ENABLE_UNIT_CONSTRUCTION"
        | "NAMED_SET_TOPPLE_DIRECTION"
        | "SET_TREE_SWAY"
        | "TEAM_ATTACK_AREA"
        | "SET_CAVE_INDEX"
        | "TEAM_WANDER"
        | "TEAM_PANIC"
        | "TEAM_SET_ATTITUDE"
        | "TEAM_SET_OVERRIDE_RELATION_TO_TEAM" => world_action(call, map)?,
        "SET_COUNTER" => Action::SetCounter(named(call)?.to_owned(), call.parameters[1].integer),
        "DECREMENT_COUNTER" => {
            Action::DecrementCounter(call.parameters[1].text.clone(), call.parameters[0].integer)
        }
        "SET_RANDOM_TIMER" => Action::RandomTimer(
            named(call)?.to_owned(),
            call.parameters[1].integer,
            call.parameters[2].integer,
        ),
        "SET_RANDOM_MSEC_TIMER" => {
            let minimum = call.parameters[1].real;
            let maximum = call.parameters[2].real;
            if [minimum, maximum].iter().any(|value| {
                !value.is_finite()
                    || ((value * 1000.0) * (30.0 / 1000.0))
                        .ceil()
                        .to_i32()
                        .is_none()
            }) {
                return Err("Map timer exceeds native frame integer".into());
            }
            Action::RandomSecondsTimer(named(call)?.to_owned(), minimum, maximum)
        }
        "STOP_TIMER" => Action::StopTimer(named(call)?.to_owned()),
        "ENABLE_SCRIPT" | "DISABLE_SCRIPT" => {
            Action::EnableScript(named(call)?.to_owned(), call.id == 8)
        }
        "PLAY_SOUND_EFFECT" => Action::Sound(named(call)?.to_owned()),
        "AUDIO_OVERRIDE_VOLUME_TYPE" => {
            Action::AudioVolume(named(call)?.to_owned(), call.parameters[1].real / 100.0)
        }
        "OPTIONS_SET_DRAWICON_UI_MODE" => Action::DrawIconUi(call.parameters[0].integer != 0),
        "CAMERA_FADE_ADD" => {
            let frames = [
                call.parameters[2].integer,
                call.parameters[3].integer,
                call.parameters[4].integer,
            ]
            .map(u32::try_from);
            let [increase, hold, decrease] = frames;
            let frames = [increase?, hold?, decrease?];
            if frames
                .iter()
                .try_fold(0_u32, |total, frame| total.checked_add(*frame))
                .is_none_or(|total| total >= 2_147_483_647)
            {
                return Err("Map fade exceeds native frame integer".into());
            }
            Action::FadeAdd(call.parameters[0].real, call.parameters[1].real, frames)
        }
        "NAMED_DELETE" => {
            let name = named(call)?;
            validate_named(name, records, layout, catalog)?;
            Action::NamedDelete(name.to_owned())
        }
        "NAMED_KILL" if call.parameters.len() == 1 => {
            let name = named(call)?;
            validate_named(name, records, layout, catalog)?;
            Action::NamedKill(name.to_owned())
        }
        "NAMED_DAMAGE" if call.parameters.len() == 2 => {
            let name = named(call)?;
            validate_named(name, records, layout, catalog)?;
            Action::NamedDamage(name.to_owned(), u32::try_from(call.parameters[1].integer)?)
        }
        "WAREHOUSE_SET_VALUE" if call.parameters.len() == 2 => {
            let name = named(call)?;
            let index = *warehouses
                .get(name)
                .ok_or_else(|| format!("Unrepresented named SupplyWarehouseDockUpdate {name}"))?;
            Action::WarehouseValue(index, u32::try_from(call.parameters[1].integer)?)
        }
        operation => {
            return Err(format!("{source_name}: unsupported map action {operation}").into());
        }
    })
}

fn world_action(
    call: &ScriptCall,
    map: &generals_sim::skirmish::OpponentMap,
) -> Result<Action, BoxError> {
    Ok(match operation(call)? {
        "PLAYER_DISABLE_BASE_CONSTRUCTION" => {
            Action::PlayerBuild(player_name(call)?.into(), false, false)
        }
        "PLAYER_DISABLE_UNIT_CONSTRUCTION" => {
            Action::PlayerBuild(player_name(call)?.into(), true, false)
        }
        "PLAYER_ENABLE_BASE_CONSTRUCTION" => {
            Action::PlayerBuild(player_name(call)?.into(), false, true)
        }
        "PLAYER_ENABLE_UNIT_CONSTRUCTION" => {
            Action::PlayerBuild(player_name(call)?.into(), true, true)
        }
        "NAMED_SET_TOPPLE_DIRECTION" => Action::ToppleDirection(
            named(call)?.to_owned(),
            call.parameters[1]
                .position
                .ok_or("Topple direction requires COORD3D")?,
        ),
        "SET_TREE_SWAY" => Action::TreeSway(
            [
                call.parameters[0].real,
                call.parameters[1].real,
                call.parameters[2].real,
            ],
            call.parameters[3].integer,
            call.parameters[4].real,
        ),
        "TEAM_ATTACK_AREA" => {
            let area = map
                .areas
                .iter()
                .find(|area| area.name == call.parameters[1].text)
                .ok_or("Map script attack area is absent")?;
            Action::TeamAttackArea(named(call)?.into(), area.points.clone())
        }
        "SET_CAVE_INDEX" => Action::SetCaveIndex(
            named(call)?.into(),
            usize::try_from(call.parameters[1].integer)?,
        ),
        operation @ ("TEAM_WANDER" | "TEAM_PANIC") => {
            let label = &call.parameters[1].text;
            if !map.waypoints.iter().any(|waypoint| {
                waypoint
                    .labels
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(label))
            }) {
                return Err(format!("Map wander path {label} has no original waypoints").into());
            }
            let graph = map
                .waypoints
                .iter()
                .map(|waypoint| generals_sim::skirmish::AiWaypoint {
                    id: waypoint.id,
                    position: waypoint.position,
                    labels: waypoint.labels.clone(),
                    links: waypoint.links.clone(),
                    bidirectional: waypoint.bidirectional,
                })
                .collect();
            if operation == "TEAM_PANIC" {
                Action::TeamPanic(named(call)?.into(), label.clone(), graph)
            } else {
                Action::TeamWander(named(call)?.into(), label.clone(), graph)
            }
        }
        "TEAM_SET_ATTITUDE" => {
            let attitude = call.parameters[1].integer;
            if !(-2..=2).contains(&attitude) {
                return Err("Invalid native AI attitude".into());
            }
            Action::TeamAttitude(named(call)?.into(), attitude)
        }
        "TEAM_SET_OVERRIDE_RELATION_TO_TEAM" => {
            let relation = u8::try_from(call.parameters[2].integer)?;
            if relation > 2 {
                return Err("Invalid native team relationship".into());
            }
            Action::TeamRelation(
                named(call)?.into(),
                call.parameters[1].text.clone(),
                relation,
            )
        }
        operation => return Err(format!("Unsupported map world action {operation}").into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{map::MapData, scripts::map_scripts};

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_tournament_desert_supply_callbacks_apply_once_and_after_restart() {
        let (assets, _) = crate::cleanup_verification::tests::installed();
        let bytes = assets
            .read("maps/tournament desert/tournament desert.map")
            .unwrap()
            .bytes;
        let mut metadata = map_scripts(&bytes).unwrap();
        let map = MapData::parse(&bytes).unwrap();
        let root = &generals_assets::game_root().unwrap();
        let settings = crate::world_data::camera_settings(&assets, root).unwrap();
        let graph = crate::opponent_data::map_data(&map).unwrap();
        let terrain = crate::terrain::Terrain::new(
            map.heightmap,
            map.tiles,
            settings.adjust_cliffs,
            settings.three_way_blends,
        )
        .unwrap();
        let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
        let (layout, _) = crate::world_data::load(
            &assets,
            &map.objects,
            &terrain,
            &data.types,
            &mut data.models,
        )
        .unwrap();
        let scripts = load(
            &assets,
            &metadata,
            &map.objects,
            &layout,
            &data.types,
            &graph,
        )
        .unwrap();
        metadata.script_lists[1].groups[0].scripts[0].delay_seconds = 1;
        let delayed = load(
            &assets,
            &metadata,
            &map.objects,
            &layout,
            &data.types,
            &graph,
        )
        .unwrap();
        assert!(delayed.iter().any(|script| script.delay_frames == 30));
        metadata.script_lists[1].groups[0].scripts[0].active = false;
        metadata.script_lists[1].groups[0].scripts[0].subroutine = true;
        let dormant = load(
            &assets,
            &metadata,
            &map.objects,
            &layout,
            &data.types,
            &graph,
        )
        .unwrap();
        assert!(
            dormant
                .iter()
                .any(|script| script.delay_frames == 30 && !script.active)
        );
        assert_eq!(scripts.iter().filter(|script| script.active).count(), 4);
        let expected: Vec<_> = scripts
            .iter()
            .flat_map(|script| &script.actions)
            .map(|action| {
                let Action::WarehouseValue(index, cash) = action else {
                    panic!("installed supply action differs");
                };
                (*index, cash.div_ceil(data.economy.supply_value))
            })
            .collect();
        assert_eq!(expected.len(), 4);
        let movement = terrain
            .classify_movement(&map.triggers, &map.objects)
            .unwrap();
        let mut game = data.into_simulation(movement, layout).unwrap();
        game.install_map_scripts(scripts);
        game.step();
        for &(index, boxes) in &expected {
            assert_eq!(game.supplies[index].boxes, boxes);
        }
        game.supplies[expected[0].0].boxes -= 1;
        game.step();
        assert_eq!(game.supplies[expected[0].0].boxes, expected[0].1 - 1);
        game.reset(
            generals_sim::skirmish::Faction::Usa,
            generals_sim::skirmish::Faction::Gla,
        )
        .unwrap();
        game.step();
        for &(index, boxes) in &expected {
            assert_eq!(game.supplies[index].boxes, boxes);
        }
    }

    #[test]
    fn admitted_player_aliases_preserve_dynamic_context_and_reject_unrepresented_enemy_selection() {
        let mut call = ScriptCall {
            id: 0,
            name: None,
            parameters: vec![generals_assets::scripts::ScriptParameter {
                kind: 3,
                text: "<This Player>".into(),
                integer: 0,
                real: 0.0,
                position: None,
            }],
        };
        for name in ["<This Player>", "<Local Player>", "player0"] {
            call.parameters[0].text = name.into();
            assert_eq!(player_name(&call).unwrap(), name);
        }
        call.parameters[0].text = "<This Player's Enemy>".into();
        assert!(
            player_name(&call)
                .unwrap_err()
                .to_string()
                .contains("enemy-player consumer")
        );
    }

    #[test]
    fn native_icon_ui_opcode_requires_boolean_parameter() {
        let mut call = ScriptCall {
            id: 296,
            name: Some("OPTIONS_SET_DRAWICON_UI_MODE".into()),
            parameters: vec![generals_assets::scripts::ScriptParameter {
                kind: 8,
                text: String::new(),
                integer: 0,
                real: 0.0,
                position: None,
            }],
        };
        assert_eq!(operation(&call).unwrap(), "OPTIONS_SET_DRAWICON_UI_MODE");
        call.parameters[0].kind = 0;
        assert!(operation(&call).is_err());
    }

    #[test]
    fn script_opcode_name_disagreement_and_unknown_actions_fail_admission() {
        let parameter = |kind, text: &str| generals_assets::scripts::ScriptParameter {
            kind,
            text: text.to_owned(),
            integer: 20000,
            real: 0.0,
            position: None,
        };
        let mut call = ScriptCall {
            id: 212,
            name: Some("WAREHOUSE_SET_VALUE".into()),
            parameters: vec![parameter(14, "SupplyDockNorth"), parameter(0, "")],
        };
        assert_eq!(operation(&call).unwrap(), "WAREHOUSE_SET_VALUE");
        call.name = Some("NAMED_KILL".into());
        assert!(operation(&call).is_err());
        call.id = 999;
        assert!(operation(&call).is_err());
        call.name = Some("WAREHOUSE_SET_VALUE".into());
        call.id = 212;
        call.parameters[0].kind = 3;
        assert!(operation(&call).is_err());
        call.parameters[0].kind = 14;
        call.parameters[1].kind = 1;
        assert!(operation(&call).is_err());
        call.id = 0;
        call.parameters.clear();
        call.name = Some("CONDITION_FALSE".into());
        assert_eq!(operation(&call).unwrap(), "CONDITION_FALSE");
    }
}
