//! Authored physical OCLs; unsupported creation forms are rejected before admission.
use super::{blocks, definition, duration_frames, field, locomotor, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::physical_ocl::{
    self, Death, DebrisDrawRules, List, Nugget, RandomForce, Rules, SlowDeath,
};
use std::error::Error;

mod collision;
pub(super) use collision::collision_rules;

#[cfg(test)]
pub mod tests;

pub(super) const OBJECTS: [&str; 39] = [
    "GenericDebris",
    "AmericaDeadDozerHulk",
    "DeadBattleBusHulk",
    "DeadBombTruckHulk",
    "GLACombatBikeFullDebris",
    "GLACombatBikeToppledHulk",
    "DeadCrusaderHulk",
    "HelixRubbleHull",
    "NapalmBomb",
    "AmericaJetRaptorHulk",
    "AmericaJetAuroraHulk",
    "AmericaJetStealthHulk",
    "ChinaJetMIGHulk",
    "ChinaVehicleListeningOutpostDeadHull",
    "AmericaVehicleSentryDroneHulk",
    "AmericaVehicleHumveeDeadHull",
    "AmericaVehicleAmbulanceDeadHull",
    "AmericaVehicleTomahawkHulk",
    "ChinaDeadDozerHulk",
    "ChinaTankDragonDeadHull",
    "ChinaTankOverlordBarrelDebris",
    "ChinaTankOverlordDeadHull",
    "ChinaVehicleBattleMasterDeadHull",
    "ChinaVehicleTroopCrawlerDeadHull",
    "DeadAvengerHulk",
    "DeadChinaECMTankHulk",
    "DeadChinaGattlingTankHulk",
    "DeadChinaSupplyTruckHulk",
    "DeadMarauderHulk",
    "DeadMicrowaveHulk",
    "DeadPaladinHulk",
    "DeadQuadCannonHulk",
    "DeadRadarVanHulk",
    "DeadSCUDLauncherHulk",
    "DeadScorpionHulk",
    "DeadToxinTractorHulk",
    "GLARocketBuggyFullDebris",
    "InfernoCannonHulk",
    "SpectreHulk",
];

pub(super) fn geometry(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<generals_sim::collision_geometry::Geometry, Box<dyn Error>> {
    let mut result = super::support::geometry(&object.lines)?;
    if OBJECTS.contains(&object.name.as_str())
        && field(&object.lines, 0, "GeometryIsSmall").is_none()
    {
        let defaults = definition(files, "Object", "DefaultThingTemplate")?;
        result.is_small = super::locomotor::boolean(&defaults.lines, "GeometryIsSmall")?;
    }
    Ok(result)
}

pub(super) fn rules(
    files: &[ContentFile],
    object: &Declaration,
    lifecycle: Option<&generals_sim::skirmish::death_lifecycle::Rules>,
) -> Result<Option<Rules>, Box<dyn Error>> {
    if !OBJECTS.contains(&object.name.as_str()) {
        return Ok(None);
    }
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let find = |name: &str| {
        modules
            .iter()
            .copied()
            .find(|module| module[0].tokens.get(1).is_some_and(|value| value == name))
    };
    for module in &modules {
        let name = module[0]
            .tokens
            .get(1)
            .ok_or("physical module name missing")?;
        if super::death_lifecycle::inherited::has_die_interface(name)
            && !matches!(
                name.as_str(),
                "SlowDeathBehavior" | "InstantDeathBehavior" | "FireWeaponWhenDeadBehavior"
            )
        {
            return Err(format!("unsupported physical DIE module {name}").into());
        }
    }
    let physics = find("PhysicsBehavior").ok_or("physical object lacks PhysicsBehavior")?;
    let death = death_rules(&modules, lifecycle)?;
    let result = Rules {
        motion: motion_rules(files, physics)?,
        kill_when_resting: boolean(physics, "KillWhenRestingOnGround", false)?,
        active_body: blocks(&object.lines, 0, "Body")?.iter().any(|body| {
            body[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "ActiveBody")
        }),
        collision: collision_rules(files, physics)?,
        death,
    };
    if result.motion.mass <= 0.0
        || matches!(&result.death, Death::Slow(death) if death.probability == 0)
    {
        return Err("invalid physical object mass/death".into());
    }
    Ok(Some(result))
}

fn death_rules(
    modules: &[&[ContentLine]],
    lifecycle: Option<&generals_sim::skirmish::death_lifecycle::Rules>,
) -> Result<Death, Box<dyn Error>> {
    let mut slow = modules.iter().filter(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "SlowDeathBehavior")
    });
    let module = slow.next();
    let instant = modules.iter().any(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "InstantDeathBehavior")
    });
    if slow.next().is_some() || module.is_some() && instant {
        return Err("physical object has untranslated multiple SlowDeath/DIE dispatch".into());
    }
    if let Some(module) = module {
        let tag = module[0]
            .tokens
            .get(2)
            .ok_or("physical SlowDeath missing tag")?;
        let rules = lifecycle.ok_or("physical object missing parsed SlowDeath consumer")?;
        if rules.slow.len() != 1 {
            return Err("physical SlowDeath differs from actual parsed module".into());
        }
        let rule = &rules.slow[0];
        if &rule.tag != tag
            || !matches!(
                rule.derived,
                generals_sim::skirmish::death_lifecycle::Derived::None
            )
        {
            return Err("physical SlowDeath tag/family differs from actual parsed module".into());
        }
        // Registered SlowDeath owns all FX/OCL and timing callbacks. The physical
        // compatibility path retains these same scalars; it never parses a second
        // restricted version that rejects already translated phase callbacks.
        Ok(Death::Slow(SlowDeath {
            probability: rule.probability,
            sink_delay: rule.sink_delay,
            destruction_delay: rule.destruction_delay,
            sink_rate: rule.sink_rate,
        }))
    } else if instant {
        Ok(Death::Instant)
    } else {
        Err("physical object lacks represented death module".into())
    }
}

