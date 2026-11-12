use super::{blocks, duration_frames, field, finite_number, scalar};
use generals_assets::content::{ContentFile, ContentLine};
use generals_sim::skirmish::countermeasures::Rules;
use std::error::Error;

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut result = Vec::new();
    for name in selected {
        let object = super::definition(files, "Object", name)?;
        for module in blocks(&object.lines, 0, "Behavior")? {
            if module[0]
                .tokens
                .get(1)
                .is_none_or(|name| name != "CountermeasuresBehavior")
            {
                continue;
            }
            let Some(line) = field(module, 1, "FlareTemplateName") else {
                continue;
            };
            let name = scalar(line)?;
            if name.is_empty()
                || name.eq_ignore_ascii_case("NONE")
                || selected.contains(&name)
                || result.contains(&name)
            {
                continue;
            }
            let object = super::definition(files, "Object", name)?;
            result.push(object.name.as_str());
        }
    }
    Ok(result)
}

pub(super) fn rules(lines: &[ContentLine]) -> Result<Vec<Rules>, Box<dyn Error>> {
    blocks(lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "CountermeasuresBehavior")
        })
        .map(parse)
        .collect()
}

fn parse(lines: &[ContentLine]) -> Result<Rules, Box<dyn Error>> {
    for key in ["FXListUpgrade", "RemovesUpgrades"] {
        if field(lines, 1, key).is_some() {
            return Err(format!("CountermeasuresBehavior: untranslated UpgradeMux {key}").into());
        }
    }
    Ok(Rules {
        triggers: names(lines, "TriggeredBy"),
        conflicts: names(lines, "ConflictsWith"),
        requires_all: match field(lines, 1, "RequiresAllTriggers")
            .map(scalar)
            .transpose()?
        {
            None => false,
            Some(value) if value.eq_ignore_ascii_case("yes") => true,
            Some(value) if value.eq_ignore_ascii_case("no") => false,
            Some(_) => return Err("invalid RequiresAllTriggers boolean".into()),
        },
        flare: field(lines, 1, "FlareTemplateName")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
        evasion_rate: field(lines, 1, "EvasionRate")
            .map(|line| {
                let value = scalar(line)?;
                finite_number(
                    value
                        .strip_suffix('%')
                        .ok_or("EvasionRate requires percent")?,
                )
                .map(|value| value * 0.01)
            })
            .transpose()?
            .unwrap_or(0.0),
        volley_size: integer(lines, "VolleySize")?,
        arc: number(lines, "VolleyArcAngle", 0.0)? * (std::f32::consts::PI / 180.0),
        velocity_factor: number(lines, "VolleyVelocityFactor", 1.0)?,
        volley_delay: frames(lines, "DelayBetweenVolleys")?,
        volleys: integer(lines, "NumberOfVolleys")?,
        reload: frames(lines, "ReloadTime")?,
        decoy_delay: frames(lines, "MissileDecoyDelay")?,
        // Like reloadFrame, allocator-zeroed when absent from the authored module.
        reaction_delay: frames(lines, "ReactionLaunchLatency")?,
    })
}
fn names(lines: &[ContentLine], name: &str) -> Vec<String> {
    field(lines, 1, name).map_or_else(Vec::new, |line| line.tokens[1..].to_vec())
}
fn integer(lines: &[ContentLine], name: &str) -> Result<u32, Box<dyn Error>> {
    Ok(field(lines, 1, name)
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse()?)
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

#[cfg(test)]
mod tests {
    #[test]
    fn native_upgrade_side_effect_fields_reject_unbound_consumers() {
        for field in [
            "FXListUpgrade=FlareUpgrade",
            "RemovesUpgrades=EarlierUpgrade",
        ] {
            let source = format!(
                "Object Aircraft\nBehavior=CountermeasuresBehavior First\n{field}\nEnd\nEnd\n"
            );
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "Object.ini".into(),
                origin: "native UpgradeMux field admission".into(),
                bytes: source.into_bytes(),
            })
            .unwrap();
            let error = super::rules(&file.declarations[0].lines).err().unwrap();
            assert!(error.to_string().contains(field.split('=').next().unwrap()));
        }
    }

    #[test]
    fn ordered_volley_data_keeps_zero_defaults_and_per_frame_delays() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Object.ini".into(),
            origin: "CountermeasuresBehavior module field contract".into(),
            bytes: b"Object Aircraft\nBehavior=CountermeasuresBehavior First\nTriggeredBy=Upgrade_AmericaCountermeasures\nFlareTemplateName=CountermeasureFlare\nVolleySize=2\nVolleyArcAngle=30\nNumberOfVolleys=3\nEvasionRate=30%\nReloadTime=4000\nReactionLaunchLatency=100\nMissileDecoyDelay=500\nEnd\nBehavior=CountermeasuresBehavior Second\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let parsed = super::rules(&file.declarations[0].lines).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].volley_size, 2);
        assert_eq!(parsed[0].volleys, 3);
        assert_eq!(parsed[0].reload, 120);
        assert_eq!(parsed[0].reaction_delay, 3);
        assert_eq!(parsed[0].decoy_delay, 15);
        assert_eq!(parsed[1].volley_size, 0);
        assert_eq!(parsed[1].reaction_delay, 0);
        assert_eq!(parsed[1].velocity_factor.to_bits(), 1.0_f32.to_bits());
    }
}
