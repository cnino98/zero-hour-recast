//! Fixed-bone particle slots used by the installed collapse/topple structures.
use super::{ContentLine, Error, blocks};
use generals_sim::skirmish::structure_death::{TransitionParticle, TransitionRules};

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
            particles: std::array::from_fn(|_| Vec::new()),
        };
        let mut slots = Vec::new();
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
                }
                continue;
            }
            let (state, suffix) = ["Pristine", "Damaged", "ReallyDamaged", "Rubble"]
                .iter()
                .enumerate()
                .find_map(|(index, prefix)| key.strip_prefix(prefix).map(|suffix| (index, suffix)))
                .ok_or("unknown structure transition field")?;
            let slot: usize = suffix
                .strip_prefix("ParticleSystem")
                .ok_or("structure transition FX/OCL requires original callback adapter")?
                .parse()?;
            if !(1..=8).contains(&slot) {
                return Err("invalid transition particle slot".into());
            }
            let [_, bone, random, template] = line.tokens.as_slice() else {
                return Err("invalid structure transition particle".into());
            };
            if random != "RandomBone:No" {
                return Err("random transition bone requires source client-index adapter".into());
            }
            slots.push((
                state,
                slot,
                TransitionParticle {
                    bone: bone
                        .strip_prefix("Bone:")
                        .ok_or("invalid transition bone")?
                        .into(),
                    template: template
                        .strip_prefix("PSys:")
                        .ok_or("invalid transition particle name")?
                        .into(),
                    position: [0.0; 3],
                },
            ));
        }
        // Native field offsets define slot order, independent of INI line order.
        slots.sort_by_key(|(state, slot, _)| (*state, *slot));
        for (state, _, particle) in slots {
            rules.particles[state].push(particle);
        }
        result.push(rules);
    }
    Ok(result)
}
