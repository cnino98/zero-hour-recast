//! Native command buttons/sets retained for `ScriptActions` and `CommandButtonHuntUpdate`.
use super::{blocks, field, scalar, source_real};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{
    command_button_hunt::{Button, Command, Definitions, Rules},
    special_ability::{Power, PowerType},
};
use std::{collections::BTreeMap, error::Error};

pub(super) fn rules(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "CommandButtonHuntUpdate")
        {
            continue;
        }
        let mut rules = Rules {
            scan_frames: 30,
            scan_range: 9999.0,
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("ScanRate") => {
                    rules.scan_frames = super::source_numbers::duration_frames(scalar(line)?)?;
                }
                Some("ScanRange") => rules.scan_range = source_real(scalar(line)?)?,
                _ => {}
            }
        }
        result.push(rules);
    }
    Ok(result)
}

pub(super) fn definitions(files: &[ContentFile]) -> Result<Definitions, Box<dyn Error>> {
    let mut buttons = Vec::new();
    for definition in files
        .iter()
        .flat_map(|file| &file.declarations)
        .filter(|declaration| declaration.kind == "CommandButton")
    {
        buttons.push(button(files, definition)?);
    }
    let mut sets = BTreeMap::new();
    for definition in files
        .iter()
        .flat_map(|file| &file.declarations)
        .filter(|declaration| declaration.kind == "CommandSet")
    {
        let mut slots = [None; 18];
        for line in definition.lines.iter().filter(|line| line.depth == Some(0)) {
            let Some(slot) = line
                .tokens
                .first()
                .and_then(|key| key.parse::<usize>().ok())
            else {
                continue;
            };
            let slot = slot
                .checked_sub(1)
                .filter(|&slot| slot < 18)
                .ok_or("native command slot outside 1..=18")?;
            let name = scalar(line)?;
            slots[slot] = if name.eq_ignore_ascii_case("NONE") {
                None
            } else {
                Some(
                    buttons
                        .iter()
                        .position(|button| button.name == name)
                        .ok_or_else(|| format!("missing command button {name}"))?,
                )
            };
        }
        sets.insert(definition.name.clone(), slots);
    }
    Ok(Definitions { buttons, sets })
}

fn button(files: &[ContentFile], definition: &Declaration) -> Result<Button, Box<dyn Error>> {
    let command = match field(&definition.lines, 0, "Command")
        .map(scalar)
        .transpose()?
        .unwrap_or("NONE")
        .to_ascii_uppercase()
        .as_str()
    {
        "SPECIAL_POWER" => Command::SpecialPower,
        "SWITCH_WEAPON" => Command::SwitchWeapon,
        "FIRE_WEAPON" => Command::FireWeapon,
        "HIJACK_VEHICLE" => Command::Hijack,
        "CONVERT_TO_CARBOMB" => Command::CarBomb,
        "SABOTAGE_BUILDING" => Command::Sabotage,
        "NONE"
        | "DOZER_CONSTRUCT"
        | "DOZER_CONSTRUCT_CANCEL"
        | "UNIT_BUILD"
        | "CANCEL_UNIT_BUILD"
        | "PLAYER_UPGRADE"
        | "OBJECT_UPGRADE"
        | "CANCEL_UPGRADE"
        | "ATTACK_MOVE"
        | "GUARD"
        | "GUARD_WITHOUT_PURSUIT"
        | "GUARD_FLYING_UNITS_ONLY"
        | "STOP"
        | "WAYPOINTS"
        | "EXIT_CONTAINER"
        | "EVACUATE"
        | "EXECUTE_RAILED_TRANSPORT"
        | "BEACON_DELETE"
        | "SET_RALLY_POINT"
        | "SELL"
        | "PURCHASE_SCIENCE"
        | "HACK_INTERNET"
        | "TOGGLE_OVERCHARGE"
        | "COMBATDROP"
        | "PLACE_BEACON"
        | "SPECIAL_POWER_FROM_SHORTCUT"
        | "SPECIAL_POWER_CONSTRUCT"
        | "SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT"
        | "SELECT_ALL_UNITS_OF_TYPE" => Command::Other,
        _ => return Err("invalid native GUI command".into()),
    };
    let weapon_slot = weapon_slot(definition)?;
    let power = if let Some(line) = field(&definition.lines, 0, "SpecialPower") {
        super::instant_death::nullable(files, "SpecialPower", scalar(line)?)?
            .map(|power| {
                Ok::<_, Box<dyn Error>>(Power {
                    name: power.name.clone(),
                    kind: PowerType::parse(
                        field(&power.lines, 0, "Enum")
                            .map(scalar)
                            .transpose()?
                            .unwrap_or("SPECIAL_INVALID"),
                    )
                    .ok_or("invalid SpecialPower Enum")?,
                })
            })
            .transpose()?
    } else {
        None
    };
    let needs_object = command_options(definition)? & 7 != 0;
    Ok(Button {
        name: definition.name.clone(),
        command,
        weapon_slot,
        power,
        needs_object,
    })
}

// ControlBar's WeaponSlot uses INI::parseLookupList: one token per field,
// case-insensitive, with PRIMARY as the constructor default. Trailing tokens
// (including the original Neutron button's TERTIARY) are not a slot mask.
pub(super) fn weapon_slot(definition: &Declaration) -> Result<usize, Box<dyn Error>> {
    let mut slot = 0;
    for line in definition.lines.iter().filter(|line| {
        line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "WeaponSlot")
    }) {
        let token = line.tokens.get(1).ok_or("missing native weapon slot")?;
        slot = ["PRIMARY", "SECONDARY", "TERTIARY"]
            .iter()
            .position(|name| token.eq_ignore_ascii_case(name))
            .ok_or("invalid native weapon slot")?;
    }
    Ok(slot)
}

