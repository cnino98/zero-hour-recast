//! Original SCB script lists, groups, conditions, actions and team dictionaries.
//!
//! `SuperHackers` 2bfaea60: `Scripts.cpp` chunk readers and `SidesList.cpp`.
//! Unknown operation names and parameter types are retained for the interpreter
//! to reject explicitly; parsing a script does not claim its behavior is ported.

use std::collections::BTreeMap;

mod map;
pub use map::{MapBuildEntry, MapScripts, MapSide, map_scripts};

#[cfg(test)]
mod tests;

use crate::{
    AssetError,
    binary::Reader,
    heightmap::read_symbols,
    map::{Chunk, MapValue, ascii, finish, integer, read_chunk, read_dict, real},
};

pub struct ScriptBundle {
    pub players: Vec<ScriptPlayer>,
    pub teams: Vec<BTreeMap<String, MapValue>>,
    pub lists: Vec<ScriptList>,
    pub unparsed_chunks: Vec<String>,
}

pub struct ScriptPlayer {
    pub name: String,
    pub properties: BTreeMap<String, MapValue>,
}

pub struct ScriptList {
    pub scripts: Vec<Script>,
    pub groups: Vec<ScriptGroup>,
}

pub struct ScriptGroup {
    pub name: String,
    pub active: bool,
    pub subroutine: bool,
    pub scripts: Vec<Script>,
}

pub struct Script {
    pub name: String,
    pub active: bool,
    pub one_shot: bool,
    /// Easy, normal and hard, in serialized source order.
    pub difficulty: [bool; 3],
    pub subroutine: bool,
    pub delay_seconds: i32,
    /// Each inner list is AND; the outer list is OR, in file order.
    pub conditions: Vec<Vec<ScriptCall>>,
    pub actions: Vec<ScriptCall>,
    pub false_actions: Vec<ScriptCall>,
}

pub struct ScriptCall {
    pub id: i32,
    pub name: Option<String>,
    pub parameters: Vec<ScriptParameter>,
}

/// Source parameters carry all three scalar slots, except COORD3D (type 16).
pub struct ScriptParameter {
    pub kind: i32,
    pub integer: i32,
    pub real: f32,
    pub text: String,
    pub position: Option<[f32; 3]>,
}

impl ScriptBundle {
    /// Read the original uncompressed skirmish script bundle.
    ///
    /// # Errors
    /// Rejects invalid chunk bounds, symbols, strings, counts and versions.
    pub fn parse(bytes: &[u8]) -> Result<Self, AssetError> {
        let mut reader = Reader::new(bytes);
        if reader.take(4)? != b"CkMp" {
            return Err(AssetError::Invalid("script bundle has no CkMp signature"));
        }
        let symbols = read_symbols(&mut reader)?;
        let mut players = None;
        let mut teams = None;
        let mut lists = None;
        let mut unparsed_chunks = Vec::new();
        while !reader.is_empty() {
            let chunk = read_chunk(&mut reader, &symbols)?;
            match chunk.name {
                "ScriptsPlayers" if players.is_none() => {
                    players = Some(read_players(&chunk, &symbols)?);
                }
                "ScriptTeams" if teams.is_none() => {
                    if chunk.version != 1 {
                        return Err(AssetError::Invalid("unsupported script teams version"));
                    }
                    let mut data = Reader::new(chunk.bytes);
                    let mut entries = Vec::new();
                    while !data.is_empty() {
                        entries.push(read_dict(&mut data, &symbols)?);
                    }
                    teams = Some(entries);
                }
                "PlayerScriptsList" if lists.is_none() => {
                    if chunk.version != 5 {
                        return Err(AssetError::Invalid("unsupported script bundle version"));
                    }
                    let mut data = Reader::new(chunk.bytes);
                    let mut entries = Vec::new();
                    while !data.is_empty() {
                        let list = read_chunk(&mut data, &symbols)?;
                        if list.name != "ScriptList" || list.version != 1 || entries.len() >= 16 {
                            return Err(AssetError::Invalid("invalid player script list"));
                        }
                        entries.push(read_list(list.bytes, &symbols)?);
                    }
                    lists = Some(entries);
                }
                "ScriptsPlayers" | "ScriptTeams" | "PlayerScriptsList" => {
                    return Err(AssetError::Invalid("duplicate script bundle chunk"));
                }
                name => unparsed_chunks.push(name.to_owned()),
            }
        }
        let players = players.ok_or(AssetError::Invalid("script bundle has no players"))?;
        let lists = lists.ok_or(AssetError::Invalid("script bundle has no script lists"))?;
        if players.len() != lists.len() {
            return Err(AssetError::Invalid("script player/list count differs"));
        }
        Ok(Self {
            players,
            teams: teams.unwrap_or_default(),
            lists,
            unparsed_chunks,
        })
    }
}

fn read_players(
    chunk: &Chunk<'_, '_>,
    symbols: &BTreeMap<u32, String>,
) -> Result<Vec<ScriptPlayer>, AssetError> {
    if !(1..=2).contains(&chunk.version) {
        return Err(AssetError::Invalid("unsupported script player version"));
    }
    let mut reader = Reader::new(chunk.bytes);
    let dictionaries = chunk.version >= 2 && integer(&mut reader)? != 0;
    let count = integer(&mut reader)?;
    if !(0..=16).contains(&count) {
        return Err(AssetError::Invalid("invalid script player count"));
    }
    let mut players = Vec::new();
    for _ in 0..count {
        players.push(ScriptPlayer {
            name: ascii(&mut reader)?,
            properties: if dictionaries {
                read_dict(&mut reader, symbols)?
            } else {
                BTreeMap::new()
            },
        });
    }
    finish(&reader)?;
    Ok(players)
}

