//! Authored `FireWeaponCollide` instances and their persistent weapon templates.
use crate::skirmish_data::{blocks, definition, instant_death::status, scalar, weapons};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{WeaponRules, fire::collide::Rules};
use std::error::Error;

pub(in crate::skirmish_data) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "FireWeaponCollide")
        {
            continue;
        }
        let mut weapon = None;
        let mut required = 0;
        let mut forbidden = 0;
        let mut fire_once = false;
        for line in module.iter().skip(1).filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("CollideWeapon") => {
                    let name = scalar(line)?;
                    weapon = if name.eq_ignore_ascii_case("NONE") {
                        None
                    } else {
                        let mut rules = WeaponRules::unarmed();
                        name.clone_into(&mut rules.name);
                        weapons::parse(&mut rules, &definition(files, "Weapon", name)?.lines)?;
                        Some(rules)
                    };
                }
                Some("RequiredStatus") => required = status(line, required)?,
                Some("ForbiddenStatus") => forbidden = status(line, forbidden)?,
                Some("FireOnce") => {
                    fire_once = match scalar(line)?.to_ascii_uppercase().as_str() {
                        "YES" => true,
                        "NO" => false,
                        _ => return Err("invalid FireWeaponCollide FireOnce boolean".into()),
                    }
                }
                Some("End") => {}
                _ => return Err(format!("unbound FireWeaponCollide field at {}", line.line).into()),
            }
        }
        // Native allocateNewWeapon unconditionally dereferences its template in the constructor.
        let weapon = weapon.ok_or("FireWeaponCollide requires non-null CollideWeapon")?;
        result.push(Rules {
            weapon,
            required,
            forbidden,
            fire_once,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn authored_order_status_operations_and_constructor_weapon_requirement() {
        let file = content::scan(Asset { path: "collide.ini".into(), origin: "fixture".into(), bytes: b"Object Tree\nBehavior=FireWeaponCollide First\nCollideWeapon=Burn\nRequiredStatus=AFLAME\nRequiredStatus=+BURNED\nRequiredStatus=-BURNED\nFireOnce=Yes\nEnd\nBehavior=FireWeaponCollide Second\nCollideWeapon=Burn\nForbiddenStatus=BURNED\nEnd\nEnd\nWeapon Burn\nPrimaryDamage=1\nDamageType=FLAME\nEnd\nObject Invalid\nBehavior=FireWeaponCollide Missing\nCollideWeapon=None\nEnd\nEnd\n".to_vec() }).unwrap();
        let rules = load(std::slice::from_ref(&file), &file.declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules[0].fire_once);
        assert_eq!(rules[0].required, 1 << 11);
        assert_eq!(rules[1].forbidden, 1 << 12);
        assert_eq!(rules[0].weapon.name, "Burn");
        assert!(load(std::slice::from_ref(&file), &file.declarations[2]).is_err());
    }
}
