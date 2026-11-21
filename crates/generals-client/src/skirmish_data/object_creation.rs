//! Original `ObjectCreationUpgrade` pointers and ordinary creation nuggets.
use super::{blocks, definition, field, physical_ocl, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    object_creation::{self, List, Object},
    physical_ocl as disposition,
};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Option<List>>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "ObjectCreationUpgrade")
        {
            continue;
        }
        let name = field(module, 1, "UpgradeObject").map(scalar).transpose()?;
        let name = name
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .filter(|name| {
                files
                    .iter()
                    .flat_map(|file| &file.declarations)
                    .any(|declaration| {
                        declaration.kind == "ObjectCreationList" && &declaration.name == name
                    })
            });
        let list = name
            .map(|name| definition(files, "ObjectCreationList", name))
            .transpose()?;
        result.push(list.map(parse_list).transpose()?);
    }
    Ok(result)
}

pub(super) fn parse_list(list: &Declaration) -> Result<List, Box<dyn Error>> {
    for line in &list.lines {
        if line.depth == Some(0)
            && !matches!(
                line.tokens.first().map(String::as_str),
                Some("CreateObject" | "End")
            )
        {
            return Err(format!(
                "{}: ordinary upgrade OCL has untranslated nugget at line {}",
                list.name, line.line
            )
            .into());
        }
    }
    Ok(List {
        name: list.name.clone(),
        objects: blocks(&list.lines, 0, "CreateObject")?
            .into_iter()
            .map(parse_object)
            .collect::<Result<_, _>>()?,
    })
}

pub(super) fn parse_object(lines: &[ContentLine]) -> Result<Object, Box<dyn Error>> {
    let mut object = Object {
        names: Vec::new(),
        count: 1,
        offset: [0.0; 3],
        disposition: disposition::ON_GROUND,
        health: [1.0; 2],
        lifetime: [0; 2],
        flags: object_creation::PRESERVE_LAYER,
        container: String::new(),
        invulnerable_time: 0,
        force_intensity: 0.0,
        force_magnitude: [0.0; 2],
        force_pitch: [0.0; 2],
        spin_rate: -1.0,
        formation_distances: [0.0; 3],
        fade_frames: 0,
        fade_sound: String::new(),
    };
    // INI callbacks mutate the same native nugget in authored order.
    for line in lines.iter().filter(|line| line.depth == Some(1)) {
        let key = line.tokens.first().map_or("", String::as_str);
        match key {
            "ObjectNames" => object.names.extend_from_slice(&line.tokens[1..]),
            "Count" => {
                object.count = i32::from_ne_bytes(
                    super::source_numbers::unsigned(scalar(line)?)?.to_ne_bytes(),
                );
            }
            "Offset" => object.offset = physical_ocl::coordinate(line)?,
            "PutInContainer" => object.container = scalar(line)?.into(),
            "MinHealth" => object.health[0] = percent(line)?,
            "MaxHealth" => object.health[1] = percent(line)?,
            "MinLifetime" => {
                object.lifetime[0] = super::source_numbers::duration_frames(scalar(line)?)?;
            }
            "MaxLifetime" => {
                object.lifetime[1] = super::source_numbers::duration_frames(scalar(line)?)?;
            }
            "InvulnerableTime" => {
                object.invulnerable_time = super::source_numbers::duration_frames(scalar(line)?)?;
            }
            "DispositionIntensity" => object.force_intensity = super::finite_number(scalar(line)?)?,
            "MinForceMagnitude" => object.force_magnitude[0] = super::finite_number(scalar(line)?)?,
            "MaxForceMagnitude" => object.force_magnitude[1] = super::finite_number(scalar(line)?)?,
            "MinForcePitch" => {
                object.force_pitch[0] =
                    super::finite_number(scalar(line)?)? * (std::f32::consts::PI / 180.0);
            }
            "MaxForcePitch" => {
                object.force_pitch[1] =
                    super::finite_number(scalar(line)?)? * (std::f32::consts::PI / 180.0);
            }
            "SpinRate" => {
                object.spin_rate = super::finite_number(scalar(line)?)?
                    * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0));
            }
            "MinDistanceAFormation" | "MinDistanceBFormation" | "MaxDistanceFormation" => {
                let index = match key {
                    "MinDistanceAFormation" => 0,
                    "MinDistanceBFormation" => 1,
                    _ => 2,
                };
                object.formation_distances[index] = super::finite_number(scalar(line)?)?;
            }
            "FadeTime" => {
                object.fade_frames = super::source_numbers::duration_frames(scalar(line)?)?;
            }
            "FadeSound" => object.fade_sound = scalar(line)?.into(),
            "Disposition" => object.disposition = parse_disposition(line)?,
            "RequiresLivePlayer"
            | "ContainInsideSourceObject"
            | "PreserveLayer"
            | "IgnorePrimaryObstacle"
            | "InheritsVeterancy"
            | "DiesOnBadLand"
            | "SpreadFormation"
            | "FadeIn"
            | "FadeOut" => {
                let flag = match key {
                    "RequiresLivePlayer" => object_creation::REQUIRES_LIVE_PLAYER,
                    "ContainInsideSourceObject" => object_creation::CONTAIN_INSIDE_SOURCE,
                    "PreserveLayer" => object_creation::PRESERVE_LAYER,
                    "IgnorePrimaryObstacle" => object_creation::IGNORE_PRIMARY_OBSTACLE,
                    "InheritsVeterancy" => object_creation::INHERITS_VETERANCY,
                    "DiesOnBadLand" => object_creation::DIES_ON_BAD_LAND,
                    "SpreadFormation" => object_creation::SPREAD_FORMATION,
                    "FadeIn" => object_creation::FADE_IN,
                    _ => object_creation::FADE_OUT,
                };
                let enabled = match scalar(line)?.to_ascii_lowercase().as_str() {
                    "yes" => true,
                    "no" => false,
                    _ => return Err(format!("invalid {key}").into()),
                };
                object.flags = (object.flags & !flag) | if enabled { flag } else { 0 };
            }
            "End" => {}
            _ => return Err(format!("untranslated ordinary OCL field {key}").into()),
        }
    }
    Ok(object)
}

