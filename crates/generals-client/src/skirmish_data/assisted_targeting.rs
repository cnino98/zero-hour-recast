//! `AssistedTargetingUpdate`'s ordered module data and nullable template lookups.
use super::{blocks, scalar};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::AssistedTargeting;
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<AssistedTargeting>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "AssistedTargetingUpdate")
        {
            continue;
        }
        let mut rules = AssistedTargeting {
            clip_size: 1,
            weapon_slot: 0,
            lasers: [String::new(), String::new()],
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("AssistingClipSize") => {
                    rules.clip_size = i32::from_ne_bytes(
                        super::source_numbers::unsigned(scalar(line)?)?.to_ne_bytes(),
                    );
                }
                Some("AssistingWeaponSlot") => {
                    let value = scalar(line)?;
                    rules.weapon_slot = ["PRIMARY", "SECONDARY", "TERTIARY"]
                        .iter()
                        .position(|slot| slot.eq_ignore_ascii_case(value))
                        .ok_or("unknown assisting weapon slot")?;
                }
                Some("LaserFromAssisted") => rules.lasers[0] = scalar(line)?.into(),
                Some("LaserToTarget") => rules.lasers[1] = scalar(line)?.into(),
                _ => {}
            }
        }
        result.push(rules);
    }
    Ok(result)
}

pub(super) fn laser_update(
    object: &Declaration,
) -> Result<Option<generals_sim::skirmish::laser_update::Rules>, Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "ClientUpdate")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "LaserUpdate")
        {
            continue;
        }
        let mut rules = generals_sim::skirmish::laser_update::Rules {
            particles: [String::new(), String::new()],
            punch_through: 0.0,
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("MuzzleParticleSystem") => rules.particles[0] = scalar(line)?.into(),
                Some("TargetParticleSystem") => rules.particles[1] = scalar(line)?.into(),
                Some("PunchThroughScalar") => {
                    rules.punch_through = super::source_real(scalar(line)?)?;
                }
                _ => {}
            }
        }
        return Ok(Some(rules));
    }
    Ok(None)
}

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut result = Vec::new();
    for name in selected {
        for rules in load(super::definition(files, "Object", name)?)? {
            for name in rules.lasers {
                // Native findTemplate can return null; absent feedback is intentional.
                if let Some(object) = files
                    .iter()
                    .flat_map(|file| &file.declarations)
                    .find(|object| object.kind == "Object" && object.name == name)
                    && !selected.contains(&object.name.as_str())
                    && !result.contains(&object.name.as_str())
                {
                    result.push(object.name.as_str());
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_instances_keep_defaults_signed_counts_and_last_authored_fields() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "assistance.ini".into(), origin: "native parser regression".into(),
            bytes: b"Object Patriot\nBehavior=AssistedTargetingUpdate First\nAssistingClipSize=3\nAssistingClipSize=-1\nLaserFromAssisted=Missing\nEnd\nBehavior=AssistedTargetingUpdate Second\nAssistingWeaponSlot=SECONDARY\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules = load(&file.declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].clip_size, -1);
        assert_eq!(rules[0].weapon_slot, 0);
        assert_eq!(rules[1].clip_size, 1);
        assert_eq!(rules[1].weapon_slot, 1);
        assert!(dependencies(&[file], &["Patriot"]).unwrap().is_empty());
    }
    #[test]
    fn laser_client_data_keeps_last_names_and_signed_punch_without_particle_gates() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "laser.ini".into(), origin: "native laser parser".into(),
            bytes: b"Object Beam\nClientUpdate=LaserUpdate First\nMuzzleParticleSystem=Missing\nMuzzleParticleSystem=Actual\nTargetParticleSystem=None\nPunchThroughScalar=-2\nEnd\nClientUpdate=LaserUpdate Second\nPunchThroughScalar=7\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules = laser_update(&file.declarations[0]).unwrap().unwrap();
        assert_eq!(rules.particles, ["Actual", "None"]);
        assert_eq!(rules.punch_through.to_bits(), (-2.0_f32).to_bits());
    }
}
