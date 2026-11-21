use super::{blocks, definition, duration_frames, field, finite_number, has_flag, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{HoleExposure, MaintenanceRules, RebuildRules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<MaintenanceRules, Box<dyn Error>> {
    let global = &definition(files, "GameData", "")?.lines;
    let mut rules = MaintenanceRules::none();
    rules.auto_heal = super::support::auto_heal(&object.lines)?;
    if let Some(sounds) = blocks(&object.lines, 0, "UnitSpecificSounds")?.first() {
        rules.construction_sound = field(sounds, 1, "UnderConstruction")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into();
    }
    rules.healing_kind_flags = super::support::kind_flags(field(&object.lines, 0, "KindOf"));
    rules.rubble_height = super::garrisons::rubble_height(object, global)?;
    rules.research = super::research::load(files, object)?;
    super::upgrades::load(files, object, &mut rules)?;
    super::capture::load(files, object, &mut rules)?;
    for (name, flag) in [
        ("CAPTURABLE", MaintenanceRules::CAPTURABLE),
        ("IMMUNE_TO_CAPTURE", MaintenanceRules::IMMUNE_TO_CAPTURE),
        ("UNSELLABLE", MaintenanceRules::UNSELLABLE),
        ("TECH_BUILDING", MaintenanceRules::TECH_BUILDING),
    ] {
        if has_flag(&object.lines, "KindOf", name) {
            rules.kind_flags |= flag;
        }
    }
    rules.sell_fraction = percent(global, 0, "SellPercentage", 0.0)?;
    rules.refund = field(&object.lines, 0, "RefundValue")
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse()?;
    for module in blocks(&object.lines, 0, "Behavior")? {
        match module[0].tokens.get(1).map(String::as_str) {
            Some("DozerAIUpdate" | "WorkerAIUpdate") => {
                rules.repair_per_second = percent(module, 1, "RepairHealthPercentPerSecond", 0.0)?;
                rules.idle_repair = Some(idle_repair(module)?);
            }
            Some("RebuildHoleExposeDie") => {
                rules.expose_hole.push(HoleExposure {
                    conditions: super::death_callbacks::conditions(module)?,
                    template: field(module, 1, "HoleName")
                        .map(scalar)
                        .transpose()?
                        .unwrap_or("")
                        .into(),
                    health: field(module, 1, "HoleMaxHealth")
                        .map_or(Ok(0.0), |line| finite_number(scalar(line)?))?,
                    transfer_attackers: field(module, 1, "TransferAttackers")
                        .map(scalar)
                        .transpose()?
                        .is_none_or(|value| value.eq_ignore_ascii_case("yes")),
                });
            }
            Some("BaseRegenerateUpdate") => {
                rules.regeneration = Some(generals_sim::skirmish::Regeneration {
                    percent_per_second: percent(global, 0, "BaseRegenHealthPercentPerSecond", 0.0)?,
                    delay: duration_frames(
                        field(global, 0, "BaseRegenDelay")
                            .ok_or("base regeneration delay missing")?,
                    )?,
                });
            }
            Some("RebuildHoleBehavior") => {
                rules.rebuild.push(RebuildRules {
                    worker: field(module, 1, "WorkerObjectName")
                        .map(scalar)
                        .transpose()?
                        .unwrap_or("")
                        .into(),
                    respawn_frames: field(module, 1, "WorkerRespawnDelay")
                        .map_or(Ok(0.0), |line| finite_number(scalar(line)?))?
                        * (30.0 / 1000.0),
                    regeneration: percent(module, 1, "HoleHealthRegen%PerSecond", 0.1)?,
                });
            }
            _ => {}
        }
    }
    Ok(rules)
}

fn idle_repair(
    lines: &[ContentLine],
) -> Result<generals_sim::skirmish::IdleRepair, Box<dyn Error>> {
    let value = |key| field(lines, 1, key).map_or(Ok(0.0), |line| finite_number(scalar(line)?));
    Ok(generals_sim::skirmish::IdleRepair {
        // INI::parseDurationReal retains fractional frames, unlike unsigned times.
        frames: value("BoredTime")? * (30.0 / 1000.0),
        range: value("BoredRange")?,
    })
}

fn percent(
    lines: &[ContentLine],
    depth: u32,
    key: &str,
    default: f32,
) -> Result<f32, Box<dyn Error>> {
    field(lines, depth, key).map_or(Ok(default), |line| {
        let value = scalar(line)?;
        let value = value
            .strip_suffix('%')
            .ok_or_else(|| format!("{key} lacks percent suffix"))?;
        Ok(finite_number(value)? / 100.0)
    })
}

pub(super) fn validate_hole_references(
    types: &[generals_sim::skirmish::UnitType],
) -> Result<(), Box<dyn std::error::Error>> {
    for unit in types {
        for hole in &unit.maintenance.expose_hole {
            if !hole.health.is_finite() || !types.iter().any(|item| item.name == hole.template) {
                return Err(format!(
                    "{} has an invalid or missing rebuild-hole template {}",
                    unit.name, hole.template
                )
                .into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_modules_keep_authored_instances_real_duration_and_die_conditions() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "rebuild.ini".into(), origin: "native RebuildHole fixture".into(),
            bytes: b"GameData\nEnd\nObject Hole\nBehavior=RebuildHoleBehavior First\nWorkerObjectName=Worker\nWorkerRespawnDelay=51\nEnd\nBehavior=RebuildHoleBehavior Second\nEnd\nBehavior=RebuildHoleExposeDie Expose\nHoleName=Hole\nDeathTypes=NONE +NORMAL\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let files = [file];
        let rules = load(&files, &files[0].declarations[1]).unwrap();
        assert_eq!(rules.rebuild.len(), 2);
        assert_eq!(
            rules.rebuild[0].respawn_frames.to_bits(),
            (51.0_f32 * (30.0 / 1000.0)).to_bits()
        );
        assert_eq!(rules.rebuild[1].respawn_frames.to_bits(), 0.0_f32.to_bits());
        assert_eq!(rules.rebuild[1].regeneration.to_bits(), 0.1_f32.to_bits());
        assert_eq!(rules.expose_hole.len(), 1);
        assert_eq!(rules.expose_hole[0].health.to_bits(), 0.0_f32.to_bits());
        assert_eq!(rules.expose_hole[0].conditions.death_mask, 1);
        assert!(rules.expose_hole[0].transfer_attackers);
    }

    #[test]
    fn idle_repair_keeps_fractional_duration_and_original_zero_defaults() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "builders.ini".into(),
            origin: "DozerAIUpdate source duration fixture".into(),
            bytes: b"Object Builder\nBehavior=DozerAIUpdate Tag\nBoredTime=51\nBoredRange=150\nEnd\nEnd\nObject Defaults\nBehavior=WorkerAIUpdate Tag\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let module = blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0];
        let rules = idle_repair(module).unwrap();
        let expected = 51.0_f32 * (30.0 / 1000.0);
        assert_eq!(rules.frames.to_bits(), expected.to_bits());
        assert_eq!(rules.range.to_bits(), 150.0_f32.to_bits());
        let module = blocks(&file.declarations[1].lines, 0, "Behavior").unwrap()[0];
        let rules = idle_repair(module).unwrap();
        assert_eq!(rules.frames.to_bits(), 0.0_f32.to_bits());
        assert_eq!(rules.range.to_bits(), 0.0_f32.to_bits());
    }
}
