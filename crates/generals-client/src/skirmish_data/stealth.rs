//! Original stealth status masks and independently scheduled detector modules.
use super::{blocks, duration_frames, field, scalar, text};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::stealth::{self, Detector, Rules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some((order, module)) = modules.iter().enumerate().find(|(_, module)| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "StealthUpdate")
    }) else {
        return Ok(None);
    };
    let mut upgrades = Vec::new();
    for upgrade in &modules {
        if upgrade[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "StealthUpgrade")
        {
            upgrades.extend(words(upgrade, "TriggeredBy"));
        }
    }
    Ok(Some(Rules {
        module_order: order,
        delay: field(module, 1, "StealthDelay").map_or(Ok(u32::MAX), duration_frames)?,
        forbidden: forbidden(module)?,
        threshold_speed: real(module, "MoveThresholdSpeed", 0.0)? * (1.0 / 30.0),
        required_status: statuses(module, "RequiredStatus")?,
        forbidden_status: statuses(module, "ForbiddenStatus")?,
        hint_status: statuses(module, "HintDetectableConditions")?,
        innate: boolean(module, "InnateStealth", true)?,
        use_rider: boolean(module, "UseRiderStealth", false)?,
        idle_enemies_on_reveal: boolean(module, "OrderIdleEnemiesToAttackMeUponReveal", false)?,
        wake: if boolean(module, "GrantedBySpecialPower", false)? {
            stealth::Wake::SpecialPower
        } else {
            stealth::Wake::Automatic
        },
        upgrades,
        reveal_distance: real(module, "RevealDistanceFromTarget", 0.0)?,
        black_market_frames: field(module, 1, "BlackMarketCheckDelay")
            .map_or(Ok(0), duration_frames)?,
        friendly_opacity: [
            percent(module, "FriendlyOpacityMin", 0.5)?,
            percent(module, "FriendlyOpacityMax", 1.0)?,
        ],
        pulse_frames: field(module, 1, "PulseFrequency").map_or(Ok(30), duration_frames)?,
        disguise: disguise(files, module, object)?,
    }))
}

fn disguise(
    files: &[ContentFile],
    module: &[ContentLine],
    object: &Declaration,
) -> Result<Option<stealth::DisguiseRules>, Box<dyn Error>> {
    if !boolean(module, "DisguisesAsTeam", false)? {
        return Ok(None);
    }
    let effect = |key| -> Result<String, Box<dyn Error>> {
        let name = field(module, 1, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default();
        Ok(super::instant_death::nullable(files, "FXList", name)?
            .map_or_else(String::new, |fx| fx.name.clone()))
    };
    let sounds = blocks(&object.lines, 0, "UnitSpecificSounds")?;
    let sound = |key| -> Result<String, Box<dyn Error>> {
        Ok(sounds
            .iter()
            .rev()
            .find_map(|block| field(block, 1, key))
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into())
    };
    Ok(Some(stealth::DisguiseRules {
        transition: field(module, 1, "DisguiseTransitionTime").map_or(Ok(0), duration_frames)?,
        reveal_transition: field(module, 1, "DisguiseRevealTransitionTime")
            .map_or(Ok(0), duration_frames)?,
        effect: effect("DisguiseFX")?,
        reveal_effect: effect("DisguiseRevealFX")?,
        sounds: [
            sound("DisguiseStarted")?,
            sound("DisguiseRevealedSuccess")?,
            sound("DisguiseRevealedFailure")?,
        ],
    }))
}

pub(super) fn detectors(object: &Declaration) -> Result<Vec<Detector>, Box<dyn Error>> {
    let mut result = Vec::new();
    for (order, module) in blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .enumerate()
    {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "StealthDetectorUpdate")
        {
            continue;
        }
        result.push(Detector {
            module_order: order,
            rate: field(module, 1, "DetectionRate").map_or(Ok(1), duration_frames)?,
            range: real(module, "DetectionRange", 0.0)?,
            initially_disabled: boolean(module, "InitiallyDisabled", false)?,
            required_kind: words(module, "ExtraRequiredKindOf"),
            forbidden_kind: words(module, "ExtraForbiddenKindOf"),
            while_garrisoned: boolean(module, "CanDetectWhileGarrisoned", false)?,
            while_contained: boolean(module, "CanDetectWhileContained", false)?,
            feedback: detector_feedback(module)?,
        });
    }
    Ok(result)
}

