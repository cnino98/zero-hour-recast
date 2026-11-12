//! `TurretAIData::buildFieldParse` primary controlled-weapon block.
use super::{blocks, field};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::turret::Rules;
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Option<Rules>, Box<dyn Error>> {
    load_named(object, "Turret")
}

pub(super) fn load_secondary(object: &Declaration) -> Result<Option<Rules>, Box<dyn Error>> {
    load_named(object, "AltTurret")
}

pub(super) fn linked(object: &Declaration) -> Result<bool, Box<dyn Error>> {
    let mut linked = false;
    for module in blocks(&object.lines, 0, "Behavior")? {
        if let Some(line) = field(module, 1, "TurretsLinked") {
            linked = match first_value(line)?.to_ascii_uppercase().as_str() {
                "YES" => true,
                "NO" => false,
                _ => return Err("invalid TurretsLinked boolean".into()),
            };
        }
    }
    Ok(linked)
}

fn load_named(object: &Declaration, name: &str) -> Result<Option<Rules>, Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        if let Some(lines) = blocks(module, 1, name)?.into_iter().next() {
            let slots =
                field(lines, 2, "ControlledWeaponSlots").ok_or("turret has no controlled slots")?;
            let mut rules = Rules::source_defaults();
            rules.controlled_slots = controlled_slots(slots)?;
            load_sweep(lines, &mut rules)?;
            for (key, output, factor) in [
                (
                    "TurretTurnRate",
                    &mut rules.turn_rate,
                    (1.0 / 30.0) * (std::f32::consts::PI / 180.0),
                ),
                (
                    "TurretPitchRate",
                    &mut rules.pitch_rate,
                    (1.0 / 30.0) * (std::f32::consts::PI / 180.0),
                ),
                (
                    "NaturalTurretAngle",
                    &mut rules.natural_angle,
                    std::f32::consts::PI / 180.0,
                ),
                (
                    "NaturalTurretPitch",
                    &mut rules.natural_pitch,
                    std::f32::consts::PI / 180.0,
                ),
                (
                    "FirePitch",
                    &mut rules.fire_pitch,
                    std::f32::consts::PI / 180.0,
                ),
                (
                    "MinPhysicalPitch",
                    &mut rules.minimum_pitch,
                    std::f32::consts::PI / 180.0,
                ),
                (
                    "GroundUnitPitch",
                    &mut rules.ground_pitch,
                    std::f32::consts::PI / 180.0,
                ),
            ] {
                if let Some(line) = field(lines, 2, key) {
                    *output = signed_number(first_value(line)?)? * factor;
                }
            }
            for (key, output) in [
                ("AllowsPitch", &mut rules.allows_pitch),
                ("InitiallyDisabled", &mut rules.initially_disabled),
            ] {
                if let Some(line) = field(lines, 2, key) {
                    *output = match first_value(line)?.to_ascii_uppercase().as_str() {
                        "YES" => true,
                        "NO" => false,
                        _ => return Err(format!("invalid {key} boolean").into()),
                    };
                }
            }
            if let Some(line) = field(lines, 2, "RecenterTime") {
                rules.recenter_frames = duration_frames(line)?;
            }
            for (index, name) in ["MinIdleScanAngle", "MaxIdleScanAngle"]
                .into_iter()
                .enumerate()
            {
                if let Some(line) = field(lines, 2, name) {
                    rules.idle_angles[index] =
                        signed_number(first_value(line)?)? * (std::f32::consts::PI / 180.0);
                }
            }
            for (index, name) in ["MinIdleScanInterval", "MaxIdleScanInterval"]
                .into_iter()
                .enumerate()
            {
                if let Some(line) = field(lines, 2, name) {
                    rules.idle_intervals[index] = duration_frames(line)?;
                }
            }
            if rules.turn_rate < 0.0
                || rules.pitch_rate < 0.0
                || rules.idle_angles[0] > rules.idle_angles[1]
                || rules.idle_intervals[0] > rules.idle_intervals[1]
            {
                return Err(format!("{}: invalid turret rates or scan limits", object.name).into());
            }
            return Ok(Some(rules));
        }
    }
    Ok(None)
}

fn controlled_slots(line: &ContentLine) -> Result<u8, Box<dyn Error>> {
    let mut mask = 0;
    for slot in line.tokens.iter().skip(1) {
        mask |= match slot.as_str() {
            "PRIMARY" => 1,
            "SECONDARY" => 2,
            "TERTIARY" => 4,
            _ => return Err(format!("invalid controlled weapon slot {slot}").into()),
        };
    }
    Ok(mask)
}

