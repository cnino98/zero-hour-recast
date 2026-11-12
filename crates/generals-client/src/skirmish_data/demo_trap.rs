//! Native `DemoTrapUpdate` data, including nullable full detonation templates.
use super::{blocks, scalar, weapons};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{WeaponRules, demo_trap::Rules};
use num_traits::ToPrimitive;
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "DemoTrapUpdate")
        {
            continue;
        }
        let mut rules = Rules {
            slots: [0; 3],
            proximity_default: false,
            friendly_detonation: false,
            detonate_when_killed: false,
            range: 0.0,
            scan_frames: 0,
            ignore_kind: Vec::new(),
            detonation_weapon: None,
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            let key = line.tokens[0].as_str();
            match key {
                "DetonationWeaponSlot" | "ProximityModeWeaponSlot" | "ManualModeWeaponSlot" => {
                    let slot = match scalar(line)?.to_ascii_uppercase().as_str() {
                        "PRIMARY" => 0,
                        "SECONDARY" => 1,
                        "TERTIARY" => 2,
                        _ => return Err("invalid DemoTrap weapon slot".into()),
                    };
                    let index = match key {
                        "DetonationWeaponSlot" => 0,
                        "ProximityModeWeaponSlot" => 1,
                        _ => 2,
                    };
                    rules.slots[index] = slot;
                }
                "DefaultProximityMode"
                | "AutoDetonationWithFriendsInvolved"
                | "DetonateWhenKilled" => {
                    let value = scalar(line)?;
                    let enabled = if value.eq_ignore_ascii_case("Yes") {
                        true
                    } else if value.eq_ignore_ascii_case("No") {
                        false
                    } else {
                        return Err(format!("invalid DemoTrap {key}").into());
                    };
                    match key {
                        "DefaultProximityMode" => rules.proximity_default = enabled,
                        "AutoDetonationWithFriendsInvolved" => rules.friendly_detonation = enabled,
                        _ => rules.detonate_when_killed = enabled,
                    }
                }
                "TriggerDetonationRange" => rules.range = super::finite_number(scalar(line)?)?,
                "ScanRate" => {
                    let milliseconds: u32 = scalar(line)?.parse()?;
                    rules.scan_frames =
                        (milliseconds.to_f32().expect("UnsignedInt converts to Real")
                            * (30.0_f32 / 1000.0))
                            .ceil()
                            .to_u32()
                            .expect("native scan frames fit UnsignedInt");
                }
                "IgnoreTargetTypes" => parse_kind_mask(&line.tokens[1..], &mut rules.ignore_kind)?,
                "DetonationWeapon" => {
                    rules.detonation_weapon =
                        super::instant_death::nullable(files, "Weapon", scalar(line)?)?
                            .map(|weapon| {
                                let mut rules = WeaponRules::unarmed();
                                rules.name.clone_from(&weapon.name);
                                weapons::parse(&mut rules, &weapon.lines)?;
                                Ok::<_, Box<dyn Error>>(rules)
                            })
                            .transpose()?;
                }
                _ => {}
            }
        }
        if rules.slots[0] == rules.slots[1]
            || rules.slots[0] == rules.slots[2]
            || rules.slots[1] == rules.slots[2]
        {
            return Err("DemoTrap requires three distinct weapon slots".into());
        }
        result.push(rules);
    }
    Ok(result)
}

