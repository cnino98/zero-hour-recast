//! Installed primary rifle `FireFX`, including the original HEROIC override.
use super::{Rifle, TracerRules, WeaponFx};
use crate::skirmish_data::{blocks, definition, field, finite_number, fx_definition, scalar};
use generals_assets::{
    content::{self, ContentFile, ContentLine},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::UnitType;
use std::error::Error;

pub fn load(assets: &VirtualAssets, types: &[UnitType]) -> Result<WeaponFx, Box<dyn Error>> {
    let mut files = Vec::new();
    for stem in [
        "weapon",
        "fxlist",
        "object/system",
        "object/americainfantry",
        "object/chinainfantry",
        "object/glainfantry",
    ] {
        files.push(content::scan(
            assets.read(&format!("data/ini/{stem}.ini"))?,
        )?);
    }
    let mut rifles = Vec::new();
    for unit in types {
        let mut slots = (0..unit.weapon_template_count())
            .map(|_| None)
            .collect::<Vec<_>>();
        if matches!(
            unit.name.as_str(),
            "AmericaVehicleHumvee"
                | "AmericaVehicleSentryDrone"
                | "AmericaInfantryRanger"
                | "ChinaInfantryRedguard"
                | "GLAInfantryRebel"
                | "ChinaGattlingCannon"
                | "ChinaTankOverlordGattlingCannon"
                | "GLAVehicleQuadCannon"
                | "GLAVehicleTechnicalChassisOne"
                | "GLAVehicleTechnicalChassisTwo"
                | "GLAVehicleTechnicalChassisThree"
        ) {
            for (slot, output) in slots.iter_mut().enumerate() {
                let Some(rules) = unit
                    .weapon_template(slot)
                    .filter(|rules| !rules.name.is_empty())
                else {
                    continue;
                };
                if unit.name == "AmericaVehicleHumvee" && unit.weapon_context(slot).0 != 0 {
                    continue;
                }
                if (unit.name == "AmericaInfantryRanger"
                    || unit.name.starts_with("GLAVehicleTechnical"))
                    && slot != 0
                {
                    continue;
                }
                let weapon = &definition(&files, "Weapon", &rules.name)?.lines;
                let normal = scalar(field(weapon, 0, "FireFX").ok_or("rifle lacks FireFX")?)?;
                let heroic = field(weapon, 0, "VeterancyFireFX")
                    .map(|line| {
                        if line.tokens.len() != 3 || line.tokens[1] != "HEROIC" {
                            return Err("unsupported rifle VeterancyFireFX".into());
                        }
                        Ok::<_, Box<dyn Error>>(line.tokens[2].as_str())
                    })
                    .transpose()?
                    .unwrap_or(normal);
                *output = Some(Rifle {
                    normal: tracer(&files, normal)?,
                    heroic: tracer(&files, heroic)?,
                    weapon_speed: rules.speed * (1.0 / 30.0),
                });
            }
        }
        rifles.push(slots);
    }
    Ok(WeaponFx {
        ropes: super::rope::Ropes::new(),
        rifles,
        pending: Vec::new(),
        recorded: None,
    })
}

fn tracer(files: &[ContentFile], name: &str) -> Result<TracerRules, Box<dyn Error>> {
    let fx = fx_definition(files, name)?.ok_or_else(|| format!("missing FXList {name}"))?;
    let tracers = blocks(&fx.lines, 0, "Tracer")?;
    if tracers.len() != 1 {
        return Err(format!("{name}: expected one authored tracer").into());
    }
    let lines = tracers[0];
    let template = field(lines, 1, "TracerName")
        .map(scalar)
        .transpose()?
        .unwrap_or("GenericTracer");
    let object = definition(files, "Object", template)?;
    let draw = blocks(&object.lines, 0, "Draw")?;
    if draw.len() != 1
        || draw[0][0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "W3DTracerDraw")
    {
        return Err(format!("{template}: unsupported tracer draw module").into());
    }
    rules(lines)
}

fn rules(lines: &[ContentLine]) -> Result<TracerRules, Box<dyn Error>> {
    let rules = TracerRules {
        speed: number(lines, 1, "Speed", 0.0)? * (1.0 / 30.0),
        decay_at: number(lines, 1, "DecayAt", 1.0)?,
        length: number(lines, 1, "Length", 10.0)?,
        width: number(lines, 1, "Width", 1.0)?,
        probability: number(lines, 1, "Probability", 1.0)?,
        color: field(lines, 1, "Color")
            .map(color)
            .transpose()?
            .unwrap_or([1.0; 3]),
    };
    if rules.speed < 0.0
        || rules.decay_at <= 0.0
        || rules.length <= 0.0
        || rules.width <= 0.0
        || !(0.0..=1.0).contains(&rules.probability)
    {
        return Err("unsupported nonpositive tracer dimensions/speed/decay or probability".into());
    }
    Ok(rules)
}

fn number(
    lines: &[ContentLine],
    depth: u32,
    key: &str,
    default: f32,
) -> Result<f32, Box<dyn Error>> {
    field(lines, depth, key)
        .map(|line| finite_number(scalar(line)?))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn color(line: &ContentLine) -> Result<[f32; 3], Box<dyn Error>> {
    // INI::parseRGBColor uses colon as a separator and divides bytes by 255.
    let values = line.tokens[1..]
        .iter()
        .flat_map(|token| token.split(':'))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if values.len() != 6 || values[0] != "R" || values[2] != "G" || values[4] != "B" {
        return Err("invalid tracer RGB color".into());
    }
    let mut result = [0.0; 3];
    for (index, value) in result.iter_mut().enumerate() {
        *value = f32::from(values[index * 2 + 1].parse::<u8>()?) / 255.0;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::rules;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn authored_tracer_scales_velocity_and_rgb_but_not_dimensions() {
        let file = content::scan(Asset {
            path: "fxlist.ini".into(), origin: "fixture".into(),
            bytes: b"FXList Rifle\nTracer\nSpeed=600\nLength=10\nWidth=0.3\nColor=R:255 G:128 B:0\nProbability=0.5\nDecayAt=1\nEnd\nEnd".to_vec(),
        }).unwrap();
        let rules = rules(&file.declarations[0].lines).unwrap();
        assert_eq!(rules.speed.to_bits(), (600.0_f32 * (1.0 / 30.0)).to_bits());
        assert_eq!(rules.width.to_bits(), 0.3_f32.to_bits());
        assert_eq!(rules.length.to_bits(), 10.0_f32.to_bits());
        assert_eq!(
            rules.color.map(f32::to_bits),
            [1.0_f32, 128.0 / 255.0, 0.0].map(f32::to_bits)
        );
    }

    #[test]
    fn malformed_tracer_cannot_enter_nonfinite_render_lifecycle() {
        for input in [
            "Width=-1",
            "Speed=NaN",
            "Probability=1.1",
            "Color=R:256 G:0 B:0",
        ] {
            let file = content::scan(Asset {
                path: "fxlist.ini".into(),
                origin: "fixture".into(),
                bytes: format!("FXList Rifle\nTracer\n{input}\nEnd\nEnd").into_bytes(),
            })
            .unwrap();
            assert!(rules(&file.declarations[0].lines).is_err());
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn researched_sentry_keeps_authored_tracer_while_empty_weapon_has_none() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let game = &battle.simulation;
        let index = game
            .catalog
            .iter()
            .position(|unit| unit.name == "AmericaVehicleSentryDrone")
            .unwrap();
        let effects = super::load(&assets, &game.catalog).unwrap();
        let unit = &game.catalog[index];
        let mut found = false;
        for (weapon, tracer) in effects.rifles[index].iter().enumerate() {
            if unit
                .weapon_template(weapon)
                .is_some_and(|rules| rules.name == "SentryDroneGun")
            {
                let tracer = tracer.as_ref().expect("original researched gun tracer");
                assert_eq!(tracer.normal.probability.to_bits(), 0.25_f32.to_bits());
                assert_eq!(tracer.normal.width.to_bits(), 0.3_f32.to_bits());
                assert_eq!(
                    tracer.normal.speed.to_bits(),
                    (300.0_f32 * (1.0 / 30.0)).to_bits()
                );
                found = true;
            } else {
                assert!(tracer.is_none(), "empty weapon must not emit a tracer");
            }
        }
        assert!(found);
    }
}
