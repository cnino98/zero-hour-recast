use std::{error::Error, f32::consts::PI};

use generals_assets::content::ContentLine;
use generals_sim::skirmish::{GroundAppearance, GroundRules, Locomotor};

use super::{blocks, field, has_flag, optional_number, scalar};

mod draw;
mod flight;

const FRAME_SECONDS: f32 = 1.0 / 30.0;
const ACCELERATION_SCALE: f32 = FRAME_SECONDS * FRAME_SECONDS;

// AIUpdateModuleData::parseLocomotorSet keeps every name in authored order.
pub fn normal_set(lines: &[ContentLine]) -> Result<Vec<&str>, Box<dyn Error>> {
    named_set(lines, "SET_NORMAL")
}

pub(super) fn named_set<'a>(
    lines: &'a [ContentLine],
    set_name: &str,
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut found = None;
    for line in lines {
        if line.depth == Some(0)
            && line.tokens.first().is_some_and(|key| key == "Locomotor")
            && line.tokens.get(1).is_some_and(|set| set == set_name)
        {
            if found.is_some() || line.tokens.len() < 3 {
                return Err(format!("ambiguous Locomotor {set_name} declaration").into());
            }
            found = Some(
                line.tokens
                    .iter()
                    .skip(2)
                    .filter(|name| !name.eq_ignore_ascii_case("None"))
                    .map(String::as_str)
                    .collect(),
            );
        }
    }
    Ok(found.unwrap_or_default())
}

pub(super) const fn stationary() -> Locomotor {
    Locomotor {
        move_priority: 1,
        wander_length: None,
        wander_about_point_radius: 0.0,
        wander_width: 0.0,
        stick_to_ground: false,
        friction_when_airborne: false,
        works_when_dead: false,
        flight: None,
        legal_surfaces: 0,
        airborne_targeting_height: 2_147_483_648.0,
        ground: None,
        speed_damaged: 0.0,
        turn_rate: 0.0,
        turn_rate_damaged: 0.0,
        turn_pivot_offset: 0.0,
        damage_health_threshold: 0.0,
    }
}

pub(super) fn load(
    lines: &[ContentLine],
    globals: &[ContentLine],
    object: &[ContentLine],
) -> Result<Locomotor, Box<dyn Error>> {
    // GameCommon::ConvertAngularVelocityInDegreesPerSecToRadsPerFrame.
    const ANGULAR_SCALE: f32 = (1.0 / 30.0) * (PI / 180.0);
    let speed = optional_number(lines, "Speed", 0.0)?;
    let turn = optional_number(lines, "TurnRate", 0.0)?;
    Ok(Locomotor {
        move_priority: movement_priority(lines)?,
        wander_length: Some(source_real(lines, "WanderLengthFactor", 1.0)?),
        wander_about_point_radius: source_real(lines, "WanderAboutPointRadius", 0.0)?,
        wander_width: source_real(lines, "WanderWidthFactor", 0.0)?,
        stick_to_ground: boolean(lines, "StickToGround")?,
        friction_when_airborne: boolean(lines, "Apply2DFrictionWhenAirborne")?,
        works_when_dead: boolean(lines, "LocomotorWorksWhenDead")?,
        flight: flight::load(lines, globals, object)?,
        legal_surfaces: super::locomotor_surface_mask(lines)?,
        airborne_targeting_height: targeting_height(lines)?,
        ground: ground_rules(lines, object)?,
        speed_damaged: damaged_value(lines, "SpeedDamaged", speed)?,
        turn_rate: turn * ANGULAR_SCALE,
        turn_rate_damaged: damaged_value(lines, "TurnRateDamaged", turn)? * ANGULAR_SCALE,
        turn_pivot_offset: source_real(lines, "TurnPivotOffset", 0.0)?,
        damage_health_threshold: penalty_threshold(globals)?,
    })
}