fn load_sweep(lines: &[ContentLine], rules: &mut Rules) -> Result<(), Box<dyn Error>> {
    for line in lines.iter().filter(|line| line.depth == Some(2)) {
        let (values, factor) = match line.tokens.first().map(String::as_str) {
            Some("TurretFireAngleSweep") => (&mut rules.fire_sweep, std::f32::consts::PI / 180.0),
            Some("TurretSweepSpeedModifier") => (&mut rules.sweep_speed, 1.0),
            _ => continue,
        };
        let slot = match line.tokens.get(1).map(String::as_str) {
            Some("PRIMARY") => 0,
            Some("SECONDARY") => 1,
            Some("TERTIARY") => 2,
            _ => return Err(format!("invalid turret sweep slot at line {}", line.line).into()),
        };
        let value = line
            .tokens
            .get(2)
            .ok_or_else(|| format!("missing turret sweep value at line {}", line.line))?;
        values[slot] = signed_number(value)? * factor;
    }
    Ok(())
}

fn first_value(line: &ContentLine) -> Result<&str, Box<dyn Error>> {
    // INI numeric/bool parsers consume getNextToken; trailing tokens are ignored.
    line.tokens
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| format!("missing turret value at line {}", line.line).into())
}

fn duration_frames(line: &ContentLine) -> Result<u32, Box<dyn Error>> {
    let milliseconds: u32 = first_value(line)?.parse()?;
    Ok(u32::try_from(
        (u64::from(milliseconds) * 30).div_ceil(1000),
    )?)
}

fn signed_number(value: &str) -> Result<f32, Box<dyn Error>> {
    let number: f32 = value.strip_suffix(['f', 'F']).unwrap_or(value).parse()?;
    if !number.is_finite() {
        return Err(format!("nonfinite turret value {value}").into());
    }
    Ok(number)
}

