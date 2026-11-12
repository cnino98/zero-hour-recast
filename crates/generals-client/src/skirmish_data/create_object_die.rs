//! `CreateObjectDieModuleData`, native generic/debris OCL dispatch in declaration order.
use super::{blocks, definition, field, scalar};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{
    create_object_die::{Action, Rules},
    object_creation, physical_ocl,
};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "CreateObjectDie")
        {
            continue;
        }
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens[0].as_str() {
                "CreationList" | "DeathTypes" | "VeterancyLevels" | "ExemptStatus"
                | "RequiredStatus" => {}
                "TransferPreviousHealth" | "TransferSelection" => {
                    if !scalar(line)?.eq_ignore_ascii_case("no") {
                        return Err(format!(
                            "CreateObjectDie {} requires actual first-object transfer behavior",
                            line.tokens[0]
                        )
                        .into());
                    }
                }
                key => return Err(format!("unsupported CreateObjectDie field {key}").into()),
            }
        }
        let name = field(module, 1, "CreationList")
            .map(scalar)
            .transpose()?
            .unwrap_or("");
        let list = super::instant_death::nullable(files, "ObjectCreationList", name)?;
        result.push(Rules {
            conditions: super::death_callbacks::conditions(module)?,
            actions: list.map(parse_list).transpose()?.unwrap_or_default(),
        });
    }
    Ok(result)
}

fn parse_list(list: &Declaration) -> Result<Vec<Action>, Box<dyn Error>> {
    for line in list.lines.iter().filter(|line| line.depth == Some(0)) {
        if !matches!(
            line.tokens[0].as_str(),
            "CreateObject" | "CreateDebris" | "ApplyRandomForce" | "End"
        ) {
            return Err(format!(
                "{}: untranslated CreateObjectDie OCL nugget {}",
                list.name, line.tokens[0]
            )
            .into());
        }
    }
    let mut nuggets = Vec::new();
    for family in ["CreateObject", "CreateDebris", "ApplyRandomForce"] {
        nuggets.extend(blocks(&list.lines, 0, family)?);
    }
    nuggets.sort_by_key(|module| module[0].line);
    let mut actions = Vec::new();
    for module in nuggets {
        let action = match module[0].tokens[0].as_str() {
            "CreateObject" => Action::Objects(object_creation::List {
                name: list.name.clone(),
                objects: vec![super::object_creation::parse_object(module)?],
            }),
            "CreateDebris" => Action::Physical(physical_ocl::List {
                name: list.name.clone(),
                initial_force: None,
                nuggets: vec![super::physical_ocl::nugget(module)?],
            }),
            _ => {
                if !actions.is_empty() {
                    return Err("only initial ApplyRandomForce is admitted".into());
                }
                Action::Physical(physical_ocl::List {
                    name: list.name.clone(),
                    initial_force: Some(super::physical_ocl::source_force(module)?),
                    nuggets: Vec::new(),
                })
            }
        };
        actions.push(action);
    }
    Ok(actions)
}

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut result = Vec::new();
    for name in selected {
        let object = definition(files, "Object", name)?;
        for action in load(files, object)?
            .into_iter()
            .flat_map(|rules| rules.actions)
        {
            match action {
                Action::Objects(list) => {
                    for object in list.objects {
                        for name in object
                            .names
                            .iter()
                            .chain(std::iter::once(&object.container))
                            .filter(|name| !name.is_empty())
                        {
                            let name = definition(files, "Object", name)?.name.as_str();
                            if !selected.contains(&name) && !result.contains(&name) {
                                result.push(name);
                            }
                        }
                    }
                }
                Action::Physical(list) => {
                    if !list.nuggets.is_empty() {
                        let name = definition(files, "Object", "GenericDebris")?.name.as_str();
                        if !selected.contains(&name) && !result.contains(&name) {
                            result.push(name);
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
