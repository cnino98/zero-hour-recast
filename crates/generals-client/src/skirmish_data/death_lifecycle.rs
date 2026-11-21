//! Native Terrorist and toxin-corpse modules; unsupported death forms fail admission.
use super::{blocks, definition, duration_frames, field, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    WeaponRules,
    death_lifecycle::{Derived, DieAction, Fling, Physics, Rules, SlowDeath, Spawn},
};
use std::error::Error;

pub(super) mod inherited;
#[cfg(test)]
mod tests;

const FRAME_SECONDS: f32 = 1.0 / 30.0;

pub(super) const TOXIC: [&str; 6] = [
    "ToxicInfantry_TerroristOnly",
    "ToxicInfantryBeta_TerroristOnly",
    "ToxicInfantry",
    "ToxicInfantryBeta",
    "ToxicInfantryGamma_TerroristOnly",
    "ToxicInfantryGamma",
];

/// Consume a reconciled object: surviving modules at depth zero, fields at depth one.
/// Raw declaration fixtures must materialize inheritance through `module_metadata` first.
pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let bus = object.name == "GLAVehicleBattleBus";
    let fake = super::has_flag(&object.lines, "KindOf", "FS_FAKE");
    let landmark = super::has_flag(&object.lines, "KindOf", "LANDMARK_BRIDGE");
    let neutral = field(&object.lines, 0, "Side")
        .is_none_or(|line| line.tokens.get(1).is_some_and(|side| side == "Civilian"));
    let modules = blocks(&object.lines, 0, "Behavior")?;
    if !modules.iter().any(|module| {
        module[0].tokens.get(1).is_some_and(|family| {
            matches!(
                family.as_str(),
                "SlowDeathBehavior"
                    | "BattleBusSlowDeathBehavior"
                    | "JetSlowDeathBehavior"
                    | "HelicopterSlowDeathBehavior"
                    | "NeutronMissileSlowDeathBehavior"
            )
        })
    }) && !fake
        && !landmark
        && !neutral
    {
        return Ok(None);
    }
    let physics = modules.iter().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "PhysicsBehavior")
    });
    let mut rules = Rules {
        // The effective declaration already contains precisely the surviving modules.
        on_die: Vec::new(),
        slow: Vec::new(),
        weapons: Vec::new(),
        physics: physics
            .map(|module| physical_rules(files, object, module))
            .transpose()?,
    };
    for module in modules {
        if let Some(
            "SlowDeathBehavior"
            | "BattleBusSlowDeathBehavior"
            | "JetSlowDeathBehavior"
            | "HelicopterSlowDeathBehavior"
            | "NeutronMissileSlowDeathBehavior",
        ) = module[0].tokens.get(1).map(String::as_str)
        {
            let mut rule = slow(files, module, &mut rules.weapons)?;
            rule.derived = match module[0].tokens[1].as_str() {
                "BattleBusSlowDeathBehavior" => {
                    Derived::BattleBus(Box::new(super::battle_bus::death(files, module)?))
                }
                "JetSlowDeathBehavior" => {
                    Derived::Jet(Box::new(super::aircraft_death::jet(files, module)?))
                }
                "HelicopterSlowDeathBehavior" => Derived::Helicopter(Box::new(
                    super::helicopter_death::load(files, object, module)?,
                )),
                "NeutronMissileSlowDeathBehavior" => {
                    Derived::Neutron(Box::new(super::neutron_missile::load_death(files, module)?))
                }
                _ => Derived::None,
            };
            rules.slow.push(rule);
            if fake || bus || landmark || neutral {
                rules.on_die.push(DieAction::Slow);
            }
        }
    }
    validate_rules(&rules)?;
    Ok(Some(rules))
}

