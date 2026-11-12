//! `SpecialAbilityUpdateModuleData`: native defaults, repeated assignments and nullable pointers.
use super::{blocks, field, scalar, source_numbers, source_real};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::special_ability::{
    Approach, Effects, Feedback, Objects, Packing, Power, PowerType, Preparation, Rules, Timings,
};
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
            .is_none_or(|family| family != "SpecialAbilityUpdate")
        {
            continue;
        }
        let mut rules = Rules {
            power: None,
            approach: Approach {
                start_range: 10_000_000.0,
                abort_range: 10_000_000.0,
                requires_los: true,
                face_target: true,
                flee_range: 0.0,
            },
            timing: Timings {
                preparation: 0,
                persistent: 0,
                pack: 0,
                unpack: 0,
                unstealth: 0,
                effect: 0,
                variation: 0.0,
            },
            objects: Objects {
                name: String::new(),
                bone: String::new(),
                maximum: 1,
                persistent: false,
                unique_targets: false,
                persist_on_death: false,
            },
            packing: Packing {
                skip_no_target: false,
                flip_after_pack: false,
                flip_after_unpack: false,
            },
            preparation: Preparation {
                always_validate: false,
                capture_fx: false,
                persistence_recharges: false,
            },
            effects: Effects {
                value: 1,
                experience: 0,
                skill_points: -1,
                lose_stealth: false,
            },
            feedback: Feedback {
                disable_particle: None,
                sounds: std::array::from_fn(|_| String::new()),
                completion_voice: String::new(),
                capture_tick: String::new(),
            },
        };
        ability_fields(files, module, &mut rules)?;
        feedback_bindings(files, object, &mut rules)?;
        result.push(rules);
    }
    Ok(result)
}

fn ability_fields(
    files: &[ContentFile],
    module: &[ContentLine],
    rules: &mut Rules,
) -> Result<(), Box<dyn Error>> {
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        let Some(key) = line.tokens.first().map(String::as_str) else {
            continue;
        };
        match key {
            "SpecialPowerTemplate" => rules.power = power_template(files, scalar(line)?)?,
            "StartAbilityRange" => rules.approach.start_range = source_real(scalar(line)?)?,
            "AbilityAbortRange" => rules.approach.abort_range = source_real(scalar(line)?)?,
            "ApproachRequiresLOS" => rules.approach.requires_los = boolean(scalar(line)?)?,
            "NeedToFaceTarget" => rules.approach.face_target = boolean(scalar(line)?)?,
            "FleeRangeAfterCompletion" => {
                rules.approach.flee_range = source_real(scalar(line)?)?;
            }
            "PreparationTime" => {
                rules.timing.preparation = source_numbers::duration_frames(scalar(line)?)?;
            }
            "PersistentPrepTime" => {
                rules.timing.persistent = source_numbers::duration_frames(scalar(line)?)?;
            }
            "PackTime" => rules.timing.pack = source_numbers::duration_frames(scalar(line)?)?,
            "UnpackTime" => {
                rules.timing.unpack = source_numbers::duration_frames(scalar(line)?)?;
            }
            "PreTriggerUnstealthTime" => {
                rules.timing.unstealth = source_numbers::duration_frames(scalar(line)?)?;
            }
            "EffectDuration" => {
                rules.timing.effect = source_numbers::duration_frames(scalar(line)?)?;
            }
            "PackUnpackVariationFactor" => rules.timing.variation = source_real(scalar(line)?)?,
            "SpecialObject" => rules.objects.name = scalar(line)?.into(),
            "SpecialObjectAttachToBone" => rules.objects.bone = scalar(line)?.into(),
            "MaxSpecialObjects" => {
                rules.objects.maximum = source_numbers::unsigned(scalar(line)?)?;
            }
            "SpecialObjectsPersistent" => rules.objects.persistent = boolean(scalar(line)?)?,
            "UniqueSpecialObjectTargets" => {
                rules.objects.unique_targets = boolean(scalar(line)?)?;
            }
            "SpecialObjectsPersistWhenOwnerDies" => {
                rules.objects.persist_on_death = boolean(scalar(line)?)?;
            }
            "SkipPackingWithNoTarget" => rules.packing.skip_no_target = boolean(scalar(line)?)?,
            "FlipOwnerAfterPacking" => rules.packing.flip_after_pack = boolean(scalar(line)?)?,
            "FlipOwnerAfterUnpacking" => {
                rules.packing.flip_after_unpack = boolean(scalar(line)?)?;
            }
            "AlwaysValidateSpecialObjects" => {
                rules.preparation.always_validate = boolean(scalar(line)?)?;
            }
            "DoCaptureFX" => rules.preparation.capture_fx = boolean(scalar(line)?)?,
            "PersistenceRequiresRecharge" => {
                rules.preparation.persistence_recharges = boolean(scalar(line)?)?;
            }
            "EffectValue" => rules.effects.value = signed(scalar(line)?)?,
            "AwardXPForTriggering" => rules.effects.experience = signed(scalar(line)?)?,
            "SkillPointsForTriggering" => rules.effects.skill_points = signed(scalar(line)?)?,
            "LoseStealthOnTrigger" => rules.effects.lose_stealth = boolean(scalar(line)?)?,
            "PackSound" => rules.feedback.sounds[0] = scalar(line)?.into(),
            "UnpackSound" => rules.feedback.sounds[1] = scalar(line)?.into(),
            "PrepSoundLoop" => rules.feedback.sounds[2] = scalar(line)?.into(),
            "TriggerSound" => rules.feedback.sounds[3] = scalar(line)?.into(),
            "DisableFXParticleSystem" => {
                rules.feedback.disable_particle =
                    super::instant_death::nullable(files, "ParticleSystem", scalar(line)?)?
                        .map(|template| template.name.clone());
            }
            _ => {}
        }
    }
    Ok(())
}

