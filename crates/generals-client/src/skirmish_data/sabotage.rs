//! Authored Saboteur crate modules, preserving collision-handler order.
use super::{blocks, duration_frames, field, scalar, unsigned};
use generals_assets::content::Declaration;
use generals_sim::skirmish::sabotage::{Effect, Rules};
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut rules = Vec::new();
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let last_contact = modules.iter().rposition(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| matches!(name.as_str(), "PhysicsBehavior" | "SquishCollide"))
    });
    for (index, module) in modules.iter().enumerate() {
        let Some(name) = module[0].tokens.get(1) else {
            continue;
        };
        if !name.starts_with("Sabotage") {
            continue;
        }
        if last_contact.is_some_and(|contact| contact > index) {
            return Err("untranslated sabotage collision module ordering".into());
        }
        if field(module, 1, "BuildingPickup")
            .map(scalar)
            .transpose()?
            .is_none_or(|value| !value.eq_ignore_ascii_case("Yes"))
        {
            return Err("Sabotage crates require original BuildingPickup=Yes".into());
        }
        for key in [
            "ForbidOwnerPlayer",
            "HumanOnly",
            "AllowMultiPickup",
            "PickupScience",
            "ExecuteFX",
            "ExecuteAnimation",
            "FXList",
        ] {
            if field(module, 1, key).is_some() {
                return Err(format!("untranslated sabotage crate {key}").into());
            }
        }
        let duration = |key| {
            field(module, 1, key)
                .ok_or("Missing authored sabotage duration")
                .and_then(|line| duration_frames(line).map_err(|_| "Invalid sabotage duration"))
        };
        let effect = match name.as_str() {
            "SabotagePowerPlantCrateCollide" => Effect::PowerPlant {
                frames: duration("SabotagePowerDuration")?,
            },
            "SabotageSuperweaponCrateCollide" => Effect::Superweapon,
            "SabotageCommandCenterCrateCollide" => Effect::CommandCenter,
            "SabotageSupplyCenterCrateCollide" => Effect::SupplyCenter {
                amount: if field(module, 1, "StealCashAmount").is_some() {
                    unsigned(module, 1, "StealCashAmount")?
                } else {
                    0
                },
            },
            "SabotageMilitaryFactoryCrateCollide" => Effect::MilitaryFactory {
                frames: duration("SabotageDuration")?,
            },
            "SabotageFakeBuildingCrateCollide" => Effect::FakeBuilding,
            "SabotageInternetCenterCrateCollide" => Effect::InternetCenter {
                frames: duration("SabotageDuration")?,
            },
            _ => return Err(format!("untranslated sabotage crate {name}").into()),
        };
        rules.push(Rules {
            effect,
            required: super::salvage::words(module, 1, "RequiredKindOf"),
            forbidden: super::salvage::words(module, 1, "ForbiddenKindOf"),
        });
    }
    Ok(rules)
}

#[cfg(test)]
mod tests;
