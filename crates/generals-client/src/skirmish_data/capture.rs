use super::{blocks, field, finite_number, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{DepositBoost, DepositRules, MaintenanceRules, OverchargeRules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
    rules: &mut MaintenanceRules,
) -> Result<(), Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        match module[0].tokens.get(1).map(String::as_str) {
            Some("AutoDepositUpdate") => {
                rules.deposits.push(DepositRules {
                    interval: field(module, 1, "DepositTiming")
                        .map(economy_duration)
                        .transpose()?
                        .unwrap_or(0),
                    amount: signed(module, "DepositAmount")?,
                    first_capture: signed(module, "InitialCaptureBonus")?,
                    actual_money: field(module, 1, "ActualMoney")
                        .map(scalar)
                        .transpose()?
                        .map(parse_bool)
                        .transpose()?
                        .unwrap_or(true),
                    upgrades: deposit_upgrades(files, module)?,
                });
            }
            Some("PowerPlantUpdate") => {
                rules.power_plants.push(
                    field(module, 1, "RodsExtendTime")
                        .map(economy_duration)
                        .transpose()?
                        .unwrap_or(0),
                );
            }
            Some("OverchargeBehavior") => {
                rules.overcharges.push(OverchargeRules {
                    drain: percentage(module, "HealthPercentToDrainPerSecond")?,
                    threshold: percentage(module, "NotAllowedWhenHealthBelowPercent")?,
                    energy_bonus: field(&object.lines, 0, "EnergyBonus")
                        .map(scalar)
                        .transpose()?
                        .unwrap_or("0")
                        .parse()?,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn deposit_upgrades(
    files: &[ContentFile],
    lines: &[ContentLine],
) -> Result<Vec<DepositBoost>, Box<dyn Error>> {
    let mut upgrades = Vec::new();
    for line in lines.iter().filter(|line| {
        line.depth == Some(1)
            && line
                .tokens
                .first()
                .is_some_and(|key| key == "UpgradedBoost")
    }) {
        let fields: Vec<_> = line.tokens[1..]
            .iter()
            .flat_map(|token| token.split(':'))
            .filter(|token| !token.is_empty())
            .collect();
        let [kind, trigger, amount, value] = fields.as_slice() else {
            return Err("invalid AutoDeposit upgrade pair".into());
        };
        if !kind.eq_ignore_ascii_case("UpgradeType") || !amount.eq_ignore_ascii_case("Boost") {
            return Err("invalid AutoDeposit upgrade keys".into());
        }
        let exists = files
            .iter()
            .flat_map(|file| &file.declarations)
            .any(|entry| entry.kind == "Upgrade" && entry.name == *trigger);
        upgrades.push(DepositBoost {
            trigger: exists.then(|| (*trigger).into()),
            amount: value.parse()?,
        });
    }
    Ok(upgrades)
}

fn economy_duration(line: &ContentLine) -> Result<u32, Box<dyn Error>> {
    super::source_numbers::duration_frames(scalar(line)?)
}

fn parse_bool(value: &str) -> Result<bool, Box<dyn Error>> {
    if value.eq_ignore_ascii_case("yes") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("no") {
        Ok(false)
    } else {
        Err("invalid native economy boolean".into())
    }
}

fn signed(lines: &[ContentLine], key: &str) -> Result<i32, Box<dyn Error>> {
    Ok(field(lines, 1, key)
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse()?)
}

fn percentage(lines: &[ContentLine], key: &str) -> Result<f32, Box<dyn Error>> {
    let Some(line) = field(lines, 1, key) else {
        return Ok(0.0);
    };
    let value = scalar(line)?;
    Ok(finite_number(value.strip_suffix('%').unwrap_or(value))? / 100.0)
}

#[cfg(test)]
mod tests;