pub(super) fn load(files: &[ContentFile], name: &str) -> Result<List, Box<dyn Error>> {
    let object = definition(files, "ObjectCreationList", name)?;
    let mut nuggets = Vec::new();
    for module in blocks(&object.lines, 0, "CreateDebris")?
        .into_iter()
        .chain(blocks(&object.lines, 0, "CreateObject")?)
    {
        nuggets.push((module[0].line, nugget(module)?));
    }
    // `blocks` selects a kind; restore the source declaration sequence across kinds.
    nuggets.sort_by_key(|(line, _)| *line);
    for line in &object.lines {
        if line.depth == Some(0)
            && !matches!(
                line.tokens.first().map(String::as_str),
                Some("CreateDebris" | "CreateObject" | "ApplyRandomForce" | "End")
            )
        {
            return Err(format!("unsupported physical OCL nugget in {name}").into());
        }
    }
    let forces = blocks(&object.lines, 0, "ApplyRandomForce")?;
    if forces.len() > 1
        || forces.first().is_some_and(|force| {
            nuggets
                .first()
                .is_some_and(|(line, _)| force[0].line > *line)
        })
    {
        return Err("only an initial ApplyRandomForce before physical creation is admitted".into());
    }
    let initial_force = forces
        .first()
        .map(|force| source_force(force))
        .transpose()?;
    Ok(List {
        name: name.into(),
        initial_force,
        nuggets: nuggets.into_iter().map(|(_, nugget)| nugget).collect(),
    })
}

fn validate_nugget_fields(module: &[ContentLine]) -> Result<(), Box<dyn Error>> {
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        let key = line.tokens.first().map_or("", String::as_str);
        if !matches!(
            key,
            "ModelNames"
                | "ObjectNames"
                | "Mass"
                | "Count"
                | "IgnorePrimaryObstacle"
                | "AnimationSet"
                | "FXFinal"
                | "OkToChangeModelColor"
                | "OrientInForceDirection"
                | "Offset"
                | "Disposition"
                | "DispositionIntensity"
                | "SpinRate"
                | "YawRate"
                | "RollRate"
                | "PitchRate"
                | "ExtraFriction"
                | "BounceSound"
                | "ParticleSystem"
                | "ExtraBounciness"
                | "MinForceMagnitude"
                | "MaxForceMagnitude"
                | "MinForcePitch"
                | "MaxForcePitch"
                | "MinHealth"
                | "MaxHealth"
                | "MinLifetime"
                | "MaxLifetime"
                | "End"
        ) {
            return Err(format!("untranslated physical OCL field {key}").into());
        }
    }
    Ok(())
}

