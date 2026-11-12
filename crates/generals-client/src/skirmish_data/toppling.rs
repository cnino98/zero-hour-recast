//! Original `ToppleUpdate` authored instances and stump-template dependencies.
use super::{blocks, definition, scalar, source_real};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::toppling::{FinishRules, Rules};
use std::error::Error;

type ErrorBox = Box<dyn Error>;

pub(super) fn load(files: &[ContentFile], object: &Declaration) -> Result<Vec<Rules>, ErrorBox> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "ToppleUpdate")
        {
            continue;
        }
        let mut rules = Rules {
            topple_fx: String::new(),
            bounce_fx: String::new(),
            stump: None,
            initial_velocity: 0.2,
            initial_acceleration: 0.01,
            bounce_velocity: 0.3,
            kill_on_start: false,
            left_or_right: false,
            finish: FinishRules {
                kill: true,
                kill_stump: false,
                reorient: false,
            },
        };
        for line in module.iter().skip(1).filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("ToppleFX") => rules.topple_fx = effect(files, scalar(line)?)?,
                Some("BounceFX") => rules.bounce_fx = effect(files, scalar(line)?)?,
                Some("StumpName") => {
                    let name = scalar(line)?;
                    rules.stump = (!name.is_empty()).then(|| name.to_owned());
                    if let Some(name) = &rules.stump {
                        definition(files, "Object", name)?;
                    }
                }
                Some("KillWhenStartToppling") => rules.kill_on_start = boolean(line)?,
                Some("KillWhenFinishedToppling") => rules.finish.kill = boolean(line)?,
                Some("KillStumpWhenToppled") => rules.finish.kill_stump = boolean(line)?,
                Some("ToppleLeftOrRightOnly") => rules.left_or_right = boolean(line)?,
                Some("ReorientToppledRubble") => rules.finish.reorient = boolean(line)?,
                Some("InitialVelocityPercent") => rules.initial_velocity = percent(line)?,
                Some("InitialAccelPercent") => rules.initial_acceleration = percent(line)?,
                Some("BounceVelocityPercent") => rules.bounce_velocity = percent(line)?,
                Some("End") => {}
                _ => return Err(format!("unsupported ToppleUpdate field at {}", line.line).into()),
            }
        }
        result.push(rules);
    }
    Ok(result)
}

fn effect(files: &[ContentFile], name: &str) -> Result<String, ErrorBox> {
    Ok(if super::fx_definition(files, name)?.is_some() {
        name.to_owned()
    } else {
        String::new()
    })
}

fn boolean(line: &ContentLine) -> Result<bool, ErrorBox> {
    match scalar(line)?.to_ascii_uppercase().as_str() {
        "YES" => Ok(true),
        "NO" => Ok(false),
        _ => Err("invalid ToppleUpdate boolean".into()),
    }
}

fn percent(line: &ContentLine) -> Result<f32, ErrorBox> {
    Ok(source_real(scalar(line)?.trim_end_matches('%'))? / 100.0)
}

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, ErrorBox> {
    let mut result = Vec::new();
    for name in selected {
        let object = definition(files, "Object", name)?;
        for rules in load(files, object)? {
            if let Some(name) = rules.stump {
                let template = definition(files, "Object", &name)?;
                if !selected.contains(&template.name.as_str())
                    && !result.contains(&template.name.as_str())
                {
                    result.push(template.name.as_str());
                }
            }
        }
    }
    Ok(result)
}

pub(super) fn sway_updates(object: &Declaration) -> Result<usize, ErrorBox> {
    let mut count = 0;
    for module in blocks(&object.lines, 0, "ClientUpdate")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "SwayClientUpdate")
        {
            continue;
        }
        if module
            .iter()
            .skip(1)
            .any(|line| line.tokens.first().is_some_and(|field| field != "End"))
        {
            return Err("SwayClientUpdate has no authored data fields".into());
        }
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn authored_instances_keep_defaults_percent_order_and_nullable_fx() {
        let file = content::scan(Asset { path: "topple.ini".into(), origin: "source fixture".into(),
            bytes: b"Object Pole\nBehavior=ToppleUpdate First\nEnd\nBehavior=ToppleUpdate Second\nInitialVelocityPercent=25%\nInitialVelocityPercent=-10%\nKillWhenFinishedToppling=No\nToppleLeftOrRightOnly=Yes\nToppleFX=Missing\nBounceFX=Bounce\nStumpName=Stump\nEnd\nClientUpdate=SwayClientUpdate FirstSway\nEnd\nClientUpdate=SwayClientUpdate SecondSway\nEnd\nEnd\nObject Stump\nEnd\nFXList Bounce\nEnd\n".to_vec() }).unwrap();
        let rules = load(std::slice::from_ref(&file), &file.declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].initial_velocity.to_bits(), 0.2_f32.to_bits());
        assert!(rules[0].finish.kill);
        assert_eq!(
            rules[1].initial_velocity.to_bits(),
            (-10.0_f32 / 100.0).to_bits()
        );
        assert!(!rules[1].finish.kill && rules[1].left_or_right);
        assert!(rules[1].topple_fx.is_empty());
        assert_eq!(rules[1].bounce_fx, "Bounce");
        assert_eq!(sway_updates(&file.declarations[0]).unwrap(), 2);
        assert_eq!(
            dependencies(std::slice::from_ref(&file), &["Pole"]).unwrap(),
            ["Stump"]
        );
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_ordinary_tree_and_lamp_keep_the_original_behavior_and_draw_modules() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        let tree = definition(&files, "Object", "GenericTree").unwrap();
        let tree_rules = load(&files, tree).unwrap();
        assert_eq!(tree_rules.len(), 1);
        assert_eq!(tree_rules[0].stump.as_deref(), Some("TreeSpruceStump"));
        assert_eq!(sway_updates(tree).unwrap(), 1);
        assert!(!super::super::has_flag(
            &tree.lines,
            "KindOf",
            "OPTIMIZED_TREE"
        ));
        let lamp = definition(&files, "Object", "StreetLamp").unwrap();
        assert_eq!(load(&files, lamp).unwrap().len(), 1);
        assert_eq!(sway_updates(lamp).unwrap(), 0);
        let optimized = definition(&files, "Object", "TreeFir01B").unwrap();
        assert!(super::super::has_flag(
            &optimized.lines,
            "KindOf",
            "OPTIMIZED_TREE"
        ));
    }
}
