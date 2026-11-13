//! Original `AIData`, SCB scripts/teams and map approach routes for the three base sides.
mod defenses;
use std::{collections::BTreeMap, error::Error};

use generals_assets::{
    content::{self, ContentLine},
    map::{MapData, MapValue},
    scripts::{Script, ScriptBundle, ScriptCall},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{
    AiArea, AiBuilding, AiCall, AiParameter, AiScript, AiScriptGroup, AiTeam, AiTeamUnit,
    AiWaypoint, Faction, OpponentFaction, OpponentMap, OpponentRules,
};

use crate::skirmish_data::{blocks, field, number, scalar, text, unsigned};

pub fn load(assets: &VirtualAssets, map: &MapData) -> Result<OpponentRules, Box<dyn Error>> {
    let mut lines = Vec::new();
    for path in ["data/ini/default/aidata.ini", "data/ini/aidata.ini"] {
        let file = content::scan(assets.read(path)?)?;
        if file.declarations.is_empty()
            && file.unclassified.is_empty()
            && file.diagnostics.is_empty()
        {
            continue;
        }
        let mut data = file
            .declarations
            .into_iter()
            .filter(|data| data.kind == "AIData");
        let declaration = data.next().ok_or("Missing AIData declaration")?;
        if data.next().is_some() {
            return Err("Duplicate AIData declaration".into());
        }
        lines.extend(declaration.lines);
    }
    if field(&lines, 0, "RotateSkirmishBases")
        .is_some_and(|line| line.tokens.get(1).is_some_and(|value| value != "No"))
    {
        return Err("Rotating AI base-layout regions are not translated".into());
    }
    let mut bundle = ScriptBundle::parse(&assets.read("data/scripts/skirmishscripts.scb")?.bytes)?;
    let defense_templates = defenses::templates(assets)?;
    let mut factions = Vec::new();
    for (side, player, faction) in [
        ("America", "SkirmishAmerica", Faction::Usa),
        ("China", "SkirmishChina", Faction::China),
        ("GLA", "SkirmishGLA", Faction::Gla),
    ] {
        let index = bundle
            .players
            .iter()
            .position(|entry| entry.name == player)
            .ok_or("Missing SCB player")?;
        let list = &mut bundle.lists[index];
        let mut scripts = std::mem::take(&mut list.scripts)
            .into_iter()
            .map(|script| convert_script(script, None))
            .collect::<Result<Vec<_>, _>>()?;
        let mut groups = Vec::new();
        for (index, group) in std::mem::take(&mut list.groups).into_iter().enumerate() {
            groups.push(AiScriptGroup {
                name: group.name,
                active: group.active,
                subroutine: group.subroutine,
            });
            for script in group.scripts {
                scripts.push(convert_script(script, Some(index))?);
            }
        }
        let buildings = defenses::buildings(&lines, side, &defense_templates)?;
        let teams = bundle
            .teams
            .iter()
            .filter(|team| dict_text(team, "teamOwner") == player)
            .map(|dict| team(dict, map))
            .collect::<Result<Vec<_>, _>>()?;

        println!(
            "Original {player}: {} scripts, {} teams, {} building entries",
            scripts.len(),
            teams.len(),
            buildings.len()
        );
        factions.push(OpponentFaction {
            default_team: format!("team{player}"),
            defense_template: defenses::template(&lines, side)?,
            faction,
            buildings,
            gatherers: gatherers(&lines, side)?,
            scripts,
            groups,
            teams,
        });
    }
    Ok(OpponentRules {
        defense_extra_distance: number(&lines, 0, "SkirmishBaseDefenseExtraDistance")?,
        max_recruit_distance: number(&lines, 0, "MaxRecruitRadius")?,
        skirmish_group_fudge: number(&lines, 0, "SkirmishGroupFudgeDistance")?,
        attack_priority_distance: number(&lines, 0, "AttackPriorityDistanceModifier")?,
        alert_range_modifier: number(&lines, 0, "AlertRangeModifier")?,
        aggressive_range_modifier: number(&lines, 0, "AggressiveRangeModifier")?,
        build_facilities: build_facilities(assets)?,
        factions,
        structure_seconds: number(&lines, 0, "StructureSeconds")?,
        team_seconds: number(&lines, 0, "TeamSeconds")?,
        wealthy: unsigned(&lines, 0, "Wealthy")?,
        poor: unsigned(&lines, 0, "Poor")?,
        structures_wealthy_rate: number(&lines, 0, "StructuresWealthyRate")?,
        structures_poor_rate: number(&lines, 0, "StructuresPoorRate")?,
        teams_wealthy_rate: number(&lines, 0, "TeamsWealthyRate")?,
        teams_poor_rate: number(&lines, 0, "TeamsPoorRate")?,
        team_resources_to_start: number(&lines, 0, "TeamResourcesToStart")?,
        rebuild_delay_seconds: unsigned(&lines, 0, "RebuildDelayTimeSeconds")?,
        supply_center_safe_radius: number(&lines, 0, "SupplyCenterSafeRadius")?,
        map: map_data(map)?,
    })
}

fn gatherers(lines: &[ContentLine], side: &str) -> Result<[u32; 3], Box<dyn Error>> {
    // AISideInfo constructor and AI::parseSideInfo merge repeated side blocks.
    let mut result = [0, 1, 2];
    for block in blocks(lines, 0, "SideInfo")? {
        if block[0].tokens.get(1).is_none_or(|name| name != side) {
            continue;
        }
        for (index, name) in [
            "ResourceGatherersEasy",
            "ResourceGatherersNormal",
            "ResourceGatherersHard",
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(line) = field(block, 1, name) {
                result[index] = scalar(line)?.parse()?;
            }
        }
    }
    Ok(result)
}

fn named_block<'a>(
    lines: &'a [ContentLine],
    key: &str,
    name: &str,
) -> Result<&'a [ContentLine], Box<dyn Error>> {
    blocks(lines, 0, key)?
        .into_iter()
        .rev()
        .find(|block| block[0].tokens.get(1).is_some_and(|value| value == name))
        .ok_or_else(|| format!("Missing {key} {name}").into())
}

