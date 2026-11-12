//! Original incendiary object and weapon module data.
use super::{blocks, duration_frames, field, number, scalar};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::fire::flammable::Rules;
use num_traits::ToPrimitive;
use std::error::Error;

pub(super) mod collide;
pub(super) mod firestorm;
mod objects;
pub(super) mod weapon_update;
mod weapons;
pub(super) mod when_damaged;
pub(super) use objects::rules;
pub(super) use weapons::load;

pub(super) const OBJECTS: [&str; 6] = [
    "FireWallSegment",
    "FireWallSegmentUpgraded",
    "FireFieldSmall",
    "FireFieldUpgradedSmall",
    "FirestormSmall",
    "BlackNapalmFirestormSmall",
];

pub(super) fn flammable(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "FlammableUpdate")
        {
            continue;
        }
        let rules = Rules {
            burned_delay: duration(module, "BurnedDelay", 0)?,
            duration: duration(module, "AflameDuration", 0)?,
            damage_delay: duration(module, "AflameDamageDelay", 0)?,
            damage: field(module, 1, "AflameDamageAmount")
                .map_or(Ok::<i32, Box<dyn Error>>(0), |line| {
                    scalar(line)?.parse::<i32>().map_err(Into::into)
                })?
                .to_f32()
                .expect("native signed flame damage"),
            limit: optional(module, "FlameDamageLimit", 20.0)?,
            expiration: duration(module, "FlameDamageExpiration", 60)?,
            sound: field(module, 1, "BurningSoundName")
                .map(scalar)
                .transpose()?
                .unwrap_or("")
                .into(),
        };
        result.push(rules);
    }
    Ok(result)
}

pub(super) fn fire_spread(
    files: &[generals_assets::content::ContentFile],
    object: &Declaration,
) -> Result<Vec<generals_sim::skirmish::fire::flammable::SpreadRules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "FireSpreadUpdate")
        {
            continue;
        }
        let delay = [
            duration(module, "MinSpreadDelay", 0)?,
            duration(module, "MaxSpreadDelay", 0)?,
        ];
        result.push(generals_sim::skirmish::fire::flammable::SpreadRules {
            delay,
            range: optional(module, "SpreadTryRange", 0.0)?,
            embers: field(module, 1, "OCLEmbers")
                .map(|line| super::physical_ocl::load(files, scalar(line)?))
                .transpose()?,
        });
    }
    Ok(result)
}

fn duration(lines: &[ContentLine], key: &str, default: u32) -> Result<u32, Box<dyn Error>> {
    field(lines, 1, key).map_or(Ok(default), duration_frames)
}

fn optional(lines: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(lines, 1, key).map_or(Ok(default), |_| number(lines, 1, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_dragon_burn_fields_preserve_module_defaults_and_durations() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "fire.ini".into(), origin: "original Dragon FlammableUpdate".into(),
            bytes: b"Object Dragon\nBehavior=FlammableUpdate Tag\nAflameDuration=5000\nAflameDamageAmount=3\nAflameDamageDelay=500\nEnd\nEnd".to_vec(),
        }).unwrap();
        let rules = &flammable(&file.declarations[0]).unwrap()[0];
        assert_eq!(rules.duration, 150);
        assert_eq!(rules.damage_delay, 15);
        assert_eq!(rules.expiration, 60);
        assert_eq!(rules.limit.to_bits(), 20.0_f32.to_bits());
        assert_eq!(rules.damage.to_bits(), 3.0_f32.to_bits());
        assert_eq!(rules.burned_delay, 0);
        assert!(rules.sound.is_empty());
    }
}
