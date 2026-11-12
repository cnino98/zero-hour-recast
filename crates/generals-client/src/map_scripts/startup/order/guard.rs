//! Admission boundary for raw priority production templates under map overrides.
use super::{BoxError, TemplateDefinition};
use generals_assets::{
    content::{self, ContentFile, Declaration},
    virtual_assets::{Asset, VirtualAssets},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Overrides {
    parents: BTreeMap<String, String>,
    changed: BTreeMap<String, &'static str>,
}

impl Overrides {
    pub(super) const fn new() -> Self {
        Self {
            parents: BTreeMap::new(),
            changed: BTreeMap::new(),
        }
    }

    pub(super) fn observe(
        &mut self,
        file: &ContentFile,
        assets: &VirtualAssets,
    ) -> Result<(), BoxError> {
        for declaration in &file.declarations {
            if !matches!(declaration.kind.as_str(), "Object" | "ObjectReskin") {
                continue;
            }
            let parent = declaration
                .parent
                .as_deref()
                .unwrap_or("DefaultThingTemplate");
            if parent != declaration.name {
                self.parents.insert(declaration.name.clone(), parent.into());
            }
        }
        let Some(asset) = assets.map_ini_asset(&file.path)? else {
            return Ok(());
        };
        let effective = content::scan(Asset {
            path: asset.path.clone(),
            origin: asset.origin.clone(),
            bytes: asset.bytes.clone(),
        })?;
        for original in &file.declarations {
            if !matches!(original.kind.as_str(), "Object" | "ObjectReskin") {
                continue;
            }
            let Some(changed) = effective.declarations.iter().find(|declaration| {
                declaration.name == original.name && declaration.kind == original.kind
            }) else {
                return Err(
                    format!("Map override lost original template {}", original.name).into(),
                );
            };
            for field in [
                "BuildCost",
                "BuildTime",
                "KindOf",
                "BuildVariations",
                "MaxSimultaneousOfType",
                "MaxSimultaneousLinkKey",
            ] {
                if values(original, field) != values(changed, field) {
                    self.changed.insert(original.name.clone(), field);
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate(&self, templates: &[TemplateDefinition]) -> Result<(), BoxError> {
        for template in templates
            .iter()
            .filter(|template| template.dozer || template.harvester)
        {
            let mut name = template.name.as_str();
            let mut seen = BTreeSet::new();
            while seen.insert(name) {
                if let Some(field) = self.changed.get(name) {
                    return Err(format!("{}: map override of {name}.{field} requires the P04 raw priority-production template consumer", template.name).into());
                }
                let Some(parent) = self.parents.get(name) else {
                    break;
                };
                name = parent;
            }
        }
        Ok(())
    }
}

fn values<'a>(declaration: &'a Declaration, field: &str) -> Vec<&'a [String]> {
    declaration
        .lines
        .iter()
        .filter(|line| {
            line.depth == Some(0)
                && line
                    .tokens
                    .first()
                    .is_some_and(|name| name.eq_ignore_ascii_case(field))
        })
        .map(|line| &line.tokens[1..])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_dozer_cost_override_is_rejected_while_unrelated_structure_changes_remain_available() {
        let original = content::scan(Asset {
            path: "data/ini/object/america.ini".into(),
            origin: "source raw-template fixture".into(),
            bytes: b"Object AmericaVehicleDozer\nKindOf=DOZER VEHICLE\nBuildCost=1000\nBuildTime=5.0\nEnd\nObject AmericaBarracks\nKindOf=STRUCTURE\nBuildCost=600\nEnd\n".to_vec(),
        }).unwrap();
        let templates = [TemplateDefinition {
            name: "AmericaVehicleDozer".into(),
            dozer: true,
            harvester: false,
        }];
        for (cost, accepted) in [("1000", true), ("100", false)] {
            let assets = VirtualAssets::from_layers(Vec::new()).with_map_ini(vec![Asset {
                path: original.path.clone(),
                origin: "maps/custom/map.ini".into(),
                bytes: format!("Object AmericaVehicleDozer\nKindOf=DOZER VEHICLE\nBuildCost={cost}\nBuildTime=5.0\nEnd\nObject AmericaBarracks\nKindOf=STRUCTURE\nBuildCost=50\nEnd\n").into_bytes(),
            }]).unwrap();
            let mut guard = Overrides::new();
            guard.observe(&original, &assets).unwrap();
            let result = guard.validate(&templates);
            assert_eq!(result.is_ok(), accepted);
            if !accepted {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("raw priority-production")
                );
            }
        }
    }
}
