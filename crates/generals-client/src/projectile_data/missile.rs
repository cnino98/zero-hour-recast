//! Native `MissileAIUpdate`, THRUST and `PhysicsBehavior` values and frame units.
use super::{blocks, definition, field, number, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::{
    collision_geometry::{Geometry, Shape},
    skirmish::{MissileRules, ProjectilePhysics, ThrustRules},
};
use std::error::Error;

const SECONDS_PER_FRAME: f32 = 1.0 / 30.0;
const ACCELERATION_SCALE: f32 = SECONDS_PER_FRAME * SECONDS_PER_FRAME;

pub(super) fn module_rules(
    files: &[ContentFile],
    object: &Declaration,
    ai: &[ContentLine],
    global: &[ContentLine],
) -> Result<MissileRules, Box<dyn Error>> {
    let physics = behavior(object, "PhysicsBehavior")?.ok_or("missile lacks PhysicsBehavior")?;
    let locomotors = crate::skirmish_data::locomotor::normal_set(&object.lines)?;
    let locomotor = locomotors.first().ok_or("missile lacks normal locomotor")?;
    let locomotor = &definition(files, "Locomotor", locomotor)?.lines;
    Ok(MissileRules {
        retirement: generals_sim::skirmish::MissileRetirement {
            calls_kill: boolean(ai, 1, "DetonateCallsKill", false)?,
            delay: duration(ai, "KillSelfDelay", 3)?,
        },
        jam_scatter: number(ai, 1, "DistanceScatterWhenJammed", 75.0)?,
        initial_velocity: number(ai, 1, "InitialVelocity", 0.0)? * SECONDS_PER_FRAME,
        ignition_delay: duration(ai, "IgnitionDelay", 0)?,
        fuel_lifetime: duration(ai, "FuelLifetime", 0)?,
        initial_distance: number(ai, 1, "DistanceToTravelBeforeTurning", 0.0)?,
        dive_distance: number(ai, 1, "DistanceToTargetBeforeDiving", 0.0)?,
        lock_distance: number(ai, 1, "DistanceToTargetForLock", 75.0)?,
        follow_target: boolean(ai, 1, "TryToFollowTarget", true)?,
        use_weapon_speed: boolean(ai, 1, "UseWeaponSpeed", false)?,
        detonate_on_no_fuel: boolean(ai, 1, "DetonateOnNoFuel", false)?,
        thrust: thrust(locomotor)?,
        physics: physical(physics, locomotor, global)?,
        geometry: geometry(&object.lines)?,
        clear: super::garrison_clear(
            ai,
            "GarrisonHitKillRequiredKindOf",
            "GarrisonHitKillForbiddenKindOf",
        )?,
    })
}

fn behavior<'a>(
    object: &'a Declaration,
    name: &str,
) -> Result<Option<&'a [ContentLine]>, Box<dyn Error>> {
    let mut found = None;
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0].tokens.get(1).is_some_and(|value| value == name) {
            found = Some(module);
        }
    }
    Ok(found)
}

fn thrust(lines: &[ContentLine]) -> Result<ThrustRules, Box<dyn Error>> {
    if field(lines, 0, "Appearance").map(scalar).transpose()? != Some("THRUST") {
        return Err("missile locomotor is not THRUST".into());
    }
    if boolean(lines, 0, "StickToGround", false)? {
        return Err("ground-sticking missile physics not translated".into());
    }
    Ok(ThrustRules {
        max_speed: number(lines, 0, "Speed", 0.0)? * SECONDS_PER_FRAME,
        min_speed: number(lines, 0, "MinSpeed", 0.0)? * SECONDS_PER_FRAME,
        acceleration: number(lines, 0, "Acceleration", 0.0)? * ACCELERATION_SCALE,
        // Constructor defaults are already frame values; only INI values convert.
        braking: converted(lines, 0, "Braking", ACCELERATION_SCALE, 99_999.0)?,
        turn_rate: number(lines, 0, "TurnRate", 0.0)?
            * (SECONDS_PER_FRAME * (std::f32::consts::PI / 180.0)),
        max_thrust_angle: number(lines, 0, "MaxThrustAngle", 0.0)?.to_radians(),
        preferred_height: number(lines, 0, "PreferredHeight", 0.0)?,
        height_damping: number(lines, 0, "PreferredHeightDamping", 1.0)?,
        close_enough: number(lines, 0, "CloseEnoughDist", 1.0)?,
        airborne_motive: boolean(lines, 0, "AllowAirborneMotiveForce", false)?,
    })
}

