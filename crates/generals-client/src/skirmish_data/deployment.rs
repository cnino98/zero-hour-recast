//! Original `DeployStyleAIUpdateModuleData` admission for automatic animations.
use super::{blocks, duration_frames, field, scalar};
use generals_assets::content::{ContentLine, Declaration};
use generals_sim::skirmish::deployment::Rules;
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some(lines) = modules.into_iter().find(|lines| {
        lines[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "DeployStyleAIUpdate")
    }) else {
        return Ok(None);
    };
    // The native field is parsed but never read by update/setMyState.
    let _reset = boolean(lines, "ResetTurretBeforePacking")?;
    let sounds = blocks(&object.lines, 0, "UnitSpecificSounds")?;
    let sounds = ["Deploy", "Undeploy"].map(|key| {
        sounds
            .first()
            .and_then(|lines| field(lines, 1, key))
            .map(scalar)
            .transpose()
            .map(|name| name.unwrap_or("").to_owned())
    });
    let [deploy, undeploy] = sounds;
    Ok(Some(Rules {
        manual: boolean(lines, "ManualDeployAnimations")?,
        pack: field(lines, 1, "PackTime").map_or(Ok(0), duration_frames)?,
        unpack: field(lines, 1, "UnpackTime").map_or(Ok(0), duration_frames)?,
        turret_only_deployed: boolean(lines, "TurretsFunctionOnlyWhenDeployed")?,
        center_before_packing: boolean(lines, "TurretsMustCenterBeforePacking")?,
        sounds: [deploy?, undeploy?],
    }))
}

fn boolean(lines: &[ContentLine], key: &str) -> Result<bool, Box<dyn Error>> {
    match field(lines, 1, key).map(scalar).transpose()? {
        None => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
        _ => Err(format!("invalid deployment flag {key}").into()),
    }
}

#[cfg(test)]
mod tests {
    use generals_assets::virtual_assets::VirtualAssets;

    #[ignore = "requires original game installation"]
    #[test]
    fn sentry_drone_keeps_native_deployment_weapon_upgrade_and_physical_death_dependencies() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        let object =
            super::super::definition(&files, "Object", "AmericaVehicleSentryDrone").unwrap();
        let unit = super::super::unit_type(
            &files,
            object,
            generals_sim::skirmish::Faction::Usa,
            generals_sim::skirmish::Kind::Vehicle,
        )
        .and_then(|unit| super::super::finish_type(&files, object, unit))
        .unwrap();
        let deployment = unit.deployment.as_ref().unwrap();
        assert_eq!([deployment.pack, deployment.unpack], [30, 30]);
        assert!(deployment.turret_only_deployed && deployment.center_before_packing);
        assert_eq!(unit.cost, 800);
        assert_eq!(unit.max_health.to_bits(), 300.0_f32.to_bits());
        assert_eq!(unit.build_seconds.to_bits(), 10.0_f32.to_bits());
        assert_eq!(unit.weapon.name, "");
        assert!(
            unit.weapon_set_rules
                .templates
                .iter()
                .any(|entry| entry.rules.name == "SentryDroneGun")
        );
        assert_eq!(
            unit.death_lifecycle.as_ref().unwrap().slow[0].destruction_delay,
            [15, 3]
        );
        assert!(
            generals_sim::skirmish::physical_ocl::lists(&unit)
                .flat_map(|list| &list.nuggets)
                .any(|entry| entry
                    .names
                    .iter()
                    .any(|name| name == "AmericaVehicleSentryDroneHulk"))
        );
        assert_eq!(unit.behavior.stealth.as_ref().unwrap().delay, 60);
    }
}
