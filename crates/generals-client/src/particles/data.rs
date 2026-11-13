use super::{
    AlphaKey, ColorKey, Drawing, Particles, Range, Style, Template, Texture, Velocity, Volume,
};
use crate::{
    skirmish_data::{definition, field, scalar},
    source_texture,
};
use generals_assets::{
    content::{self, ContentLine},
    virtual_assets::VirtualAssets,
};
use std::{collections::BTreeMap, error::Error};

const INITIAL_PARTICLES: [&str; 26] = [
    "GattlingCannonLenzflare",
    "HeroicGattlingCannonLenzflare",
    "FallingShells",
    "FallingShellsGattlingTilted",
    "MissileDefenderMissileExhaust",
    "HeroicMissileDefenderMissileExhaust",
    "MissileExhaust",
    "HeroicMissileExhaust",
    "InfantryStingerMissileLenzflare",
    "MissileLenzflare",
    "HeroicComancheMissileLenzflare",
    "MissileBackblast",
    "MissileBackblastClouds",
    "BuggyMissileExplosion",
    "BuggyMissileExplosionSmoke",
    "BuggyMissileExplosionLenzflare",
    "FlashBangBuildingFlash",
    "FlashBangBuildingSpark",
    "FlashBangBuildingSmoke",
    "FlashBangNormal",
    "FlashBangNormalSpark",
    "GenericLaserFlare",
    "DisabledEffectBinaryShower0",
    "DisabledEffectBinaryShower1",
    "SonicRange",
    "SonicRangeUpgraded",
];

pub fn load(
    assets: &VirtualAssets,
    types: &[generals_sim::skirmish::UnitType],
) -> Result<Particles, Box<dyn Error>> {
    load_with_position_effects(assets, types, &std::collections::BTreeSet::new())
}

pub fn load_with_position_effects(
    assets: &VirtualAssets,
    types: &[generals_sim::skirmish::UnitType],
    position_effects: &std::collections::BTreeSet<String>,
) -> Result<Particles, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/particlesystem.ini")?)?;
    let files = [file];
    let model_states = super::model_states::Pending::load(assets, types)?;
    let body_particles = super::body_particles::Pending::load(assets, &files)?;
    let incendiaries = super::fire::Pending::load(assets, types)?;
    let mut names: Vec<String> = INITIAL_PARTICLES.into_iter().map(str::to_owned).collect();
    for name in body_particles.names() {
        super::dependencies::append(&mut names, name);
    }
    for name in model_states
        .names()
        .chain(model_states.wheel_names())
        .chain(model_states.tank_names())
    {
        super::dependencies::append(&mut names, name);
    }
    for name in incendiaries.names() {
        super::dependencies::append(&mut names, name);
    }
    for name in super::detectors::names(types) {
        super::dependencies::append(&mut names, name);
    }
    for name in super::dependencies::weapon_particles(assets, types)? {
        super::dependencies::append(&mut names, &name);
    }
    for name in super::dependencies::position_particles(assets, position_effects)? {
        super::dependencies::append(&mut names, &name);
    }
    for repair in types
        .iter()
        .flat_map(|unit| &unit.behavior.slaves)
        .filter_map(|rules| rules.repair.as_ref())
    {
        super::dependencies::append(&mut names, &repair.welding_system);
    }
    append_module_particles(types, &files, &mut names);
    super::dependencies::slaves(&files, &mut names)?;
    let (templates, textures) = load_templates(assets, &files, &names)?;
    let detectors = super::detectors::load(types, &templates)?;
    let global = [content::scan(assets.read("data/ini/gamedata.ini")?)?];
    let scale = number(&definition(&global, "GameData", "")?.lines, "ParticleScale")?;
    if scale <= 0.0 {
        return Err("nonpositive ParticleScale".into());
    }
    let mut effects = super::effects::load(assets, types, &templates)?;
    super::actor_fx::load_position_effects(assets, position_effects, &templates, &mut effects)?;
    let model_states = model_states.resolve(&templates)?;
    let incendiaries = incendiaries.resolve(&templates)?;
    let body_particles = body_particles.resolve(&templates)?;
    Ok(Particles {
        templates,
        textures,
        emitters: Vec::new(),
        smudges: Vec::new(),
        object_emitters: super::object_effects::Attachments::new(),
        next_id: 0,
        generation: 0,
        frame: None,
        processed: None,
        advanced: None,
        phase: 0.0,
        scale,
        bindings: effects.bindings,
        bone_systems: std::collections::BTreeMap::new(),
        bodies: body_particles,
        detectors,
        model_states,
        fires: incendiaries,
        lists: effects.lists,
        propaganda: effects.propaganda,
        actor_fx: effects.actor_fx,
        sounds: Vec::new(),
        pulses: crate::light_pulse::Pool::new(),
        scorches: crate::scorch::Scorches::load(assets)?,
        repair_sound: crate::source_audio::repair_sound(assets)?,
        shake: super::view_shake::Shake::load(&definition(&global, "GameData", "")?.lines)?,
        reported_streak: std::collections::BTreeSet::new(),
    })
}