fn physical_rules(
    files: &[ContentFile],
    object: &Declaration,
    module: &[ContentLine],
) -> Result<Physics, Box<dyn Error>> {
    let global = &definition(files, "GameData", "")?.lines;
    let names = super::locomotor::normal_set(&object.lines)?;
    let normal = names
        .first()
        .map(|name| definition(files, "Locomotor", name))
        .transpose()?;
    let lines = normal.map_or(&[][..], |source| source.lines.as_slice());
    let stick_to_ground = super::locomotor::boolean(lines, "StickToGround")?;
    let airborne_friction = super::locomotor::boolean(lines, "Apply2DFrictionWhenAirborne")?;
    for name in names {
        let lines = &definition(files, "Locomotor", name)?.lines;
        if super::locomotor::boolean(lines, "StickToGround")? != stick_to_ground
            || super::locomotor::boolean(lines, "Apply2DFrictionWhenAirborne")? != airborne_friction
        {
            return Err("Surface locomotors require differing unported death physics".into());
        }
    }
    Ok(Physics {
        gravity: super::locomotor::per_frame(
            global,
            0,
            "Gravity",
            -1.0,
            FRAME_SECONDS * FRAME_SECONDS,
        )?,
        stiffness: super::locomotor::signed_real(global, 0, "GroundStiffness", 0.5)?,
        mass: field(module, 1, "Mass")
            .map(scalar)
            .transpose()?
            .unwrap_or("1")
            .parse()?,
        forward: coefficient(module, "ForwardFriction", 0.15)?,
        lateral: coefficient(module, "LateralFriction", 0.15)?,
        aerodynamic: coefficient(module, "AerodynamicFriction", 0.0)?,
        stick_to_ground,
        airborne_friction,
    })
}

fn coefficient(lines: &[ContentLine], name: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    // PhysicsBehavior stores parsed velocities per frame; defaults already use native units.
    super::locomotor::per_frame(lines, 1, name, default, FRAME_SECONDS)
}

fn frames(lines: &[ContentLine], name: &str) -> Result<u32, Box<dyn Error>> {
    field(lines, 1, name).map_or(Ok(0), duration_frames)
}

fn slow(
    files: &[ContentFile],
    module: &[ContentLine],
    weapons: &mut Vec<WeaponRules>,
) -> Result<SlowDeath, Box<dyn Error>> {
    let family = module[0].tokens[1].as_str();
    let supported = matches!(
        family,
        "SlowDeathBehavior"
            | "BattleBusSlowDeathBehavior"
            | "JetSlowDeathBehavior"
            | "HelicopterSlowDeathBehavior"
            | "NeutronMissileSlowDeathBehavior"
    );
    for line in module
        .iter()
        .filter(|line| supported && line.depth == Some(1))
    {
        if !matches!(
            line.tokens.first().map(String::as_str),
            Some(
                "DeathTypes"
                    | "VeterancyLevels"
                    | "RequiredStatus"
                    | "ExemptStatus"
                    | "SinkRate"
                    | "ProbabilityModifier"
                    | "ModifierBonusPerOverkillPercent"
                    | "SinkDelay"
                    | "SinkDelayVariance"
                    | "DestructionDelay"
                    | "DestructionDelayVariance"
                    | "DestructionAltitude"
                    | "FX"
                    | "OCL"
                    | "Weapon"
                    | "FlingForce"
                    | "FlingForceVariance"
                    | "FlingPitch"
                    | "FlingPitchVariance"
                    | "End"
            )
        ) && !derived_field(family, &line.tokens[0])
        {
            return Err(format!("untranslated {family} field {}", line.tokens[0]).into());
        }
    }
    let fling = if field(module, 1, "FlingForce").is_some() {
        Some(Fling {
            force: [
                super::locomotor::signed_real(module, 1, "FlingForce", 0.0)?,
                super::locomotor::signed_real(module, 1, "FlingForceVariance", 0.0)?,
            ],
            pitch: [
                super::locomotor::signed_real(module, 1, "FlingPitch", 0.0)?.to_radians(),
                super::locomotor::signed_real(module, 1, "FlingPitchVariance", 0.0)?.to_radians(),
            ],
        })
    } else {
        None
    };
    let mut rule = SlowDeath {
        derived: Derived::None,
        tag: module[0]
            .tokens
            .get(2)
            .ok_or("SlowDeath missing tag")?
            .clone(),
        conditions: super::death_callbacks::conditions(module)?,
        probability: field(module, 1, "ProbabilityModifier")
            .map(scalar)
            .transpose()?
            .unwrap_or("10")
            .parse()?,
        overkill_bonus: field(module, 1, "ModifierBonusPerOverkillPercent")
            .map(scalar)
            .transpose()?
            .map_or(Ok(0.0), |value| {
                super::finite_number(value.trim_end_matches('%')).map(|value| value * 0.01)
            })?,
        sink_delay: [
            frames(module, "SinkDelay")?,
            frames(module, "SinkDelayVariance")?,
        ],
        destruction_delay: [
            frames(module, "DestructionDelay")?,
            frames(module, "DestructionDelayVariance")?,
        ],
        sink_rate: coefficient(module, "SinkRate", 0.0)?,
        fling,
        fx: std::array::from_fn(|_| Vec::new()),
        objects: std::array::from_fn(|_| Vec::new()),
        weapons: std::array::from_fn(|_| Vec::new()),
    };
    if rule.probability == 0 || rule.probability > i32::MAX.unsigned_abs() {
        return Err("SlowDeath ProbabilityModifier must be positive".into());
    }
    phase_callbacks(files, module, &mut rule, weapons)?;
    Ok(rule)
}

