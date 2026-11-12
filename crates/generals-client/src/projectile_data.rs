use crate::{
    projectile_render::Projectiles,
    skirmish_data::{blocks, definition, field, finite_number, scalar},
};
use generals_assets::{
    content::{ContentLine, Declaration},
    virtual_assets::VirtualAssets,
    w3d::Model,
};
use generals_sim::skirmish::{
    ClearBuilding, FlightRules, LaunchTransform, ShellContact, ShellRules, ShellSpeedScale,
    UnitType, WeaponMotion,
};
use std::error::Error;

mod missile;
#[cfg(test)]
mod tests;

pub fn load(
    assets: &VirtualAssets,
    types: &mut [UnitType],
    models: &[Model],
    animations: &crate::model_animation::Animations,
) -> Result<Projectiles, Box<dyn Error>> {
    let selected: Vec<_> = types.iter().map(|unit| unit.name.as_str()).collect();
    let (files, _) = crate::skirmish_data::source_files::effective(assets, &selected)?;
    let template_indices: std::collections::BTreeMap<_, _> = types
        .iter()
        .enumerate()
        .map(|(index, unit)| (unit.name.clone(), index))
        .collect();
    let global = &definition(&files, "GameData", "")?.lines;
    // PartitionManager::init clamps configured cell size before caching terrain.
    let partition_cell_size = number(global, 0, "PartitionCellSize", 40.0)?.max(1.0);
    let damage_thresholds = [
        number(global, 0, "UnitDamagedThreshold", 0.5)?,
        number(global, 0, "UnitReallyDamagedThreshold", 0.1)?,
    ];
    let mut result = Projectiles {
        partition_cell_size,
        models: types
            .iter()
            .map(|unit| vec![None; unit.weapon_template_count()])
            .collect(),
    };
    for (index, unit) in types.iter_mut().enumerate() {
        crate::container_data::bind(unit, index, models, animations, damage_thresholds)?;
        for index_in_set in 0..unit.weapon_template_count() {
            let Some(rules) = unit.weapon_template(index_in_set) else {
                continue;
            };
            if rules.name.is_empty() {
                continue;
            }
            let name = rules.name.clone();
            let weapon = &definition(&files, "Weapon", &name)?.lines;
            let projectile = field(weapon, 0, "ProjectileObject")
                .map(scalar)
                .transpose()?
                .unwrap_or("NONE");
            let projectile_type = if projectile.eq_ignore_ascii_case("NONE") {
                None
            } else {
                let template = *template_indices.get(projectile).ok_or_else(|| {
                    format!("{projectile}: missing concrete native object template")
                })?;
                result.models[index][index_in_set] = Some(template);
                Some(template)
            };
            let shots = field(weapon, 0, "ShotsPerBarrel")
                .map(scalar)
                .transpose()?
                .unwrap_or("1")
                .parse()?;
            if shots == 0 {
                return Err(format!("{name}: zero ShotsPerBarrel").into());
            }
            let rules = unit
                .weapon_template_mut(index_in_set)
                .expect("loaded weapon template");
            rules.motion = Some(WeaponMotion {
                turret_mounts: None,
                launches: [Vec::new(), Vec::new(), Vec::new()],
                damage_thresholds,
                shots_per_barrel: shots,
                projectile_type,
                collide_mask: missile::collide_mask(weapon)?,
                shell_speed_scale: speed_scale(weapon)?,
            });
            if unit.fire.is_none() {
                bind_launches(animations, models, index, index_in_set, unit)
                    .map_err(|error| format!("{} weapon {name}: {error}", unit.name))?;
            }
        }
    }
    for unit in types.iter() {
        validate_location_weapons(unit, types)?;
    }
    Ok(result)
}

