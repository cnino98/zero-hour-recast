//! Installed refinery `CostModifierUpgrade`, restricted to the translated create grant.
use super::{blocks, field, finite_number, scalar, support};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::{
    NeutralTechRules, ProductionCostModifier, RepairDock, TechBuildingRules,
};
use std::error::Error;

pub(super) fn rules(lines: &[ContentLine]) -> Result<NeutralTechRules, Box<dyn Error>> {
    let behaviors = blocks(lines, 0, "Behavior")?;
    let mut cost_modifier = None;
    let repair_dock = repair_dock_rules(&behaviors)?;
    for module in &behaviors {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|kind| kind != "CostModifierUpgrade")
        {
            continue;
        }
        if cost_modifier.is_some() {
            return Err("multiple CostModifierUpgrade modules are not translated".into());
        }
        if module
            .iter()
            .filter(|line| line.depth == Some(1))
            .any(|line| {
                line.tokens.first().is_some_and(|key| {
                    !["TriggeredBy", "EffectKindOf", "Percentage", "End"].contains(&key.as_str())
                })
            })
        {
            return Err("unsupported CostModifierUpgrade trigger/removal conditions".into());
        }
        let trigger = field(module, 1, "TriggeredBy").ok_or("CostModifierUpgrade lacks trigger")?;
        let upgrade = scalar(trigger)?;
        let grant = behaviors
            .iter()
            .find(|grant| {
                grant[0]
                    .tokens
                    .get(1)
                    .is_some_and(|kind| kind == "GrantUpgradeCreate")
                    && field(grant, 1, "UpgradeToGrant").and_then(|line| scalar(line).ok())
                        == Some(upgrade)
            })
            .ok_or("CostModifierUpgrade without an object create grant is not translated")?;
        if grant
            .iter()
            .filter(|line| line.depth == Some(1))
            .any(|line| {
                line.tokens
                    .first()
                    .is_some_and(|key| !["UpgradeToGrant", "End"].contains(&key.as_str()))
            })
        {
            return Err("conditional GrantUpgradeCreate cost modifier is not translated".into());
        }
        let kinds = field(module, 1, "EffectKindOf").ok_or("CostModifierUpgrade lacks mask")?;
        if kinds
            .tokens
            .iter()
            .skip(1)
            .any(|kind| !["VEHICLE", "INFANTRY", "AIRCRAFT"].contains(&kind.as_str()))
        {
            return Err("unsupported CostModifierUpgrade EffectKindOf".into());
        }
        let percentage =
            scalar(field(module, 1, "Percentage").ok_or("CostModifierUpgrade lacks percentage")?)?
                .strip_suffix('%')
                .ok_or("CostModifierUpgrade percentage lacks percent suffix")?
                .parse::<f32>()?
                / 100.0;
        if !percentage.is_finite() || percentage < -1.0 {
            return Err("invalid CostModifierUpgrade percentage".into());
        }
        cost_modifier = Some(ProductionCostModifier {
            kind_mask: support::kind_flags(Some(kinds)),
            percentage,
        });
    }
    Ok(NeutralTechRules {
        tech_buildings: tech_buildings(&behaviors)?,
        cost_modifier,
        repair_dock,
    })
}

fn tech_buildings(behaviors: &[&[ContentLine]]) -> Result<Vec<TechBuildingRules>, Box<dyn Error>> {
    behaviors
        .iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|kind| kind == "TechBuildingBehavior")
        })
        .map(|module| {
            for line in module.iter().filter(|line| line.depth == Some(1)) {
                if line
                    .tokens
                    .first()
                    .is_some_and(|key| !["PulseFX", "PulseFXRate", "End"].contains(&key.as_str()))
                {
                    return Err("unsupported TechBuildingBehavior field".into());
                }
            }
            let pulse_fx = field(module, 1, "PulseFX")
                .map(scalar)
                .transpose()?
                .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
                .map(str::to_owned);
            let pulse_frames = field(module, 1, "PulseFXRate").map_or(Ok(0), |line| {
                super::source_numbers::duration_frames(scalar(line)?)
            })?;
            Ok(TechBuildingRules {
                pulse_fx,
                pulse_frames,
            })
        })
        .collect()
}

