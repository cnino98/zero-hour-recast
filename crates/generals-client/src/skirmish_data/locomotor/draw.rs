//! `LocomotorTemplate` constructor and original hover/wing drawable coefficient fields.
use super::{field, scalar, signed_real};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::flight::draw::Rules;
use std::error::Error;

pub(super) fn load(lines: &[ContentLine]) -> Result<Rules, Box<dyn Error>> {
    let number = |key, default| signed_real(lines, 0, key, default);
    let angle = |key| -> Result<f32, Box<dyn Error>> {
        Ok(field(lines, 0, key)
            .map(scalar)
            .transpose()?
            .map_or(Ok(0.0), crate::skirmish_data::finite_number)?
            * (std::f32::consts::PI / 180.0))
    };
    let acceleration_limit = angle("AccelerationPitchLimit")?;
    let deceleration_limit = angle("DecelerationPitchLimit")?;
    Ok(Rules {
        stiffness: [
            number("PitchStiffness", 0.1)?,
            number("RollStiffness", 0.1)?,
        ],
        damping: [number("PitchDamping", 0.9)?, number("RollDamping", 0.9)?],
        axial_damping: number("UniformAxialDamping", 1.0)?,
        velocity: [
            number("ForwardVelocityPitchFactor", 0.0)?,
            number("LateralVelocityRollFactor", 0.0)?,
            number("PitchInDirectionOfZVelFactor", 0.0)?,
        ],
        acceleration: [
            number("ForwardAccelerationPitchFactor", 0.0)?,
            number("LateralAccelerationRollFactor", 0.0)?,
        ],
        pitch_limits: [
            acceleration_limit,
            if deceleration_limit == 0.0 {
                acceleration_limit
            } else {
                deceleration_limit
            },
        ],
        // Despite their names, these four original INI fields use parseReal,
        // not angle or per-frame conversion.
        rudder: [
            number("RudderCorrectionDegree", 0.0)?,
            number("RudderCorrectionRate", 0.0)?,
        ],
        elevator: [
            number("ElevatorCorrectionDegree", 0.0)?,
            number("ElevatorCorrectionRate", 0.0)?,
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};
    #[test]
    fn native_angle_limits_default_deceleration_and_raw_oscillators_keep_distinct_units() {
        let source = content::scan(Asset { path: "loco.ini".into(), origin: "original field formats".into(), bytes: b"Locomotor Test\nAccelerationPitchLimit=30\nDecelerationPitchLimit=0\nRudderCorrectionDegree=0.02\nRudderCorrectionRate=0.01\nEnd\n".to_vec() }).unwrap();
        let rules = load(&source.declarations[0].lines).unwrap();
        assert_eq!(
            rules.pitch_limits.map(f32::to_bits),
            [30.0_f32 * (std::f32::consts::PI / 180.0); 2].map(f32::to_bits)
        );
        assert_eq!(
            rules.rudder.map(f32::to_bits),
            [0.02_f32, 0.01].map(f32::to_bits)
        );
        assert_eq!(
            rules.stiffness.map(f32::to_bits),
            [0.1_f32; 2].map(f32::to_bits)
        );
    }
}