fn append_module_particles(
    types: &[generals_sim::skirmish::UnitType],
    files: &[content::ContentFile],
    names: &mut Vec<String>,
) {
    for rules in types.iter().filter_map(|unit| unit.laser_update.as_ref()) {
        for name in &rules.particles {
            // LaserUpdate stores raw names; missing particle templates are null.
            if files
                .iter()
                .flat_map(|file| &file.declarations)
                .any(|entry| entry.kind == "ParticleSystem" && entry.name == *name)
            {
                super::dependencies::append(names, name);
            }
        }
    }
    for rules in types.iter().filter_map(|unit| unit.spectre.as_ref()) {
        if files
            .iter()
            .flat_map(|file| &file.declarations)
            .any(|entry| entry.kind == "ParticleSystem" && entry.name == rules.strafe_particle)
        {
            super::dependencies::append(names, &rules.strafe_particle);
        }
    }
    for rules in types.iter().filter_map(|unit| unit.chinook.as_ref()) {
        if files
            .iter()
            .flat_map(|file| &file.declarations)
            .any(|entry| entry.kind == "ParticleSystem" && entry.name == rules.rotor_wash)
        {
            super::dependencies::append(names, &rules.rotor_wash);
        }
    }
}

fn load_templates(
    assets: &VirtualAssets,
    files: &[content::ContentFile],
    names: &[String],
) -> Result<(Vec<Template>, Vec<Texture>), Box<dyn Error>> {
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut templates = Vec::new();
    let mut textures = Vec::new();
    let mut loaded = BTreeMap::new();
    let mut failures = Vec::new();
    for &name in &names {
        let lines = &definition(files, "ParticleSystem", name)?.lines;
        let texture_name = text(lines, "ParticleName")?;
        let texture = if let Some(&index) = loaded.get(texture_name) {
            index
        } else {
            let index = textures.len();
            textures.push(Texture {
                image: if matches!(style(lines)?, Style::Smudge) {
                    None
                } else {
                    Some(source_texture::load(assets, texture_name)?)
                },
                handle: None,
                materials: [None, None, None],
            });
            loaded.insert(texture_name.to_owned(), index);
            index
        };
        match template(lines, name, texture, &names) {
            Ok(template) => templates.push(template),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("\n").into());
    }
    Ok((templates, textures))
}

