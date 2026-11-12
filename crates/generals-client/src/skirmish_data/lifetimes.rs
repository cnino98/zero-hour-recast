//! Original ordered `LifetimeUpdate` and `HeightDieUpdate` declaration data.
use super::{blocks, duration_frames, field, finite_number, scalar};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::height_die;
use std::error::Error;

pub(super) fn lifetime_rules(lines: &[ContentLine]) -> Result<Vec<[u32; 2]>, Box<dyn Error>> {
    lifetime_ranges(lines, "LifetimeUpdate")
}

pub(super) fn deletion_rules(lines: &[ContentLine]) -> Result<Vec<[u32; 2]>, Box<dyn Error>> {
    lifetime_ranges(lines, "DeletionUpdate")
}

fn lifetime_ranges(lines: &[ContentLine], family: &str) -> Result<Vec<[u32; 2]>, Box<dyn Error>> {
    blocks(lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| module[0].tokens.get(1).is_some_and(|name| name == family))
        .map(|module| {
            Ok([
                frames(module, "MinLifetime")?,
                frames(module, "MaxLifetime")?,
            ])
        })
        .collect()
}

pub(super) fn height_rules(
    lines: &[ContentLine],
) -> Result<Vec<height_die::Rules>, Box<dyn Error>> {
    blocks(lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "HeightDieUpdate")
        })
        .map(|module| {
            Ok(height_die::Rules {
                target_height: number(module, "TargetHeight", 0.0)?,
                includes_structures: boolean(module, "TargetHeightIncludesStructures")?,
                only_moving_down: boolean(module, "OnlyWhenMovingDown")?,
                particle_height: number(module, "DestroyAttachedParticlesAtHeight", -1.0)?,
                snap_to_ground: boolean(module, "SnapToGroundOnDeath")?,
                initial_delay: frames(module, "InitialDelay")?,
            })
        })
        .collect()
}

fn frames(lines: &[ContentLine], name: &str) -> Result<u32, Box<dyn Error>> {
    field(lines, 1, name)
        .map(duration_frames)
        .transpose()
        .map(|value| value.unwrap_or(0))
}
fn number(lines: &[ContentLine], name: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(lines, 1, name)
        .map(|line| finite_number(scalar(line)?))
        .transpose()
        .map(|value| value.unwrap_or(default))
}
fn boolean(lines: &[ContentLine], name: &str) -> Result<bool, Box<dyn Error>> {
    field(lines, 1, name)
        .map(|line| match scalar(line)? {
            value if value.eq_ignore_ascii_case("yes") => Ok(true),
            value if value.eq_ignore_ascii_case("no") => Ok(false),
            _ => Err(format!("invalid {name} boolean").into()),
        })
        .transpose()
        .map(|value| value.unwrap_or(false))
}

#[cfg(test)]
mod deletion_tests {
    use super::*;

    #[test]
    fn deletion_defaults_and_repeated_ranges_retain_source_order() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "deletion.ini".into(),
            origin: "authored deletion fixture".into(),
            bytes: b"Object Field\nBehavior=DeletionUpdate First\nEnd\nBehavior=LifetimeUpdate Other\nMinLifetime=9000\nMaxLifetime=9000\nEnd\nBehavior=DeletionUpdate Second\nMinLifetime=1000\nMaxLifetime=2000\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            deletion_rules(&file.declarations[0].lines).unwrap(),
            [[0, 0], [30, 60]]
        );
        assert_eq!(
            lifetime_rules(&file.declarations[0].lines).unwrap(),
            [[270, 270]]
        );
    }
}
