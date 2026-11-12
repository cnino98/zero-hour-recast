//! Pinned Core DamageFX.cpp ordered fields and ArmorSet.cpp references.
use super::{blocks, damage_type, field, finite_number, instant_death, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    DAMAGE_TYPE_COUNT,
    damage_fx::{ArmorSet, Rules},
};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<ArmorSet>, Box<dyn Error>> {
    blocks(&object.lines, 0, "ArmorSet")?
        .into_iter()
        .map(|lines| {
            let conditions = armor_set_conditions(lines)?;
            let effects = field(lines, 1, "DamageFX")
                .map(scalar)
                .transpose()?
                .map(|name| load_rules(files, name))
                .transpose()?
                .flatten();
            Ok(ArmorSet {
                conditions,
                effects,
            })
        })
        .collect()
}

pub(super) fn armor_set_conditions(lines: &[ContentLine]) -> Result<u8, Box<dyn Error>> {
    let mut conditions = 0;
    for line in lines.iter().filter(|line| {
        line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "Conditions")
    }) {
        conditions = armor_conditions(line, conditions)?;
    }
    Ok(conditions)
}

fn armor_conditions(line: &ContentLine, previous: u8) -> Result<u8, Box<dyn Error>> {
    let names = [
        "VETERAN",
        "ELITE",
        "HERO",
        "PLAYER_UPGRADE",
        "WEAK_VERSUS_BASEDEFENSES",
        "SECOND_LIFE",
        "CRATE_UPGRADE_ONE",
        "CRATE_UPGRADE_TWO",
    ];
    let mut result = previous;
    let mut normal = false;
    let mut modifying = false;
    for token in &line.tokens[1..] {
        if token.eq_ignore_ascii_case("NONE") {
            if normal || modifying {
                return Err("mixed ArmorSet condition operations".into());
            }
            result = 0;
            break;
        }
        let subtract = token.starts_with('-');
        let operation = subtract || token.starts_with('+');
        if (operation && normal) || (!operation && modifying) {
            return Err("mixed ArmorSet condition operations".into());
        }
        if !operation && !normal {
            result = 0;
        }
        normal |= !operation;
        modifying |= operation;
        let name = if operation { &token[1..] } else { token };
        let bit = 1
            << names
                .iter()
                .position(|value| name.eq_ignore_ascii_case(value))
                .ok_or_else(|| format!("unknown ArmorSet condition {name}"))?;
        if subtract {
            result &= !bit;
        } else {
            result |= bit;
        }
    }
    Ok(result)
}

fn load_rules(files: &[ContentFile], name: &str) -> Result<Option<Rules>, Box<dyn Error>> {
    // DamageFXStore clears a repeated definition before parsing its replacement.
    let Some(definition) = files
        .iter()
        .flat_map(|file| &file.declarations)
        .rfind(|declaration| declaration.kind == "DamageFX" && declaration.name == name)
    else {
        return Ok(None);
    };
    if definition.parent.is_some() || definition.lines.iter().any(|line| line.depth.is_none()) {
        return Err(format!("unresolved DamageFX {name}").into());
    }
    let mut rules = Rules::empty();
    for line in &definition.lines {
        if line.depth != Some(0) || line.tokens.is_empty() {
            continue;
        }
        let key = &line.tokens[0];
        if key.eq_ignore_ascii_case("End") {
            continue;
        }
        let (key, rank, offset) = if let Some(key) = key.strip_prefix("Veterancy") {
            let rank = line.tokens.get(1).ok_or("missing DamageFX veterancy")?;
            let rank = ["REGULAR", "VETERAN", "ELITE", "HEROIC"]
                .iter()
                .position(|value| rank.eq_ignore_ascii_case(value))
                .ok_or("invalid DamageFX veterancy")?;
            (key, rank..rank + 1, 2)
        } else {
            (key.as_str(), 0..4, 1)
        };
        let name = line
            .tokens
            .get(offset)
            .ok_or("missing DamageFX damage type")?;
        let damage = if name.eq_ignore_ascii_case("Default") {
            0..DAMAGE_TYPE_COUNT
        } else {
            let index = damage_type(name).ok_or("invalid DamageFX damage type")? as usize;
            index..index + 1
        };
        let value = line
            .tokens
            .get(offset + 1)
            .ok_or("missing DamageFX field value")?;
        if line.tokens.len() != offset + 2 {
            return Err("extra DamageFX field tokens".into());
        }
        let effect = if matches!(key, "MajorFX" | "MinorFX") {
            instant_death::nullable(files, "FXList", value)?
                .map(|_| value.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        for damage in damage {
            for rank in rank.clone() {
                let entry = &mut rules.entries[damage][rank];
                match key {
                    "AmountForMajorFX" => entry.major_threshold = finite_number(value)?,
                    "MajorFX" => entry.major.clone_from(&effect),
                    "MinorFX" => entry.minor.clone_from(&effect),
                    "ThrottleTime" => {
                        entry.throttle = super::source_numbers::duration_frames(value)?;
                    }
                    _ => return Err(format!("unsupported DamageFX field {key}").into()),
                }
            }
        }
    }
    Ok(Some(rules))
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};
    use generals_sim::skirmish::DamageType;

    #[test]
    fn ordered_defaults_veterancy_and_nullable_fx_use_native_replacement() {
        let files = [content::scan(Asset {
            path: "damage-fx.ini".into(), origin: "native DamageFX regression".into(),
            bytes: b"FXList Minor\nEnd\nFXList Major\nEnd\nDamageFX Hits\nAmountForMajorFX=Default 10\nMinorFX=Default Minor\nMajorFX=Default Major\nThrottleTime=Default 101\nVeterancyMajorFX=ELITE CRUSH Missing\nVeterancyAmountForMajorFX=HEROIC CRUSH 4\nEnd\nObject Vehicle\nArmorSet\nConditions=\nArmor=None\nDamageFX=Hits\nEnd\nArmorSet\nConditions=PLAYER_UPGRADE\nDamageFX=Missing\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let sets = load(&files, &files[0].declarations[3]).unwrap();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[1].conditions, 8);
        assert!(sets[1].effects.is_none());
        let rules = sets[0].effects.as_ref().unwrap();
        let crush = &rules.entries[DamageType::Crush as usize];
        assert_eq!(crush[0].major, "Major");
        assert_eq!(crush[0].minor, "Minor");
        assert_eq!(crush[0].throttle, 4);
        assert!(crush[2].major.is_empty());
        assert_eq!(crush[3].major_threshold.to_bits(), 4.0_f32.to_bits());
        assert_eq!(
            rules.entries[DamageType::Flame as usize][3]
                .major_threshold
                .to_bits(),
            10.0_f32.to_bits()
        );
    }

    #[test]
    fn repeated_definition_clears_prior_fields() {
        let files = [content::scan(Asset {
            path: "damage-fx-override.ini".into(), origin: "native DamageFX replacement".into(),
            bytes: b"FXList Old\nEnd\nDamageFX Hits\nMajorFX=Default Old\nThrottleTime=Default 1000\nEnd\nDamageFX Hits\nAmountForMajorFX=CRUSH 12\nEnd\nObject Vehicle\nArmorSet\nConditions=\nDamageFX=Hits\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let sets = load(&files, &files[0].declarations[3]).unwrap();
        let entry = &sets[0].effects.as_ref().unwrap().entries[DamageType::Crush as usize][0];
        assert_eq!(entry.major_threshold.to_bits(), 12.0_f32.to_bits());
        assert!(entry.major.is_empty());
        assert_eq!(entry.throttle, 0);
    }
}
