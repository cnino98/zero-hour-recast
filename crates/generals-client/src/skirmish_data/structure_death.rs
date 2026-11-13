//! Original civilian collapse/topple module data. Gameplay and presentation share
//! the ordinary content snapshot; no reference-copy data is installed here.
use super::{blocks, definition, duration_frames, field, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    WeaponRules, death_lifecycle,
    structure_death::{CollapseRules, Module, Phase, Rules, ToppleRules},
};
use std::error::Error;
mod transitions;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
    lifecycle: Option<&mut death_lifecycle::Rules>,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let modules: Vec<_> = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            matches!(
                module[0].tokens.get(1).map(String::as_str),
                Some("StructureCollapseUpdate" | "StructureToppleUpdate")
            )
        })
        .collect();
    let Some(module) = modules.first() else {
        return Ok(None);
    };
    if modules.len() != 1 {
        return Err(
            "multiple structure death updates require native module instance ordering".into(),
        );
    }
    let lifecycle = lifecycle.ok_or("structure death module lacks death lifecycle")?;
    let collapse = module[0].tokens[1] == "StructureCollapseUpdate";
    validate_fields(module, collapse)?;
    let mut phases = std::array::from_fn(|_| Phase {
        effects: Vec::new(),
        objects: Vec::new(),
    });
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        if !matches!(line.tokens[0].as_str(), "FXList" | "OCL") {
            continue;
        }
        let phase = match line.tokens.get(1).map(String::as_str) {
            Some("INITIAL") => 0,
            Some("DELAY") => 1,
            Some("BURST") if collapse => 2,
            Some("FINAL") => 3,
            _ => return Err("invalid structure death phase".into()),
        };
        for name in &line.tokens[2..] {
            if line.tokens[0] == "FXList" {
                phases[phase].effects.push(name.clone());
            } else {
                phases[phase]
                    .objects
                    .push(if name.eq_ignore_ascii_case("NONE") {
                        None
                    } else {
                        Some(super::physical_ocl::load(files, name)?)
                    });
            }
        }
    }
    let data = if collapse {
        Module::Collapse(collapse_rules(files, module)?)
    } else {
        Module::Topple(topple_rules(files, module, &mut lifecycle.weapons)?)
    };
    Ok(Some(Rules {
        conditions: super::death_callbacks::conditions(module)?,
        module: data,
        phases,
    }))
}

pub(super) fn damage_transitions(
    object: &Declaration,
) -> Result<Vec<generals_sim::skirmish::structure_death::TransitionRules>, Box<dyn Error>> {
    if matches!(
        object.name.as_str(),
        "AmericaVehicleHumvee"
            | "AmericaVehicleHumveeDeadHull"
            | "GLAVehicleCombatBike"
            | "GLAVehicleBombTruck"
    ) || crate::fake_building_verification::NAMES.contains(&object.name.as_str())
        || super::REAL_FAKE_REPLACEMENTS.contains(&object.name.as_str())
        || blocks(&object.lines, 0, "Behavior")?.iter().any(|module| {
            matches!(
                module[0].tokens.get(1).map(String::as_str),
                Some("StructureCollapseUpdate" | "StructureToppleUpdate" | "TechBuildingBehavior")
            )
        })
    {
        transitions::load(&object.lines)
    } else {
        Ok(Vec::new())
    }
}

fn validate_fields(module: &[ContentLine], collapse: bool) -> Result<(), Box<dyn Error>> {
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        let key = line.tokens[0].as_str();
        let valid = matches!(
            key,
            "DeathTypes" | "VeterancyLevels" | "ExemptStatus" | "RequiredStatus"
        ) || if collapse {
            matches!(
                key,
                "MinCollapseDelay"
                    | "MaxCollapseDelay"
                    | "MinBurstDelay"
                    | "MaxBurstDelay"
                    | "CollapseDamping"
                    | "MaxShudder"
                    | "BigBurstFrequency"
                    | "OCL"
                    | "FXList"
            )
        } else {
            matches!(
                key,
                "MinToppleDelay"
                    | "MaxToppleDelay"
                    | "MinToppleBurstDelay"
                    | "MaxToppleBurstDelay"
                    | "StructuralIntegrity"
                    | "StructuralDecay"
                    | "DamageFXTypes"
                    | "TopplingFX"
                    | "ToppleDelayFX"
                    | "ToppleStartFX"
                    | "ToppleDoneFX"
                    | "CrushingFX"
                    | "CrushingWeaponName"
                    | "AngleFX"
            )
        };
        if !valid {
            return Err(format!("untranslated structure death field {key}; topple client-timed OCL/FXBone callbacks require their P04 adapter").into());
        }
    }
    Ok(())
}

