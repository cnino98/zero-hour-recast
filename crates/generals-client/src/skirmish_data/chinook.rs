use super::{blocks, definition, field, locomotor, optional_number, scalar, selected_pair};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::chinook::{DropRules, Rules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
    normal: &[ContentLine],
) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some(module) = modules.into_iter().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "ChinookAIUpdate")
    }) else {
        return Ok(None);
    };
    let globals = &definition(files, "GameData", "")?.lines;
    let taxi = selected_pair(&object.lines, 0, "Locomotor", "SET_TAXIING")?
        .ok_or("Chinook lacks taxi locomotor")?;
    let taxi = &definition(files, "Locomotor", taxi)?.lines;
    let z = field(normal, 0, "ZAxisBehavior")
        .map(scalar)
        .transpose()?
        .unwrap_or("NO_Z_MOTIVE_FORCE");
    let friction = field(normal, 0, "Apply2DFrictionWhenAirborne")
        .map(scalar)
        .transpose()?
        .unwrap_or("No");
    if z != "SURFACE_RELATIVE_HEIGHT" || !friction.eq_ignore_ascii_case("Yes") {
        return Err("Chinook Z behavior or airborne friction is not translated".into());
    }
    let scale = (1.0_f32 / 30.0) * (1.0_f32 / 30.0);
    let lift = optional_number(normal, "Lift", 0.0)?;
    let damage = locomotor::signed_real(normal, 0, "LiftDamaged", -1.0)?;
    let gravity = if field(globals, 0, "Gravity").is_some() {
        locomotor::signed_real(globals, 0, "Gravity", -1.0)? * scale
    } else {
        -1.0
    };
    let rules = Rules {
        lift: lift * scale,
        lift_damaged: if damage < 0.0 { lift } else { damage } * scale,
        gravity,
        damping: optional_number(normal, "PreferredHeightDamping", 1.0)?,
        speed_limit_z: if field(normal, 0, "SpeedLimitZ").is_some() {
            optional_number(normal, "SpeedLimitZ", 0.0)? * (1.0 / 30.0)
        } else {
            999_999.0
        },
        taxi: locomotor::load(taxi, globals, &object.lines)?,
        taxi_speed: optional_number(taxi, "Speed", 0.0)?,
        drop: drop_rules(module, gravity)?,
        rotor_wash: name(module, "RotorWashParticleSystem", "")?,
        upgraded_supply_boost: field(module, 1, "UpgradedSupplyBoost").map_or(Ok(0), |line| {
            scalar(line)?
                .parse::<i32>()
                .map_err(Into::<Box<dyn Error>>::into)
        })?,
    };
    if rules.lift <= 0.0
        || rules.lift_damaged <= 0.0
        || rules.gravity >= 0.0
        || rules.damping <= 0.0
        || rules.taxi.ground.is_none()
    {
        return Err("unsupported Chinook lift/taxi configuration".into());
    }
    let rope = definition(files, "Object", &rules.drop.rope_name)?;
    let draws = blocks(&rope.lines, 0, "Draw")?;
    if draws.len() != 1
        || draws[0][0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "W3DRopeDraw")
        || !blocks(&rope.lines, 0, "ClientUpdate")?.is_empty()
    {
        return Err(format!("{}: unsupported combat-drop drawable modules", rope.name).into());
    }
    Ok(Some(rules))
}

/// `ThingTemplate`'s `UnitSpecificFX` lookup used by `AIRappelState`.
pub(super) fn combat_drop_fx(object: &Declaration) -> Result<Option<String>, Box<dyn Error>> {
    let effects = blocks(&object.lines, 0, "UnitSpecificFX")?;
    effects
        .first()
        .and_then(|lines| field(lines, 1, "CombatDropKillFX"))
        .map(|line| scalar(line).map(str::to_owned))
        .transpose()
}

fn name(lines: &[ContentLine], key: &str, default: &str) -> Result<String, Box<dyn Error>> {
    Ok(field(lines, 1, key)
        .map(scalar)
        .transpose()?
        .unwrap_or(default)
        .into())
}

