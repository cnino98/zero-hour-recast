//! Authored weapon locks and science grants.
use super::{blocks, field, scalar};
use generals_assets::content::{ContentFile, Declaration};
use std::error::Error;

pub(super) fn lock_weapons(object: &Declaration) -> Result<Vec<usize>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|family| family == "LockWeaponCreate")
        {
            let name = field(module, 1, "SlotToLock")
                .map(scalar)
                .transpose()?
                .unwrap_or("PRIMARY");
            result.push(match name {
                "PRIMARY" => 0,
                "SECONDARY" => 1,
                "TERTIARY" => 2,
                _ => return Err(format!("invalid native weapon slot {name}").into()),
            });
        }
    }
    Ok(result)
}

/// Resolve immutable `ScienceStore` entries; invalid/non-grantable names cannot grant.
pub(super) fn granted_sciences(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Option<String>>, Box<dyn Error>> {
    let mut grants = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "GrantScienceUpgrade")
        {
            continue;
        }
        let name = field(module, 1, "GrantScience")
            .map(scalar)
            .transpose()?
            .unwrap_or("");
        if !files
            .iter()
            .flat_map(|file| &file.declarations)
            .any(|item| item.kind == "Science" && item.name == name)
        {
            grants.push(None);
            continue;
        }
        let science = super::definition(files, "Science", name)?;
        let grantable = match field(&science.lines, 0, "IsGrantable")
            .map(scalar)
            .transpose()?
            .unwrap_or("Yes")
        {
            value if value.eq_ignore_ascii_case("Yes") => true,
            value if value.eq_ignore_ascii_case("No") => false,
            _ => return Err(format!("invalid IsGrantable for {name}").into()),
        };
        grants.push(grantable.then(|| name.to_owned()));
    }
    Ok(grants)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn grant_science_keeps_invalid_slots_and_respects_authored_grantability() {
        let file = content::scan(Asset {
            path: "test.ini".into(),
            origin: "fixture".into(),
            bytes: br"
Science Granted
End
Science Forbidden
 IsGrantable = No
End
Object Grantor
 Behavior = GrantScienceUpgrade Missing
  GrantScience = Missing
 End
 Behavior = GrantScienceUpgrade Granted
  GrantScience = Granted
 End
 Behavior = GrantScienceUpgrade Forbidden
  GrantScience = Forbidden
 End
End
"
            .to_vec(),
        })
        .unwrap();
        let files = vec![file];
        let object = super::super::definition(&files, "Object", "Grantor").unwrap();
        assert_eq!(
            granted_sciences(&files, object).unwrap(),
            [None, Some("Granted".into()), None]
        );
    }
}
