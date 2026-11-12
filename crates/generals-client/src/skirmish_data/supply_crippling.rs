//! Native `SupplyWarehouseCripplingBehaviorModuleData` fields and constructor values.
use super::{blocks, scalar, source_numbers, source_real};
use generals_assets::content::Declaration;
use generals_sim::skirmish::supply_crippling::Rules;
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "SupplyWarehouseCripplingBehavior")
        {
            continue;
        }
        let mut rules = Rules {
            suppression: 0,
            delay: 0,
            amount: 0.0,
        };
        for line in module.iter().skip(1).filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("SelfHealSupression") => {
                    rules.suppression = source_numbers::duration_frames(scalar(line)?)?;
                }
                Some("SelfHealDelay") => {
                    rules.delay = source_numbers::duration_frames(scalar(line)?)?;
                }
                Some("SelfHealAmount") => rules.amount = source_real(scalar(line)?)?,
                Some("End") => {}
                _ => {
                    return Err(format!(
                        "unsupported SupplyWarehouseCripplingBehavior field at {}",
                        line.line
                    )
                    .into());
                }
            }
        }
        result.push(rules);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::load;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn instances_keep_defaults_signed_amount_and_native_unsigned_duration_order() {
        let file = content::scan(Asset {
            path: "warehouse.ini".into(), origin: "native warehouse fixture".into(),
            bytes: b"Object Warehouse\nBehavior=SupplyWarehouseCripplingBehavior First\nEnd\nBehavior=SupplyWarehouseCripplingBehavior Second\nSelfHealSupression=100\nSelfHealSupression=101ms\nSelfHealDelay=4294967296\nSelfHealAmount=-2.5\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules = load(&file.declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].suppression, 0);
        assert_eq!(rules[0].delay, 0);
        assert_eq!(rules[0].amount.to_bits(), 0.0_f32.to_bits());
        assert_eq!(rules[1].suppression, 4);
        assert_eq!(rules[1].delay, 0);
        assert_eq!(rules[1].amount.to_bits(), (-2.5_f32).to_bits());
    }
}
