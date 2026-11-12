//! Installed `HiveStructureBody` masks; the Fire Base uses container riders.
use super::{damage_type, field};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::hive_body::Rules;
use std::error::Error;

pub(super) fn load(body: Option<&[ContentLine]>) -> Result<Option<Rules>, Box<dyn Error>> {
    let Some(body) = body.filter(|body| {
        body[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "HiveStructureBody")
    }) else {
        return Ok(None);
    };
    Ok(Some(Rules {
        propagate: mask(body, "PropagateDamageTypesToSlavesWhenExisting")?,
        swallow: mask(body, "SwallowDamageTypesIfSlavesNotExisting")?,
    }))
}

fn mask(body: &[ContentLine], key: &str) -> Result<u64, Box<dyn Error>> {
    let mut mask = 0;
    if let Some(line) = field(body, 1, key) {
        for value in &line.tokens[1..] {
            if value == "NONE" {
                mask = 0;
                continue;
            }
            if value == "ALL" {
                mask = u64::MAX;
                continue;
            }
            let damage = damage_type(value.trim_start_matches(['+', '-']))
                .ok_or("unrepresented HiveStructureBody damage type")?;
            let bit = 1 << damage as usize;
            if value.starts_with('-') {
                mask &= !bit;
            } else {
                mask |= bit;
            }
        }
    }
    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hive_masks_preserve_native_defaults_and_ordered_add_remove() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "hive.ini".into(), origin: "HiveStructureBody parser fixture".into(),
            bytes: b"Object FireBase\nBody=HiveStructureBody Body\nPropagateDamageTypesToSlavesWhenExisting=NONE +SNIPER +SMALL_ARMS -SMALL_ARMS\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let body = super::super::blocks(&file.declarations[0].lines, 0, "Body").unwrap()[0];
        let rules = load(Some(body)).unwrap().unwrap();
        assert_eq!(
            rules.propagate,
            1 << generals_sim::skirmish::DamageType::Sniper as usize
        );
        assert_eq!(rules.swallow, 0);
    }
}