pub(super) fn nugget(module: &[ContentLine]) -> Result<Nugget, Box<dyn Error>> {
    validate_nugget_fields(module)?;
    let debris = module[0].tokens[0] == "CreateDebris";
    let names = field(module, 1, if debris { "ModelNames" } else { "ObjectNames" })
        .ok_or("physical OCL has no object/model names")?
        .tokens[1..]
        .to_vec();
    let angular = |key| -> Result<Option<f32>, Box<dyn Error>> {
        Ok(field(module, 1, key)
            .map(|line| -> Result<f32, Box<dyn Error>> {
                let value: f32 = scalar(line)?.parse()?;
                if !value.is_finite() {
                    return Err("invalid physical OCL angular rate".into());
                }
                Ok(value.to_radians() * (1.0 / 30.0))
            })
            .transpose()?
            .filter(|value| *value >= 0.0))
    };
    let mut disposition = physical_ocl::ON_GROUND;
    if let Some(line) = field(module, 1, "Disposition") {
        disposition = 0;
        for value in &line.tokens[1..] {
            disposition |= match value.as_str() {
                "LIKE_EXISTING" => physical_ocl::LIKE_EXISTING,
                "ON_GROUND_ALIGNED" => physical_ocl::ON_GROUND,
                "INHERIT_VELOCITY" => physical_ocl::INHERIT_VELOCITY,
                "SEND_IT_FLYING" => physical_ocl::SEND_FLYING,
                "SEND_IT_UP" => physical_ocl::SEND_UP,
                "RANDOM_FORCE" => physical_ocl::RANDOM_FORCE,
                _ => return Err(format!("untranslated physical OCL disposition {value}").into()),
            };
        }
    }
    let maximum = frames(module, "MaxLifetime")?;
    let rule = Nugget {
        ignore_primary_obstacle: boolean(module, "IgnorePrimaryObstacle", false)?,
        orient_in_force_direction: boolean(module, "OrientInForceDirection", false)?,
        draw: debris_draw(module)?,
        names,
        debris_mass: debris.then(|| real(module, "Mass", 0.0)).transpose()?,
        count: field(module, 1, "Count")
            .map(scalar)
            .transpose()?
            .unwrap_or("1")
            .parse()?,
        offset: field(module, 1, "Offset")
            .map(coordinate)
            .transpose()?
            .unwrap_or([0.0; 3]),
        disposition,
        intensity: real(module, "DispositionIntensity", 0.0)?,
        spin_rate: angular("SpinRate")?,
        angular_rates: [
            angular("YawRate")?,
            angular("RollRate")?,
            angular("PitchRate")?,
        ],
        extra_friction: real(module, "ExtraFriction", 0.0)? * (1.0 / 30.0),
        bounce_sound: field(module, 1, "BounceSound")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
        particle: field(module, 1, "ParticleSystem")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
        force_magnitude: [
            real(module, "MinForceMagnitude", 0.0)?,
            real(module, "MaxForceMagnitude", 0.0)?,
        ],
        force_pitch: [
            real(module, "MinForcePitch", 0.0)?.to_radians(),
            real(module, "MaxForcePitch", 0.0)?.to_radians(),
        ],
        health: [percent(module, "MinHealth")?, percent(module, "MaxHealth")?],
        lifetime: (maximum > 0)
            .then(|| frames(module, "MinLifetime").map(|minimum| [minimum, maximum]))
            .transpose()?,
    };
    if rule.debris_mass.is_some_and(|mass| mass <= 0.0)
        || rule.health[0] > rule.health[1]
        || rule.force_magnitude[0] > rule.force_magnitude[1]
        || rule.force_pitch[0] > rule.force_pitch[1]
        || rule.lifetime.is_some_and(|range| range[0] > range[1])
    {
        return Err("invalid physical OCL range/mass".into());
    }
    Ok(rule)
}

pub(super) fn source_force(module: &[ContentLine]) -> Result<RandomForce, Box<dyn Error>> {
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        if !matches!(
            line.tokens.first().map(String::as_str),
            Some(
                "MinForceMagnitude"
                    | "MaxForceMagnitude"
                    | "MinForcePitch"
                    | "MaxForcePitch"
                    | "SpinRate"
                    | "End"
            )
        ) {
            return Err("untranslated ApplyRandomForce field".into());
        }
    }
    let force = RandomForce {
        magnitude: [
            real(module, "MinForceMagnitude", 0.0)?,
            real(module, "MaxForceMagnitude", 0.0)?,
        ],
        pitch: [
            real(module, "MinForcePitch", 0.0)?.to_radians(),
            real(module, "MaxForcePitch", 0.0)?.to_radians(),
        ],
        spin_rate: real(module, "SpinRate", 0.0)?.to_radians() * (1.0 / 30.0),
    };
    if force.magnitude[0] > force.magnitude[1] || force.pitch[0] > force.pitch[1] {
        return Err("invalid ApplyRandomForce range".into());
    }
    Ok(force)
}