fn validate_location_weapons(unit: &UnitType, types: &[UnitType]) -> Result<(), Box<dyn Error>> {
    for slot in 0..3 {
        if unit.location_weapons & (1 << slot) == 0 {
            continue;
        }
        let ordinary = unit.weapon_in_slot(slot).into_iter();
        let conditional = unit
            .weapon_set_rules
            .templates
            .iter()
            .filter(|weapon| weapon.slot == slot)
            .map(|weapon| &weapon.rules);
        let mut found = false;
        for weapon in ordinary
            .chain(conditional)
            .filter(|weapon| !weapon.name.is_empty())
        {
            found = true;
            if !matches!(
                weapon
                    .motion
                    .as_ref()
                    .and_then(|motion| motion.projectile_type)
                    .and_then(|index| types[index].projectile_rules.first()),
                Some(FlightRules::Missile(_))
            ) {
                return Err(format!(
                    "{}: location weapon {} requires an explicit non-missile command consumer",
                    unit.name, weapon.name
                )
                .into());
            }
        }
        if !found {
            return Err(format!("{}: location command lacks an original weapon", unit.name).into());
        }
    }
    Ok(())
}

fn bind_launches(
    animations: &crate::model_animation::Animations,
    models: &[Model],
    index: usize,
    index_in_set: usize,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    let slot = unit.weapon_context(index_in_set).0;
    let alternate = unit
        .turret
        .as_ref()
        .is_none_or(|rules| rules.controlled_slots & (1 << slot) == 0)
        && unit
            .secondary_turret
            .as_ref()
            .is_some_and(|rules| rules.controlled_slots & (1 << slot) != 0);
    let mut launches = [Vec::new(), Vec::new(), Vec::new()];
    let mut turret_mounts = [
        generals_sim::skirmish::turret::Mount::ORIGIN,
        generals_sim::skirmish::turret::Mount::ORIGIN,
        generals_sim::skirmish::turret::Mount::ORIGIN,
    ];
    for (state, damage) in [None, Some("DAMAGED"), Some("REALLYDAMAGED")]
        .into_iter()
        .enumerate()
    {
        let Some(model) = animations.firing_model(models, index, damage, index_in_set) else {
            if !animations.firing_model_absent(index, damage, index_in_set) {
                return Err("firing model missing".into());
            }
            // Drawable::getProjectileLaunchOffset finds no barrel without a
            // model draw. Weapon::calcProjectileLaunchPosition uses identity.
            launches[state].push(LaunchTransform::IDENTITY);
            continue;
        };
        let fields = animations
            .firing_fields(index, damage, index_in_set)
            .ok_or("firing state fields missing")?;
        launches[state] = launch_bones(fields, model, slot);
        let offset = animations
            .firing_offset(index, damage, index_in_set)
            .unwrap_or([0.0; 3]);
        turret_mounts[state] = turret_mount(fields, model, alternate)
            .map_err(|error| format!("{}: {error}", unit.name))?;
        for mount in [
            &mut turret_mounts[state].yaw,
            &mut turret_mounts[state].pitch,
        ] {
            for (axis, offset) in mount.iter_mut().zip(offset) {
                *axis += offset;
            }
        }
        rotate_art(&mut launches[state], fields, alternate)?;
        if index_in_set == 0
            && let Some(turret) = &mut unit.turret
        {
            turret.mounts[state] = generals_sim::skirmish::turret::Mount {
                yaw: turret_mounts[state].yaw,
                pitch: turret_mounts[state].pitch,
            };
        }
    }
    let rules = unit
        .weapon_template_mut(index_in_set)
        .expect("loaded weapon template");
    let motion = rules.motion.as_mut().expect("loaded flight");
    motion.launches = launches;
    motion.turret_mounts = Some(turret_mounts);
    Ok(())
}

// Native getProjectileLaunchOffset pre-rotates the full pristine matrix by art angles.
fn rotate_art(
    launches: &mut [LaunchTransform],
    fields: &[Vec<String>],
    alternate: bool,
) -> Result<(), Box<dyn Error>> {
    let read = |key: &str| {
        fields
            .iter()
            .rev()
            .find_map(|line| {
                (line.first().is_some_and(|name| name == key))
                    .then(|| line.get(1))
                    .flatten()
            })
            .map_or(Ok(0.0), |value| finite_number(value))
    };
    let yaw = read(if alternate {
        "AltTurretArtAngle"
    } else {
        "TurretArtAngle"
    })? * (std::f32::consts::PI / 180.0);
    let pitch = -read(if alternate {
        "AltTurretArtPitch"
    } else {
        "TurretArtPitch"
    })? * (std::f32::consts::PI / 180.0);
    if yaw == 0.0 && pitch == 0.0 {
        return Ok(());
    }
    let rotate = |point: [f32; 3]| {
        let (sine, cosine) = yaw.sin_cos();
        let x = cosine * point[0];
        let y = sine * point[1];
        let cross_x = sine * point[0];
        let cross_y = cosine * point[1];
        let turned = [x - y, cross_x + cross_y, point[2]];
        let (sine, cosine) = pitch.sin_cos();
        let forward = cosine * turned[0];
        let height = sine * turned[2];
        let forward_z = -sine * turned[0];
        let height_z = cosine * turned[2];
        [forward + height, turned[1], forward_z + height_z]
    };
    for launch in launches {
        launch.position = rotate(launch.position);
        launch.axes = launch.axes.map(rotate);
    }
    Ok(())
}

