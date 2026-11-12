//! Native `CleanupHazardUpdate` and separate `CleanupAreaPower` data.
use super::{blocks, duration_frames, field, number, scalar};
use generals_assets::content::Declaration;
use generals_sim::skirmish::cleanup::{AreaRules, Rules};
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "CleanupHazardUpdate")
        {
            continue;
        }
        let slot = match field(module, 1, "WeaponSlot")
            .map(scalar)
            .transpose()?
            .unwrap_or("PRIMARY")
        {
            "PRIMARY" => 0,
            "SECONDARY" => 1,
            "TERTIARY" => 2,
            _ => return Err("invalid native cleanup weapon slot".into()),
        };
        result.push(Rules {
            slot,
            scan_frames: field(module, 1, "ScanRate").map_or(Ok(0), duration_frames)?,
            scan_range: field(module, 1, "ScanRange")
                .map_or(Ok(0.0), |_| number(module, 1, "ScanRange"))?,
        });
    }
    Ok(result)
}

pub(super) fn powers(object: &Declaration) -> Result<Vec<AreaRules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "CleanupAreaPower")
        {
            continue;
        }
        result.push(AreaRules {
            range: field(module, 1, "MaxMoveDistanceFromLocation").map_or(Ok(0.0), |_| {
                number(module, 1, "MaxMoveDistanceFromLocation")
            })?,
            power: field(module, 1, "SpecialPowerTemplate")
                .map(scalar)
                .transpose()?
                .unwrap_or("")
                .into(),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passive_cleanup_instances_retain_native_defaults_and_do_not_require_area_power() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "cleanup.ini".into(), origin: "native independent cleanup module parser".into(),
            bytes: b"Object AmericaVehicleMedic\nBehavior=CleanupHazardUpdate A\nScanRate=1000\nScanRange=100\nEnd\nBehavior=CleanupHazardUpdate B\nWeaponSlot=SECONDARY\nEnd\nBehavior=CleanupAreaPower C\nMaxMoveDistanceFromLocation=300\nSpecialPowerTemplate=SpecialAbilityAmbulanceCleanupArea\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules = load(&file.declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].scan_frames, 30);
        assert_eq!(rules[0].scan_range.to_bits(), 100.0_f32.to_bits());
        assert_eq!(rules[1].slot, 1);
        assert_eq!(rules[1].scan_frames, 0);
        assert_eq!(rules[1].scan_range.to_bits(), 0.0_f32.to_bits());
        assert_eq!(
            powers(&file.declarations[0]).unwrap()[0].range.to_bits(),
            300.0_f32.to_bits()
        );
    }
}