fn phase_callbacks(
    files: &[ContentFile],
    module: &[ContentLine],
    rule: &mut SlowDeath,
    weapons: &mut Vec<WeaponRules>,
) -> Result<(), Box<dyn Error>> {
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        let Some(key) = line.tokens.first().map(String::as_str) else {
            continue;
        };
        if !matches!(key, "FX" | "OCL" | "Weapon") {
            continue;
        }
        let phase = match line.tokens.get(1).map(String::as_str) {
            Some("INITIAL") => 0,
            Some("MIDPOINT") => 1,
            Some("FINAL") => 2,
            _ => return Err("invalid SlowDeath phase".into()),
        };
        for name in &line.tokens[2..] {
            if key == "FX" {
                let resolved = super::fx_definition(files, name)?;
                rule.fx[phase]
                    .push(resolved.map_or_else(String::new, |source| source.name.clone()));
                continue;
            }
            let kind = if key == "OCL" {
                "ObjectCreationList"
            } else {
                "Weapon"
            };
            let resolved = files
                .iter()
                .flat_map(|file| &file.declarations)
                .find(|declaration| declaration.kind == kind && &declaration.name == name);
            if key == "OCL" {
                rule.objects[phase].push(if resolved.is_some() {
                    spawn(files, name)?
                } else {
                    Spawn::None
                });
            } else if let Some(source) = resolved {
                let mut weapon = WeaponRules::unarmed();
                name.clone_into(&mut weapon.name);
                super::weapons::parse(&mut weapon, &source.lines)?;
                rule.weapons[phase].push(weapons.len());
                weapons.push(weapon);
            } else {
                rule.weapons[phase].push(usize::MAX);
            }
        }
    }
    Ok(())
}

