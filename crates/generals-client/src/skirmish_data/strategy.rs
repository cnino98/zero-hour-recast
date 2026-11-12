//! Original `BattlePlanUpdate` fields and `SpecialAbility` module contract.
use super::{blocks, definition, duration_frames, field, number, scalar, text};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::battle_plan::Rules;
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some(module) = modules.iter().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "BattlePlanUpdate")
    }) else {
        return Ok(None);
    };
    let special = text(module, 1, "SpecialPowerTemplate")?;
    let power = definition(files, "SpecialPower", special)?;
    if text(&power.lines, 0, "Enum")? != "SPECIAL_CHANGE_BATTLE_PLANS"
        || duration_frames(
            field(&power.lines, 0, "ReloadTime").ok_or("battle-plan power lacks reload time")?,
        )? != 0
        || !modules.iter().any(|candidate| {
            candidate[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "SpecialAbility")
                && field(candidate, 1, "SpecialPowerTemplate")
                    .and_then(|line| line.tokens.get(1))
                    .is_some_and(|name| name == special)
                && field(candidate, 1, "UpdateModuleStartsAttack")
                    .and_then(|line| line.tokens.get(1))
                    .is_some_and(|value| value.eq_ignore_ascii_case("Yes"))
        })
    {
        return Err("unsupported battle-plan special-power contract".into());
    }
    if text(module, 1, "StrategyCenterHoldTheLineMaxHealthChangeType")? != "PRESERVE_RATIO" {
        return Err("unsupported Strategy Center health change mode".into());
    }
    let animation_frames = [
        "BombardmentPlanAnimationTime",
        "HoldTheLinePlanAnimationTime",
        "SearchAndDestroyPlanAnimationTime",
    ]
    .map(|key| {
        duration_frames(field(module, 1, key).ok_or("missing battle-plan animation duration")?)
    });
    let [bombardment, hold, search] = animation_frames;
    Ok(Some(Rules {
        animation_frames: [bombardment?, hold?, search?],
        transition_idle_frames: duration_frames(
            field(module, 1, "TransitionIdleTime").ok_or("missing battle-plan idle duration")?,
        )?,
        paralyze_frames: duration_frames(
            field(module, 1, "BattlePlanChangeParalyzeTime")
                .ok_or("missing battle-plan paralyze duration")?,
        )?,
        valid_kind_of: super::salvage::words(module, 1, "ValidMemberKindOf"),
        invalid_kind_of: super::salvage::words(module, 1, "InvalidMemberKindOf"),
        armor_scalar: number(module, 1, "HoldTheLinePlanArmorDamageScalar")?,
        sight_scalar: number(module, 1, "SearchAndDestroyPlanSightRangeScalar")?,
        center_sight_scalar: number(module, 1, "StrategyCenterSearchAndDestroySightRangeScalar")?,
        center_health_scalar: number(module, 1, "StrategyCenterHoldTheLineMaxHealthScalar")?,
        center_detects_stealth: field(module, 1, "StrategyCenterSearchAndDestroyDetectsStealth")
            .map(scalar)
            .transpose()?
            .is_none_or(|value| value.eq_ignore_ascii_case("Yes")),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    fn fixture(change: &str) -> ContentFile {
        content::scan(Asset {
            path: "StrategyCenter.ini".into(),
            origin: "original BattlePlanUpdate field contract".into(),
            bytes: format!(
                "Object Center
Behavior=SpecialAbility Special
SpecialPowerTemplate=ChangePlans
UpdateModuleStartsAttack=Yes
End
Behavior=BattlePlanUpdate Plans
SpecialPowerTemplate=ChangePlans
BombardmentPlanAnimationTime=7000
HoldTheLinePlanAnimationTime=7000
SearchAndDestroyPlanAnimationTime=7000
TransitionIdleTime=0
BattlePlanChangeParalyzeTime=5000
ValidMemberKindOf=INFANTRY CAN_ATTACK VEHICLE
InvalidMemberKindOf=DOZER STRUCTURE AIRCRAFT DRONE
HoldTheLinePlanArmorDamageScalar=0.9
SearchAndDestroyPlanSightRangeScalar=1.2
StrategyCenterSearchAndDestroySightRangeScalar=2.0
StrategyCenterHoldTheLineMaxHealthScalar=2.0
StrategyCenterHoldTheLineMaxHealthChangeType={change}
StrategyCenterSearchAndDestroyDetectsStealth=Yes
End
End
SpecialPower ChangePlans
Enum=SPECIAL_CHANGE_BATTLE_PLANS
ReloadTime=0
End
"
            )
            .into_bytes(),
        })
        .unwrap()
    }

    #[test]
    fn original_strategy_module_loads_frame_durations_and_exact_member_filters() {
        let file = fixture("PRESERVE_RATIO");
        let rules = load(std::slice::from_ref(&file), &file.declarations[0])
            .unwrap()
            .unwrap();
        assert_eq!(rules.animation_frames, [210; 3]);
        assert_eq!(rules.paralyze_frames, 150);
        assert_eq!(rules.valid_kind_of, ["INFANTRY", "CAN_ATTACK", "VEHICLE"]);
        assert_eq!(
            rules.invalid_kind_of,
            ["DOZER", "STRUCTURE", "AIRCRAFT", "DRONE"]
        );
        assert_eq!(rules.armor_scalar.to_bits(), 0.9_f32.to_bits());
        assert_eq!(rules.center_health_scalar.to_bits(), 2.0_f32.to_bits());
        let file = fixture("SAME_CURRENTHEALTH");
        assert!(load(std::slice::from_ref(&file), &file.declarations[0]).is_err());
    }
}
