//! Installed command mappings and Object flags used by native mass selection.
use crate::skirmish_data;
use bevy::prelude::*;
use generals_assets::{
    content::{self, ContentFile, Declaration},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::UnitType;
use std::error::Error;

pub(super) struct TypeSelection {
    pub excluded: bool,
    pub aircraft: bool,
    pub equivalent: Vec<usize>,
}

pub(super) struct SelectionData {
    pub keys: [KeyCode; 3],
    pub types: Vec<TypeSelection>,
    pub maximum: usize,
}

impl SelectionData {
    pub fn load(assets: &VirtualAssets, catalog: &[UnitType]) -> Result<Self, Box<dyn Error>> {
        let commands = content::scan(assets.read("data/english/commandmap.ini")?)?;
        let keys = [
            ("SELECT_MATCHING_UNITS", "KEY_E", KeyCode::KeyE),
            ("SELECT_ALL", "KEY_Q", KeyCode::KeyQ),
            ("SELECT_ALL_AIRCRAFT", "KEY_W", KeyCode::KeyW),
        ];
        for (name, key, _) in keys {
            validate_key(&commands, name, key, "NONE")?;
        }
        // playtest::keyboard dispatches this separate native DOWN meta command.
        validate_key(&commands, "STOP", "KEY_S", "NONE")?;
        validate_key(&commands, "CREATE_FORMATION", "KEY_F", "CTRL")?;
        // Gameplay already resolved source inheritance and repeated KindOf fields.
        let types = catalog
            .iter()
            .enumerate()
            .map(|(source, object)| {
                let flags = &object.kind_of;
                let mut equivalent = Vec::new();
                for (index, other) in catalog.iter().enumerate() {
                    if catalog[source].equivalent_to(other) {
                        equivalent.push(index);
                    }
                }
                TypeSelection {
                    excluded: flags.iter().any(|flag| {
                        matches!(flag.as_str(), "DOZER" | "HARVESTER" | "IGNORES_SELECT_ALL")
                    }),
                    aircraft: flags.iter().any(|flag| flag == "AIRCRAFT"),
                    equivalent,
                }
            })
            .collect();
        let ui = content::scan(assets.read("data/ini/ingameui.ini")?)?;
        let definition = skirmish_data::definition(std::slice::from_ref(&ui), "InGameUI", "")?;
        let maximum = match values(definition, "MaxSelectionSize")?.as_slice() {
            [] => 0,
            [number] => usize::try_from(number.parse::<i32>()?).unwrap_or(0),
            _ => return Err("invalid InGameUI MaxSelectionSize".into()),
        };
        Ok(Self {
            keys: keys.map(|(_, _, key)| key),
            types,
            maximum,
        })
    }
}

fn validate_key(
    file: &ContentFile,
    name: &str,
    key: &str,
    modifier: &str,
) -> Result<(), Box<dyn Error>> {
    let definition = skirmish_data::definition(std::slice::from_ref(file), "CommandMap", name)?;
    for (field, expected) in [
        ("Key", key),
        ("Transition", "DOWN"),
        ("Modifiers", modifier),
        ("UseableIn", "GAME"),
    ] {
        if values(definition, field)? != [expected] {
            return Err(format!("unsupported CommandMap {name} {field}").into());
        }
    }
    Ok(())
}

fn values<'a>(definition: &'a Declaration, field: &str) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut lines = definition.lines.iter().filter(|line| {
        line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == field)
    });
    let values = lines.next().map_or_else(Vec::new, |line| {
        line.tokens.iter().skip(1).map(String::as_str).collect()
    });
    if lines.next().is_some() {
        return Err(format!("duplicate {} {field}", definition.name).into());
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::validate_key;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn meta_shortcuts_reject_an_unrepresented_modifier_override() {
        for (name, key, modifier) in [
            ("SELECT_ALL", "KEY_Q", "NONE"),
            ("STOP", "KEY_S", "NONE"),
            ("CREATE_FORMATION", "KEY_F", "CTRL"),
        ] {
            let definition = |modifier| {
                content::scan(Asset {
                    path: "commandmap.ini".into(),
                    origin: "fixture".into(),
                    bytes: format!("CommandMap {name}\n Key = {key}\n Transition = DOWN\n Modifiers = {modifier}\n UseableIn = GAME\nEnd\n").into_bytes(),
                }).expect("valid fixture")
            };
            assert!(validate_key(&definition(modifier), name, key, modifier).is_ok());
            assert!(validate_key(&definition("SHIFT"), name, key, modifier).is_err());
        }
    }
}
