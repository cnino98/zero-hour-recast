use super::module_metadata::{self, TemplateMetadata};
use generals_assets::{
    content::{self, ContentFile},
    virtual_assets::VirtualAssets,
};
use std::{collections::BTreeMap, error::Error};

type BoxError = Box<dyn Error>;

const SOURCE_NAMES: &[&str] = &[
    "default/object",
    "object/factionbuilding",
    "object/civilianbuilding",
    "object/civilianprop",
    "object/natureprop",
    "object/techbuildings",
    "object/civilianunit",
    "object/americaair",
    "object/americamiscunit",
    "object/chinaair",
    "object/chinamiscunit",
    "object/glaair",
    "object/glamiscunit",
    "object/hulk",
    "object/weaponobjects",
    "object/specialpowerobjects",
    "object/system",
    "object/gc_chem_glasystem",
    "object/americainfantry",
    "object/americavehicle",
    "object/chinainfantry",
    "object/chinavehicle",
    "objectcreationlist",
    "object/glainfantry",
    "object/glavehicle",
    "weapon",
    "crate",
    "locomotor",
    "armor",
    "damagefx",
    "fxlist",
    "gamedata",
    "roads",
    "miscaudio",
    "upgrade",
    "specialpower",
    "science",
    "commandbutton",
    "commandset",
    "particlesystem",
];

pub(super) fn lexical(
    assets: &VirtualAssets,
    baseline: bool,
) -> Result<Vec<ContentFile>, BoxError> {
    let mut files = Vec::new();
    for name in SOURCE_NAMES {
        let path = format!("data/ini/{name}.ini");
        let asset = if baseline {
            assets.read_without_map_ini(&path)?
        } else {
            assets.read(&path)?
        };
        files.push(content::scan(asset)?);
    }
    Ok(files)
}

pub fn metadata_baseline(assets: &VirtualAssets) -> Result<Vec<ContentFile>, BoxError> {
    lexical(assets, true)
}

pub(super) fn overrides(assets: &VirtualAssets) -> Result<Vec<ContentFile>, BoxError> {
    assets
        .map_ini_sources()
        .iter()
        .map(|asset| {
            content::scan(generals_assets::virtual_assets::Asset {
                path: asset.path.clone(),
                origin: asset.origin.clone(),
                bytes: asset.bytes.clone(),
            })
            .map_err(Into::into)
        })
        .collect()
}

/// Resolve the concrete native object closure before any catalog/model consumer.
pub(super) fn with_runtime_dependencies(
    assets: &VirtualAssets,
    roster: &mut Vec<(
        String,
        generals_sim::skirmish::Faction,
        generals_sim::skirmish::Kind,
    )>,
) -> Result<(Vec<ContentFile>, BTreeMap<String, TemplateMetadata>), BoxError> {
    loop {
        let selected: Vec<_> = roster.iter().map(|(name, _, _)| name.as_str()).collect();
        let (files, metadata) = effective(assets, &selected)?;
        let ropes = super::chinook::rope_templates(&files, &selected)?;
        let (files, metadata) = if ropes.is_empty() {
            (files, metadata)
        } else {
            let mut drawable_selection = selected.clone();
            for rope in &ropes {
                if !drawable_selection.contains(&rope.as_str()) {
                    drawable_selection.push(rope);
                }
            }
            effective(assets, &drawable_selection)?
        };
        let additions = runtime_dependencies(&files, &selected)?;
        if additions.is_empty() {
            return Ok((files, metadata));
        }
        roster.extend(additions.into_iter().map(|name| {
            (
                name.to_owned(),
                generals_sim::skirmish::Faction::Civilian,
                generals_sim::skirmish::Kind::Effect,
            )
        }));
    }
}