fn template(
    lines: &[ContentLine],
    name: &str,
    texture: usize,
    names: &[&str],
) -> Result<Template, Box<dyn Error>> {
    // ParticleSys.cpp parses/copies/saves m_isOneShot but never reads it in
    // update or emission. FallingShells=Yes still uses SystemLifetime=1.
    let _one_shot = boolean(lines, "IsOneShot")?;
    let shader = match text(lines, "Shader")? {
        "ADDITIVE" => super::Shader::Additive,
        "ALPHA" => super::Shader::Alpha,
        "ALPHA_TEST" => super::Shader::AlphaTest,
        value => return Err(format!("unsupported particle Shader {value}").into()),
    };
    let style = style(lines)?;
    let slave = linked_template(lines, "SlaveSystem", names)?;
    let attached = linked_template(lines, "PerParticleAttachedSystem", names)?;
    let (alpha, color) = keyframes(lines)?;
    Ok(Template {
        name: name.into(),
        texture,
        drawing: Drawing {
            style,
            shader,
            ground_aligned: boolean(lines, "IsGroundAligned")?,
        },
        emit_above_ground_only: boolean(lines, "IsEmitAboveGroundOnly")?,
        up_towards_emitter: boolean(lines, "IsParticleUpTowardsEmitter")?,
        lifetime: bounded_range(lines, "Lifetime", 0.0, 65535.0)?,
        system_lifetime: bounded_number(lines, "SystemLifetime", 0.0, 65535.0)?.trunc(),
        initial_delay: bounded_range(lines, "InitialDelay", 0.0, 16_777_216.0)?,
        burst_delay: bounded_range(lines, "BurstDelay", 0.0, 16_777_216.0)?,
        burst_count: bounded_range(lines, "BurstCount", 0.0, 4096.0)?,
        volume: volume(lines)?,
        velocity: velocity(lines)?,
        velocity_damping: range(lines, "VelocityDamping")?,
        angle: range(lines, "AngleZ")?,
        angular_rate: range(lines, "AngularRateZ")?,
        angular_damping: range(lines, "AngularDamping")?,
        gravity: number(lines, "Gravity")?,
        drift: coordinate(values(lines, "DriftVelocity")?)?,
        size: range(lines, "Size")?,
        start_size_rate: range(lines, "StartSizeRate")?,
        size_rate: range(lines, "SizeRate")?,
        size_rate_damping: range(lines, "SizeRateDamping")?,
        alpha,
        color,
        color_scale: range(lines, "ColorScale")?,
        slave,
        attached,
        slave_offset: field(lines, 0, "SlavePosOffset")
            .map_or(Ok([0.0; 3]), |line| coordinate(&line.tokens[1..]))?,
        wind_motion: super::wind::Motion::parse(text(lines, "WindMotion")?)?,
        wind_change: Range {
            low: number(lines, "WindAngleChangeMin")?,
            high: number(lines, "WindAngleChangeMax")?,
        },
        wind_start: Range {
            low: number(lines, "WindPingPongStartAngleMin")?,
            high: number(lines, "WindPingPongStartAngleMax")?,
        },
        wind_end: Range {
            low: number(lines, "WindPingPongEndAngleMin")?,
            high: number(lines, "WindPingPongEndAngleMax")?,
        },
    })
}

fn style(lines: &[ContentLine]) -> Result<Style, Box<dyn Error>> {
    // PRESERVE_RETAIL_PARTICLES=1: ParticleSystemInfo::validate promotes SMUDGE.*.
    if field(lines, 0, "ParticleName")
        .map(scalar)
        .transpose()?
        .is_some_and(|name| {
            name.get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("SMUDGE."))
        })
    {
        return Ok(Style::Smudge);
    }
    match text(lines, "Type")? {
        "SMUDGE" => Ok(Style::Smudge),
        "PARTICLE" => Ok(Style::Billboard),
        "STREAK" => Ok(Style::Streak),
        "VOLUME_PARTICLE" => {
            let depth = if let Some(line) = field(lines, 0, "VolParticleDepth") {
                scalar(line)?.parse::<u32>()?
            } else {
                0
            };
            // ParticleSystemTemplate::validate defaults zero to six;
            // PointGroupClass::RenderVolumeParticle caps the rendered depth.
            let depth = if depth == 0 { 6 } else { depth.min(16) };
            Ok(Style::Volume(u8::try_from(depth)?))
        }
        value => Err(format!("unsupported particle Type {value}").into()),
    }
}

fn linked_template(
    lines: &[ContentLine],
    key: &str,
    names: &[&str],
) -> Result<Option<usize>, Box<dyn Error>> {
    let Some(line) = field(lines, 0, key) else {
        return Ok(None);
    };
    let name = scalar(line)?;
    if name.eq_ignore_ascii_case("NONE") {
        return Ok(None);
    }
    Ok(names.iter().position(|candidate| *candidate == name))
}

fn keyframes(lines: &[ContentLine]) -> Result<([AlphaKey; 8], [ColorKey; 8]), Box<dyn Error>> {
    let mut alpha = Vec::new();
    let mut color = Vec::new();
    for index in 1..=8 {
        let a = values(lines, &format!("Alpha{index}"))?;
        if a.len() != 3 {
            return Err("malformed particle alpha key".into());
        }
        alpha.push(AlphaKey {
            value: Range {
                low: real(&a[0])?,
                high: real(&a[1])?,
            },
            frame: a[2].parse()?,
        });
        let c = values(lines, &format!("Color{index}"))?;
        if c.len() != 4 {
            return Err("malformed particle color key".into());
        }
        color.push(ColorKey {
            value: rgb(&c[..3])?,
            frame: c[3].parse()?,
        });
    }
    normalize_alpha_frames(&mut alpha);
    // ParticleInfo::loadPostProcess deliberately leaves color keys unchanged.
    // Equal frames retain native infinite rates and the draw-time color clamp.
    Ok((
        alpha.try_into().map_err(|_| "particle alpha count")?,
        color.try_into().map_err(|_| "particle color count")?,
    ))
}