fn building(lines: &[ContentLine]) -> Result<AiBuilding, Box<dyn Error>> {
    if text(lines, 2, "InitiallyBuilt")? != "No" {
        return Err("AI initially-built structures are not translated".into());
    }
    let location = field(lines, 2, "Location").ok_or("Missing AI building location")?;
    let mut position = [0.0; 2];
    for (index, key) in ["X:", "Y:"].iter().enumerate() {
        position[index] = location
            .tokens
            .iter()
            .find_map(|token| token.strip_prefix(key))
            .ok_or("Missing AI location coordinate")?
            .parse::<f32>()?;
        if !position[index].is_finite() {
            return Err("Nonfinite AI location".into());
        }
    }
    let angle = text(lines, 2, "Angle")?.parse::<f32>()?.to_radians();
    if !angle.is_finite() {
        return Err("Nonfinite AI angle".into());
    }
    Ok(AiBuilding {
        bounding_radius: 0.0,
        name: scalar(&lines[0])?.to_owned(),
        position,
        angle,
        rebuilds: text(lines, 2, "Rebuilds")?.parse()?,
        automatic: match text(lines, 2, "AutomaticallyBuild")? {
            "Yes" => true,
            "No" => false,
            _ => return Err("Invalid AI automatic-build flag".into()),
        },
    })
}

