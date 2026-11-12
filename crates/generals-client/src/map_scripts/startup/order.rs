//! Original `ThingFactory::addTemplate` reverse registration order.
use super::BoxError;
use generals_assets::{AssetError, content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::map_scripts::TemplateDefinition;
use std::collections::BTreeMap;

mod guard;

pub(super) fn load(assets: &VirtualAssets) -> Result<Vec<TemplateDefinition>, BoxError> {
    let mut names = Vec::new();
    let mut registered = BTreeMap::new();
    let mut overrides = guard::Overrides::new();
    // GameEngine::init registers ThingFactory before CrateSystem; each
    // subsystem loads its Default path before the ordinary path.
    for prefix in [
        "data/ini/default/object",
        "data/ini/object",
        "data/ini/default/crate",
        "data/ini/crate",
    ] {
        let standalone = format!("{prefix}.ini");
        match assets.read_without_map_ini(&standalone) {
            Ok(asset) => {
                let file = content::scan(asset)?;
                overrides.observe(&file, assets)?;
                append(file, &mut names, &mut registered)?;
            }
            Err(AssetError::Missing(_)) => {}
            Err(error) => return Err(error.into()),
        }
        for path in assets.list_without_map_ini(prefix)? {
            if std::path::Path::new(&path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
            {
                let file = content::scan(assets.read_without_map_ini(&path)?)?;
                overrides.observe(&file, assets)?;
                append(file, &mut names, &mut registered)?;
            }
        }
    }
    names.reverse();
    overrides.validate(&names)?;
    Ok(names)
}

fn append(
    file: content::ContentFile,
    names: &mut Vec<TemplateDefinition>,
    registered: &mut BTreeMap<String, usize>,
) -> Result<(), BoxError> {
    for object in file.declarations {
        if !matches!(object.kind.as_str(), "Object" | "ObjectReskin") {
            continue;
        }
        let index = if let Some(&index) = registered.get(&object.name) {
            index
        } else {
            let parent = object.parent.as_deref().unwrap_or("DefaultThingTemplate");
            let inherited = registered.get(parent).map(|&index| &names[index]);
            let definition = TemplateDefinition {
                name: object.name.clone(),
                dozer: inherited.is_some_and(|template| template.dozer),
                harvester: inherited.is_some_and(|template| template.harvester),
            };
            if object.parent.is_some() && inherited.is_none() {
                return Err(format!(
                    "ObjectReskin {} has no registered parent {parent}",
                    object.name
                )
                .into());
            }
            let index = names.len();
            names.push(definition);
            registered.insert(object.name, index);
            index
        };
        for line in object.lines.iter().filter(|line| {
            line.depth == Some(0) && line.tokens.first().is_some_and(|field| field == "KindOf")
        }) {
            flags(&mut names[index], &line.tokens[1..])?;
        }
    }
    Ok(())
}

fn flags(template: &mut TemplateDefinition, tokens: &[String]) -> Result<(), BoxError> {
    let mut normal = false;
    let mut delta = false;
    for token in tokens {
        if token.eq_ignore_ascii_case("NONE") {
            if normal || delta {
                return Err("Mixed native KindOf flag operations".into());
            }
            template.dozer = false;
            template.harvester = false;
            break;
        }
        let (flag, enabled, change) = token.strip_prefix('+').map_or_else(
            || {
                token
                    .strip_prefix('-')
                    .map_or((token.as_str(), true, false), |flag| (flag, false, true))
            },
            |flag| (flag, true, true),
        );
        if change {
            if normal {
                return Err("Mixed native KindOf flag operations".into());
            }
            delta = true;
        } else {
            if delta {
                return Err("Mixed native KindOf flag operations".into());
            }
            if !normal {
                template.dozer = false;
                template.harvester = false;
            }
            normal = true;
        }
        if flag.eq_ignore_ascii_case("DOZER") {
            template.dozer = enabled;
        }
        if flag.eq_ignore_ascii_case("HARVESTER") {
            template.harvester = enabled;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;
    #[test]
    fn overrides_keep_registration_identity_while_reskins_register_as_new_templates() {
        let mut names = Vec::new();
        let mut registered = BTreeMap::new();
        let file = content::scan(Asset {path: "objects.ini".into(), origin: "native registration fixture".into(), bytes: b"Object Worker\nEnd\nObject Factory\nEnd\nObject Worker\nEnd\nObjectReskin NewWorker Worker\nEnd\n".to_vec()}).unwrap();
        append(file, &mut names, &mut registered).unwrap();
        names.reverse();
        assert_eq!(
            names
                .iter()
                .map(|template| template.name.as_str())
                .collect::<Vec<_>>(),
            ["NewWorker", "Factory", "Worker"]
        );
    }

    #[test]
    fn raw_registration_inherits_default_and_reskin_flags_and_preserves_delta_fields() {
        let mut names = Vec::new();
        let mut registered = BTreeMap::new();
        let file = content::scan(Asset {
            path: "objects.ini".into(),
            origin: "native raw-template flags fixture".into(),
            bytes: b"Object DefaultThingTemplate\nKindOf=HARVESTER\nEnd\nObject Worker\nKindOf=+DOZER\nEnd\nObjectReskin NewWorker Worker\nEnd\nObject Factory\nKindOf=STRUCTURE\nEnd\nObject Worker\nKindOf=-HARVESTER\nEnd\n".to_vec(),
        }).unwrap();
        append(file, &mut names, &mut registered).unwrap();
        let worker = &names[registered["Worker"]];
        assert!(worker.dozer && !worker.harvester);
        let reskin = &names[registered["NewWorker"]];
        assert!(reskin.dozer && reskin.harvester);
        let factory = &names[registered["Factory"]];
        assert!(!factory.dozer && !factory.harvester);
    }
}