fn volume(lines: &[ContentLine]) -> Result<Volume, Box<dyn Error>> {
    Ok(match text(lines, "VolumeType")? {
        "NONE" if text(lines, "VelocityType")? != "OUTWARD" => Volume::None,
        // OUTWARD + NONE retains a process-static prior velocity in native code.
        "NONE" => return Err("particle NONE volume with OUTWARD velocity not translated".into()),
        "POINT" => Volume::Point,
        "BOX" if !boolean(lines, "IsHollow")? => {
            Volume::Box(coordinate(values(lines, "VolBoxHalfSize")?)?)
        }
        "LINE" => Volume::Line(
            coordinate(values(lines, "VolLineStart")?)?,
            coordinate(values(lines, "VolLineEnd")?)?,
        ),
        "CYLINDER" => Volume::Cylinder {
            radius: number(lines, "VolCylinderRadius")?,
            length: number(lines, "VolCylinderLength")?,
            hollow: boolean(lines, "IsHollow")?,
        },
        "SPHERE" => Volume::Sphere {
            radius: number(lines, "VolSphereRadius")?,
            hollow: boolean(lines, "IsHollow")?,
        },
        value => return Err(format!("particle VolumeType {value} not translated").into()),
    })
}
fn velocity(lines: &[ContentLine]) -> Result<Velocity, Box<dyn Error>> {
    Ok(match text(lines, "VelocityType")? {
        "NONE" => Velocity::None,
        "ORTHO" => Velocity::Ortho([
            range(lines, "VelOrthoX")?,
            range(lines, "VelOrthoY")?,
            range(lines, "VelOrthoZ")?,
        ]),
        "OUTWARD" => Velocity::Outward(
            range(lines, "VelOutward")?,
            range(lines, "VelOutwardOther")?,
        ),
        "SPHERICAL" => Velocity::Spherical(range(lines, "VelSpherical")?),
        "HEMISPHERICAL" => Velocity::Hemispherical(range(lines, "VelHemispherical")?),
        "CYLINDRICAL" => Velocity::Cylindrical(
            range(lines, "VelCylindricalRadial")?,
            range(lines, "VelCylindricalNormal")?,
        ),
        value => return Err(format!("particle VelocityType {value} not translated").into()),
    })
}
fn bounded_range(
    lines: &[ContentLine],
    key: &str,
    low: f32,
    high: f32,
) -> Result<Range, Box<dyn Error>> {
    let value = range(lines, key)?;
    if !(low..=high).contains(&value.low) || !(low..=high).contains(&value.high) {
        return Err(format!("particle {key} outside supported numeric range").into());
    }
    Ok(value)
}
fn bounded_number(
    lines: &[ContentLine],
    key: &str,
    low: f32,
    high: f32,
) -> Result<f32, Box<dyn Error>> {
    let value = number(lines, key)?;
    if !(low..=high).contains(&value) {
        return Err(format!("particle {key} outside supported numeric range").into());
    }
    Ok(value)
}
fn range(lines: &[ContentLine], key: &str) -> Result<Range, Box<dyn Error>> {
    let v = values(lines, key)?;
    if !(2..=3).contains(&v.len()) || v.get(2).is_some_and(|s| s != "UNIFORM" && s != "CONSTANT") {
        return Err(format!("invalid particle range {key}").into());
    }
    Ok(Range {
        low: real(&v[0])?,
        high: real(&v[1])?,
    })
}
fn number(lines: &[ContentLine], key: &str) -> Result<f32, Box<dyn Error>> {
    real(text(lines, key)?)
}
pub(super) fn real(value: &str) -> Result<f32, Box<dyn Error>> {
    let n = value.parse::<f32>()?;
    if !n.is_finite() {
        return Err("nonfinite particle value".into());
    }
    Ok(n)
}
fn text<'a>(lines: &'a [ContentLine], key: &str) -> Result<&'a str, Box<dyn Error>> {
    scalar(field(lines, 0, key).ok_or_else(|| format!("missing particle {key}"))?)
}
fn values<'a>(lines: &'a [ContentLine], key: &str) -> Result<&'a [String], Box<dyn Error>> {
    Ok(&field(lines, 0, key)
        .ok_or_else(|| format!("missing particle {key}"))?
        .tokens[1..])
}
fn boolean(lines: &[ContentLine], key: &str) -> Result<bool, Box<dyn Error>> {
    match text(lines, key)? {
        "Yes" => Ok(true),
        "No" => Ok(false),
        _ => Err(format!("invalid particle {key}").into()),
    }
}
pub(super) fn coordinate(values: &[String]) -> Result<[f32; 3], Box<dyn Error>> {
    components(values, ["X:", "Y:", "Z:"])
}
pub(super) fn rgb(values: &[String]) -> Result<[f32; 3], Box<dyn Error>> {
    Ok(components(values, ["R:", "G:", "B:"])?.map(|v| v / 255.0))
}
fn components(values: &[String], prefixes: [&str; 3]) -> Result<[f32; 3], Box<dyn Error>> {
    if values.len() != 3 {
        return Err("malformed particle vector".into());
    }
    let mut out = [0.0; 3];
    for ((dest, value), prefix) in out.iter_mut().zip(values).zip(prefixes) {
        *dest = real(
            value
                .strip_prefix(prefix)
                .ok_or("invalid particle vector label")?,
        )?;
    }
    Ok(out)
}