fn convert_script(script: Script, group: Option<usize>) -> Result<AiScript, Box<dyn Error>> {
    let flags = [
        script.active,
        script.one_shot,
        script.difficulty[0],
        script.difficulty[1],
        script.difficulty[2],
        script.subroutine,
    ]
    .into_iter()
    .enumerate()
    .fold(0, |flags, (bit, on)| flags | (u8::from(on) << bit));
    Ok(AiScript {
        name: script.name,
        flags,
        group,
        delay_frames: u32::try_from(script.delay_seconds)?
            .checked_mul(30)
            .ok_or("Script delay overflow")?,
        conditions: script
            .conditions
            .into_iter()
            .map(calls)
            .collect::<Result<Vec<_>, _>>()?,
        actions: calls(script.actions)?,
        false_actions: calls(script.false_actions)?,
    })
}

fn calls(calls: Vec<ScriptCall>) -> Result<Vec<AiCall>, Box<dyn Error>> {
    calls
        .into_iter()
        .map(|call| {
            let name = call
                .name
                .unwrap_or_else(|| format!("UNMAPPED_ID_{}", call.id));
            let parameters: Vec<_> = call
                .parameters
                .into_iter()
                .map(|value| AiParameter {
                    integer: value.integer,
                    real: value.real,
                    text: value.text,
                    position: value.position,
                })
                .collect();
            if matches!(
                name.as_str(),
                "SET_MILLISECOND_TIMER" | "SET_RANDOM_MSEC_TIMER"
            ) && parameters
                .iter()
                .skip(1)
                .any(|value| !value.real.is_finite())
            {
                return Err("Invalid script timer".into());
            }
            Ok(AiCall { name, parameters })
        })
        .collect()
}

fn team(dict: &BTreeMap<String, MapValue>, map: &MapData) -> Result<AiTeam, Box<dyn Error>> {
    let mut units = Vec::new();
    for index in 1..=7 {
        let name = dict_text(dict, &format!("teamUnitType{index}"));
        let maximum = dict_int(dict, &format!("teamUnitMaxCount{index}"));
        if !name.is_empty() && maximum > 0 {
            units.push(AiTeamUnit {
                name: name.to_owned(),
                minimum: u32::try_from(dict_int(dict, &format!("teamUnitMinCount{index}")))?,
                maximum: u32::try_from(maximum)?,
            });
        }
    }
    let destroyed_threshold = match dict.get("teamDestroyedThreshold") {
        Some(MapValue::Real(value)) => *value,
        _ => 0.0,
    };
    Ok(AiTeam {
        recruitment: u8::from(dict_int(dict, "teamIsAIRecruitable") != 0)
            | (u8::from(dict_int(dict, "teamAutoReinforce") != 0) << 1),
        home: map
            .objects
            .iter()
            .rev()
            .find(|object| {
                object.waypoint_id().is_some()
                    && dict_text(&object.properties, "waypointName") == dict_text(dict, "teamHome")
                    && !dict_text(dict, "teamHome").is_empty()
            })
            .map(|object| [object.position[0], object.position[1]]),
        singleton: dict_int(dict, "teamIsSingleton") != 0,
        attack_common_target: dict_int(dict, "teamAttackCommonTarget") != 0,
        name: dict_text(dict, "teamName").to_owned(),
        units,
        priority: dict_int(dict, "teamProductionPriority"),
        max_instances: u32::try_from(dict_int(dict, "teamMaxInstances"))?,
        condition: dict_text(dict, "teamProductionCondition").to_owned(),
        on_create: dict_text(dict, "teamOnCreateScript").to_owned(),
        on_idle: dict_text(dict, "teamOnIdleScript").to_owned(),
        on_destroyed: dict_text(dict, "teamOnDestroyedScript").to_owned(),
        generic_scripts: (0..16)
            .map(|index| dict_text(dict, &format!("teamGenericScriptHook{index}")))
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect(),
        initial_idle_frames: u32::try_from(dict_int(dict, "teamInitialIdleFrames"))?,
        priority_success: dict_int(dict, "teamProductionPrioritySuccessIncrease"),
        priority_failure: dict_int(dict, "teamProductionPriorityFailureDecrease"),
        execute_actions: dict_int(dict, "teamExecutesActionsOnCreate") != 0,
        destroyed_threshold,
        aggression: dict_int(dict, "teamAggressiveness"),
    })
}