fn collapse_rules(
    files: &[ContentFile],
    module: &[ContentLine],
) -> Result<CollapseRules, Box<dyn Error>> {
    let global = &definition(files, "GameData", "")?.lines;
    Ok(CollapseRules {
        delay: range(module, "MinCollapseDelay", "MaxCollapseDelay", 0)?,
        // Native constructor leaves max burst delay uninitialized: content must
        // supply it, as all installed collapse templates do.
        burst_delay: [
            field(module, 1, "MinBurstDelay").map_or(Ok(9999), duration_frames)?,
            duration_frames(
                field(module, 1, "MaxBurstDelay").ok_or("collapse MaxBurstDelay is required")?,
            )?,
        ],
        big_burst_frequency: field(module, 1, "BigBurstFrequency")
            .map(scalar)
            .transpose()?
            .unwrap_or("0")
            .parse()?,
        damping: real(module, "CollapseDamping", 0.0)?,
        shudder: real(module, "MaxShudder", 0.0)?,
        gravity: super::locomotor::per_frame(
            global,
            0,
            "Gravity",
            -1.0,
            (1.0 / 30.0) * (1.0 / 30.0),
        )?,
    })
}

fn topple_rules(
    files: &[ContentFile],
    module: &[ContentLine],
    weapons: &mut Vec<WeaponRules>,
) -> Result<ToppleRules, Box<dyn Error>> {
    let damage = field(module, 1, "DamageFXTypes").map(|line| &line.tokens[1..]);
    let suppress_water_fx = match damage {
        None => false,
        Some(tokens) if tokens == ["ALL"] => false,
        Some(tokens) if tokens == ["ALL", "-WATER"] => true,
        _ => {
            return Err(
                "topple DamageFXTypes requires additional original damage mask consumer".into(),
            );
        }
    };
    let mut angles = Vec::new();
    for line in module
        .iter()
        .filter(|line| line.depth == Some(1) && line.tokens[0] == "AngleFX")
    {
        let [_, degrees, effect] = line.tokens.as_slice() else {
            return Err("invalid topple AngleFX".into());
        };
        let scaled = super::finite_number(degrees)? * std::f32::consts::PI;
        let radians = scaled / 180.0;
        angles.push((radians, effect.clone()));
    }
    let name =
        scalar(field(module, 1, "CrushingWeaponName").ok_or("topple requires crushing weapon")?)?;
    let mut weapon = WeaponRules::unarmed();
    name.clone_into(&mut weapon.name);
    super::weapons::parse(&mut weapon, &definition(files, "Weapon", name)?.lines)?;
    let crushing_weapon = weapons.len();
    weapons.push(weapon);
    Ok(ToppleRules {
        delay: range(module, "MinToppleDelay", "MaxToppleDelay", 0)?,
        burst_delay: range(module, "MinToppleBurstDelay", "MaxToppleBurstDelay", 0)?,
        integrity: real(module, "StructuralIntegrity", 0.1)?,
        decay: real(module, "StructuralDecay", 0.0)?,
        suppress_water_fx,
        start_effect: effect(module, "ToppleStartFX")?,
        delay_effect: effect(module, "ToppleDelayFX")?,
        done_effect: effect(module, "ToppleDoneFX")?,
        crushing_effect: effect(module, "CrushingFX")?,
        angles,
        crushing_weapon,
    })
}

fn range(
    module: &[ContentLine],
    minimum: &str,
    maximum: &str,
    default: u32,
) -> Result<[u32; 2], Box<dyn Error>> {
    Ok([
        field(module, 1, minimum).map_or(Ok(default), duration_frames)?,
        field(module, 1, maximum).map_or(Ok(default), duration_frames)?,
    ])
}
fn real(module: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(default), |line| super::finite_number(scalar(line)?))
}
fn effect(module: &[ContentLine], key: &str) -> Result<String, Box<dyn Error>> {
    Ok(field(module, 1, key)
        .map(scalar)
        .transpose()?
        .unwrap_or("")
        .into())
}

/// Native resolveBoneLocations asks pristine rubble draw geometry; missing bones
/// retain the constructor's zero vector rather than inventing a surface point.
pub fn bind_bones(
    catalog: &mut [generals_sim::skirmish::UnitType],
    models: &[generals_assets::w3d::Model],
    animations: &crate::model_animation::Animations,
) {
    for (index, unit) in catalog.iter_mut().enumerate() {
        for transition in &mut unit.behavior.damage_transitions {
            for (state, particles) in transition.particles.iter_mut().enumerate() {
                let flag = [None, Some("DAMAGED"), Some("REALLYDAMAGED"), Some("RUBBLE")][state];
                for particle in particles {
                    if !particle.random_bone {
                        particle.position = animations
                            .logical_bone_position(models, index, flag, &particle.bone)
                            .unwrap_or([0.0; 3]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod installed_callbacks;
