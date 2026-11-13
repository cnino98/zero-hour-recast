//! Authored transition particle slots and positional FX callbacks.
use super::{ContentLine, Error, blocks};
use generals_sim::skirmish::structure_death::{
    TRANSITION_SLOTS, TransitionEffect, TransitionParticle, TransitionRules,
};

pub(super) fn load(lines: &[ContentLine]) -> Result<Vec<TransitionRules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(lines, 0, "Behavior")?.into_iter().filter(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "TransitionDamageFX")
    }) {
        let mut rules = TransitionRules {
            suppress_water: false,
            suppress_water_fx: false,
            particles: std::array::from_fn(|_| Vec::new()),
            effects: std::array::from_fn(|_| Vec::new()),
        };
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            let key = &line.tokens[0];
            if matches!(
                key.as_str(),
                "DamageFXTypes" | "DamageOCLTypes" | "DamageParticleTypes"
            ) {
                let suppress = match &line.tokens[1..] {
                    tokens if tokens == ["ALL"] => false,
                    tokens if tokens == ["ALL", "-WATER"] => true,
                    _ => {
                        return Err(
                            "structure transition damage mask requires additional consumer".into(),
                        );
                    }
                };
                if key == "DamageParticleTypes" {
                    rules.suppress_water = suppress;
                } else if key == "DamageFXTypes" {
                    rules.suppress_water_fx = suppress;
                }
                continue;
            }
            let (state, suffix) = ["Pristine", "Damaged", "ReallyDamaged", "Rubble"]
                .iter()
                .enumerate()
                .find_map(|(index, prefix)| key.strip_prefix(prefix).map(|suffix| (index, suffix)))
                .ok_or("unknown structure transition field")?;
            if let Some(slot) = suffix.strip_prefix("ParticleSystem") {
                let slot = slot_index(slot)?;
                rules.particles[state].retain(|particle| particle.slot != slot);
                rules.particles[state].push(particle(line, slot)?);
            } else if let Some(slot) = suffix.strip_prefix("FXList") {
                let slot = slot_index(slot)?;
                rules.effects[state].retain(|effect| effect.slot != slot);
                rules.effects[state].push(effect(line, slot)?);
            } else {
                return Err("unsupported structure transition callback".into());
            }
        }
        // Native field offsets define slot order, independent of INI line order.
        for particles in &mut rules.particles {
            particles.sort_by_key(|particle| particle.slot);
        }
        for effects in &mut rules.effects {
            effects.sort_by_key(|effect| effect.slot);
        }
        result.push(rules);
    }
    Ok(result)
}

fn slot_index(slot: &str) -> Result<usize, Box<dyn Error>> {
    let slot: usize = slot.parse()?;
    if !(1..=TRANSITION_SLOTS).contains(&slot) {
        return Err("invalid transition slot".into());
    }
    Ok(slot)
}

fn particle(line: &ContentLine, slot: usize) -> Result<TransitionParticle, Box<dyn Error>> {
    let [_, bone, random, template] = line.tokens.as_slice() else {
        return Err("invalid transition particle".into());
    };
    let random_bone = match random.as_str() {
        "RandomBone:No" => false,
        "RandomBone:Yes" => true,
        _ => return Err("invalid transition random bone field".into()),
    };
    Ok(TransitionParticle {
        slot,
        bone: bone
            .strip_prefix("Bone:")
            .ok_or("invalid transition bone")?
            .into(),
        random_bone,
        template: template
            .strip_prefix("PSys:")
            .ok_or("invalid transition particle name")?
            .into(),
        position: [0.0; 3],
    })
}

fn effect(line: &ContentLine, slot: usize) -> Result<TransitionEffect, Box<dyn Error>> {
    let [_, location, x, y, z, template] = line.tokens.as_slice() else {
        return Err("invalid transition FX location".into());
    };
    if location != "Loc:" {
        return Err("unsupported transition FX bone location".into());
    }
    let coordinate = |value: &str, axis| {
        super::super::source_real(value.strip_prefix(axis).ok_or("invalid transition axis")?)
    };
    Ok(TransitionEffect {
        slot,
        template: template
            .strip_prefix("FXList:")
            .ok_or("invalid transition FX name")?
            .into(),
        local: [
            coordinate(x, "X:")?,
            coordinate(y, "Y:")?,
            coordinate(z, "Z:")?,
        ],
    })
}