fn debris_draw(module: &[ContentLine]) -> Result<DebrisDrawRules, Box<dyn Error>> {
    let animations = module
        .iter()
        .filter(|line| {
            line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "AnimationSet")
        })
        .map(|line| {
            <[String; 3]>::try_from(line.tokens[1..].to_vec())
                .map_err(|_| "AnimationSet requires initial, flying and final names".into())
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(DebrisDrawRules {
        animations,
        final_fx: field(module, 1, "FXFinal")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
        keep_color: boolean(module, "OkToChangeModelColor", false)?,
    })
}

fn frames(module: &[ContentLine], key: &str) -> Result<u32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(0), duration_frames)
}
pub(super) fn coordinate(line: &ContentLine) -> Result<[f32; 3], Box<dyn Error>> {
    let mut result = [0.0; 3];
    let mut tokens = line.tokens[1..].iter();
    while let Some(token) = tokens.next() {
        let (axis, value) = token
            .split_once(':')
            .ok_or("physical OCL offset lacks colon")?;
        let axis = match axis.to_ascii_uppercase().as_str() {
            "X" => 0,
            "Y" => 1,
            "Z" => 2,
            _ => return Err("invalid physical OCL offset axis".into()),
        };
        let value = if value.is_empty() {
            tokens
                .next()
                .ok_or("physical OCL offset lacks value")?
                .as_str()
        } else {
            value
        };
        let value: f32 = value.parse()?;
        if !value.is_finite() {
            return Err("invalid physical OCL offset".into());
        }
        result[axis] = value;
    }
    Ok(result)
}
fn real(module: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    locomotor::signed_real(module, 1, key, default)
}
fn boolean(module: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    match field(module, 1, key).map(scalar).transpose()? {
        None => Ok(default),
        Some("Yes") => Ok(true),
        Some("No") => Ok(false),
        _ => Err("invalid physical module boolean".into()),
    }
}
fn percent(module: &[ContentLine], key: &str) -> Result<f32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(1.0), |line| {
        let value: f32 = scalar(line)?.trim_end_matches('%').parse()?;
        if !value.is_finite() || value < 0.0 {
            return Err("invalid physical OCL health percentage".into());
        }
        Ok(value * 0.01)
    })
}

pub fn motion_rules(
    files: &[ContentFile],
    physics: &[ContentLine],
) -> Result<physical_ocl::MotionRules, Box<dyn Error>> {
    let global = &definition(files, "GameData", "")?.lines;
    Ok(physical_ocl::MotionRules {
        center_of_mass_offset: real(physics, "CenterOfMassOffset", 0.0)?,
        mass: real(physics, "Mass", 1.0)?,
        gravity: locomotor::per_frame(global, 0, "Gravity", -1.0, (1.0 / 30.0) * (1.0 / 30.0))?,
        stiffness: locomotor::signed_real(global, 0, "GroundStiffness", 0.5)?,
        forward_friction: locomotor::per_frame(physics, 1, "ForwardFriction", 0.15, 1.0 / 30.0)?,
        lateral_friction: locomotor::per_frame(physics, 1, "LateralFriction", 0.15, 1.0 / 30.0)?,
        aerodynamic_friction: locomotor::per_frame(
            physics,
            1,
            "AerodynamicFriction",
            0.0,
            1.0 / 30.0,
        )?,
        angular_factor: real(physics, "PitchRollYawFactor", 2.0)?,
        allow_bouncing: boolean(physics, "AllowBouncing", false)?,
    })
}

pub(super) fn live_rules(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<physical_ocl::LiveRules>, Box<dyn Error>> {
    if OBJECTS.contains(&object.name.as_str()) {
        return Ok(None);
    }
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some(physics) = modules.into_iter().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "PhysicsBehavior")
    }) else {
        return Ok(None);
    };
    let motion = motion_rules(files, physics)?;
    let height = real(physics, "MinFallHeightForDamage", 40.0)?;
    let min_fall_speed = (2.0 * motion.gravity * height).abs().sqrt();
    Ok(Some(physical_ocl::LiveRules {
        motion,
        resistance: real(physics, "ShockResistance", 0.0)?,
        kill_when_resting: boolean(physics, "KillWhenRestingOnGround", false)?,
        collision: collision_rules(files, physics)?,
        shock_rotation: [
            real(physics, "ShockMaxYaw", 0.05)?,
            real(physics, "ShockMaxPitch", 0.025)?,
            real(physics, "ShockMaxRoll", 0.025)?,
        ],
        min_fall_speed,
        fall_damage_factor: real(physics, "FallHeightDamageFactor", 1.0)?,
    }))
}

#[cfg(test)]
mod runtime_tests;