#[cfg(test)]
mod tests {
    use super::load;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn authored_negative_pitch_and_natural_angles_are_signed_but_rates_are_nonnegative() {
        let source = "Object Medic\nBehavior=AIUpdateInterface AI\nTurret\nControlledWeaponSlots=PRIMARY\nTurretTurnRate=180\nTurretPitchRate=180\nAllowsPitch=Yes\nMinPhysicalPitch=-20\nNaturalTurretAngle=-90\nNaturalTurretPitch=-15\nFirePitch=-10\nGroundUnitPitch=-5\nMinIdleScanAngle=-60\nMaxIdleScanAngle=-30\nEnd\nEnd\nEnd";
        let parse = |source: &str| {
            content::scan(Asset {
                path: "turret.ini".into(),
                origin: "fixture".into(),
                bytes: source.as_bytes().to_vec(),
            })
            .unwrap()
        };
        let file = parse(source);
        let rules = load(&file.declarations[0]).unwrap().unwrap();
        for (actual, degrees) in [
            (rules.minimum_pitch, -20.0_f32),
            (rules.natural_angle, -90.0),
            (rules.natural_pitch, -15.0),
            (rules.fire_pitch, -10.0),
            (rules.ground_pitch, -5.0),
            (rules.idle_angles[0], -60.0),
            (rules.idle_angles[1], -30.0),
        ] {
            assert!((actual - degrees.to_radians()).abs() < 0.000_001);
        }
        let suffixed = parse(&source.replace("MinPhysicalPitch=-20", "MinPhysicalPitch=-20.0f"));
        assert!(load(&suffixed.declarations[0]).is_ok());
        for invalid in [
            source.replace("TurretTurnRate=180", "TurretTurnRate=-180"),
            source.replace("MinPhysicalPitch=-20", "MinPhysicalPitch=NaN"),
            source.replace("NaturalTurretPitch=-15", "NaturalTurretPitch=inf"),
        ] {
            assert!(load(&parse(&invalid).declarations[0]).is_err());
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn all_ten_installed_base_faction_turret_blocks_parse_original_tokens() {
        let file = content::scan(Asset {
            path: "turret-fixtures.ini".into(),
            origin: "installed INIZH.big original turret blocks".into(),
            bytes: std::fs::read(
                generals_assets::game_root()
                    .unwrap()
                    .join("test-fixtures/turret-fixtures.ini"),
            )
            .unwrap(),
        })
        .unwrap();
        assert_eq!(file.declarations.len(), 10);
        for object in &file.declarations {
            let rules = load(object)
                .unwrap_or_else(|error| panic!("{}: {error}", object.name))
                .expect("represented primary turret");
            assert!(rules.turn_rate >= 0.0 && rules.pitch_rate >= 0.0);
            if object.name == "AmericaVehicleMedic" {
                assert_eq!(
                    rules.fire_sweep[0].to_bits(),
                    (25.0_f32 * (std::f32::consts::PI / 180.0)).to_bits()
                );
                assert_eq!(rules.sweep_speed.map(f32::to_bits), [1.0_f32.to_bits(); 3]);
            }
        }
    }

    #[test]
    fn sweep_fields_preserve_per_slot_defaults_and_authored_overwrite_order() {
        let file = content::scan(Asset {
            path: "sweep.ini".into(),
            origin: "fixture".into(),
            bytes: b"Object Medic\nBehavior=AIUpdateInterface AI\nTurret\nControlledWeaponSlots=PRIMARY SECONDARY\nTurretFireAngleSweep=PRIMARY 25\nTurretFireAngleSweep=SECONDARY 40\nTurretSweepSpeedModifier=SECONDARY 0.25\nTurretFireAngleSweep=PRIMARY 15.0f trailing tokens\nTurretSweepSpeedModifier=PRIMARY 2.0f\nEnd\nEnd\nEnd".to_vec(),
        })
        .unwrap();
        let rules = load(&file.declarations[0]).unwrap().unwrap();
        for (actual, degrees) in rules.fire_sweep.into_iter().zip([15.0_f32, 40.0, 0.0]) {
            assert_eq!(
                actual.to_bits(),
                (degrees * (std::f32::consts::PI / 180.0)).to_bits()
            );
        }
        assert_eq!(
            rules.sweep_speed.map(f32::to_bits),
            [2.0_f32.to_bits(), 0.25_f32.to_bits(), 1.0_f32.to_bits()]
        );
    }

    #[test]
    fn malformed_sweep_fields_reject_missing_values_slots_and_nonfinite_numbers() {
        for field in [
            "TurretFireAngleSweep=PRIMARY",
            "TurretFireAngleSweep=QUATERNARY 25",
            "TurretFireAngleSweep=PRIMARY NaN",
            "TurretSweepSpeedModifier=PRIMARY inf",
        ] {
            let file = content::scan(Asset {
                path: "sweep.ini".into(),
                origin: "fixture".into(),
                bytes: format!("Object Medic\nBehavior=AIUpdateInterface AI\nTurret\nControlledWeaponSlots=PRIMARY\n{field}\nEnd\nEnd\nEnd").into_bytes(),
            })
            .unwrap();
            assert!(load(&file.declarations[0]).is_err());
        }
    }

    #[test]
    fn primary_turret_reads_nested_source_rates_and_duration() {
        let file = content::scan(Asset { path: "test.ini".into(), origin: "fixture".into(), bytes: b"Object Tank\nBehavior=AIUpdateInterface AI\nTurret\nControlledWeaponSlots=PRIMARY SECONDARY\nTurretTurnRate=180 // turn rate, in degrees per sec\nTurretPitchRate=90\nNaturalTurretPitch=45\nAllowsPitch=Yes\nRecenterTime=5000\nEnd\nEnd\nEnd".to_vec() }).unwrap();
        let rules = load(&file.declarations[0]).unwrap().unwrap();
        assert!((rules.turn_rate - std::f32::consts::PI / 30.0).abs() < 0.000_001);
        assert!((rules.pitch_rate - std::f32::consts::PI / 60.0).abs() < 0.000_001);
        assert_eq!(rules.recenter_frames, 150);
        assert_eq!(rules.controlled_slots, 3);
        assert!(rules.allows_pitch);
        assert!((rules.natural_pitch - std::f32::consts::FRAC_PI_4).abs() < 0.000_001);
    }
}

#[cfg(test)]
mod alternate_tests {
    use super::{linked, load, load_secondary};
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn primary_and_alternate_blocks_keep_independent_slot_masks_and_rates() {
        let file = content::scan(Asset {
            path: "two-turrets.ini".into(), origin: "fixture".into(),
            bytes: b"Object Tank\nBehavior=AIUpdateInterface AI\nTurretsLinked=Yes\nTurret\nControlledWeaponSlots=SECONDARY\nTurretTurnRate=90\nEnd\nAltTurret\nControlledWeaponSlots=PRIMARY TERTIARY\nTurretTurnRate=180\nEnd\nEnd\nEnd".to_vec(),
        }).unwrap();
        let object = &file.declarations[0];
        let primary = load(object).unwrap().unwrap();
        let secondary = load_secondary(object).unwrap().unwrap();
        assert_eq!(primary.controlled_slots, 2);
        assert_eq!(secondary.controlled_slots, 5);
        assert_eq!(
            secondary.turn_rate.to_bits(),
            (primary.turn_rate * 2.0).to_bits()
        );
        assert!(linked(object).unwrap());
    }
}

pub(super) fn enter_guard(object: &Declaration) -> Result<bool, Box<dyn Error>> {
    match field(&object.lines, 0, "EnterGuard")
        .map(super::scalar)
        .transpose()?
    {
        Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
        Some(_) => Err("Invalid EnterGuard boolean".into()),
        None => Ok(false),
    }
}