fn turret_mount(
    fields: &[Vec<String>],
    model: &Model,
    alternate: bool,
) -> Result<generals_sim::skirmish::turret::Mount, Box<dyn Error>> {
    let mut mount = generals_sim::skirmish::turret::Mount::ORIGIN;
    for (key, position) in [
        (
            if alternate { "AltTurret" } else { "Turret" },
            &mut mount.yaw,
        ),
        (
            if alternate {
                "AltTurretPitch"
            } else {
                "TurretPitch"
            },
            &mut mount.pitch,
        ),
    ] {
        if let Some(name) = fields.iter().rev().find_map(|tokens| {
            (tokens.first().is_some_and(|name| name == key))
                .then(|| tokens.get(1))
                .flatten()
        }) {
            *position = model
                .bone_position(name)
                .ok_or_else(|| format!("missing {key} bone {name}"))?;
        }
    }
    Ok(mount)
}

fn speed_scale(lines: &[ContentLine]) -> Result<Option<ShellSpeedScale>, Box<dyn Error>> {
    if !field(lines, 0, "ScaleWeaponSpeed")
        .map(scalar)
        .transpose()?
        .is_some_and(|v| v.eq_ignore_ascii_case("Yes"))
    {
        return Ok(None);
    }
    // INI::parseVelocityReal converts only explicit values; the constructor's
    // omitted MinWeaponSpeed default is already in distance per logic frame.
    let minimum_speed = field(lines, 0, "MinWeaponSpeed")
        .map_or(Ok::<_, Box<dyn Error>>(999_999.0), |line| {
            Ok(finite_number(scalar(line)?)? * (1.0 / 30.0))
        })?;
    Ok(Some(ShellSpeedScale {
        minimum_speed,
        minimum_range: (number(lines, 0, "MinimumAttackRange", 0.0)? - 2.5).max(0.0),
        maximum_range: number(lines, 0, "AttackRange", 0.0)?,
    }))
}

pub fn template_rules(
    files: &[generals_assets::content::ContentFile],
    object: &Declaration,
) -> Result<Vec<FlightRules>, Box<dyn Error>> {
    let global = &definition(files, "GameData", "")?.lines;
    let mut rules = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        match module[0].tokens.get(1).map(String::as_str) {
            Some("DumbProjectileBehavior") => {
                rules.push(FlightRules::Shell(shell_rules(files, object, module)?));
            }
            Some("MissileAIUpdate") => rules.push(FlightRules::Missile(missile::module_rules(
                files, object, module, global,
            )?)),
            _ => {}
        }
    }
    Ok(rules)
}

fn shell_rules(
    files: &[generals_assets::content::ContentFile],
    object: &Declaration,
    module: &[ContentLine],
) -> Result<ShellRules, Box<dyn Error>> {
    let tumble = shell_boolean(module, "TumbleRandomly", false)?;
    let orient_to_flight_path = shell_boolean(module, "OrientToFlightPath", true)?;
    let physics = shell_physics(files, object)?;
    let milliseconds = field(module, 1, "MaxLifespan")
        .map(next_value)
        .transpose()?
        .unwrap_or("10000")
        .parse::<u32>()?;
    let lifespan = u32::try_from((u64::from(milliseconds) * 30).div_ceil(1000))?;
    Ok(ShellRules {
        detonate_calls_kill: shell_boolean(module, "DetonateCallsKill", false)?,
        tumble,
        orient_to_flight_path,
        physics,
        contact: Some(shell_contact_rules(object, module)?),
        heights: [
            number(module, 1, "FirstHeight", 0.0)?,
            number(module, 1, "SecondHeight", 0.0)?,
        ],
        indents: [
            percent(module, "FirstPercentIndent")?,
            percent(module, "SecondPercentIndent")?,
        ],
        adjust_per_frame: number(module, 1, "FlightPathAdjustDistPerSecond", 0.0)? / 30.0,
        lifespan,
    })
}