fn repair_dock_rules(behaviors: &[&[ContentLine]]) -> Result<Option<RepairDock>, Box<dyn Error>> {
    let mut repair_dock = None;
    for module in behaviors {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|kind| kind != "RepairDockUpdate")
        {
            continue;
        }
        if repair_dock.is_some() {
            return Err("multiple RepairDockUpdate modules are not translated".into());
        }
        if module
            .iter()
            .filter(|line| line.depth == Some(1))
            .any(|line| {
                line.tokens.first().is_some_and(|key| {
                    ![
                        "TimeForFullHeal",
                        "AllowsPassthrough",
                        "NumberApproachPositions",
                        "End",
                    ]
                    .contains(&key.as_str())
                })
            })
        {
            return Err("unsupported RepairDockUpdate field".into());
        }
        let full_heal_frames = field(module, 1, "TimeForFullHeal")
            .map(|line| {
                finite_number(scalar(line)?).map(|milliseconds| milliseconds * 30.0 / 1000.0)
            })
            .transpose()?
            .unwrap_or(1.0);
        let approaches = field(module, 1, "NumberApproachPositions")
            .map(scalar)
            .transpose()?
            .unwrap_or("0")
            .parse::<usize>()?;
        if full_heal_frames <= 0.0
            || !full_heal_frames.is_finite()
            || !(1..=64).contains(&approaches)
        {
            return Err("unsupported repair dock timing or approach count".into());
        }
        repair_dock = Some(RepairDock {
            full_heal_frames,
            approaches,
            approach_bones: 0,
        });
    }
    Ok(repair_dock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn repair_pad_preserves_fractional_frames_and_original_approach_limit() {
        let file = content::scan(Asset {
            origin: "installed TechRepairPad fields".into(), path: "tech.ini".into(),
            bytes: b"Object TechRepairPad\nBehavior=RepairDockUpdate Repair\nTimeForFullHeal=5000\nAllowsPassthrough=Yes\nNumberApproachPositions=5\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let parsed = rules(&file.declarations[0].lines).unwrap();
        let dock = parsed.repair_dock.unwrap();
        assert_eq!(dock.full_heal_frames.to_bits(), 150.0_f32.to_bits());
        assert_eq!(dock.approaches, 5);
        let fractional = content::scan(Asset {
            origin: "duration real".into(), path: "tech.ini".into(),
            bytes: b"Object Pad\nBehavior=RepairDockUpdate Repair\nTimeForFullHeal=50\nNumberApproachPositions=5\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            rules(&fractional.declarations[0].lines)
                .unwrap()
                .repair_dock
                .unwrap()
                .full_heal_frames
                .to_bits(),
            1.5_f32.to_bits()
        );
    }

    #[test]
    fn refinery_requires_create_granted_upgrade_and_original_percent() {
        let file = content::scan(Asset { origin: "fixture".into(), path: "refinery.ini".into(), bytes: b"Object Refinery\nBehavior = GrantUpgradeCreate Grant\nUpgradeToGrant = Upgrade_CostReduction\nEnd\nBehavior = CostModifierUpgrade Discount\nTriggeredBy = Upgrade_CostReduction\nEffectKindOf = VEHICLE\nPercentage = -10%\nEnd\nEnd\n".to_vec() }).unwrap();
        let parsed = rules(&file.declarations[0].lines).unwrap();
        let modifier = parsed.cost_modifier.unwrap();
        assert_eq!(modifier.kind_mask, 2);
        assert!((modifier.percentage + 0.1).abs() < f32::EPSILON);
    }
    #[test]
    fn conditional_cost_upgrade_and_create_grant_are_rejected() {
        for (grant_extra, cost_extra) in [
            ("ExemptStatus = UNDER_CONSTRUCTION\n", ""),
            ("", "ConflictsWith = Upgrade_Alternative\n"),
            ("", "RequiresAllTriggers = Yes\n"),
            ("", "RemoveUpgrade = Yes\n"),
        ] {
            let text = format!(
                "Object Refinery\nBehavior = GrantUpgradeCreate Grant\nUpgradeToGrant = Upgrade_CostReduction\n{grant_extra}End\nBehavior = CostModifierUpgrade Discount\nTriggeredBy = Upgrade_CostReduction\nEffectKindOf = VEHICLE\nPercentage = -10%\n{cost_extra}End\nEnd\n"
            );
            let file = content::scan(Asset {
                origin: "fixture".into(),
                path: "refinery.ini".into(),
                bytes: text.into_bytes(),
            })
            .unwrap();
            assert!(rules(&file.declarations[0].lines).is_err());
        }
    }
}

#[cfg(test)]
mod tech_tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn each_tech_module_keeps_its_pulse_and_unsigned_duration() {
        let file=content::scan(Asset {
            path:"tech.ini".into(),origin:"authored TechBuilding modules".into(),
            bytes:b"Object Tech\nBehavior=TechBuildingBehavior First\nPulseFX=FXPulse\nPulseFXRate=101ms\nEnd\nBehavior=TechBuildingBehavior Second\nPulseFX=NONE\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let parsed = rules(&file.declarations[0].lines).unwrap();
        assert_eq!(parsed.tech_buildings.len(), 2);
        assert_eq!(
            parsed.tech_buildings[0].pulse_fx.as_deref(),
            Some("FXPulse")
        );
        assert_eq!(parsed.tech_buildings[0].pulse_frames, 4);
        assert!(parsed.tech_buildings[1].pulse_fx.is_none());
        assert_eq!(parsed.tech_buildings[1].pulse_frames, 0);
    }
}
