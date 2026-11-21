//! Native `CashHackSpecialPowerModuleData`.
use super::{blocks, field, scalar};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::cash_hack::Rules;
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "CashHackSpecialPower")
        })
        .map(parse)
        .collect()
}

fn parse(module: &[ContentLine]) -> Result<Rules, Box<dyn Error>> {
    let default_amount = field(module, 1, "MoneyAmount").map_or(Ok(0), |line| {
        scalar(line)?.parse().map_err(Into::<Box<dyn Error>>::into)
    })?;
    let mut upgrades = Vec::new();
    for line in module.iter().filter(|line| {
        line.depth == Some(1)
            && line
                .tokens
                .first()
                .is_some_and(|name| name == "UpgradeMoneyAmount")
    }) {
        let [_, science, amount] = line.tokens.as_slice() else {
            return Err("invalid Cash Hack science/amount pair".into());
        };
        upgrades.push((science.clone(), amount.parse()?));
    }
    Ok(Rules {
        default_amount,
        upgrades,
    })
}
