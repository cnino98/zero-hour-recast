//! Concrete base Overlord OCL/command-upgrade bindings; no guessed child types.
use super::{blocks, definition, field, number, scalar, text};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::overlord::{Propaganda, Upgrade};
use std::error::Error;

pub(super) fn supported(name: &str) -> bool {
    matches!(
        name,
        "Upgrade_ChinaOverlordGattlingCannon"
            | "Upgrade_ChinaOverlordBattleBunker"
            | "Upgrade_ChinaOverlordPropagandaTower"
            | "Upgrade_ChinaSubliminalMessaging"
    )
}

pub(super) fn upgrades(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Upgrade>, Box<dyn Error>> {
    if !matches!(
        object.name.as_str(),
        "ChinaTankOverlord" | "ChinaVehicleHelix"
    ) {
        return Ok(Vec::new());
    }
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let mut upgrades = Vec::new();
    for module in &modules {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "ObjectCreationUpgrade")
        {
            continue;
        }
        let trigger = text(module, 1, "TriggeredBy")?;
        if !supported(trigger) && !super::helix::supported(trigger) {
            continue;
        }
        let ocl = definition(
            files,
            "ObjectCreationList",
            text(module, 1, "UpgradeObject")?,
        )?;
        let creates = blocks(&ocl.lines, 0, "CreateObject")?;
        if creates.len() != 1
            || text(creates[0], 1, "Count")? != "1"
            || text(creates[0], 1, "ContainInsideSourceObject")? != "Yes"
        {
            return Err("unsupported attachment creation list".into());
        }
        let command = modules
            .iter()
            .find(|candidate| {
                candidate[0]
                    .tokens
                    .get(1)
                    .is_some_and(|name| name == "CommandSetUpgrade")
                    && field(candidate, 1, "TriggeredBy")
                        .and_then(|line| line.tokens.get(1))
                        .is_some_and(|name| name == trigger)
            })
            .ok_or("attachment lacks command-set upgrade")?;
        upgrades.push(Upgrade {
            trigger: trigger.into(),
            conflicts: field(module, 1, "ConflictsWith")
                .ok_or("attachment lacks conflict list")?
                .tokens[1..]
                .to_vec(),
            object: text(creates[0], 1, "ObjectNames")?.into(),
            command_set: text(command, 1, "CommandSet")?.into(),
        });
    }
    Ok(upgrades)
}

pub(super) fn bone(object: &Declaration) -> Result<Option<String>, Box<dyn Error>> {
    for draw in blocks(&object.lines, 0, "Draw")? {
        if draw[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "W3DDependencyModelDraw")
        {
            return Ok(Some(text(draw, 1, "AttachToBoneInContainer")?.into()));
        }
    }
    Ok(None)
}

pub(super) fn propaganda(object: &Declaration) -> Result<Option<Propaganda>, Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "PropagandaTowerBehavior")
        {
            continue;
        }
        let percent = |key| -> Result<f32, Box<dyn Error>> {
            Ok(super::finite_number(text(module, 1, key)?.trim_end_matches('%'))? / 100.0)
        };
        return Ok(Some(Propaganda {
            radius: number(module, 1, "Radius")?,
            delay: super::duration_frames(
                field(module, 1, "DelayBetweenUpdates").ok_or("missing propaganda delay")?,
            )?,
            heal: [
                percent("HealPercentEachSecond")?,
                percent("UpgradedHealPercentEachSecond")?,
            ],
            upgrade: text(module, 1, "UpgradeRequired")?.into(),
            pulse: [
                text(module, 1, "PulseFX")?.into(),
                text(module, 1, "UpgradedPulseFX")?.into(),
            ],
            affects_self: field(module, 1, "AffectsSelf")
                .map(scalar)
                .transpose()?
                .is_some_and(|value| value.eq_ignore_ascii_case("Yes")),
        }));
    }
    Ok(None)
}

pub(super) fn thresholds(files: &[ContentFile]) -> Result<[f32; 2], Box<dyn Error>> {
    let game = &definition(files, "GameData", "")?.lines;
    Ok([
        super::optional_number(game, "UnitDamagedThreshold", 0.5)?,
        super::optional_number(game, "UnitReallyDamagedThreshold", 0.1)?,
    ])
}
