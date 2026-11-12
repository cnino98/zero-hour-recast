//! Selected `SupplyTruckAIUpdate` and `DockUpdate::loadDockPositions` data.
use crate::skirmish_data::{blocks, field, scalar};
use generals_assets::{
    content::{ContentLine, Declaration},
    w3d::Model,
};
use generals_sim::skirmish::{SupplyBoost, SupplyDock, UnitType};
use std::error::Error;

pub fn load(
    object: &Declaration,
    model: &Model,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        let name = module[0].tokens.get(1).map(String::as_str);
        collector(module, unit)?;
        if !matches!(
            name,
            Some("SupplyCenterDockUpdate" | "RepairDockUpdate" | "SupplyWarehouseDockUpdate")
        ) {
            continue;
        }
        if name == Some("SupplyWarehouseDockUpdate") {
            unit.supply.warehouse = Some(generals_sim::skirmish::supply::WarehouseRules {
                starting_boxes: field(module, 1, "StartingBoxes")
                    .map(scalar)
                    .transpose()?
                    .unwrap_or("1")
                    .parse()?,
                delete_when_empty: field(module, 1, "DeleteWhenEmpty")
                    .map(scalar)
                    .transpose()?
                    .unwrap_or("No")
                    .eq_ignore_ascii_case("Yes"),
            });
        }
        let count: i32 = field(module, 1, "NumberApproachPositions")
            .map(scalar)
            .transpose()?
            .unwrap_or("0")
            .parse()?;
        if !(-1..=64).contains(&count) {
            return Err("unsupported dock approach count".into());
        }
        let ignore = field(&object.lines, 0, "KindOf").is_some_and(|line| {
            line.tokens
                .iter()
                .any(|flag| flag == "IGNORE_DOCKING_BONES")
        });
        let bone = |name| {
            (!ignore)
                .then(|| model.bone_position(name))
                .flatten()
                .filter(|point| point.iter().any(|value| *value != 0.0))
        };
        let approaches = if count == -1 || ignore {
            Vec::new()
        } else {
            (1..=count)
                .map(|index| {
                    // Upstream zero initializes missing approach bone slots.
                    model
                        .bone_position(&format!("DockWaiting{index:02}"))
                        .unwrap_or([0.0; 3])
                })
                .collect()
        };
        if let Some(repair) = &mut unit.neutral_tech.repair_dock {
            repair.approach_bones = if ignore {
                0
            } else {
                (1..=count)
                    .take_while(|index| {
                        model
                            .bone_position(&format!("DockWaiting{index:02}"))
                            .is_some()
                    })
                    .count()
            };
        }
        let approach_bones = if ignore || count == -1 {
            0
        } else {
            (1..=count)
                .take_while(|index| {
                    model
                        .bone_position(&format!("DockWaiting{index:02}"))
                        .is_some()
                })
                .count()
        };
        let temporary_stealth_frames = field(module, 1, "GrantTemporaryStealth")
            .map(crate::skirmish_data::duration_frames)
            .transpose()?
            .unwrap_or(0);
        unit.supply.dock = Some(SupplyDock {
            number_approaches: count,
            approach_bones,
            temporary_stealth_frames,
            approaches,
            enter: bone("DockStart"),
            action: bone("DockAction"),
            exit: bone("DockEnd"),
            allows_passthrough: field(module, 1, "AllowsPassthrough")
                .map(scalar)
                .transpose()?
                .is_none_or(|value| value.eq_ignore_ascii_case("Yes")),
        });
    }
    Ok(())
}

fn collector(module: &[ContentLine], unit: &mut UnitType) -> Result<(), Box<dyn Error>> {
    let name = module[0].tokens.get(1).map(String::as_str);
    if matches!(name, Some("ChinookAIUpdate" | "WorkerAIUpdate")) {
        // Both AI module data constructors default the supply boost to zero.
        let amount = field(module, 1, "UpgradedSupplyBoost")
            .map(scalar)
            .transpose()?
            .unwrap_or("0")
            .parse()?;
        unit.maintenance.collector_boost = (amount != 0).then(|| SupplyBoost {
            trigger: if name == Some("WorkerAIUpdate") {
                "Upgrade_GLAWorkerShoes"
            } else {
                "Upgrade_AmericaSupplyLines"
            }
            .into(),
            amount,
        });
    }
    if matches!(
        name,
        Some("SupplyTruckAIUpdate" | "WorkerAIUpdate" | "ChinookAIUpdate")
    ) {
        field(module, 1, "SuppliesDepletedVoice")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .clone_into(&mut unit.supply.depleted_voice);
        // SupplyTruckAIUpdateModuleData and WorkerAIUpdateModuleData default to 100.
        unit.supply.scan_distance = field(module, 1, "SupplyWarehouseScanDistance")
            .map(scalar)
            .transpose()?
            .unwrap_or("100")
            .parse()?;
        if !unit.supply.scan_distance.is_finite() || unit.supply.scan_distance <= 0.0 {
            return Err("invalid warehouse scan distance".into());
        }
    }
    Ok(())
}
