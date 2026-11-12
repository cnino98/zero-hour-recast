//! Installed `AutoFindHealingUpdate` data, native defaults from cpp53–60.
use super::{blocks, duration_frames, field, finite_number, scalar};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::auto_find_healing::Rules;
use std::error::Error;

pub(super) fn load(lines: &[ContentLine]) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "AutoFindHealingUpdate")
        {
            continue;
        }
        let real = |key, default| {
            field(module, 1, key).map_or(Ok(default), |line| finite_number(scalar(line)?))
        };
        result.push(Rules {
            scan_frames: field(module, 1, "ScanRate").map_or(Ok(0), duration_frames)?,
            scan_range: real("ScanRange", 0.0)?,
            never_heal: real("NeverHeal", 0.95)?,
            always_heal: real("AlwaysHeal", 0.25)?,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn preserves_authored_instance_order_and_native_unreachable_always_heal_value() {
        let file = content::scan(Asset { path: "object.ini".into(), origin: "native healing module".into(), bytes: b"Object Infantry\nBehavior=AutoFindHealingUpdate first\nEnd\nBehavior=AutoFindHealingUpdate second\nScanRate=1001\nScanRange=300\nNeverHeal=0.75\nAlwaysHeal=0.1\nEnd\nEnd\n".to_vec() }).unwrap();
        let rules = load(&file.declarations[0].lines).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].scan_frames, 0);
        assert_eq!(rules[0].never_heal.to_bits(), 0.95_f32.to_bits());
        assert_eq!(rules[1].scan_frames, 31);
        assert_eq!(rules[1].scan_range.to_bits(), 300.0_f32.to_bits());
        assert_eq!(rules[1].always_heal.to_bits(), 0.1_f32.to_bits());
    }
}