fn command_options(definition: &Declaration) -> Result<u32, Box<dyn Error>> {
    const OPTIONS: &[&str] = &[
        "NEED_TARGET_ENEMY_OBJECT",
        "NEED_TARGET_NEUTRAL_OBJECT",
        "NEED_TARGET_ALLY_OBJECT",
        "unused-reserved",
        "ALLOW_SHRUBBERY_TARGET",
        "NEED_TARGET_POS",
        "NEED_UPGRADE",
        "NEED_SPECIAL_POWER_SCIENCE",
        "OK_FOR_MULTI_SELECT",
        "CONTEXTMODE_COMMAND",
        "CHECK_LIKE",
        "ALLOW_MINE_TARGET",
        "ATTACK_OBJECTS_POSITION",
        "OPTION_ONE",
        "OPTION_TWO",
        "OPTION_THREE",
        "NOT_QUEUEABLE",
        "SINGLE_USE_COMMAND",
        "---DO-NOT-USE---",
        "SCRIPT_ONLY",
        "IGNORES_UNDERPOWERED",
        "USES_MINE_CLEARING_WEAPONSET",
        "CAN_USE_WAYPOINTS",
        "MUST_BE_STOPPED",
    ];
    let mut bits = 0;
    for line in definition.lines.iter().filter(|line| {
        line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "Options")
    }) {
        let (mut normal, mut change) = (false, false);
        for token in &line.tokens[1..] {
            if token.eq_ignore_ascii_case("NONE") {
                if normal || change {
                    return Err("mixed native bitstring operations".into());
                }
                bits = 0;
                break;
            }
            let (operation, name) = token.strip_prefix('+').map_or_else(
                || {
                    token
                        .strip_prefix('-')
                        .map_or((0, token.as_str()), |name| (2, name))
                },
                |name| (1, name),
            );
            let index = OPTIONS
                .iter()
                .position(|name_in_list| name.eq_ignore_ascii_case(name_in_list))
                .ok_or("invalid native command option")?;
            if operation == 0 {
                if change {
                    return Err("mixed native bitstring operations".into());
                }
                if !normal {
                    bits = 0;
                }
                normal = true;
            } else {
                if normal {
                    return Err("mixed native bitstring operations".into());
                }
                change = true;
            }
            if operation == 2 {
                bits &= !(1 << index);
            } else {
                bits |= 1 << index;
            }
        }
    }
    Ok(bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_scan_fields_and_all_eighteen_command_slots_keep_native_values() {
        let files = vec![generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "hunt.ini".into(), origin: "test".into(),
            bytes: b"SpecialPower Hack\nEnum=SPECIAL_HACKER_DISABLE_BUILDING\nEnd\nCommandButton HackButton\nCommand=SPECIAL_POWER\nSpecialPower=Hack\nOptions=NEED_TARGET_ENEMY_OBJECT\nEnd\nCommandButton WeaponButton\nCommand=SWITCH_WEAPON\nWeaponSlot=SECONDARY\nOptions=NEED_TARGET_ENEMY_OBJECT\nOptions=-NEED_TARGET_ENEMY_OBJECT\nEnd\nCommandSet Commands\n18=WeaponButton\n18=HackButton\nEnd\nObject Host\nBehavior=CommandButtonHuntUpdate A\nEnd\nBehavior=CommandButtonHuntUpdate B\nScanRate=100ms\nScanRate=101ms\nScanRange=42.5\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let host = super::super::definition(&files, "Object", "Host").unwrap();
        let data = rules(host).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].scan_frames, 30);
        assert_eq!(data[0].scan_range.to_bits(), 9999.0_f32.to_bits());
        assert_eq!(data[1].scan_frames, 4);
        assert_eq!(data[1].scan_range.to_bits(), 42.5_f32.to_bits());
        let definitions = definitions(&files).unwrap();
        let button = definitions.sets["Commands"][17].unwrap();
        assert_eq!(definitions.buttons[button].name, "HackButton");
        assert!(definitions.buttons[button].needs_object);
        assert_eq!(definitions.buttons[1].weapon_slot, 1);
        assert!(!definitions.buttons[1].needs_object);
    }

    #[test]
    fn weapon_slot_reads_only_first_token_in_authored_field_order() {
        for (fields, expected) in [
            ("", 0),
            ("WeaponSlot=SECONDARY TERTIARY\n", 1),
            (
                "WeaponSlot=secondary ignored\nWeaponSlot=tertiary PRIMARY\n",
                2,
            ),
        ] {
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "commandbutton.ini".into(),
                origin: "ControlBar.cpp WeaponSlot / INI.cpp parseLookupList".into(),
                bytes: format!("CommandButton Slot\nCommand=SWITCH_WEAPON\n{fields}End\n")
                    .into_bytes(),
            })
            .unwrap();
            let parsed = button(&[], &file.declarations[0]).unwrap();
            assert_eq!(parsed.weapon_slot, expected, "{fields}");
        }
    }

    #[test]
    fn weapon_slot_rejects_empty_or_invalid_first_tokens_before_later_assignments() {
        for fields in [
            "WeaponSlot=\n",
            "WeaponSlot=UNKNOWN SECONDARY\n",
            "WeaponSlot=UNKNOWN\nWeaponSlot=PRIMARY\n",
        ] {
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "commandbutton.ini".into(),
                origin: "INI.cpp getNextToken / scanLookupList errors".into(),
                bytes: format!("CommandButton Invalid\n{fields}End\n").into_bytes(),
            })
            .unwrap();
            assert!(weapon_slot(&file.declarations[0]).is_err(), "{fields}");
        }
    }
}
