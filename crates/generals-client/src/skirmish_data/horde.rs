//! Installed `HordeUpdate` fields and the original global morale weapon rates.
use super::{blocks, duration_frames, finite_number, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::horde::{Action, Rules};
use std::error::Error;

pub(super) fn load(
    _files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some(module) = modules.iter().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "HordeUpdate")
    }) else {
        return Ok(None);
    };
    let mut rules = Rules {
        update_frames: 30,
        count: 0,
        radius: 0.0,
        rub_off_radius: 20.0,
        kind_of: Vec::new(),
        allies_only: true,
        exact_match: false,
        allowed_nationalism: true,
        // PRESERVE_PERPETUAL_HORDE_BONUS=1, the pinned default. Installed modules
        // explicitly select HORDE, independently of that default.
        action: Action::Classic,
    };
    for line in module.iter().skip(1).filter(|line| line.depth == Some(1)) {
        let Some(key) = line.tokens.first() else {
            continue;
        };
        match key.as_str() {
            "UpdateRate" => rules.update_frames = duration_frames(line)?,
            "Count" => rules.count = scalar(line)?.parse()?,
            "Radius" => rules.radius = finite_number(scalar(line)?)?,
            "RubOffRadius" => rules.rub_off_radius = finite_number(scalar(line)?)?,
            "KindOf" => {
                rules.kind_of = line
                    .tokens
                    .iter()
                    .skip(1)
                    .filter(|token| token.as_str() != "NONE")
                    .cloned()
                    .collect();
            }
            "AlliesOnly" => rules.allies_only = boolean(line)?,
            "ExactMatch" => rules.exact_match = boolean(line)?,
            "AllowedNationalism" => rules.allowed_nationalism = boolean(line)?,
            "Action" => {
                rules.action = match scalar(line)? {
                    "HORDE" => Action::Classic,
                    "HORDE_FIXED" => Action::Fixed,
                    _ => return Err("unknown native HordeUpdate action".into()),
                }
            }
            "FlagSubObjectNames" => return Err("unrepresented legacy horde flag subobjects".into()),
            _ => return Err(format!("unrepresented HordeUpdate field {key}").into()),
        }
    }
    Ok(Some(rules))
}

fn boolean(line: &ContentLine) -> Result<bool, Box<dyn Error>> {
    match scalar(line)? {
        value if value.eq_ignore_ascii_case("yes") => Ok(true),
        value if value.eq_ignore_ascii_case("no") => Ok(false),
        _ => Err("invalid HordeUpdate boolean".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    fn file(fields: &str, extra: &str) -> ContentFile {
        content::scan(Asset {
            path: "horde.ini".into(),
            origin: "original module parser fixture".into(),
            bytes: format!("GameData\nWeaponBonus=HORDE RATE_OF_FIRE 150%\nWeaponBonus=NATIONALISM RATE_OF_FIRE 125%\nWeaponBonus=FANATICISM RATE_OF_FIRE 125%\nEnd\nObject Infantry\nBehavior=HordeUpdate Tag\n{fields}\nEnd\nEnd\n{extra}").into_bytes(),
        }).unwrap()
    }

    #[test]
    fn source_defaults_and_authored_fixed_action_keep_distinct_nationalism_behavior() {
        let parsed = file("", "");
        let rules = load(std::slice::from_ref(&parsed), &parsed.declarations[1])
            .unwrap()
            .unwrap();
        assert!(matches!(rules.action, Action::Classic));
        assert_eq!(rules.update_frames, 30);
        assert_eq!(rules.count, 0);
        assert_eq!(rules.rub_off_radius.to_bits(), 20.0_f32.to_bits());
        assert!(rules.allies_only && rules.allowed_nationalism && !rules.exact_match);
        let parsed = file(
            "Action=HORDE_FIXED\nUpdateRate=1250\nCount=5\nKindOf=INFANTRY VEHICLE\nAllowedNationalism=No",
            "",
        );
        let rules = load(std::slice::from_ref(&parsed), &parsed.declarations[1])
            .unwrap()
            .unwrap();
        assert!(matches!(rules.action, Action::Fixed));
        assert_eq!(rules.update_frames, 38);
        assert_eq!(rules.kind_of, ["INFANTRY", "VEHICLE"]);
        assert!(!rules.allowed_nationalism);
    }

    #[test]
    fn unknown_actions_fail_and_local_weapon_tables_belong_to_each_weapon() {
        let parsed = file("Action=Other", "");
        assert!(load(std::slice::from_ref(&parsed), &parsed.declarations[1]).is_err());
        let parsed = file("", "Weapon Rifle\nWeaponBonus=HORDE RATE_OF_FIRE 200%\nEnd");
        assert!(load(std::slice::from_ref(&parsed), &parsed.declarations[1]).is_ok());
    }
}