fn detector_feedback(
    lines: &[ContentLine],
) -> Result<Option<stealth::DetectorFeedback>, Box<dyn Error>> {
    let name = |key| -> Result<String, Box<dyn Error>> {
        Ok(field(lines, 1, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into())
    };
    let particles = [
        name("IRParticleSysName")?,
        name("IRBrightParticleSysName")?,
        name("IRBeaconParticleSysName")?,
    ];
    let sounds = [name("PingSound")?, name("LoudPingSound")?];
    if particles
        .iter()
        .chain(&sounds)
        .all(|name| name.is_empty() || name.eq_ignore_ascii_case("NONE"))
    {
        return Ok(None);
    }
    // IRGridParticleSysName is declared by the native module but has no
    // consumer in StealthDetectorUpdate::update at the pinned revision.
    Ok(Some(stealth::DetectorFeedback {
        bone: name("IRParticleSysBone")?,
        particles,
        sounds,
    }))
}

fn words(lines: &[ContentLine], key: &str) -> Vec<String> {
    field(lines, 1, key).map_or_else(Vec::new, |line| {
        line.tokens[1..]
            .iter()
            .filter(|word| !word.eq_ignore_ascii_case("NONE"))
            .cloned()
            .collect()
    })
}

fn statuses(lines: &[ContentLine], key: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let result = words(lines, key);
    if result.iter().any(|status| {
        !matches!(
            status.as_str(),
            "CAN_STEALTH"
                | "STEALTHED"
                | "DETECTED"
                | "IS_FIRING_WEAPON"
                | "USING_ABILITY"
                | "IS_USING_ABILITY"
                | "UNDER_CONSTRUCTION"
                | "SOLD"
        )
    }) {
        return Err(format!("unsupported stealth status mask {key}").into());
    }
    Ok(result)
}

fn forbidden(lines: &[ContentLine]) -> Result<u32, Box<dyn Error>> {
    let mut flags = 0;
    for value in words(lines, "StealthForbiddenConditions") {
        flags |= match value.as_str() {
            "ATTACKING" => stealth::ATTACKING,
            "MOVING" => stealth::MOVING,
            "USING_ABILITY" => stealth::USING_ABILITY,
            "FIRING_PRIMARY" => stealth::FIRING_PRIMARY,
            "FIRING_SECONDARY" => stealth::FIRING_SECONDARY,
            "FIRING_TERTIARY" => stealth::FIRING_TERTIARY,
            "NO_BLACK_MARKET" => stealth::NO_BLACK_MARKET,
            "TAKING_DAMAGE" => stealth::TAKING_DAMAGE,
            "RIDERS_ATTACKING" => stealth::RIDERS_ATTACKING,
            _ => return Err(format!("unsupported stealth forbidden condition {value}").into()),
        };
    }
    Ok(flags)
}

fn boolean(lines: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    let Some(line) = field(lines, 1, key) else {
        return Ok(default);
    };
    match scalar(line)? {
        "Yes" => Ok(true),
        "No" => Ok(false),
        _ => Err(format!("invalid stealth boolean {key}").into()),
    }
}

fn real(lines: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(lines, 1, key).map_or(Ok(default), |line| super::finite_number(scalar(line)?))
}

fn percent(lines: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    if field(lines, 1, key).is_none() {
        return Ok(default);
    }
    Ok(super::finite_number(text(lines, 1, key)?.trim_end_matches('%'))? * 0.01)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::content;

    fn fixture(bytes: &[u8]) -> generals_assets::content::ContentFile {
        content::scan(generals_assets::virtual_assets::Asset {
            path: "stealth.ini".into(),
            origin: "original stealth fields".into(),
            bytes: bytes.to_vec(),
        })
        .unwrap()
    }

    #[test]
    fn original_pathfinder_velocity_opacity_and_scan_defaults_preserve_source_units() {
        let file = fixture(b"Object Pathfinder\nBehavior = StealthDetectorUpdate Scan\nDetectionRate = 500\nEnd\nBehavior = StealthUpdate Cloak\nStealthDelay = 0\nStealthForbiddenConditions = MOVING\nMoveThresholdSpeed = 3\nFriendlyOpacityMin = 30%\nFriendlyOpacityMax = 80%\nPulseFrequency = 500\nEnd\nEnd\n");
        let files = [file];
        let object = super::super::definition(&files, "Object", "Pathfinder").unwrap();
        let stealth = load(&files, object).unwrap().unwrap();
        assert_eq!(stealth.delay, 0);
        assert_eq!(stealth.forbidden, stealth::MOVING);
        assert_eq!(
            stealth.threshold_speed.to_bits(),
            (3.0_f32 * (1.0 / 30.0)).to_bits()
        );
        assert_eq!(
            stealth.friendly_opacity.map(f32::to_bits),
            [(30.0_f32 * 0.01).to_bits(), (80.0_f32 * 0.01).to_bits()]
        );
        assert_eq!(stealth.pulse_frames, 15);
        let detection = detectors(object).unwrap();
        assert_eq!(detection[0].rate, 15);
        assert_eq!(detection[0].range.to_bits(), 0.0_f32.to_bits());
        assert!(detection[0].module_order < stealth.module_order);
    }

    #[test]
    fn original_camo_upgrade_and_forbidden_status_remain_explicit() {
        let file = fixture(b"Object Rebel\nBehavior = StealthUpdate Cloak\nStealthDelay = 2500\nStealthForbiddenConditions = ATTACKING USING_ABILITY\nForbiddenStatus = SOLD UNDER_CONSTRUCTION\nInnateStealth = No\nEnd\nBehavior = StealthUpgrade Upgrade\nTriggeredBy = Upgrade_GLACamouflage\nEnd\nEnd\n");
        let files = [file];
        let object = super::super::definition(&files, "Object", "Rebel").unwrap();
        let stealth = load(&files, object).unwrap().unwrap();
        assert!(!stealth.innate);
        assert_eq!(stealth.delay, 75);
        assert_eq!(stealth.upgrades, ["Upgrade_GLACamouflage"]);
        assert_eq!(stealth.forbidden_status, ["SOLD", "UNDER_CONSTRUCTION"]);
    }
}
