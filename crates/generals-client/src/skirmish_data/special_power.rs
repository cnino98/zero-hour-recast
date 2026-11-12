//! Native `SpecialAbility` base and `UnpauseSpecialPowerUpgrade` metadata.
use super::{blocks, definition, field, finite_number, scalar, source_numbers};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::special_power::{Rules, Startup, Timer};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "SpecialAbility")
        {
            continue;
        }
        let name = string(module, 1, "SpecialPowerTemplate")?;
        let template = definition(files, "SpecialPower", &name)?;
        let lines = &template.lines;
        let power_type = string(lines, 0, "Enum")?;
        let public_timer = boolean(lines, 0, "PublicTimer")?;
        if public_timer {
            return Err("SpecialAbility public superweapon timer UI is not implemented".into());
        }
        let global = definition(files, "GameData", "")?;
        let view_name = string(&global.lines, 0, "SpecialPowerViewObjectName")?;
        let view_template = super::instant_death::nullable(files, "Object", &view_name)?
            .map(|definition| definition.name.clone());
        result.push(Rules {
            tag: module[0]
                .tokens
                .get(2)
                .ok_or("SpecialAbility has no module tag")?
                .clone(),
            name,
            power_type,
            required_science: string(lines, 0, "RequiredScience")?,
            timer: Timer {
                reload_frames: duration(lines, 0, "ReloadTime")?,
                shared: boolean(lines, 0, "SharedSyncedTimer")?,
                public: public_timer,
            },
            view_range: field(lines, 0, "ViewObjectRange")
                .map_or(Ok(0.0), |line| finite_number(scalar(line)?))?,
            view_frames: duration(lines, 0, "ViewObjectDuration")?,
            view_template,
            initiate_sound: string(lines, 0, "InitiateSound")?,
            target_sound: string(lines, 0, "InitiateAtLocationSound")?,
            module_initiate_sound: string(module, 1, "InitiateSound")?,
            academy_classification: field(lines, 0, "AcademyClassify")
                .map(scalar)
                .transpose()?
                .unwrap_or("ACT_NONE")
                .into(),
            update_starts_attack: boolean(module, 1, "UpdateModuleStartsAttack")?,
            startup: Startup {
                paused: boolean(module, 1, "StartsPaused")?,
                ready: boolean(module, 1, "StartsReady")?,
            },
            script_only: boolean(module, 1, "ScriptedSpecialPowerOnly")?,
        });
    }
    Ok(result)
}

pub(super) fn load_unpause(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Option<String>>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "UnpauseSpecialPowerUpgrade")
        })
        .map(|module| {
            let name = string(module, 1, "SpecialPowerTemplate")?;
            // parseSpecialPowerTemplate stores the resolved pointer, including
            // null for an absent or unresolved name; a debug assert is not a throw.
            Ok(
                super::instant_death::nullable(files, "SpecialPower", &name)?
                    .map(|template| template.name.clone()),
            )
        })
        .collect()
}

fn string(lines: &[ContentLine], depth: u32, name: &str) -> Result<String, Box<dyn Error>> {
    Ok(field(lines, depth, name)
        .map(scalar)
        .transpose()?
        .unwrap_or("")
        .into())
}
fn duration(lines: &[ContentLine], depth: u32, name: &str) -> Result<u32, Box<dyn Error>> {
    field(lines, depth, name).map_or(Ok(0), |line| source_numbers::duration_frames(scalar(line)?))
}
fn boolean(lines: &[ContentLine], depth: u32, name: &str) -> Result<bool, Box<dyn Error>> {
    match field(lines, depth, name)
        .map(scalar)
        .transpose()?
        .unwrap_or("No")
    {
        value if value.eq_ignore_ascii_case("Yes") => Ok(true),
        value if value.eq_ignore_ascii_case("No") => Ok(false),
        _ => Err(format!("invalid special power boolean {name}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn unpause_instances_preserve_authored_order_and_resolved_null_templates() {
        let file = content::scan(Asset {
            path: "unpause.ini".into(),
            origin: "native upgrade fixture".into(),
            bytes: b"SpecialPower Capture\nEnum = SPECIAL_INFANTRY_CAPTURE_BUILDING\nEnd\nObject Host\nBehavior = UnpauseSpecialPowerUpgrade First\nSpecialPowerTemplate = Capture\nEnd\nBehavior = UnpauseSpecialPowerUpgrade Null\nSpecialPowerTemplate = None\nEnd\nBehavior = UnpauseSpecialPowerUpgrade Missing\nSpecialPowerTemplate = NotDefined\nEnd\nBehavior = UnpauseSpecialPowerUpgrade Default\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let files = [file];
        let rules = load_unpause(&files, definition(&files, "Object", "Host").unwrap()).unwrap();
        assert_eq!(rules, vec![Some("Capture".into()), None, None, None]);
    }

    #[test]
    fn air_force_special_ability_preserves_module_tag_and_science() {
        let file = content::scan(Asset {
            path: "spectre-power.ini".into(), origin: "native special power fixture".into(),
            bytes: b"GameData\nEnd\nSpecialPower AirF_SuperweaponSpectreGunship\nEnum = AIRF_SPECIAL_SPECTRE_GUNSHIP\nRequiredScience = SCIENCE_SpectreGunship1\nSharedSyncedTimer = Yes\nReloadTime = 240000\nEnd\nObject Host\nBehavior = SpecialAbility SpectreTag\nSpecialPowerTemplate = AirF_SuperweaponSpectreGunship\nUpdateModuleStartsAttack = Yes\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let files = [file];
        let rules = load(&files, definition(&files, "Object", "Host").unwrap()).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].tag, "SpectreTag");
        assert_eq!(rules[0].required_science, "SCIENCE_SpectreGunship1");
        assert_eq!(rules[0].timer.reload_frames, 7200);
        assert!(rules[0].timer.shared && rules[0].update_starts_attack);
    }
}