fn parse_disposition(line: &ContentLine) -> Result<u16, Box<dyn Error>> {
    let mut flags = 0;
    for name in &line.tokens[1..] {
        flags |= match name.as_str() {
            "LIKE_EXISTING" => disposition::LIKE_EXISTING,
            "ON_GROUND_ALIGNED" => disposition::ON_GROUND,
            "RANDOM_FORCE" => disposition::RANDOM_FORCE,
            "SEND_IT_FLYING" => disposition::SEND_FLYING,
            "SEND_IT_UP" => disposition::SEND_UP,
            "SEND_IT_OUT" => disposition::SEND_OUT,
            "INHERIT_VELOCITY" => disposition::INHERIT_VELOCITY,
            _ => {
                return Err(format!("untranslated ordinary OCL disposition {name}").into());
            }
        };
    }
    Ok(flags)
}

fn percent(line: &ContentLine) -> Result<f32, Box<dyn Error>> {
    Ok(super::finite_number(scalar(line)?.trim_end_matches('%'))? * 0.01)
}

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut result = Vec::new();
    for name in selected {
        let object = definition(files, "Object", name)?;
        for list in load(files, object)?.into_iter().flatten() {
            for nugget in list.objects {
                for name in nugget
                    .names
                    .into_iter()
                    .chain(std::iter::once(nugget.container))
                {
                    if !selected.contains(&name.as_str())
                        && !result.contains(&name.as_str())
                        && files
                            .iter()
                            .flat_map(|file| &file.declarations)
                            .any(|declaration| {
                                declaration.kind == "Object" && declaration.name == name
                            })
                    {
                        result.push(definition(files, "Object", &name)?.name.as_str());
                    }
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{dependencies, load};
    use generals_assets::{content, virtual_assets::Asset};

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_ranger_death_list_keeps_outward_disposition_and_live_player_gate() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = crate::skirmish_data::source_files(&assets).unwrap();
        let list =
            super::definition(&files, "ObjectCreationList", "OCL_AmericanRangerDebris06").unwrap();
        let parsed = super::parse_list(list).unwrap();
        assert_eq!(parsed.objects.len(), 1);
        let object = &parsed.objects[0];
        assert_eq!(object.names, ["AmericaInfantryRanger"]);
        assert_eq!(object.count, 6);
        assert_eq!(object.disposition, super::disposition::SEND_OUT);
        assert_eq!(object.force_intensity.to_bits(), 4.0_f32.to_bits());
        assert_eq!(
            object.flags,
            super::object_creation::PRESERVE_LAYER
                | super::object_creation::IGNORE_PRIMARY_OBSTACLE
                | super::object_creation::REQUIRES_LIVE_PLAYER
        );
    }

    #[test]
    fn repeated_object_names_append_in_source_order_and_discover_every_template() {
        let file = content::scan(Asset {
            path: "ocl.ini".into(),
            origin: "source-backed fixture".into(),
            bytes: b"Object Parent
  Behavior = ObjectCreationUpgrade First
    UpgradeObject = MissingList
  End
  Behavior = ObjectCreationUpgrade Second
    UpgradeObject = RepeatedNames
  End
End
Object FirstChild
End
Object SecondChild
End
Object ThirdChild
End
ObjectCreationList RepeatedNames
  CreateObject
    ObjectNames = FirstChild SecondChild
    ObjectNames = ThirdChild FirstChild
  End
End
"
            .to_vec(),
        })
        .unwrap();
        let files = vec![file];
        let object = super::definition(&files, "Object", "Parent").unwrap();
        let lists = load(&files, object).unwrap();
        assert_eq!(lists.len(), 2);
        assert!(lists[0].is_none());
        let list = lists[1].as_ref().unwrap();
        assert_eq!(
            list.objects[0].names,
            ["FirstChild", "SecondChild", "ThirdChild", "FirstChild"]
        );
        assert_eq!(
            dependencies(&files, &["Parent"]).unwrap(),
            ["FirstChild", "SecondChild", "ThirdChild"]
        );
    }
}
