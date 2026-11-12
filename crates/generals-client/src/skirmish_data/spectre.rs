//! Spectre authored module data and source defaults, SH 2bfaea60.
use super::{blocks, definition, field, finite_number, scalar, source_numbers, weapons};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    WeaponRules,
    spectre::{CreationEdge, Decal, DeploymentRules, Rules},
};
use std::error::Error;

pub(super) fn gunship(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let mut selected = modules.into_iter().filter(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "SpectreGunshipUpdate")
    });
    let Some(module) = selected.next() else {
        return Ok(None);
    };
    if selected.next().is_some() {
        return Err(
            "multiple SpectreGunshipUpdate instances require separate native power bindings".into(),
        );
    }
    let power = name(module, "SpecialPowerTemplate", "")?;
    definition(files, "SpecialPower", &power)?;
    let gunner = name(module, "GattlingTemplateName", "")?;
    definition(files, "Object", &gunner)?;
    let howitzer = name(module, "HowitzerWeaponTemplate", "")?;
    let howitzer = super::instant_death::nullable(files, "Weapon", &howitzer)?
        .map(|declaration| {
            let mut rules = WeaponRules::unarmed();
            rules.name = howitzer;
            weapons::parse(&mut rules, &declaration.lines)?;
            Ok::<_, Box<dyn Error>>(rules)
        })
        .transpose()?;
    let firing_rate = duration(module, "HowitzerFiringRate", 10)?;
    if firing_rate == 0 {
        return Err("Spectre HowitzerFiringRate zero causes native division by zero".into());
    }
    let sounds = blocks(&object.lines, 0, "UnitSpecificSounds")?;
    let sound = |key| -> Result<String, Box<dyn Error>> {
        sounds
            .first()
            .map_or(Ok(String::new()), |lines| name(lines, key, ""))
    };
    Ok(Some(Rules {
        power,
        gunner,
        howitzer,
        orbit_frames: duration(module, "OrbitTime", 0)?,
        firing_rate,
        follow_lag: duration(module, "HowitzerFollowLag", 0)?,
        attack_radius: real(module, "AttackAreaRadius", 200.0)?,
        reticle_radius: real(module, "TargetingReticleRadius", 25.0)?,
        orbit_radius: real(module, "GunshipOrbitRadius", 250.0)?,
        strafe_increment: real(module, "StrafingIncrement", 20.0)?,
        insertion_slope: real(module, "OrbitInsertionSlope", 0.7)?,
        howitzer_offset: real(module, "RandomOffsetForHowitzer", 20.0)?,
        strafe_particle: name(module, "GattlingStrafeFXParticleSystem", "")?,
        decals: [
            decal(module, "AttackAreaDecal")?,
            decal(module, "TargetingReticleDecal")?,
        ],
        afterburner_sound: sound("Afterburner")?,
        howitzer_sound: sound("HowitzerFire")?,
        attack_voice: field(&object.lines, 0, "VoiceAttack")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
    }))
}

pub(super) fn deployments(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<DeploymentRules>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "SpectreGunshipDeploymentUpdate")
        })
        .map(|module| {
            let power = name(module, "SpecialPowerTemplate", "")?;
            definition(files, "SpecialPower", &power)?;
            let gunship = name(module, "GunshipTemplateName", "")?;
            definition(files, "Object", &gunship)?;
            let edge = match name(
                module,
                "CreateLocation",
                "CREATE_AT_EDGE_FARTHEST_FROM_TARGET",
            )?
            .as_str()
            {
                "CREATE_AT_EDGE_NEAR_SOURCE" => CreationEdge::NearSource,
                "CREATE_AT_EDGE_FARTHEST_FROM_SOURCE" => CreationEdge::FarSource,
                "CREATE_AT_EDGE_NEAR_TARGET" => CreationEdge::NearTarget,
                "CREATE_AT_EDGE_FARTHEST_FROM_TARGET" => CreationEdge::FarTarget,
                _ => return Err("invalid Spectre deployment CreateLocation".into()),
            };
            Ok(DeploymentRules {
                power,
                gunship,
                edge,
                required_science: name(module, "RequiredScience", "")?,
                attack_radius: real(module, "AttackAreaRadius", 200.0)?,
            })
        })
        .collect()
}

