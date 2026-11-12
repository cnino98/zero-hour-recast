//! Map sides, `TeamPrototype` declarations and native `BuildListInfo` defaults.
use super::{BoxError, Startup};
use generals_assets::{content, map::MapValue, scripts::MapScripts, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{
    UnitType,
    map_scripts::{BuildEntry, BuildList, SideDefinition, TeamDefinition},
};
use std::collections::{BTreeMap, BTreeSet};

mod gatherers;
mod order;

type Properties = BTreeMap<String, MapValue>;

fn text<'a>(properties: &'a Properties, key: &str, default: &'a str) -> Result<&'a str, BoxError> {
    match properties.get(key) {
        None => Ok(default),
        Some(MapValue::Ascii(value)) => Ok(value),
        _ => Err(format!("Map property {key} requires ASCII text").into()),
    }
}

fn integer(properties: &Properties, key: &str, default: i32) -> Result<i32, BoxError> {
    match properties.get(key) {
        None => Ok(default),
        Some(MapValue::Int(value)) => Ok(*value),
        _ => Err(format!("Map property {key} requires an integer").into()),
    }
}

fn boolean(properties: &Properties, key: &str, default: bool) -> Result<bool, BoxError> {
    match properties.get(key) {
        None => Ok(default),
        Some(MapValue::Bool(value)) => Ok(*value),
        _ => Err(format!("Map property {key} requires a boolean").into()),
    }
}

pub(super) fn load(
    metadata: &MapScripts,
    catalog: &[UnitType],
    assets: &VirtualAssets,
) -> Result<Startup, BoxError> {
    Ok(Startup {
        template_order: order::load(assets)?,
        sides: sides(metadata, assets)?,
        teams: teams(metadata)?,
        build_lists: build_lists(metadata, catalog)?,
    })
}

fn sides(metadata: &MapScripts, assets: &VirtualAssets) -> Result<Vec<SideDefinition>, BoxError> {
    let templates = content::scan(assets.read("data/ini/playertemplate.ini")?)?;
    let civilian = templates
        .declarations
        .iter()
        .find(|declaration| {
            declaration.kind == "PlayerTemplate" && declaration.name == "FactionCivilian"
        })
        .ok_or("Original FactionCivilian player template missing")?;
    let template_money =
        crate::skirmish_data::field(&civilian.lines, 0, "StartMoney").map_or(Ok(0), |line| {
            let signed = crate::skirmish_data::scalar(line)?.parse::<i32>()?;
            Ok::<_, BoxError>(u32::from_ne_bytes(signed.to_ne_bytes()))
        })?;
    let gatherers = gatherers::load(assets)?;
    let mut names = BTreeSet::new();
    let mut definitions = Vec::new();
    for side in &metadata.sides {
        let name = text(&side.properties, "playerName", "")?;
        if !names.insert(name) {
            return Err("Duplicate map side name".into());
        }
        definitions.push(SideDefinition {
            gatherers,
            name: name.into(),
            faction: text(&side.properties, "playerFaction", "")?.into(),
            human: boolean(&side.properties, "playerIsHuman", false)?,
            template_money,
            start_money: u32::from_ne_bytes(
                integer(&side.properties, "playerStartMoney", 0)?.to_ne_bytes(),
            ),
            allies: text(&side.properties, "playerAllies", "")?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            enemies: text(&side.properties, "playerEnemies", "")?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        });
    }
    Ok(definitions)
}

fn teams(metadata: &MapScripts) -> Result<Vec<TeamDefinition>, BoxError> {
    let mut names = BTreeSet::new();
    let mut teams = Vec::new();
    for source in &metadata.teams {
        let name = text(source, "teamName", "")?;
        if name.is_empty() || !names.insert(name) {
            return Err("Empty or duplicate authored team name".into());
        }
        let owner = text(source, "teamOwner", "")?;
        let side = metadata.sides.iter().find(|side| matches!(side.properties.get("playerName"), Some(MapValue::Ascii(name)) if name == owner))
            .ok_or("Authored team references an absent side")?;
        // Production/recruitment belongs to AI team templates; these map teams
        // are instantiated from their placed objects by GameLogic::loadMap.
        for index in 1..=7 {
            for prefix in ["teamUnitMinCount", "teamUnitMaxCount"] {
                if integer(source, &format!("{prefix}{index}"), 0)? != 0 {
                    return Err(format!(
                        "{name}: authored team production requires its AI template consumer"
                    )
                    .into());
                }
            }
        }
        if boolean(source, "teamExecutesActionsOnCreate", false)? {
            return Err(format!(
                "{name}: authored team creation actions require an attached script consumer"
            )
            .into());
        }
        teams.push(TeamDefinition {
            name: name.into(),
            owner: owner.into(),
            singleton: boolean(source, "teamIsSingleton", false)?,
            attack_common_target: boolean(source, "teamAttackCommonTarget", false)?,
            initial_idle_frames: u32::try_from(integer(source, "teamInitialIdleFrames", 0)?)?,
            owner_faction: text(&side.properties, "playerFaction", "")?.into(),
            owner_allies: text(&side.properties, "playerAllies", "")?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            owner_enemies: text(&side.properties, "playerEnemies", "")?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            owner_human: boolean(&side.properties, "playerIsHuman", false)?,
        });
    }
    // Player::setDefaultTeam creates the side's singleton even when the editor
    // did not serialize its TeamPrototype (Civ_Gat in MountainGuns).
    for side in &metadata.sides {
        let owner = text(&side.properties, "playerName", "")?;
        let name = format!("team{owner}");
        if names.contains(name.as_str()) {
            continue;
        }
        teams.push(TeamDefinition {
            name,
            owner: owner.into(),
            singleton: true,
            attack_common_target: false,
            initial_idle_frames: 0,
            owner_faction: text(&side.properties, "playerFaction", "")?.into(),
            owner_allies: text(&side.properties, "playerAllies", "")?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            owner_enemies: text(&side.properties, "playerEnemies", "")?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            owner_human: boolean(&side.properties, "playerIsHuman", false)?,
        });
    }
    Ok(teams)
}

fn build_lists(metadata: &MapScripts, catalog: &[UnitType]) -> Result<Vec<BuildList>, BoxError> {
    let mut lists = Vec::new();
    for side in &metadata.sides {
        if !side.build_list.is_empty()
            && text(&side.properties, "playerFaction", "")? != "FactionCivilian"
        {
            return Err(
                "Noncivilian authored build lists require their qualified skirmish AI consumer"
                    .into(),
            );
        }
        if side.build_list.is_empty()
            && (text(&side.properties, "playerName", "")?.is_empty()
                || text(&side.properties, "playerFaction", "")? != "FactionCivilian"
                || boolean(&side.properties, "playerIsHuman", false)?)
        {
            continue;
        }
        let mut entries = Vec::new();
        for entry in &side.build_list {
            if !entry
                .position
                .into_iter()
                .chain([entry.angle])
                .all(f32::is_finite)
            {
                return Err("Authored build list has nonfinite placement".into());
            }
            let unit = if entry.template_name.is_empty() {
                None
            } else {
                Some(
                    catalog
                        .iter()
                        .find(|unit| unit.name == entry.template_name)
                        .ok_or_else(|| {
                            format!(
                                "Authored build template {} has no gameplay consumer",
                                entry.template_name
                            )
                        })?,
                )
            };
            if unit.is_some_and(|unit| !unit.kind_of.iter().any(|kind| kind == "STRUCTURE")) {
                return Err("Authored build lists require structure templates".into());
            }
            if entry.initially_built > 1
                || [entry.whiner, entry.unsellable, entry.repairable]
                    .into_iter()
                    .flatten()
                    .any(|value| value > 1)
            {
                return Err("Authored build list boolean is malformed".into());
            }
            entries.push(BuildEntry {
                name: entry.building_name.clone(),
                template: entry.template_name.clone(),
                // SidesList::parseSidesList zeroes serialized Z.
                position: [entry.position[0], entry.position[1]],
                angle: entry.angle,
                initially_built: entry.initially_built != 0,
                rebuilds: entry.rebuilds,
                script: entry.script.clone().unwrap_or_default(),
                health: entry.health.unwrap_or(100),
                properties: (u8::from(entry.whiner.is_none_or(|value| value != 0))
                    * BuildEntry::WHINER)
                    | (u8::from(entry.unsellable.is_some_and(|value| value != 0))
                        * BuildEntry::UNSELLABLE)
                    | (u8::from(entry.repairable.is_none_or(|value| value != 0))
                        * BuildEntry::REPAIRABLE),
            });
        }
        lists.push(BuildList {
            owner: text(&side.properties, "playerName", "")?.into(),
            entries,
        });
    }
    Ok(lists)
}