fn shell_physics(
    files: &[generals_assets::content::ContentFile],
    object: &Declaration,
) -> Result<Option<generals_sim::skirmish::physical_ocl::MotionRules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "PhysicsBehavior")
        })
        .collect::<Vec<_>>();
    if modules.len() > 1 {
        return Err("shell has repeated PhysicsBehavior".into());
    }
    let Some(module) = modules.first() else {
        return Ok(None);
    };
    for line in module.iter().skip(1).filter(|line| line.depth == Some(1)) {
        if line.tokens.first().is_none_or(|key| {
            !matches!(
                key.as_str(),
                "Mass"
                    | "ForwardFriction"
                    | "LateralFriction"
                    | "AerodynamicFriction"
                    | "PitchRollYawFactor"
                    | "AllowBouncing"
            )
        }) {
            return Err("shell PhysicsBehavior field is not translated".into());
        }
    }
    let rules = crate::skirmish_data::shell_physics_rules(files, module)?;
    if rules.mass <= 0.0 {
        return Err("shell physics requires positive Mass".into());
    }
    Ok(Some(rules))
}

fn shell_boolean(module: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    let Some(value) = field(module, 1, key).map(next_value).transpose()? else {
        return Ok(default);
    };
    if value.eq_ignore_ascii_case("Yes") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("No") {
        Ok(false)
    } else {
        Err(format!("invalid shell {key} boolean").into())
    }
}

fn launch_bones(fields: &[Vec<String>], model: &Model, weapon_slot: usize) -> Vec<LaunchTransform> {
    // ModelConditionInfo::validateWeaponBarrelInfo: numbered bones across all
    // four barrel roles precede the unnumbered fallback. Missing launch bones
    // retain the source identity transform even when another role exists.
    let names = [
        "WeaponFireFXBone",
        "WeaponRecoilBone",
        "WeaponMuzzleFlash",
        "WeaponLaunchBone",
    ]
    .map(|key| {
        fields.iter().rev().find_map(|tokens| {
            (tokens.first().is_some_and(|value| value == key)
                && tokens
                    .get(1)
                    .is_some_and(|slot| slot == ["PRIMARY", "SECONDARY", "TERTIARY"][weapon_slot]))
            .then(|| tokens.get(2).map(String::as_str))
            .flatten()
        })
    });
    let mut positions = Vec::new();
    for index in 1..=99 {
        let bones = names
            .map(|name| name.and_then(|name| model.bone_position(&format!("{name}{index:02}"))));
        if bones.iter().all(Option::is_none) {
            break;
        }
        let axes = names[3]
            .and_then(|name| model.bone_axes(&format!("{name}{index:02}")))
            .unwrap_or(LaunchTransform::IDENTITY.axes);
        positions.push(LaunchTransform {
            position: bones[3].unwrap_or([0.0; 3]),
            axes,
        });
    }
    if positions.is_empty() {
        let position = names[3].and_then(|name| model.bone_position(name));
        let axes = names[3]
            .and_then(|name| model.bone_axes(name))
            .unwrap_or(LaunchTransform::IDENTITY.axes);
        positions.push(LaunchTransform {
            position: position.unwrap_or([0.0; 3]),
            axes,
        });
    }
    positions
}

fn next_value(line: &ContentLine) -> Result<&str, Box<dyn Error>> {
    // Native INI callbacks consume getNextToken(), then advance to the next
    // line. Original Molotov heights contain further // comment tokens.
    line.tokens
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| format!("projectile field value missing at line {}", line.line).into())
}

fn number(
    lines: &[ContentLine],
    depth: u32,
    name: &str,
    fallback: f32,
) -> Result<f32, Box<dyn Error>> {
    field(lines, depth, name).map_or(Ok(fallback), |line| finite_number(next_value(line)?))
}
fn percent(lines: &[ContentLine], name: &str) -> Result<f32, Box<dyn Error>> {
    field(lines, 1, name).map_or(
        Ok(0.0),
        |line| Ok(finite_number(next_value(line)?)? / 100.0),
    )
}