fn name(lines: &[ContentLine], key: &str, default: &str) -> Result<String, Box<dyn Error>> {
    Ok(field(lines, 1, key)
        .map(scalar)
        .transpose()?
        .unwrap_or(default)
        .into())
}
fn real(lines: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(lines, 1, key).map_or(Ok(default), |line| finite_number(scalar(line)?))
}
fn duration(lines: &[ContentLine], key: &str, default: u32) -> Result<u32, Box<dyn Error>> {
    field(lines, 1, key).map_or(Ok(default), |line| {
        source_numbers::duration_frames(scalar(line)?)
    })
}
fn decal(lines: &[ContentLine], key: &str) -> Result<Decal, Box<dyn Error>> {
    let mut result = Decal {
        texture: String::new(),
        opacity: [1.0; 2],
        throb_frames: 30,
        color: [0; 4],
        only_owner: true,
    };
    for module in blocks(lines, 1, key)? {
        for line in module.iter().filter(|line| line.depth == Some(2)) {
            match line.tokens[0].as_str() {
                "Texture" => result.texture = scalar(line)?.into(),
                "Style" if scalar(line)? == "SHADOW_ALPHA_DECAL" => {}
                "OpacityMin" => result.opacity[0] = percent(scalar(line)?)?,
                "OpacityMax" => result.opacity[1] = percent(scalar(line)?)?,
                "OpacityThrobTime" => {
                    result.throb_frames = source_numbers::duration_frames(scalar(line)?)?;
                }
                "OnlyVisibleToOwningPlayer" => {
                    result.only_owner = match scalar(line)? {
                        value if value.eq_ignore_ascii_case("Yes") => true,
                        value if value.eq_ignore_ascii_case("No") => false,
                        _ => return Err("invalid Spectre decal owner visibility".into()),
                    }
                }
                "Color" => {
                    let tokens: Vec<_> = line.tokens[1..]
                        .iter()
                        .flat_map(|token| token.split(':'))
                        .filter(|token| !token.is_empty())
                        .collect();
                    if !matches!(tokens.len(), 6 | 8) {
                        return Err("invalid Spectre decal color".into());
                    }
                    result.color = [0, 0, 0, 255];
                    for (index, pair) in tokens.chunks_exact(2).enumerate() {
                        if !pair[0].eq_ignore_ascii_case(["R", "G", "B", "A"][index]) {
                            return Err("invalid Spectre decal color order".into());
                        }
                        result.color[index] = pair[1].parse()?;
                    }
                }
                field => return Err(format!("unsupported Spectre decal field {field}").into()),
            }
        }
    }
    if result.throb_frames == 0 {
        return Err("zero Spectre decal throb duration".into());
    }
    Ok(result)
}
fn percent(value: &str) -> Result<f32, Box<dyn Error>> {
    Ok(finite_number(value.trim_end_matches('%'))? * 0.01)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    fn files(text: &str) -> Vec<ContentFile> {
        vec![
            content::scan(Asset {
                path: "spectre.ini".into(),
                origin: "spectre fixture".into(),
                bytes: text.as_bytes().to_vec(),
            })
            .unwrap(),
        ]
    }

    #[test]
    fn authored_durations_decal_color_and_independent_deployment_radius() {
        let files = files(
            "SpecialPower Spectre\nEnd\nObject Gunner\nEnd\nObject Ship\nBehavior = SpectreGunshipUpdate Main\nSpecialPowerTemplate = Spectre\nGattlingTemplateName = Gunner\nOrbitTime = 101ms\nHowitzerFiringRate = 250\nGunshipOrbitRadius = 375\nAttackAreaDecal\nTexture = SCCArtillery\nColor = R: 127 G:0 B:255\nOpacityThrobTime = 1000\nEnd\nEnd\nEnd\nObject Host\nBehavior = SpectreGunshipDeploymentUpdate Delivery\nSpecialPowerTemplate = Spectre\nGunshipTemplateName = Ship\nRequiredScience = SCIENCE_Spectre\nEnd\nEnd\n",
        );
        let ship = gunship(&files, definition(&files, "Object", "Ship").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(ship.orbit_frames, 4);
        assert_eq!(ship.firing_rate, 8);
        assert_eq!(ship.orbit_radius.to_bits(), 375.0_f32.to_bits());
        assert_eq!(ship.decals[0].color, [127, 0, 255, 255]);
        assert_eq!(ship.decals[0].throb_frames, 30);
        let hosts = deployments(&files, definition(&files, "Object", "Host").unwrap()).unwrap();
        assert!(matches!(hosts[0].edge, CreationEdge::FarTarget));
        assert_eq!(hosts[0].required_science, "SCIENCE_Spectre");
        // Deployment owns no orbit-radius parser field; its source allocation is zero.
        assert_eq!(hosts[0].attack_radius.to_bits(), 200.0_f32.to_bits());
    }
}