fn derived_field(family: &str, key: &str) -> bool {
    match family {
        "BattleBusSlowDeathBehavior" => matches!(
            key,
            "FXStartUndeath"
                | "OCLStartUndeath"
                | "FXHitGround"
                | "OCLHitGround"
                | "ThrowForce"
                | "PercentDamageToPassengers"
                | "EmptyHulkDestructionDelay"
        ),
        "JetSlowDeathBehavior" => matches!(
            key,
            "FXOnGroundDeath"
                | "OCLOnGroundDeath"
                | "FXInitialDeath"
                | "OCLInitialDeath"
                | "DelaySecondaryFromInitialDeath"
                | "FXSecondary"
                | "OCLSecondary"
                | "FXHitGround"
                | "OCLHitGround"
                | "DelayFinalBlowUpFromHitGround"
                | "FXFinalBlowUp"
                | "OCLFinalBlowUp"
                | "DeathLoopSound"
                | "RollRate"
                | "RollRateDelta"
                | "PitchRate"
                | "FallHowFast"
        ),
        "HelicopterSlowDeathBehavior" => matches!(
            key,
            "SpiralOrbitTurnRate"
                | "SpiralOrbitForwardSpeed"
                | "SpiralOrbitForwardSpeedDamping"
                | "MinSelfSpin"
                | "MaxSelfSpin"
                | "SelfSpinUpdateDelay"
                | "SelfSpinUpdateAmount"
                | "FallHowFast"
                | "MinBladeFlyOffDelay"
                | "MaxBladeFlyOffDelay"
                | "AttachParticle"
                | "AttachParticleBone"
                | "AttachParticleLoc"
                | "BladeObjectName"
                | "BladeBoneName"
                | "OCLEjectPilot"
                | "FXBlade"
                | "OCLBlade"
                | "FXHitGround"
                | "OCLHitGround"
                | "FXFinalBlowUp"
                | "OCLFinalBlowUp"
                | "DelayFromGroundToFinalDeath"
                | "FinalRubbleObject"
                | "SoundDeathLoop"
                | "MaxBraking"
        ),
        "NeutronMissileSlowDeathBehavior" => {
            key == "FXList"
                || key == "ScorchMarkSize"
                || key.strip_prefix("Blast").is_some_and(|field| {
                    matches!(field.as_bytes().first(), Some(b'1'..=b'9'))
                        && matches!(
                            &field[1..],
                            "Enabled"
                                | "Delay"
                                | "ScorchDelay"
                                | "InnerRadius"
                                | "OuterRadius"
                                | "MaxDamage"
                                | "MinDamage"
                                | "ToppleSpeed"
                                | "PushForce"
                        )
                })
        }
        _ => false,
    }
}

fn spawn(files: &[ContentFile], name: &str) -> Result<Spawn, Box<dyn Error>> {
    let ocl = definition(files, "ObjectCreationList", name)?;
    let toxin = blocks(&ocl.lines, 0, "CreateObject")?.iter().any(|lines| {
        field(lines, 1, "ObjectNames").is_some_and(|line| {
            line.tokens[1..]
                .iter()
                .any(|name| TOXIC.contains(&name.as_str()) || name == "FlamingInfantry")
        })
    });
    if !toxin {
        return Ok(Spawn::Physical(super::physical_ocl::load(files, name)?));
    }
    let ocl = definition(files, "ObjectCreationList", name)?;
    let nuggets = blocks(&ocl.lines, 0, "CreateObject")?;
    if nuggets.len() != 1 {
        return Err("Terrorist toxin OCL requires exactly one original object".into());
    }
    let lines = nuggets[0];
    for line in &lines[1..lines.len() - 1] {
        if !matches!(
            line.tokens[0].as_str(),
            "ObjectNames" | "Disposition" | "SkipIfSignificantlyAirborne"
        ) {
            return Err("untranslated toxin OCL override".into());
        }
    }
    let name = scalar(field(lines, 1, "ObjectNames").ok_or("missing toxin ObjectNames")?)?;
    if !(TOXIC.contains(&name) || name == "FlamingInfantry")
        || scalar(field(lines, 1, "Disposition").ok_or("missing toxin disposition")?)?
            != "LIKE_EXISTING"
    {
        return Err("untranslated toxin corpse object/disposition".into());
    }
    let skip_airborne = match field(lines, 1, "SkipIfSignificantlyAirborne")
        .map(scalar)
        .transpose()?
    {
        Some("Yes" | "YES") => true,
        Some("No" | "NO") | None => false,
        _ => return Err("invalid SkipIfSignificantlyAirborne".into()),
    };
    Ok(Spawn::Object {
        name: name.into(),
        skip_airborne,
    })
}

fn validate_rules(rules: &Rules) -> Result<(), Box<dyn Error>> {
    if rules.physics.as_ref().is_some_and(|physics| {
        physics.mass <= 0.0 || !physics.mass.is_finite() || physics.gravity >= 0.0
    }) {
        return Err("invalid death physics".into());
    }
    Ok(())
}