fn drop_rules(lines: &[ContentLine], gravity: f32) -> Result<DropRules, Box<dyn Error>> {
    let real = |key, default| locomotor::signed_real(lines, 1, key, default);
    let velocity = |key, default| {
        if field(lines, 1, key).is_some() {
            Ok(real(key, 0.0)? * (1.0 / 30.0))
        } else {
            Ok::<_, Box<dyn Error>>(default)
        }
    };
    let duration = |key| field(lines, 1, key).map_or(Ok(0x7fff_ffff), super::duration_frames);
    let color = if let Some(line) = field(lines, 1, "RopeColor") {
        let mut rgb = [0.0; 3];
        for (index, key) in ["R:", "G:", "B:"].into_iter().enumerate() {
            let value = line
                .tokens
                .get(index + 1)
                .and_then(|token| token.strip_prefix(key))
                .ok_or("invalid RopeColor component")?
                .parse::<u8>()?;
            rgb[index] = f32::from(value) / 255.0;
        }
        rgb
    } else {
        [0.9, 0.8, 0.7]
    };
    let rules = DropRules {
        rope_name: name(lines, "RopeName", "GenericRope")?,
        ropes: field(lines, 1, "NumRopes").map_or(Ok(4), |line| {
            scalar(line)?
                .parse::<u32>()
                .map_err(Into::<Box<dyn Error>>::into)
        })?,
        width: real("RopeWidth", 0.5)?,
        color,
        wobble_length: real("RopeWobbleLen", 10.0)?,
        wobble_amplitude: real("RopeWobbleAmplitude", 1.0)?,
        wobble_rate: if field(lines, 1, "RopeWobbleRate").is_some() {
            real("RopeWobbleRate", 0.0)? * (std::f32::consts::PI / 180.0) * (1.0 / 30.0)
        } else {
            0.1
        },
        final_height: real("RopeFinalHeight", 0.0)?,
        drop_speed: velocity("RopeDropSpeed", 1e10)?,
        rappel_speed: velocity("RappelSpeed", gravity.abs() * 30.0 * 0.5)?,
        delay: [duration("PerRopeDelayMin")?, duration("PerRopeDelayMax")?],
        min_height: real("MinDropHeight", 30.0)?,
        wait_for_ropes: match name(lines, "WaitForRopesToDrop", "Yes")?
            .to_ascii_lowercase()
            .as_str()
        {
            "yes" => true,
            "no" => false,
            _ => return Err("invalid WaitForRopesToDrop".into()),
        },
    };
    if rules.delay[0] > rules.delay[1] {
        return Err("reversed Chinook rope delay interval".into());
    }
    Ok(rules)
}

/// Drawable templates need resolved module overrides but are not `GameLogic` objects.
pub(super) fn rope_templates(
    files: &[ContentFile],
    selected: &[&str],
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names = Vec::new();
    for selected in selected {
        let object = definition(files, "Object", selected)?;
        for module in blocks(&object.lines, 0, "Behavior")? {
            if module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "ChinookAIUpdate")
            {
                let rope = name(module, "RopeName", "GenericRope")?;
                definition(files, "Object", &rope)?;
                if !names.contains(&rope) {
                    names.push(rope);
                }
            }
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_drop_fields_keep_authored_units_and_default_client_wobble() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset { path: "chinook.ini".into(), origin: "native field regression".into(), bytes: b"Object Test\n Behavior = ChinookAIUpdate Tag\n RopeDropSpeed = 60\n RappelSpeed = 30\n RopeWidth = 2\n RopeColor = R:255 G:128 B:0\n PerRopeDelayMin = 100\n PerRopeDelayMax = 200\n End\nEnd\n".to_vec() }).unwrap();
        let module = blocks(&file.declarations[0].lines, 0, "Behavior").unwrap();
        let rules = drop_rules(module[0], -64.0 / 900.0).unwrap();
        assert_eq!(
            rules.drop_speed.to_bits(),
            (60.0_f32 * (1.0 / 30.0)).to_bits()
        );
        assert_eq!(
            rules.rappel_speed.to_bits(),
            (30.0_f32 * (1.0 / 30.0)).to_bits()
        );
        assert_eq!(
            rules.color.map(f32::to_bits),
            [1.0_f32, 128.0 / 255.0, 0.0].map(f32::to_bits)
        );
        assert_eq!(rules.delay, [3, 6]);
        assert_eq!(rules.wobble_rate.to_bits(), 0.1_f32.to_bits());
    }
}
