use super::{
    ACCELERATION_SCALE, FRAME_SECONDS, boolean, damaged_value, field, optional_number, per_frame,
    scalar, signed_real,
};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::flight::{Appearance, Height, Rules};
use std::error::Error;

pub(super) fn load(
    lines: &[ContentLine],
    globals: &[ContentLine],
    object: &[ContentLine],
) -> Result<Option<Rules>, Box<dyn Error>> {
    let appearance = match field(lines, 0, "Appearance")
        .map(scalar)
        .transpose()?
        .unwrap_or("OTHER")
    {
        "HOVER" => Appearance::Hover,
        "WINGS" => Appearance::Wings,
        _ => return Ok(None),
    };
    let height = match field(lines, 0, "ZAxisBehavior")
        .map(scalar)
        .transpose()?
        .unwrap_or("NO_Z_MOTIVE_FORCE")
    {
        "NO_Z_MOTIVE_FORCE" => Height::None,
        "SURFACE_RELATIVE_HEIGHT" | "RELATIVE_TO_HIGHEST_LAYER" => Height::Surface,
        "ABSOLUTE_HEIGHT" => Height::Absolute,
        "FIXED_SURFACE_RELATIVE_HEIGHT" => Height::FixedSurface,
        "FIXED_ABSOLUTE_HEIGHT" => Height::FixedAbsolute,
        value => return Err(format!("Untranslated aircraft Z behavior {value}").into()),
    };
    let modules = super::blocks(object, 0, "Behavior")?;
    let mut candidates = modules.into_iter().filter(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "PhysicsBehavior")
    });
    let physics = candidates.next().ok_or("Aircraft lacks PhysicsBehavior")?;
    if candidates.next().is_some() {
        return Err("Ambiguous aircraft PhysicsBehavior".into());
    }
    if optional_number(lines, "WanderWidthFactor", 0.0)? != 0.0 || boolean(lines, "DownhillOnly")? {
        return Err(
            "Selected aircraft locomotor requires untranslated wandering or downhill motion".into(),
        );
    }
    let acceleration = optional_number(lines, "Acceleration", 0.0)?;
    let lift = optional_number(lines, "Lift", 0.0)?;
    let rules = Rules {
        draw: super::draw::load(lines)?,
        appearance,
        height,
        acceleration: acceleration * ACCELERATION_SCALE,
        acceleration_damaged: damaged_value(lines, "AccelerationDamaged", acceleration)?
            * ACCELERATION_SCALE,
        braking: per_frame(lines, 0, "Braking", 99_999.0, ACCELERATION_SCALE)?,
        min_speed: per_frame(lines, 0, "MinSpeed", 0.0, FRAME_SECONDS)?,
        close_enough: optional_number(lines, "CloseEnoughDist", 1.0)?,
        close_3d: boolean(lines, "CloseEnoughDist3D")?,
        preferred_height: signed_real(lines, 0, "PreferredHeight", 0.0)?,
        lift: lift * ACCELERATION_SCALE,
        lift_damaged: damaged_value(lines, "LiftDamaged", lift)? * ACCELERATION_SCALE,
        damping: optional_number(lines, "PreferredHeightDamping", 1.0)?,
        speed_limit_z: per_frame(lines, 0, "SpeedLimitZ", 999_999.0, FRAME_SECONDS)?,
        gravity: per_frame(globals, 0, "Gravity", -1.0, ACCELERATION_SCALE)?,
        mass: signed_real(physics, 1, "Mass", 1.0)?,
        forward_friction: per_frame(physics, 1, "ForwardFriction", 0.15, FRAME_SECONDS)?,
        lateral_friction: per_frame(physics, 1, "LateralFriction", 0.15, FRAME_SECONDS)?,
        aerodynamic_friction: per_frame(physics, 1, "AerodynamicFriction", 0.0, FRAME_SECONDS)?,
        extra_friction: per_frame(lines, 0, "Extra2DFriction", 0.0, FRAME_SECONDS)?,
        friction_2d: boolean(lines, "Apply2DFrictionWhenAirborne")?,
        airborne_force: boolean(lines, "AllowAirborneMotiveForce")?,
        slide_frames: per_frame(lines, 0, "SlideIntoPlaceTime", 0.0, 30.0 / 1000.0)?,
        circling_radius: signed_real(lines, 0, "CirclingRadius", 0.0)?,
    };
    generals_sim::skirmish::flight::validate(&rules)?;
    Ok(Some(rules))
}

#[cfg(test)]
mod tests {
    use super::{Appearance, Height, load};
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn flight_values_preserve_constructor_defaults_and_native_frame_conversions() {
        let file = content::scan(Asset {
            path: "flight.ini".into(),
            origin: "Native HOVER physics fixture".into(),
            bytes: b"Locomotor Flight\nAppearance=HOVER\nAcceleration=60\nAccelerationDamaged=-1\nLift=120\nLiftDamaged=80\nSpeedLimitZ=30\nSlideIntoPlaceTime=100\nCloseEnoughDist3D=Yes\nAllowAirborneMotiveForce=Yes\nApply2DFrictionWhenAirborne=Yes\nZAxisBehavior=RELATIVE_TO_HIGHEST_LAYER\nEnd\nGameData\nGravity=-64\nEnd\nObject Drone\nBehavior=PhysicsBehavior PhysicsTag\nMass=50\nLateralFriction=3\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules = load(
            &file.declarations[0].lines,
            &file.declarations[1].lines,
            &file.declarations[2].lines,
        )
        .unwrap()
        .unwrap();
        assert!(matches!(rules.appearance, Appearance::Hover));
        assert!(matches!(rules.height, Height::Surface));
        assert!(rules.close_3d && rules.airborne_force && rules.friction_2d);
        assert_eq!(rules.braking.to_bits(), 99_999.0_f32.to_bits());
        assert_eq!(rules.forward_friction.to_bits(), 0.15_f32.to_bits());
        assert_eq!(
            rules.lateral_friction.to_bits(),
            (3.0_f32 * (1.0 / 30.0)).to_bits()
        );
        assert_eq!(
            rules.acceleration.to_bits(),
            rules.acceleration_damaged.to_bits()
        );
        let scale = (1.0_f32 / 30.0) * (1.0 / 30.0);
        assert_eq!(rules.gravity.to_bits(), (-64.0 * scale).to_bits());
        assert_eq!(rules.lift_damaged.to_bits(), (80.0 * scale).to_bits());
        assert_eq!(rules.slide_frames.to_bits(), 3.0_f32.to_bits());
    }

    #[test]
    fn ambiguous_physics_and_untranslated_motion_are_rejected() {
        for (extra, physics) in [
            (
                "",
                "Behavior=PhysicsBehavior First\nEnd\nBehavior=PhysicsBehavior Second\nEnd",
            ),
            ("WanderWidthFactor=2", "Behavior=PhysicsBehavior First\nEnd"),
            ("DownhillOnly=Yes", "Behavior=PhysicsBehavior First\nEnd"),
        ] {
            let file = content::scan(Asset {
                path: "flight.ini".into(),
                origin: "Unsupported source aircraft branch fixture".into(),
                bytes: format!("Locomotor Flight\nAppearance=WINGS\n{extra}\nEnd\nObject Aircraft\n{physics}\nEnd\n").into_bytes(),
            }).unwrap();
            assert!(
                load(
                    &file.declarations[0].lines,
                    &[],
                    &file.declarations[1].lines
                )
                .is_err()
            );
        }
    }
}
