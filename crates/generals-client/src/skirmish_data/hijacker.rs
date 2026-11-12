//! Authored hijack crate admission and the separate hidden-driver update module.
use super::{blocks, field, salvage::words, scalar};
use generals_assets::content::Declaration;
use generals_sim::skirmish::hijacker::{Rules, UpdateRules};
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let find = |name: &str| {
        modules
            .iter()
            .enumerate()
            .find(|(_, module)| module[0].tokens.get(1).is_some_and(|value| value == name))
    };
    let Some((order, crate_module)) = find("ConvertToHijackedVehicleCrateCollide") else {
        return Ok(None);
    };
    find("HijackerUpdate")
        .ok_or("Hijack crate requires HijackerUpdate for hidden driver membership")?;
    for key in ["EnterGuard", "HijackGuard"] {
        if field(&object.lines, 0, key)
            .map(scalar)
            .transpose()?
            .is_none_or(|value| !value.eq_ignore_ascii_case("Yes"))
        {
            return Err(format!("Hijacker requires translated {key}=Yes").into());
        }
    }
    if find("PhysicsBehavior").is_none_or(|(physics, _)| physics > order)
        || find("SquishCollide").is_none_or(|(squish, _)| squish < order)
    {
        return Err("Hijack conversion requires original Physics/crate/Squish ordering".into());
    }
    for key in [
        "BuildingPickup",
        "HumanOnly",
        "AllowMultiPickup",
        "ExecuteFX",
        "ExecuteAnimation",
        "ExecuteAnimationTime",
        "ExecuteAnimationZRise",
        "ExecuteAnimationFades",
    ] {
        if field(crate_module, 1, key).is_some() {
            return Err(format!("Untranslated hijack crate option {key}").into());
        }
    }
    Ok(Some(Rules {
        required: words(crate_module, 1, "RequiredKindOf"),
        forbidden: words(crate_module, 1, "ForbiddenKindOf"),
        pickup_science: field(crate_module, 1, "PickupScience")
            .map(scalar)
            .transpose()?
            .map(str::to_owned),
    }))
}

pub(super) fn updates(object: &Declaration) -> Result<Vec<UpdateRules>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "HijackerUpdate")
        })
        .map(|module| {
            Ok(UpdateRules {
                attach_bone: field(module, 1, "AttachToTargetBone")
                    .map(scalar)
                    .transpose()?
                    .unwrap_or_default()
                    .to_owned(),
                parachute: field(module, 1, "ParachuteName")
                    .map(scalar)
                    .transpose()?
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn separate_hijacker_updates_keep_authored_order_and_unused_target_bone() {
        let file = content::scan(Asset {
            path: "hijacker.ini".into(),
            origin: "authored update data".into(),
            bytes: b"Object Driver\nBehavior=HijackerUpdate First\nAttachToTargetBone=UnusedBone\nParachuteName=FirstCanopy\nEnd\nBehavior=HijackerUpdate Second\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let object = &file.declarations[0];
        assert!(load(object).unwrap().is_none());
        let parsed = updates(object).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].attach_bone, "UnusedBone");
        assert_eq!(parsed[0].parachute, "FirstCanopy");
        assert!(parsed[1].attach_bone.is_empty());
        assert!(parsed[1].parachute.is_empty());
    }
}