fn physical(
    lines: &[ContentLine],
    locomotor: &[ContentLine],
    global: &[ContentLine],
) -> Result<ProjectilePhysics, Box<dyn Error>> {
    if boolean(lines, 1, "AllowBouncing", false)? {
        return Err("bouncing missile physics not translated".into());
    }
    let mass = number(lines, 1, "Mass", 1.0)?;
    if mass <= 0.0 {
        return Err("missile physics requires positive Mass".into());
    }
    Ok(ProjectilePhysics {
        mass,
        gravity: converted(global, 0, "Gravity", ACCELERATION_SCALE, -1.0)?,
        forward_friction: converted(lines, 1, "ForwardFriction", SECONDS_PER_FRAME, 0.15)?,
        lateral_friction: converted(lines, 1, "LateralFriction", SECONDS_PER_FRAME, 0.15)?,
        aerodynamic_friction: number(lines, 1, "AerodynamicFriction", 0.0)? * SECONDS_PER_FRAME,
        extra_friction: number(locomotor, 0, "Extra2DFriction", 0.0)? * SECONDS_PER_FRAME,
        apply_friction_when_airborne: boolean(locomotor, 0, "Apply2DFrictionWhenAirborne", false)?,
    })
}

fn converted(
    lines: &[ContentLine],
    depth: u32,
    name: &str,
    scale: f32,
    fallback: f32,
) -> Result<f32, Box<dyn Error>> {
    if let Some(line) = field(lines, depth, name) {
        let value = scalar(line)?.parse::<f32>()?;
        if !value.is_finite() {
            return Err(format!("nonfinite missile {name}").into());
        }
        Ok(value * scale)
    } else {
        Ok(fallback)
    }
}

pub(super) fn geometry(lines: &[ContentLine]) -> Result<Geometry, Box<dyn Error>> {
    let shape = field(lines, 0, "Geometry")
        .map(scalar)
        .transpose()?
        .unwrap_or("SPHERE");
    let shape = if shape.eq_ignore_ascii_case("SPHERE") {
        Shape::Sphere
    } else if shape.eq_ignore_ascii_case("CYLINDER") {
        Shape::Cylinder
    } else {
        return Err(format!("projectile Geometry {shape} is not translated").into());
    };
    Ok(Geometry {
        shape,
        is_small: boolean(lines, 0, "GeometryIsSmall", false)?,
        major: number(lines, 0, "GeometryMajorRadius", 1.0)?,
        minor: number(lines, 0, "GeometryMinorRadius", 1.0)?,
        height: number(lines, 0, "GeometryHeight", 1.0)?,
    })
}

pub(super) fn collide_mask(lines: &[ContentLine]) -> Result<u16, Box<dyn Error>> {
    let Some(line) = field(lines, 0, "ProjectileCollidesWith") else {
        return Ok(0x0004);
    };
    let mut mask = 0;
    for token in &line.tokens[1..] {
        mask |= match token.as_str() {
            "ALLIES" => 0x0001,
            "ENEMIES" => 0x0002,
            "STRUCTURES" => 0x0004,
            "SHRUBBERY" => 0x0008,
            "PROJECTILE" => 0x0010,
            "WALLS" => 0x0020,
            "SMALL_MISSILES" => 0x0040,
            "BALLISTIC_MISSILES" => 0x0080,
            "CONTROLLED_STRUCTURES" => 0x0100,
            _ => return Err(format!("ProjectileCollidesWith {token} is not translated").into()),
        };
    }
    Ok(mask)
}

fn boolean(
    lines: &[ContentLine],
    depth: u32,
    name: &str,
    fallback: bool,
) -> Result<bool, Box<dyn Error>> {
    field(lines, depth, name).map_or(Ok(fallback), |line| {
        let value = scalar(line)?;
        if value.eq_ignore_ascii_case("Yes") {
            Ok(true)
        } else if value.eq_ignore_ascii_case("No") {
            Ok(false)
        } else {
            Err(format!("invalid {name} boolean {value}").into())
        }
    })
}