fn parse_kind_mask(values: &[String], kinds: &mut Vec<String>) -> Result<(), Box<dyn Error>> {
    let mut replacement = false;
    let mut incremental = false;
    for value in values {
        if value.eq_ignore_ascii_case("NONE") {
            if replacement || incremental {
                return Err("mixed kind mask operations".into());
            }
            kinds.clear();
            break;
        }
        let operation = value.starts_with(['+', '-']);
        if (operation && replacement) || (!operation && incremental) {
            return Err("mixed kind mask operations".into());
        }
        if !operation && !replacement {
            kinds.clear();
        }
        replacement |= !operation;
        incremental |= operation;
        let name = value.trim_start_matches(['+', '-']).to_ascii_uppercase();
        if value.starts_with('-') {
            kinds.retain(|kind| kind != &name);
        } else if !kinds.contains(&name) {
            kinds.push(name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use generals_assets::virtual_assets::VirtualAssets;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_demo_trap_keeps_mode_commands_stealth_and_delayed_weapon() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        let object = super::super::definition(&files, "Object", "GLADemoTrap").unwrap();
        let unit = super::super::unit_type(
            &files,
            object,
            generals_sim::skirmish::Faction::Gla,
            generals_sim::skirmish::Kind::Defense,
        )
        .and_then(|unit| super::super::finish_type(&files, object, unit))
        .unwrap();
        assert_eq!(unit.cost, 400);
        assert_eq!(unit.build_seconds.to_bits(), 5.0_f32.to_bits());
        let trap = unit.demo_trap.first().unwrap();
        assert!(trap.proximity_default && trap.friendly_detonation && trap.detonate_when_killed);
        assert_eq!(trap.slots, [0, 1, 2]);
        assert_eq!(trap.range.to_bits(), 40.0_f32.to_bits());
        assert_eq!(trap.scan_frames, 0);
        for slot in 0..3 {
            let weapon = unit.weapon_in_slot(slot).unwrap();
            assert_eq!(weapon.name, "DummyWeapon");
            assert_eq!(weapon.auto_choose, 0);
        }
        let stealth = unit.behavior.stealth.as_ref().unwrap();
        assert_eq!(stealth.delay, 0);
        assert_eq!(stealth.forbidden, 0);
        let death = unit.death_lifecycle.as_ref().unwrap();
        assert!(death.physics.is_none());
        assert_eq!(death.slow[0].conditions.exempt_status, 1 << 3);
        assert_eq!(death.slow[0].destruction_delay, [30, 0]);
        assert_eq!(death.slow[0].weapons[2], [0]);
        assert_eq!(death.weapons[0].name, "DemoTrapDetonationWeapon");
        assert_eq!(death.weapons[0].damage.to_bits(), 600.0_f32.to_bits());
        let commands =
            super::super::definition(&files, "CommandSet", "GLADemoTrapCommandSet").unwrap();
        assert!(commands.lines.iter().any(|line| {
            line.tokens
                .iter()
                .any(|token| token == "Command_DetonateDemoTrap")
        }));
    }
    #[test]
    fn repeated_instances_nullable_weapons_masks_and_native_unsigned_scan_duration() {
        use generals_assets::{content, virtual_assets::Asset};
        let files = [content::scan(Asset {
            path: "trap.ini".into(), origin: "native DemoTrap fields".into(),
            bytes: b"Object Trap\nBehavior=DemoTrapUpdate First\nDetonationWeaponSlot=PRIMARY\nProximityModeWeaponSlot=SECONDARY\nManualModeWeaponSlot=TERTIARY\nDetonationWeapon=Missing\nScanRate=4294967295\nIgnoreTargetTypes=PROJECTILE VEHICLE\nIgnoreTargetTypes=-VEHICLE +DOZER\nDefaultProximityMode=yEs\nEnd\nBehavior=DemoTrapUpdate Second\nDetonationWeaponSlot=TERTIARY\nProximityModeWeaponSlot=PRIMARY\nManualModeWeaponSlot=SECONDARY\nDetonationWeapon=Blast\nEnd\nEnd\nWeapon Blast\nPrimaryDamage=125\nPrimaryDamageRadius=30\nDamageType=EXPLOSION\nEnd\n".to_vec(),
        }).unwrap()];
        let rules = super::load(&files, &files[0].declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules[0].proximity_default);
        assert_eq!(rules[0].ignore_kind, ["PROJECTILE", "DOZER"]);
        assert_eq!(rules[0].scan_frames, 128_849_016);
        assert!(rules[0].detonation_weapon.is_none());
        let weapon = rules[1].detonation_weapon.as_ref().unwrap();
        assert_eq!(weapon.name, "Blast");
        assert_eq!(weapon.damage.to_bits(), 125.0_f32.to_bits());
        assert_eq!(
            weapon.area.as_ref().unwrap().primary_radius.to_bits(),
            30.0_f32.to_bits()
        );
    }
}
