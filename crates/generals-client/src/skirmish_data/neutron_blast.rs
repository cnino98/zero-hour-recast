//! Native `NeutronBlastBehaviorModuleData`, including original defaults.
use super::{blocks, field, number, scalar};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::neutron_blast::Rules;
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "NeutronBlastBehavior")
        {
            continue;
        }
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            if !matches!(
                line.tokens[0].as_str(),
                "BlastRadius" | "AffectAirborne" | "AffectAllies"
            ) {
                return Err(
                    format!("unsupported NeutronBlastBehavior field {}", line.tokens[0]).into(),
                );
            }
        }
        result.push(Rules {
            radius: field(module, 1, "BlastRadius")
                .map_or(Ok(10.0), |_| number(module, 1, "BlastRadius"))?,
            affect_airborne: field(module, 1, "AffectAirborne").map_or(Ok(true), boolean)?,
            affect_allies: field(module, 1, "AffectAllies").map_or(Ok(true), boolean)?,
        });
    }
    Ok(result)
}

fn boolean(line: &ContentLine) -> Result<bool, Box<dyn Error>> {
    match scalar(line)? {
        value if value.eq_ignore_ascii_case("yes") => Ok(true),
        value if value.eq_ignore_ascii_case("no") => Ok(false),
        value => Err(format!("invalid NeutronBlastBehavior boolean {value}").into()),
    }
}

#[cfg(test)]
mod tests;