fn normalize_alpha_frames(keys: &mut [AlphaKey]) {
    for index in 1..keys.len() {
        if keys[index].frame == 0 {
            break;
        }
        if keys[index].frame <= keys[index - 1].frame {
            keys[index].frame = keys[index - 1].frame.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn none_volume_is_distinct_from_point_for_outward_velocity() {
        for (volume_type, admitted) in [("NONE", false), ("POINT", true)] {
            let file = content::scan(generals_assets::virtual_assets::Asset {
                path: "particles.ini".into(),
                origin: "native outward volume dispatch".into(),
                bytes: format!(
                    "ParticleSystem Test\nVolumeType={volume_type}\nVelocityType=OUTWARD\nEnd\n"
                )
                .into_bytes(),
            })
            .unwrap();
            assert_eq!(volume(&file.declarations[0].lines).is_ok(), admitted);
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_damage_smoke_slave_uses_none_emission_and_keeps_master_motion() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = [content::scan(assets.read("data/ini/particlesystem.ini").unwrap()).unwrap()];
        let names = ["SmallTankStruckSmoke", "TankStruckFlameSlave"];
        let templates = names
            .iter()
            .map(|name| {
                template(
                    &definition(&files, "ParticleSystem", name).unwrap().lines,
                    name,
                    0,
                    &names,
                )
                .unwrap()
            })
            .collect();
        let mut particles = super::super::tests::system(templates);
        assert!(matches!(particles.templates[1].volume, Volume::None));
        assert!(matches!(particles.templates[1].velocity, Velocity::None));
        let pose = super::super::Pose {
            position: [100.0, 200.0, 300.0],
            axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let mut random = |low: f32, high: f32| {
            if low.to_bits() == 0.0_f32.to_bits() && high.to_bits() == 10.0_f32.to_bits() {
                high
            } else {
                low
            }
        };
        let master = particles.create(0, pose, None, &mut random);
        assert_eq!(particles.emitters[0].template, 1);
        assert_eq!(particles.emitters[1].id, master);
        particles.logic(0, &mut random, &|_| 0.0);
        let slave = &particles.emitters[0];
        assert!(slave.master);
        assert_eq!(slave.particles.len(), 15);
        assert_eq!(particles.emitters[1].particles.len(), 15);
        for particle in &slave.particles {
            assert_eq!(
                particle.position.map(f32::to_bits),
                [100.0_f32, 210.0, 300.0].map(f32::to_bits)
            );
            assert_eq!(
                particle.velocity.map(f32::to_bits),
                [0.0_f32, 0.08, 0.0].map(f32::to_bits)
            );
            assert_eq!(particle.remaining.to_bits(), 60.0_f32.to_bits());
            assert_eq!(particle.size.to_bits(), 1.0_f32.to_bits());
            assert_eq!(particle.angular_damping.to_bits(), 0.0_f32.to_bits());
            assert_eq!(
                particle.color.map(f32::to_bits),
                [43.0_f32 / 255.0; 3].map(f32::to_bits)
            );
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_jet_debris_keeps_alpha_test_instead_of_alpha_blending() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = [content::scan(assets.read("data/ini/particlesystem.ini").unwrap()).unwrap()];
        let declaration = definition(&files, "ParticleSystem", "JetDebris").unwrap();
        let rules = template(&declaration.lines, "JetDebris", 0, &[]).unwrap();
        assert!(matches!(
            rules.drawing.shader,
            super::super::Shader::AlphaTest
        ));
        assert!(matches!(rules.drawing.style, Style::Billboard));
        assert!(rules.lifetime.low > 0.0 && rules.lifetime.high >= rules.lifetime.low);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_fire_base_arms_keep_zero_lifetime_and_long_burst_delay() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = [content::scan(assets.read("data/ini/particlesystem.ini").unwrap()).unwrap()];
        let name = "FireBaseHowitzerPillarArms";
        let declaration = definition(&files, "ParticleSystem", name).unwrap();
        let rules = template(&declaration.lines, name, 0, &["FirebasePillarArmFlame"]).unwrap();
        assert_eq!(rules.lifetime.low.to_bits(), 0.0_f32.to_bits());
        assert_eq!(rules.lifetime.high.to_bits(), 0.0_f32.to_bits());
        assert_eq!(rules.burst_delay.low.to_bits(), 99_999.0_f32.to_bits());
        assert_eq!(rules.burst_delay.high.to_bits(), 99_999.0_f32.to_bits());
        assert_eq!(rules.attached, Some(0));
        assert_eq!(rules.size.high.to_bits(), 0.0_f32.to_bits());
        assert!(matches!(
            rules.drawing.shader,
            super::super::Shader::Additive
        ));
        assert_eq!(
            rules.color[0].value[0].to_bits(),
            (3.0_f32 / 255.0).to_bits()
        );
    }
    #[test]
    fn volume_depth_defaults_and_native_cap_preserve_authored_single_layer() {
        for (value, expected) in [
            ("", 6),
            ("VolParticleDepth=0", 6),
            ("VolParticleDepth=1", 1),
            ("VolParticleDepth=4", 4),
            ("VolParticleDepth=4294967295", 16),
        ] {
            let file = content::scan(generals_assets::virtual_assets::Asset {
                path: "volume.ini".into(),
                origin: "source volume depth".into(),
                bytes: format!("ParticleSystem Volume\nType=VOLUME_PARTICLE\n{value}\nEnd\n")
                    .into_bytes(),
            })
            .unwrap();
            let Style::Volume(depth) = style(&file.declarations[0].lines).unwrap() else {
                panic!("volume style")
            };
            assert_eq!(depth, expected);
        }
        for value in ["-1", "1.5", "4294967296"] {
            let file = content::scan(generals_assets::virtual_assets::Asset {
                path: "volume.ini".into(),
                origin: "invalid volume depth".into(),
                bytes: format!(
                    "ParticleSystem Volume\nType=VOLUME_PARTICLE\nVolParticleDepth={value}\nEnd\n"
                )
                .into_bytes(),
            })
            .unwrap();
            assert!(style(&file.declarations[0].lines).is_err());
        }
    }
    #[test]
    fn absent_slave_and_per_particle_templates_remain_null_links() {
        let file = content::scan(generals_assets::virtual_assets::Asset {
            path: "particles.ini".into(), origin: "original optional particle links".into(),
            bytes: b"ParticleSystem Parent\nSlaveSystem=Present\nPerParticleAttachedSystem=ScudMissleLauncherSmoke\nEnd\n".to_vec(),
        }).unwrap();
        let lines = &file.declarations[0].lines;
        assert_eq!(
            linked_template(lines, "SlaveSystem", &["Parent", "Present"]).unwrap(),
            Some(1)
        );
        assert_eq!(
            linked_template(lines, "PerParticleAttachedSystem", &["Parent", "Present"]).unwrap(),
            None
        );
    }
    #[test]
    fn alpha_normalization_retains_terminal_zero_and_native_unsigned_frame_width() {
        for (frames, expected) in [
            ([0, 2, 2, 0], [0, 2, 3, 0]),
            ([0, 3, 2, 0], [0, 3, 4, 0]),
            ([0, 2, 0, 7], [0, 2, 0, 7]),
            ([65535, 1, 2, 0], [65535, 65536, 65537, 0]),
            ([0, 1, 2, 0], [0, 1, 2, 0]),
        ] {
            let mut keys = frames.map(|frame| AlphaKey {
                value: Range {
                    low: 0.0,
                    high: 0.0,
                },
                frame,
            });
            normalize_alpha_frames(&mut keys);
            assert_eq!(keys.map(|key| key.frame), expected);
        }
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn original_dragon_trail_alpha_keys_are_normalized_before_particle_creation() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = [content::scan(assets.read("data/ini/particlesystem.ini").unwrap()).unwrap()];
        let name = "FlameThrowerTrail01";
        let source = definition(&files, "ParticleSystem", name).unwrap();
        let rules = template(&source.lines, name, 0, &[]).unwrap();
        assert_eq!(
            rules.alpha.each_ref().map(|key| key.frame),
            [0, 8, 9, 0, 0, 0, 0, 0]
        );
        let mut particles = crate::particles::tests::system(vec![rules]);
        particles.create(
            0,
            crate::particles::Pose::at([0.0; 3]),
            None,
            &mut |_, high| high,
        );
        let rules = &particles.templates[0];
        let mut particle = particles.emitters[0].generate(rules, 1.0, 0, 1, 0, &mut |_, high| high);
        assert_eq!(
            particle.alpha_keys.map(|key| key.1),
            [0, 8, 9, 0, 0, 0, 0, 0]
        );
        assert_eq!(particle.alpha_rate.to_bits(), (1.0_f32 / 8.0).to_bits());
        // The actual trail is additive and dies at six frames; normalized alpha
        // keys remain part of its original template but do not control its color.
        assert!(matches!(
            rules.drawing.shader,
            super::super::Shader::Additive
        ));
        for frame in 0..5 {
            assert!(particle.update(rules, frame));
            particle.draw(rules, 1.0, None);
            assert_eq!(particle.alpha.to_bits(), 0.0_f32.to_bits());
            assert!(particle.color.iter().all(|value| value.is_finite()));
        }
        assert!(!particle.update(rules, 5));
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn original_upgraded_firestorm_equal_color_keys_keep_native_clamped_transition() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = [content::scan(assets.read("data/ini/particlesystem.ini").unwrap()).unwrap()];
        let name = "FireStormMainUpgraded";
        let source = definition(&files, "ParticleSystem", name).unwrap();
        let rules = template(&source.lines, name, 0, &["FireStormMainEmbersUpgraded"]).unwrap();
        assert_eq!(rules.color[1].frame, 20);
        assert_eq!(rules.color[2].frame, 20);
        let mut particles = crate::particles::tests::system(vec![rules]);
        particles.create(
            0,
            crate::particles::Pose::at([0.0; 3]),
            None,
            &mut |low, _| low,
        );
        let rules = &particles.templates[0];
        let mut particle = particles.emitters[0].generate(rules, 1.0, 0, 1, 0, &mut |low, _| low);
        for frame in 0..=20 {
            assert!(particle.update(rules, frame));
            particle.draw(rules, 1.0, None);
        }
        assert_eq!(particle.color_target, 1);
        assert!(particle.color_rate.iter().all(|rate| rate.is_finite()));
        assert!(particle.update(rules, 21));
        assert_eq!(particle.color_target, 2);
        assert_eq!(
            particle.color_rate.map(f32::to_bits),
            [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::INFINITY].map(f32::to_bits)
        );
        particle.draw(rules, 0.25, None);
        assert_eq!(
            particle.color.map(f32::to_bits),
            [0.0, 0.0, 1.0].map(f32::to_bits)
        );
        particle.draw(rules, 0.75, None);
        assert_eq!(
            particle.color.map(f32::to_bits),
            [0.0, 0.0, 1.0].map(f32::to_bits)
        );
        assert!(particle.update(rules, 22));
        assert_eq!(particle.color_target, 3);
        assert!(particle.color_rate.iter().all(|rate| rate.is_finite()));
        particle.draw(rules, 1.0, None);
        let blue = 1.0_f32 - (208.0 / 255.0) / 30.0 - 1.0 / 255.0;
        assert_eq!(
            particle.color.map(f32::to_bits),
            [0.0, 0.0, blue].map(f32::to_bits)
        );
    }
    #[test]
    fn nonfinite_ranges_and_wrong_coordinate_axes_are_rejected() {
        assert!(real("NaN").is_err());
        assert!(real("inf").is_err());
        let fields = ["X:1", "Y:2", "X:3"].map(str::to_owned);
        assert!(coordinate(&fields).is_err());
    }
}