fn runtime_dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, BoxError> {
    let mut additions = super::projectile_templates::dependencies(files, selected)?;
    for name in super::power_ocl::dependencies(files, selected)?
        .into_iter()
        .chain(super::particle_uplink::dependencies(files, selected)?)
    {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::hazards::field_templates(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::create_object_die::dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::eject_pilot::dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::reinforcement::dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::countermeasures::dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in aircraft_dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in slow_death_dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::object_creation::dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in variation_dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::minefield::dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::assisted_targeting::dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    for name in super::toppling::dependencies(files, selected)? {
        if !selected.contains(&name) && !additions.contains(&name) {
            additions.push(name);
        }
    }
    Ok(additions)
}

fn variation_dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, BoxError> {
    let mut result = Vec::new();
    for name in selected {
        let object = super::definition(files, "Object", name)?;
        if let Some(line) = super::field(&object.lines, 0, "BuildVariations") {
            for name in line.tokens.iter().skip(1) {
                let template = super::definition(files, "Object", name)?;
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

fn aircraft_dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, BoxError> {
    let mut result = Vec::new();
    for name in selected {
        let object = super::definition(files, "Object", name)?;
        for module in super::blocks(&object.lines, 0, "Behavior")? {
            if module[0].tokens.get(1).is_some_and(|family| {
                matches!(
                    family.as_str(),
                    "SpecialAbility"
                        | "OCLSpecialPower"
                        | "SpyVisionSpecialPower"
                        | "CashHackSpecialPower"
                )
            }) {
                for rules in super::special_power::load(files, object)? {
                    if rules.view_range != 0.0
                        && rules.view_frames != 0
                        && let Some(name) = rules.view_template
                        && !selected.contains(&name.as_str())
                        && !result.contains(&name.as_str())
                    {
                        result.push(super::definition(files, "Object", &name)?.name.as_str());
                    }
                }
            }
            if module[0]
                .tokens
                .get(1)
                .is_some_and(|family| family == "SpecialAbilityUpdate")
                && let Some(line) = super::field(module, 1, "SpecialObject")
            {
                let name = super::scalar(line)?;
                if let Some(template) = files
                    .iter()
                    .flat_map(|file| &file.declarations)
                    .find(|template| template.kind == "Object" && template.name == name)
                    && !selected.contains(&template.name.as_str())
                    && !result.contains(&template.name.as_str())
                {
                    result.push(template.name.as_str());
                }
            }
            let key = match module[0].tokens.get(1).map(String::as_str) {
                Some("SpectreGunshipDeploymentUpdate") => "GunshipTemplateName",
                Some("SpectreGunshipUpdate") => "GattlingTemplateName",
                Some("JetAIUpdate") => "LockonCursor",
                Some("HelicopterSlowDeathBehavior") => "FinalRubbleObject",
                _ => continue,
            };
            let Some(line) = super::field(module, 1, key) else {
                continue;
            };
            let name = super::scalar(line)?;
            if name.is_empty()
                || name.eq_ignore_ascii_case("NONE")
                || selected.contains(&name)
                || result.contains(&name)
            {
                continue;
            }
            result.push(super::definition(files, "Object", name)?.name.as_str());
        }
    }
    Ok(result)
}

fn slow_death_dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, BoxError> {
    let mut result = Vec::new();
    for name in selected {
        let object = super::definition(files, "Object", name)?;
        for module in super::blocks(&object.lines, 0, "Behavior")? {
            if module[0].tokens.get(1).is_none_or(|family| {
                !matches!(
                    family.as_str(),
                    "SlowDeathBehavior"
                        | "JetSlowDeathBehavior"
                        | "HelicopterSlowDeathBehavior"
                        | "NeutronMissileSlowDeathBehavior"
                )
            }) {
                continue;
            }
            for line in module.iter().filter(|line| {
                line.depth == Some(1)
                    && line
                        .tokens
                        .first()
                        .is_some_and(|key| key == "OCL" || key.starts_with("OCL"))
            }) {
                let start = if line.tokens[0] == "OCL" { 2 } else { 1 };
                for name in &line.tokens[start..] {
                    let Some(list) = files
                        .iter()
                        .flat_map(|file| &file.declarations)
                        .find(|entry| entry.kind == "ObjectCreationList" && &entry.name == name)
                    else {
                        continue;
                    };
                    for nugget in super::blocks(&list.lines, 0, "CreateObject")? {
                        for line in nugget.iter().filter(|line| {
                            line.tokens
                                .first()
                                .is_some_and(|key| key == "ObjectNames" || key == "PutInContainer")
                        }) {
                            for name in &line.tokens[1..] {
                                if let Some(object) = files
                                    .iter()
                                    .flat_map(|file| &file.declarations)
                                    .find(|entry| entry.kind == "Object" && &entry.name == name)
                                    && !result.contains(&object.name.as_str())
                                {
                                    result.push(object.name.as_str());
                                }
                            }
                        }
                    }
                    if !super::blocks(&list.lines, 0, "CreateDebris")?.is_empty()
                        && !result.contains(&"GenericDebris")
                    {
                        result.push(
                            super::definition(files, "Object", "GenericDebris")?
                                .name
                                .as_str(),
                        );
                    }
                }
            }
        }
    }
    Ok(result)
}

pub fn effective(
    assets: &VirtualAssets,
    selected: &[&str],
) -> Result<(Vec<ContentFile>, BTreeMap<String, TemplateMetadata>), BoxError> {
    let baseline = metadata_baseline(assets)?;
    let overrides = overrides(assets)?;
    if overrides.is_empty() {
        for file in &baseline {
            if file
                .declarations
                .iter()
                .any(|declaration| matches!(declaration.kind.as_str(), "Object" | "ObjectReskin"))
                && assets.map_ini_asset(&file.path)?.is_some()
            {
                return Err(
                    "Map-local object content requires original ordered override provenance".into(),
                );
            }
        }
    }
    let metadata = module_metadata::load(&baseline, &overrides, selected)?;
    let mut files = lexical(assets, false)?;
    super::resolve_reskins(&mut files)?;
    module_metadata::reconcile(&mut files, &baseline, &overrides, &metadata)?;
    Ok((files, metadata))
}

#[cfg(test)]
mod tests;