fn power_template(files: &[ContentFile], name: &str) -> Result<Option<Power>, Box<dyn Error>> {
    super::instant_death::nullable(files, "SpecialPower", name)?
        .map(|template| {
            Ok::<_, Box<dyn Error>>(Power {
                name: template.name.clone(),
                kind: PowerType::parse(
                    field(&template.lines, 0, "Enum")
                        .map(scalar)
                        .transpose()?
                        .unwrap_or("SPECIAL_INVALID"),
                )
                .ok_or("invalid native SpecialPower Enum")?,
            })
        })
        .transpose()
}

fn feedback_bindings(
    files: &[ContentFile],
    object: &Declaration,
    rules: &mut Rules,
) -> Result<(), Box<dyn Error>> {
    let kind = rules
        .power
        .as_ref()
        .map_or(PowerType::SpecialInvalid, |power| power.kind);
    let complete = match kind {
        PowerType::SpecialBlacklotusCaptureBuilding => "VoiceCaptureBuildingComplete",
        PowerType::SpecialBlacklotusDisableVehicleHack => "VoiceDisableVehicleComplete",
        PowerType::SpecialBlacklotusStealCashHack => "VoiceStealCashComplete",
        _ => "VoiceTaskComplete",
    };
    let mut voice = field(&object.lines, 0, complete);
    if voice.is_none() {
        for sounds in blocks(&object.lines, 0, "UnitSpecificSounds")? {
            if let Some(found) = field(sounds, 1, complete) {
                voice = Some(found);
            }
        }
    }
    rules.feedback.completion_voice = voice.map(scalar).transpose()?.unwrap_or_default().into();
    if let Ok(misc) = super::definition(files, "MiscAudio", "") {
        rules.feedback.capture_tick = field(&misc.lines, 0, "DefectorTimerTickSound")
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into();
    }
    Ok(())
}

fn boolean(value: &str) -> Result<bool, Box<dyn Error>> {
    if value.eq_ignore_ascii_case("Yes") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("No") {
        Ok(false)
    } else {
        Err("invalid SpecialAbilityUpdate boolean".into())
    }
}

fn signed(value: &str) -> Result<i32, Box<dyn Error>> {
    // scanType<Int> uses Int64 prefix parsing then native Int narrowing.
    Ok(i32::from_ne_bytes(
        source_numbers::unsigned(value)?.to_ne_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn actual_ability_data_keeps_unsigned_duration_prefixes_and_nullable_template_slots() {
        let file = content::scan(Asset {
            path: "ability.ini".into(), origin: "native parser fixture".into(),
            bytes: b"SpecialPower Hack\nEnum = SPECIAL_HACKER_DISABLE_BUILDING\nEnd\nParticleSystem Shower\nEnd\nObject Host\nBehavior = SpecialAbilityUpdate First\nSpecialPowerTemplate = Hack\nSpecialObject = MissingObject\nPreparationTime = 100ms\nPackTime = 101ms\nEffectDuration = -1\nEffectValue = -2147483649\nDisableFXParticleSystem = Shower\nEnd\nBehavior = SpecialAbilityUpdate Second\nSpecialPowerTemplate = MissingPower\nDisableFXParticleSystem = MissingShower\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let object = file
            .declarations
            .iter()
            .find(|entry| entry.kind == "Object")
            .unwrap();
        let rules = load(std::slice::from_ref(&file), object).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(matches!(
            rules[0].power.as_ref().unwrap().kind,
            PowerType::SpecialHackerDisableBuilding
        ));
        assert_eq!(rules[0].objects.name, "MissingObject");
        assert_eq!(rules[0].timing.preparation, 3);
        assert_eq!(rules[0].timing.pack, 4);
        assert_eq!(rules[0].timing.effect, 128_849_016);
        assert_eq!(rules[0].effects.value, i32::MAX);
        assert_eq!(
            rules[0].feedback.disable_particle.as_deref(),
            Some("Shower")
        );
        assert!(rules[1].power.is_none());
        assert!(rules[1].feedback.disable_particle.is_none());
        assert_eq!(rules[1].objects.maximum, 1);
        assert_eq!(rules[1].effects.skill_points, -1);
        assert!(rules[1].approach.requires_los);
        assert!(rules[1].approach.face_target);
    }
}