fn duration(lines: &[ContentLine], name: &str, fallback: u32) -> Result<u32, Box<dyn Error>> {
    field(lines, 1, name).map_or(Ok(fallback), |line| {
        let milliseconds = scalar(line)?.parse::<u32>()?;
        Ok(u32::try_from(
            (u64::from(milliseconds) * 30).div_ceil(1000),
        )?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    fn fixture() -> ContentFile {
        content::scan(Asset {
            path: "missile.ini".into(), origin: "fixture".into(),
            bytes: b"Object Rocket\nBehavior=MissileAIUpdate Missile\nInitialVelocity=150\nFuelLifetime=1250\nEnd\nBehavior=PhysicsBehavior Physics\nMass=1\nEnd\nLocomotor=SET_NORMAL RocketFlight\nGeometry=Sphere\nGeometryMajorRadius=2\nGeometryIsSmall=Yes\nEnd\nLocomotor RocketFlight\nAppearance=THRUST\nSpeed=225\nMinSpeed=120\nAcceleration=675\nBraking=0\nTurnRate=100\nMaxThrustAngle=25\nAllowAirborneMotiveForce=Yes\nEnd\nWeapon Launcher\nProjectileCollidesWith=STRUCTURES\nEnd\nGameData\nGravity=-64\nEnd\n".to_vec(),
        }).unwrap()
    }

    fn constructor_lengths(files: &[ContentFile]) -> Vec<u32> {
        crate::skirmish_data::locomotor::normal_set(&files[0].declarations[0].lines)
            .unwrap()
            .into_iter()
            .map(|name| {
                number(
                    &definition(files, "Locomotor", name).unwrap().lines,
                    0,
                    "WanderLengthFactor",
                    1.0,
                )
                .unwrap()
                .to_bits()
            })
            .collect()
    }

    #[test]
    fn missile_constructor_retains_only_ordered_normal_templates() {
        let mut file = fixture();
        let normal = file.declarations[0]
            .lines
            .iter_mut()
            .find(|line| {
                line.tokens
                    .first()
                    .is_some_and(|token| token == "Locomotor")
            })
            .unwrap();
        normal.tokens.extend(["None".into(), "ExtraFlight".into()]);
        let extra = content::scan(Asset {
            path: "extra.ini".into(),
            origin: "fixture".into(),
            bytes: b"Locomotor ExtraFlight\nWanderLengthFactor=4\nEnd\n".to_vec(),
        })
        .unwrap();
        file.declarations.extend(extra.declarations);
        let files = [file];
        assert_eq!(
            crate::skirmish_data::locomotor::normal_set(&files[0].declarations[0].lines).unwrap(),
            ["RocketFlight", "ExtraFlight"]
        );
        assert_eq!(
            constructor_lengths(&files),
            [1.0_f32.to_bits(), 4.0_f32.to_bits()]
        );
    }

    #[test]
    fn missile_ini_converts_rates_and_preserves_constructor_frame_defaults() {
        let files = [fixture()];
        let rules = module_rules(
            &files,
            &files[0].declarations[0],
            behavior(&files[0].declarations[0], "MissileAIUpdate")
                .unwrap()
                .unwrap(),
            &files[0].declarations[3].lines,
        )
        .unwrap();
        assert_eq!(constructor_lengths(&files), [1.0_f32.to_bits()]);
        assert_eq!(
            rules.initial_velocity.to_bits(),
            (150.0 * SECONDS_PER_FRAME).to_bits()
        );
        assert_eq!(rules.fuel_lifetime, 38);
        assert_eq!(rules.retirement.delay, 3);
        assert_eq!(
            rules.thrust.max_speed.to_bits(),
            (225.0 * SECONDS_PER_FRAME).to_bits()
        );
        assert_eq!(
            rules.thrust.acceleration.to_bits(),
            (675.0 * ACCELERATION_SCALE).to_bits()
        );
        assert_eq!(rules.thrust.height_damping.to_bits(), 1.0_f32.to_bits());
        assert_eq!(rules.physics.forward_friction.to_bits(), 0.15_f32.to_bits());
        assert_eq!(
            rules.physics.aerodynamic_friction.to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            rules.physics.gravity.to_bits(),
            (-64.0 * ACCELERATION_SCALE).to_bits()
        );
        assert_eq!(collide_mask(&files[0].declarations[2].lines).unwrap(), 4);
        assert!(rules.thrust.airborne_motive);
    }

    #[test]
    fn original_artillery_missile_keeps_nontracking_thrust_and_cylinder_geometry() {
        let mut files = [fixture()];
        let object = content::scan(Asset {
            path: "artillery.ini".into(), origin: "installed StrategyCenterArtilleryShell fields".into(),
            bytes: b"Object StrategyCenterArtilleryShell\nBehavior=MissileAIUpdate AI\nTryToFollowTarget=No\nFuelLifetime=4000\nIgnitionDelay=0\nInitialVelocity=20\nDistanceToTravelBeforeTurning=40\nDistanceToTargetBeforeDiving=50\nEnd\nBehavior=PhysicsBehavior Physics\nMass=1\nEnd\nLocomotor=SET_NORMAL RocketFlight\nGeometry=Cylinder\nGeometryMajorRadius=4\nGeometryHeight=4\nGeometryIsSmall=Yes\nEnd\n".to_vec(),
        }).unwrap();
        files[0].declarations[0] = object.declarations.into_iter().next().unwrap();
        let rules = module_rules(
            &files,
            &files[0].declarations[0],
            behavior(&files[0].declarations[0], "MissileAIUpdate")
                .unwrap()
                .unwrap(),
            &[],
        )
        .unwrap();
        assert!(!rules.follow_target);
        assert_eq!(rules.fuel_lifetime, 120);
        assert_eq!(
            rules.initial_velocity.to_bits(),
            (20.0 * SECONDS_PER_FRAME).to_bits()
        );
        assert_eq!(rules.initial_distance.to_bits(), 40.0_f32.to_bits());
        assert_eq!(rules.dive_distance.to_bits(), 50.0_f32.to_bits());
        assert!(matches!(rules.geometry.shape, Shape::Cylinder));
        assert!(rules.geometry.is_small);
        assert_eq!(rules.geometry.major.to_bits(), 4.0_f32.to_bits());
        assert_eq!(rules.geometry.minor.to_bits(), 1.0_f32.to_bits());
        assert_eq!(rules.geometry.height.to_bits(), 4.0_f32.to_bits());
    }

    #[test]
    fn invalid_physics_is_reported_instead_of_loading_a_straight_flight_fallback() {
        let mut files = [fixture()];
        let mass = files[0].declarations[0]
            .lines
            .iter_mut()
            .find(|line| line.tokens[0] == "Mass")
            .unwrap();
        mass.tokens[1] = "0".into();
        assert!(
            module_rules(
                &files,
                &files[0].declarations[0],
                behavior(&files[0].declarations[0], "MissileAIUpdate")
                    .unwrap()
                    .unwrap(),
                &[]
            )
            .is_err()
        );
        mass_reset(&mut files[0]);
        files[0].declarations[1]
            .lines
            .iter_mut()
            .find(|line| line.tokens[0] == "Appearance")
            .unwrap()
            .tokens[1] = "WINGS".into();
        assert!(
            module_rules(
                &files,
                &files[0].declarations[0],
                behavior(&files[0].declarations[0], "MissileAIUpdate")
                    .unwrap()
                    .unwrap(),
                &[]
            )
            .is_err()
        );
    }

    fn mass_reset(file: &mut ContentFile) {
        file.declarations[0]
            .lines
            .iter_mut()
            .find(|line| line.tokens[0] == "Mass")
            .unwrap()
            .tokens[1] = "1".into();
    }
    #[test]
    fn collision_parser_keeps_original_wall_projectile_and_ownership_bits() {
        let file = content::scan(Asset { path: "mask.ini".into(), origin: "test".into(), bytes: b"Weapon Grenade\nProjectileCollidesWith=ALLIES ENEMIES STRUCTURES SHRUBBERY PROJECTILE WALLS SMALL_MISSILES BALLISTIC_MISSILES CONTROLLED_STRUCTURES\nEnd\n".to_vec() }).unwrap();
        assert_eq!(collide_mask(&file.declarations[0].lines).unwrap(), 0x01ff);
    }
}