fn read_list(bytes: &[u8], symbols: &BTreeMap<u32, String>) -> Result<ScriptList, AssetError> {
    let mut reader = Reader::new(bytes);
    let mut scripts = Vec::new();
    let mut groups = Vec::new();
    while !reader.is_empty() {
        let chunk = read_chunk(&mut reader, symbols)?;
        match chunk.name {
            "Script" => scripts.push(read_script(&chunk, symbols)?),
            "ScriptGroup" => {
                if !(1..=2).contains(&chunk.version) {
                    return Err(AssetError::Invalid("unsupported script group version"));
                }
                let mut data = Reader::new(chunk.bytes);
                let name = ascii(&mut data)?;
                let active = data.byte()? != 0;
                let subroutine = chunk.version >= 2 && data.byte()? != 0;
                let mut group_scripts = Vec::new();
                while !data.is_empty() {
                    let script = read_chunk(&mut data, symbols)?;
                    if script.name != "Script" {
                        return Err(AssetError::Invalid("unexpected script group child"));
                    }
                    group_scripts.push(read_script(&script, symbols)?);
                }
                groups.push(ScriptGroup {
                    name,
                    active,
                    subroutine,
                    scripts: group_scripts,
                });
            }
            _ => return Err(AssetError::Invalid("unexpected script list child")),
        }
    }
    Ok(ScriptList { scripts, groups })
}

fn read_script(
    chunk: &Chunk<'_, '_>,
    symbols: &BTreeMap<u32, String>,
) -> Result<Script, AssetError> {
    if !(1..=2).contains(&chunk.version) {
        return Err(AssetError::Invalid("unsupported script version"));
    }
    let mut reader = Reader::new(chunk.bytes);
    let name = ascii(&mut reader)?;
    for _ in 0..3 {
        ascii(&mut reader)?; // Editor comments have no runtime effect.
    }
    let mut script = Script {
        name,
        active: reader.byte()? != 0,
        one_shot: reader.byte()? != 0,
        difficulty: [
            reader.byte()? != 0,
            reader.byte()? != 0,
            reader.byte()? != 0,
        ],
        subroutine: reader.byte()? != 0,
        delay_seconds: if chunk.version >= 2 {
            integer(&mut reader)?
        } else {
            0
        },
        conditions: Vec::new(),
        actions: Vec::new(),
        false_actions: Vec::new(),
    };
    while !reader.is_empty() {
        let child = read_chunk(&mut reader, symbols)?;
        match child.name {
            "OrCondition" => {
                if child.version != 1 {
                    return Err(AssetError::Invalid("unsupported OR condition version"));
                }
                let mut data = Reader::new(child.bytes);
                let mut conditions = Vec::new();
                while !data.is_empty() {
                    let condition = read_chunk(&mut data, symbols)?;
                    if condition.name != "Condition" {
                        return Err(AssetError::Invalid("unexpected OR condition child"));
                    }
                    conditions.push(read_call(&condition, symbols, 4)?);
                }
                script.conditions.push(conditions);
            }
            "ScriptAction" => script.actions.push(read_call(&child, symbols, 2)?),
            "ScriptActionFalse" => script.false_actions.push(read_call(&child, symbols, 2)?),
            _ => return Err(AssetError::Invalid("unexpected script child")),
        }
    }
    Ok(script)
}

fn read_call(
    chunk: &Chunk<'_, '_>,
    symbols: &BTreeMap<u32, String>,
    named_version: u16,
) -> Result<ScriptCall, AssetError> {
    if !(1..=named_version).contains(&chunk.version) {
        return Err(AssetError::Invalid("unsupported script call version"));
    }
    let mut reader = Reader::new(chunk.bytes);
    let id = integer(&mut reader)?;
    let name = if chunk.version >= named_version {
        let key = reader.le_u32()?;
        if key & 255 != 3 {
            return Err(AssetError::Invalid("script name key has wrong type"));
        }
        Some(
            symbols
                .get(&(key >> 8))
                .ok_or(AssetError::Invalid("unknown script name key"))?
                .clone(),
        )
    } else {
        None
    };
    let count = integer(&mut reader)?;
    if !(0..=12).contains(&count) {
        return Err(AssetError::Invalid("invalid script parameter count"));
    }
    let mut parameters = Vec::new();
    for _ in 0..count {
        let kind = integer(&mut reader)?;
        let mut parameter = ScriptParameter {
            kind,
            integer: 0,
            real: 0.0,
            text: String::new(),
            position: None,
        };
        if kind == 16 {
            parameter.position = Some([real(&mut reader)?, real(&mut reader)?, real(&mut reader)?]);
        } else {
            parameter.integer = integer(&mut reader)?;
            parameter.real = real(&mut reader)?;
            parameter.text = ascii(&mut reader)?;
            // Original loader migrations for old editor template names.
            if kind == 15
                && let Some(suffix) = parameter.text.strip_prefix("Fundamentalist")
            {
                parameter.text = format!("GLA{suffix}");
            }
            if kind == 32
                && matches!(
                    parameter.text.as_str(),
                    "Upgrade_AmericaRangerCaptureBuilding"
                        | "Upgrade_ChinaRedguardCaptureBuilding"
                        | "Upgrade_GLARebelCaptureBuilding"
                )
            {
                "Upgrade_InfantryCaptureBuilding".clone_into(&mut parameter.text);
            }
        }
        parameters.push(parameter);
    }
    finish(&reader)?;
    Ok(ScriptCall {
        id,
        name,
        parameters,
    })
}