fn movement_priority(lines: &[ContentLine]) -> Result<i32, Box<dyn Error>> {
    match field(lines, 0, "GroupMovementPriority")
        .map(scalar)
        .transpose()?
        .unwrap_or("MOVES_MIDDLE")
    {
        "MOVES_BACK" => Ok(0),
        "MOVES_MIDDLE" => Ok(1),
        "MOVES_FRONT" => Ok(2),
        _ => Err("Unsupported locomotor GroupMovementPriority".into()),
    }
}

fn targeting_height(lines: &[ContentLine]) -> Result<f32, Box<dyn Error>> {
    let Some(line) = field(lines, 0, "AirborneTargetingHeight") else {
        // Native INT_MAX converts to this Real value in the height comparison.
        return Ok(2_147_483_648.0);
    };
    let text = scalar(line)?;
    // Validate the integer INI field, then retain its Real comparison value.
    text.parse::<i32>()?;
    Ok(text.parse::<f32>()?)
}

fn ground_rules(
    lines: &[ContentLine],
    object: &[ContentLine],
) -> Result<Option<GroundRules>, Box<dyn Error>> {
    let appearance = field(lines, 0, "Appearance")
        .map(scalar)
        .transpose()?
        .unwrap_or("OTHER");
    let appearance = match appearance {
        "TWO_LEGS" => GroundAppearance::Legs,
        "CLIMBER" => GroundAppearance::Climber,
        "TREADS" => GroundAppearance::Treads,
        "FOUR_WHEELS" => GroundAppearance::Wheels {
            min_turn_speed: per_frame(lines, 0, "MinTurnSpeed", 99_999.0, FRAME_SECONDS)?,
            can_move_backwards: boolean(lines, "CanMoveBackwards")?,
        },
        // HOVER/WINGS use their separate force integration; OTHER has no ground rules.
        "HOVER" | "WINGS" | "THRUST" | "OTHER" => return Ok(None),
        _ => return Err(format!("Unsupported locomotor Appearance {appearance}").into()),
    };
    let z_behavior = field(lines, 0, "ZAxisBehavior")
        .map(scalar)
        .transpose()?
        .unwrap_or("NO_Z_MOTIVE_FORCE");
    if z_behavior != "NO_Z_MOTIVE_FORCE"
        || optional_number(lines, "MinSpeed", 0.0)? != 0.0
        || boolean(lines, "CloseEnoughDist3D")?
    {
        return Err("Selected ground locomotor requires unported Z or minimum-speed motion".into());
    }
    let modules = blocks(object, 0, "Behavior")?;
    let mut candidates = modules.into_iter().filter(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "PhysicsBehavior")
    });
    let physics = candidates
        .next()
        .ok_or("Ground locomotor lacks PhysicsBehavior")?;
    if candidates.next().is_some() {
        return Err("Ambiguous ground PhysicsBehavior".into());
    }
    let acceleration = optional_number(lines, "Acceleration", 0.0)?;
    let width = optional_number(lines, "WanderWidthFactor", 0.0)?;
    let rules = GroundRules {
        wander: if width == 0.0 {
            None
        } else {
            Some([width, optional_number(lines, "WanderLengthFactor", 1.0)?])
        },
        appearance,
        is_dozer: has_flag(object, "KindOf", "DOZER"),
        downhill_only: boolean(lines, "DownhillOnly")?,
        acceleration: acceleration * ACCELERATION_SCALE,
        acceleration_damaged: damaged_value(lines, "AccelerationDamaged", acceleration)?
            * ACCELERATION_SCALE,
        braking: per_frame(lines, 0, "Braking", 99_999.0, ACCELERATION_SCALE)?,
        min_speed: 0.0,
        close_enough: optional_number(lines, "CloseEnoughDist", 1.0)?,
        mass: signed_real(physics, 1, "Mass", 1.0)?,
        forward_friction: per_frame(physics, 1, "ForwardFriction", 0.15, FRAME_SECONDS)?,
        lateral_friction: per_frame(physics, 1, "LateralFriction", 0.15, FRAME_SECONDS)?,
        extra_friction: per_frame(lines, 0, "Extra2DFriction", 0.0, FRAME_SECONDS)?,
    };
    if rules.mass <= 0.0
        || rules.acceleration <= 0.0
        || rules.acceleration_damaged <= 0.0
        || rules.braking <= 0.0
        || rules.close_enough <= 0.0
    {
        return Err("Unsupported nonpositive ground locomotor or physics value".into());
    }
    Ok(Some(rules))
}

