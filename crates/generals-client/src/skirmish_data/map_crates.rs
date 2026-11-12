//! Original fixed money-crate family and `CrateCollide` defaults.
use super::{blocks, field, finite_number, scalar};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::map_crates::{Boost, Rules};
use std::error::Error;

pub(super) const OBJECTS: [&str; 7] = [
    "100DollarCrate",
    "200DollarCrate",
    "1000DollarCrate",
    "1500DollarCrate",
    "2500DollarCrate",
    "100DollarCrateForBuildings",
    "SupplyDropZoneCrate",
];

pub(super) fn load(lines: &[ContentLine]) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(lines, 0, "Behavior")?;
    let mut rules = None;
    for module in modules.into_iter().filter(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "MoneyCrateCollide")
    }) {
        if rules.is_some() {
            return Err("multiple MoneyCrateCollide modules are not translated".into());
        }
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            if line.tokens.first().is_some_and(|name| {
                ![
                    "ForbiddenKindOf",
                    "MoneyProvided",
                    "BuildingPickup",
                    "UpgradedBoost",
                    "ExecuteAnimation",
                    "ExecuteAnimationTime",
                    "ExecuteAnimationZRise",
                    "ExecuteAnimationFades",
                    "End",
                ]
                .contains(&name.as_str())
            }) {
                return Err("unsupported money crate collision field".into());
            }
        }
        if !boolean(module, "ExecuteAnimationFades", true)? {
            return Err("unsupported nonfading money crate animation".into());
        }
        let mut boosts = Vec::new();
        for line in module.iter().filter(|line| {
            line.depth == Some(1)
                && line
                    .tokens
                    .first()
                    .is_some_and(|key| key == "UpgradedBoost")
        }) {
            let [_, upgrade, amount] = line.tokens.as_slice() else {
                return Err("malformed money crate upgrade pair".into());
            };
            boosts.push(Boost {
                upgrade: upgrade
                    .strip_prefix("UpgradeType:")
                    .ok_or("money crate upgrade type missing")?
                    .into(),
                amount: amount
                    .strip_prefix("Boost:")
                    .ok_or("money crate boost missing")?
                    .parse()?,
            });
        }
        rules = Some(Rules {
            money: field(module, 1, "MoneyProvided")
                .map(scalar)
                .transpose()?
                .unwrap_or("0")
                .parse()?,
            forbidden: field(module, 1, "ForbiddenKindOf")
                .map_or_else(Vec::new, |line| line.tokens[1..].to_vec()),
            building_pickup: boolean(module, "BuildingPickup", false)?,
            boosts,
            animation: field(module, 1, "ExecuteAnimation")
                .map(scalar)
                .transpose()?
                .unwrap_or_default()
                .into(),
            animation_seconds: real(module, "ExecuteAnimationTime")?,
            animation_rise: real(module, "ExecuteAnimationZRise")?,
        });
    }
    if rules.as_ref().is_some_and(|rules| {
        (!rules.animation.is_empty() && rules.animation != "MoneyPickUp")
            || rules.animation_seconds < 0.0
    }) {
        return Err("unsupported money crate animation template or duration".into());
    }
    Ok(rules)
}

fn real(module: &[ContentLine], key: &str) -> Result<f32, Box<dyn Error>> {
    field(module, 1, key)
        .map(scalar)
        .transpose()?
        .map_or(Ok(0.0), finite_number)
}

fn boolean(module: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    match field(module, 1, key).map(scalar).transpose()? {
        Some(value) if value.eq_ignore_ascii_case("Yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("No") => Ok(false),
        None => Ok(default),
        Some(_) => Err(format!("invalid money crate {key}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_family_keeps_fixed_money_and_original_feedback() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        for (name, amount) in OBJECTS
            .into_iter()
            .zip([100, 200, 1000, 1500, 2500, 100, 250])
        {
            let object = super::super::definition(&files, "Object", name).unwrap();
            let rules = load(&object.lines).unwrap().unwrap();
            assert_eq!(rules.forbidden, ["PROJECTILE"]);
            assert_eq!(rules.animation, "MoneyPickUp");
            assert_eq!(rules.animation_seconds.to_bits(), 4.0_f32.to_bits());
            assert_eq!(rules.animation_rise.to_bits(), 15.0_f32.to_bits());
            assert_eq!(rules.money, amount);
            assert_eq!(
                rules.building_pickup,
                name == "100DollarCrateForBuildings" || name == "SupplyDropZoneCrate"
            );
            let unit = super::super::unit_type(
                &files,
                object,
                generals_sim::skirmish::Faction::Civilian,
                generals_sim::skirmish::Kind::Crate,
            )
            .unwrap();
            let unit = super::super::finish_type(&files, object, unit).unwrap();
            assert_eq!(unit.max_health.to_bits(), 0.0_f32.to_bits());
            assert_eq!(unit.initial_health.to_bits(), 0.0_f32.to_bits());
            assert_eq!(unit.behavior.money_crate.as_ref().unwrap().money, amount);
            assert_eq!(unit.geometry.major.to_bits(), 12.0_f32.to_bits());
            assert_eq!(unit.geometry.minor.to_bits(), 12.0_f32.to_bits());
            assert_eq!(unit.geometry.height.to_bits(), 12.0_f32.to_bits());
            assert_eq!(unit.live_physics.is_some(), name != "2500DollarCrate");
            if name == "SupplyDropZoneCrate" {
                assert_eq!(rules.boosts.len(), 1);
                assert_eq!(rules.boosts[0].upgrade, "Upgrade_AmericaSupplyLines");
                assert_eq!(rules.boosts[0].amount, 25);
            } else {
                assert!(rules.boosts.is_empty());
            }
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_cash_maps_still_reject_unrepresented_reinforcement_pad() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let definitions = crate::world_data::object_definitions(&assets).unwrap();
        for path in [
            "maps/hostile dawn/hostile dawn.map",
            "maps/northamerica/northamerica.map",
        ] {
            let map =
                generals_assets::map::MapData::parse(&assets.read(path).unwrap().bytes).unwrap();
            let records = map
                .objects
                .into_iter()
                .filter(|record| record.template == "TechReinforcementPad")
                .collect::<Vec<_>>();
            assert_eq!(records.len(), 1);
            let error =
                crate::world_data::validate_map_objects(&records, &definitions, &[]).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Placed gameplay object TechReinforcementPad is not translated"
            );
        }
    }
    #[test]
    fn unsupported_multipickup_and_science_are_rejected_before_admission() {
        for extra in ["AllowMultiPickup=Yes", "PickupScience=SCIENCE_Salvage"] {
            let file = content::scan(Asset {
                path: "crate.ini".into(),
                origin: "money crate unsupported branch".into(),
                bytes: format!(
                    "Object Crate\nBehavior=MoneyCrateCollide Module\n{extra}\nEnd\nEnd\n"
                )
                .into_bytes(),
            })
            .unwrap();
            assert!(load(&file.declarations[0].lines).is_err());
        }
    }
}