fn dict_text<'a>(dict: &'a BTreeMap<String, MapValue>, key: &str) -> &'a str {
    match dict.get(key) {
        Some(MapValue::Ascii(value)) => value,
        _ => "",
    }
}
fn dict_int(dict: &BTreeMap<String, MapValue>, key: &str) -> i32 {
    match dict.get(key) {
        Some(MapValue::Int(value)) => *value,
        Some(MapValue::Bool(value)) => i32::from(*value),
        _ => 0,
    }
}

pub fn map_data(map: &MapData) -> Result<OpponentMap, Box<dyn Error>> {
    let mut areas = Vec::new();
    for trigger in &map.triggers {
        let mut points = Vec::new();
        for point in &trigger.points {
            points.push([point[0].to_string().parse()?, point[1].to_string().parse()?]);
        }
        areas.push(AiArea {
            name: trigger.name.clone(),
            points,
        });
    }
    let mut waypoints = Vec::new();
    for object in map.objects.iter().rev() {
        let Some(id) = object.waypoint_id() else {
            continue;
        };
        waypoints.push(AiWaypoint {
            id,
            position: [object.position[0], object.position[1]],
            labels: (1..=3)
                .map(|index| dict_text(&object.properties, &format!("waypointPathLabel{index}")))
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .collect(),
            links: Vec::new(),
            bidirectional: dict_int(&object.properties, "waypointPathBiDirectional") != 0,
        });
    }
    for [from, to] in &map.waypoint_links {
        let Some(first) = waypoints.iter().position(|waypoint| waypoint.id == *from) else {
            continue;
        };
        let Some(second) = waypoints.iter().position(|waypoint| waypoint.id == *to) else {
            continue;
        };
        if first == second || waypoints[first].links.contains(to) {
            continue;
        }
        waypoints[first].links.push(*to);
        if waypoints[first].bidirectional && !waypoints[second].links.contains(from) {
            waypoints[second].links.push(*from);
        }
    }
    Ok(OpponentMap { areas, waypoints })
}

fn build_facilities(assets: &VirtualAssets) -> Result<Vec<String>, Box<dyn Error>> {
    let objects = crate::world_data::object_definitions(assets)?;
    let mut names = std::collections::BTreeSet::new();
    for object in objects.values() {
        if crate::skirmish_data::has_flag(&object.lines, "KindOf", "COMMANDCENTER") {
            names.insert(object.name.clone());
        }
        // ProductionPrerequisite::getAllPossibleBuildFacilityTemplates takes
        // only the first Object prerequisite and its OR-with-previous entries.
        for prerequisite in blocks(&object.lines, 0, "Prerequisites")? {
            if let Some(line) = prerequisite.iter().find(|line| {
                line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "Object")
            }) {
                for name in line.tokens.iter().skip(1).take(32) {
                    if objects.contains_key(name) {
                        names.insert(name.clone());
                    }
                }
            }
        }
    }
    Ok(names.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::gatherers;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn side_gatherer_overrides_merge_fields_and_keep_constructor_defaults() {
        let file = content::scan(Asset {
            path: "aidata.ini".into(), origin: "fixture".into(),
            bytes: b"AIData\nSideInfo America\nResourceGatherersEasy=3\nResourceGatherersHard=4\nEnd\nSideInfo China\nResourceGatherersNormal=5\nEnd\nSideInfo America\nResourceGatherersEasy=7\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let lines = &file.declarations[0].lines;
        assert_eq!(gatherers(lines, "America").unwrap(), [7, 1, 4]);
        assert_eq!(gatherers(lines, "China").unwrap(), [0, 5, 2]);
    }
}