pub(super) fn boolean(lines: &[ContentLine], key: &str) -> Result<bool, Box<dyn Error>> {
    let Some(line) = field(lines, 0, key) else {
        return Ok(false);
    };
    let value = scalar(line)?;
    if value.eq_ignore_ascii_case("Yes") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("No") {
        Ok(false)
    } else {
        Err(format!("Invalid locomotor {key} boolean {value}").into())
    }
}

// Constructor defaults are already in frame units; only INI overrides convert.
pub(super) fn per_frame(
    lines: &[ContentLine],
    depth: u32,
    key: &str,
    default: f32,
    scale: f32,
) -> Result<f32, Box<dyn Error>> {
    if field(lines, depth, key).is_none() {
        return Ok(default);
    }
    Ok(signed_real(lines, depth, key, default)? * scale)
}

// LocomotorTemplate::validate copies pristine values for any negative sentinel.
fn damaged_value(lines: &[ContentLine], key: &str, pristine: f32) -> Result<f32, Box<dyn Error>> {
    let value = source_real(lines, key, -1.0)?;
    Ok(if value < 0.0 { pristine } else { value })
}

fn source_real(lines: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    signed_real(lines, 0, key, default)
}

pub fn signed_real(
    lines: &[ContentLine],
    depth: u32,
    key: &str,
    default: f32,
) -> Result<f32, Box<dyn Error>> {
    let Some(line) = field(lines, depth, key) else {
        return Ok(default);
    };
    let text = scalar(line)?;
    let value = text
        .strip_suffix(['f', 'F'])
        .unwrap_or(text)
        .parse::<f32>()?;
    if !value.is_finite() {
        return Err(format!("non-finite locomotor {key}").into());
    }
    Ok(value)
}

fn penalty_threshold(globals: &[ContentLine]) -> Result<f32, Box<dyn Error>> {
    // GlobalData constructor defaults differ from the installed INI overrides.
    let damaged = optional_number(globals, "UnitDamagedThreshold", 0.5)?;
    let really_damaged = optional_number(globals, "UnitReallyDamagedThreshold", 0.1)?;
    if damaged > 1.0 || really_damaged > damaged {
        return Err("Unsupported body damage thresholds".into());
    }
    let state = field(globals, 0, "MovementPenaltyDamageState")
        .map(scalar)
        .transpose()?
        .unwrap_or("REALLYDAMAGED");
    if state.eq_ignore_ascii_case("PRISTINE") {
        Ok(1.0)
    } else if state.eq_ignore_ascii_case("DAMAGED") {
        Ok(damaged)
    } else if state.eq_ignore_ascii_case("REALLYDAMAGED") {
        Ok(really_damaged)
    } else if state.eq_ignore_ascii_case("RUBBLE") {
        Ok(0.0)
    } else {
        Err(format!("Unsupported MovementPenaltyDamageState {state}").into())
    }
}

#[cfg(test)]
mod tests {
    use super::load;
    use generals_assets::{content, virtual_assets::Asset};