fn shell_contact_rules(
    object: &Declaration,
    module: &[ContentLine],
) -> Result<ShellContact, Box<dyn Error>> {
    Ok(ShellContact {
        geometry: missile::geometry(&object.lines)?,
        clear: garrison_clear(module, "GarrisonHitKillKindof", "GarrisonHitKillKindofNot")?,
    })
}

fn garrison_clear(
    module: &[ContentLine],
    allowed: &str,
    forbidden: &str,
) -> Result<Option<ClearBuilding>, Box<dyn Error>> {
    let count = field(module, 1, "GarrisonHitKillCount")
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse::<u32>()?;
    let kinds = |key| {
        field(module, 1, key).map_or_else(Vec::new, |line| {
            line.tokens[1..]
                .iter()
                .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                .cloned()
                .collect()
        })
    };
    Ok((count > 0).then(|| ClearBuilding {
        count,
        allowed: kinds(allowed),
        forbidden: kinds(forbidden),
    }))
}

#[cfg(test)]
mod garrison_tests {
    #[test]
    fn artillery_shell_preserves_cylinder_height_and_source_partition_contact() {
        use generals_sim::collision_geometry::{Body, Geometry, Shape, intersects};
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "artillery.ini".into(), origin: "source cylinder fixture".into(),
            bytes: b"Object Artillery\nGeometry=Cylinder\nGeometryMajorRadius=1\nGeometryHeight=2\nGeometryIsSmall=Yes\nBehavior=DumbProjectileBehavior Tag\nEnd\nEnd\nWeapon Gun\nProjectileCollidesWith=STRUCTURES WALLS\nEnd\n".to_vec(),
        }).unwrap();
        let rules = super::shell_rules(
            std::slice::from_ref(&file),
            &file.declarations[0],
            super::blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0],
        )
        .unwrap();
        let contact = rules.contact.unwrap();
        assert!(matches!(contact.geometry.shape, Shape::Cylinder));
        assert!(contact.geometry.is_small);
        assert_eq!(contact.geometry.minor.to_bits(), 1.0_f32.to_bits());
        assert_eq!(
            super::missile::collide_mask(&file.declarations[1].lines).unwrap(),
            0x0024
        );
        let target = Geometry {
            shape: Shape::Cylinder,
            is_small: false,
            major: 1.0,
            minor: 1.0,
            height: 1.0,
        };
        let shell = Body {
            geometry: &contact.geometry,
            position: [0.0; 3],
            angle: 0.0,
        };
        let mut target = Body {
            geometry: &target,
            position: [0.0, 0.0, 2.0],
            angle: 0.0,
        };
        assert!(intersects(&shell, &target));
        target.position[2] = 2.001;
        assert!(!intersects(&shell, &target));
        target.position[2] = -1.001;
        assert!(!intersects(&shell, &target));
    }

    #[test]
    fn flashbang_imports_real_collision_geometry_and_clear_count_masks() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "flashbang.ini".into(), origin: "source collision fixture".into(),
            bytes: b"Object Flashbang\nGeometry=SPHERE\nGeometryMajorRadius=1.5\nBehavior=DumbProjectileBehavior Tag\nGarrisonHitKillCount=2\nGarrisonHitKillKindof=INFANTRY\nGarrisonHitKillKindofNot=HERO\nEnd\nEnd\nWeapon Throw\nProjectileCollidesWith=STRUCTURES ENEMIES\nEnd\n".to_vec(),
        }).unwrap();
        let rules = super::shell_rules(
            std::slice::from_ref(&file),
            &file.declarations[0],
            super::blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0],
        )
        .unwrap();
        let contact = rules.contact.unwrap();
        assert_eq!(contact.geometry.major.to_bits(), 1.5_f32.to_bits());
        assert_eq!(
            super::missile::collide_mask(&file.declarations[1].lines).unwrap(),
            0x0004 | 0x0002
        );
        let clear = contact.clear.unwrap();
        assert_eq!(clear.count, 2);
        assert_eq!(clear.allowed, ["INFANTRY"]);
        assert_eq!(clear.forbidden, ["HERO"]);
    }
}