    #[ignore = "requires original game installation"]
    #[test]
    fn original_pilot_priority_and_spectre_effect_flight_sets_are_loaded() {
        use crate::skirmish_data::{definition, finish_type, source_files, unit_type};
        use generals_assets::virtual_assets::VirtualAssets;
        use generals_sim::skirmish::{Faction, Kind};
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = source_files(&assets).unwrap();
        let pilot = definition(&files, "Object", "AmericaInfantryPilot").unwrap();
        let pilot = finish_type(
            &files,
            pilot,
            unit_type(&files, pilot, Faction::Usa, Kind::Infantry).unwrap(),
        )
        .unwrap();
        assert_eq!(pilot.locomotor.move_priority, 0);
        let upgrade = pilot.maintenance.movement_upgrade.unwrap();
        assert_eq!(upgrade.locomotor.move_priority, 2);
        assert_eq!(upgrade.speed.to_bits(), 25.0_f32.to_bits());
        let ship = definition(&files, "Object", "AmericaJetSpectreGunship").unwrap();
        let ship = finish_type(
            &files,
            ship,
            unit_type(&files, ship, Faction::Usa, Kind::Effect).unwrap(),
        )
        .unwrap();
        assert!(ship.locomotor.works_when_dead);
        assert!(ship.locomotor.flight.is_some());
        assert_eq!(ship.locomotor.legal_surfaces, 8);
        assert_eq!(ship.speed.to_bits(), 120.0_f32.to_bits());
        assert_eq!(ship.locomotor_sets.panic.len(), 1);
        let transit = &ship.locomotor_sets.panic[0];
        assert!(transit.locomotor.works_when_dead);
        assert!(transit.locomotor.flight.is_some());
        assert_eq!(transit.speed.to_bits(), 290.0_f32.to_bits());
        assert_eq!(transit.locomotor.legal_surfaces, 8);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_saboteur_preserves_authored_locomotor_order_and_cliff_rules() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let objects =
            content::scan(assets.read("Data/INI/Object/GLAInfantry.ini").unwrap()).unwrap();
        let object = objects
            .declarations
            .iter()
            .find(|object| object.name == "GLAInfantrySaboteur")
            .unwrap();
        let names = super::normal_set(&object.lines).unwrap();
        assert_eq!(names, ["SaboteurGroundLocomotor", "SaboteurCliffLocomotor"]);
        let source = content::scan(assets.read("Data/INI/Locomotor.ini").unwrap()).unwrap();
        let files = [source];
        let ground = super::super::definition(&files, "Locomotor", names[0]).unwrap();
        let cliff = super::super::definition(&files, "Locomotor", names[1]).unwrap();
        let ground = load(&ground.lines, &[], &object.lines).unwrap();
        let cliff = load(&cliff.lines, &[], &object.lines).unwrap();
        assert_eq!(
            ground.wander_length.map(f32::to_bits),
            Some(1.0_f32.to_bits())
        );
        assert!(ground.ground.as_ref().unwrap().wander.is_none());
        assert_eq!(ground.legal_surfaces, 17);
        assert_eq!(cliff.legal_surfaces, 4);
        assert_eq!(ground.speed_damaged.to_bits(), 20.0_f32.to_bits());
        assert_eq!(cliff.speed_damaged.to_bits(), 15.0_f32.to_bits());
        assert!(matches!(
            cliff.ground.unwrap().appearance,
            super::GroundAppearance::Climber
        ));
        assert!(super::super::definition(&files, "Locomotor", "MissingSecondaryCliff").is_err());
    }

    #[test]
    fn locomotor_set_rejects_duplicate_declarations_but_keeps_all_names() {
        let file = content::scan(Asset {
            path: "Object.ini".into(),
            origin: "fixture".into(),
            bytes: b"Object Test\nLocomotor=SET_NORMAL Ground MissingSecondaryCliff\nEnd\n"
                .to_vec(),
        })
        .unwrap();
        let names = super::normal_set(&file.declarations[0].lines).unwrap();
        assert_eq!(names, ["Ground", "MissingSecondaryCliff"]);
        let file = content::scan(Asset {
            path: "Object.ini".into(),
            origin: "fixture".into(),
            bytes: b"Object Test\nLocomotor=SET_NORMAL Ground\nLocomotor=SET_NORMAL Cliff\nEnd\n"
                .to_vec(),
        })
        .unwrap();
        assert!(super::normal_set(&file.declarations[0].lines).is_err());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_panic_human_preserves_wander_width_length_and_ground_motion() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let file = content::scan(assets.read("Data/INI/Locomotor.ini").unwrap()).unwrap();
        let source = file
            .declarations
            .iter()
            .find(|declaration| declaration.name == "PanicHumanLocomotor")
            .unwrap();
        let objects = content::scan(assets.read("Data/INI/Object/System.ini").unwrap()).unwrap();
        let actor = objects
            .declarations
            .iter()
            .find(|declaration| declaration.name == "FlamingInfantry")
            .unwrap();
        let locomotor = load(&source.lines, &[], &actor.lines).unwrap();
        assert!(locomotor.flight.is_none());
        assert_eq!(
            locomotor.wander_length.map(f32::to_bits),
            Some(4.0_f32.to_bits())
        );
        let ground = locomotor.ground.unwrap();
        assert!(matches!(ground.appearance, super::GroundAppearance::Legs));
        assert_eq!(
            ground.wander.unwrap().map(f32::to_bits),
            [1.25_f32, 4.0].map(f32::to_bits)
        );
        assert_eq!(ground.mass.to_bits(), 5.0_f32.to_bits());
        assert_eq!(locomotor.speed_damaged.to_bits(), 10.0_f32.to_bits());
    }

    #[test]
    fn airborne_targeting_height_keeps_native_integer_default_and_override() {
        let file = content::scan(Asset {
            path: "Locomotor.ini".into(),
            origin: "Original airborne targeting threshold fixture".into(),
            bytes: b"Locomotor Default\nEnd\nLocomotor Hover\nAirborneTargetingHeight=10\nEnd\nLocomotor Invalid\nAirborneTargetingHeight=10.5\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            super::targeting_height(&file.declarations[0].lines)
                .unwrap()
                .to_bits(),
            2_147_483_648.0f32.to_bits()
        );
        assert_eq!(
            super::targeting_height(&file.declarations[1].lines)
                .unwrap()
                .to_bits(),
            10.0f32.to_bits()
        );
        assert!(super::targeting_height(&file.declarations[2].lines).is_err());
    }

    #[test]
    fn omitted_and_negative_damage_caps_inherit_and_signed_pivots_survive() {
        let file = content::scan(Asset {
            path: "locomotor.ini".into(),
            origin: "fixture".into(),
            bytes: b"Locomotor Test\nSpeed=30\nTurnRate=180\nTurnRateDamaged=-1\nTurnPivotOffset=-0.33\nEnd\nGameData\nUnitDamagedThreshold=0.7\nUnitReallyDamagedThreshold=0.35\nMovementPenaltyDamageState=REALLYDAMAGED\nEnd\n".to_vec(),
        })
        .unwrap();
        let caps = load(
            &file.declarations[0].lines,
            &file.declarations[1].lines,
            &[],
        )
        .unwrap();
        assert_eq!(caps.speed_damaged.to_bits(), 30.0_f32.to_bits());
        assert_eq!(caps.turn_rate_damaged.to_bits(), caps.turn_rate.to_bits());
        assert_eq!(caps.turn_pivot_offset.to_bits(), (-0.33_f32).to_bits());
        assert_eq!(caps.damage_health_threshold.to_bits(), 0.35_f32.to_bits());
        let defaults = load(&file.declarations[0].lines, &[], &[]).unwrap();
        assert_eq!(
            defaults.damage_health_threshold.to_bits(),
            0.1_f32.to_bits()
        );
    }

    #[test]
    fn invalid_damage_state_and_nonfinite_pivot_are_rejected() {
        for text in [
            "Locomotor Test\nSpeed=30\nTurnRate=180\nTurnPivotOffset=NaN\nEnd\nGameData\nEnd\n",
            "Locomotor Test\nSpeed=30\nTurnRate=180\nEnd\nGameData\nMovementPenaltyDamageState=UNKNOWN\nEnd\n",
        ] {
            let file = content::scan(Asset {
                path: "locomotor.ini".into(),
                origin: "fixture".into(),
                bytes: text.as_bytes().to_vec(),
            })
            .unwrap();
            assert!(
                load(
                    &file.declarations[0].lines,
                    &file.declarations[1].lines,
                    &[]
                )
                .is_err()
            );
        }
    }

    #[test]
    fn ground_physics_preserves_default_units_and_converts_explicit_overrides() {
        let file = content::scan(Asset {
            path: "motion.ini".into(),
            origin: "fixture".into(),
            bytes: b"Locomotor Test\nAppearance=TREADS\nSpeed=20\nTurnRate=60\nAcceleration=15\nAccelerationDamaged=-1\nExtra2DFriction=3\nEnd\nObject Tank\nKindOf=VEHICLE DOZER\nBehavior=PhysicsBehavior PhysicsTag\nMass=50\nLateralFriction=3\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let caps = load(
            &file.declarations[0].lines,
            &[],
            &file.declarations[1].lines,
        )
        .unwrap();
        let rules = caps.ground.unwrap();
        assert_eq!(rules.braking.to_bits(), 99_999.0_f32.to_bits());
        assert_eq!(rules.forward_friction.to_bits(), 0.15_f32.to_bits());
        assert_eq!(
            rules.lateral_friction.to_bits(),
            (3.0_f32 * (1.0 / 30.0)).to_bits()
        );
        assert_eq!(
            rules.extra_friction.to_bits(),
            rules.lateral_friction.to_bits()
        );
        assert_eq!(
            rules.acceleration_damaged.to_bits(),
            rules.acceleration.to_bits()
        );
        assert!((rules.acceleration - 0.016_666_668).abs() < 0.000_000_01);
        assert_eq!(rules.mass.to_bits(), 50.0_f32.to_bits());
        assert_eq!(rules.close_enough.to_bits(), 1.0_f32.to_bits());
        assert!(rules.is_dozer);
    }

    #[test]
    fn wheel_turn_speed_retains_native_default_and_signed_frame_conversion() {
        for (source, expected) in [
            ("", 99_999.0_f32),
            ("MinTurnSpeed=0", 0.0),
            ("MinTurnSpeed=-15", -15.0 * (1.0 / 30.0)),
            ("MinTurnSpeed=20", 20.0 * (1.0 / 30.0)),
        ] {
            let file = content::scan(Asset {
                path: "wheel.ini".into(),
                origin: "fixture".into(),
                bytes: format!("Locomotor Test\nAppearance=FOUR_WHEELS\nAcceleration=1000\n{source}\nCanMoveBackwards=Yes\nEnd\nObject Vehicle\nBehavior=PhysicsBehavior PhysicsTag\nMass=50\nEnd\nEnd\n").into_bytes(),
            }).unwrap();
            let rules = load(
                &file.declarations[0].lines,
                &[],
                &file.declarations[1].lines,
            )
            .unwrap()
            .ground
            .unwrap();
            let super::GroundAppearance::Wheels {
                min_turn_speed,
                can_move_backwards,
            } = rules.appearance
            else {
                panic!("FOUR_WHEELS must select the native wheel branch");
            };
            assert_eq!(min_turn_speed.to_bits(), expected.to_bits());
            assert!(can_move_backwards);
        }
    }

    #[test]
    fn a_selected_ground_locomotor_cannot_silently_use_a_3d_arrival_rule() {
        let file = content::scan(Asset {
            path: "motion.ini".into(),
            origin: "fixture".into(),
            bytes: b"Locomotor Test\nAppearance=TWO_LEGS\nSpeed=20\nTurnRate=500\nAcceleration=100\nCloseEnoughDist3D=Yes\nEnd\n".to_vec(),
        }).unwrap();
        assert!(load(&file.declarations[0].lines, &[], &[]).is_err());
    }
}
