//! Source FX capture readiness after transient systems and particles expire.
use super::{Nugget, Particles};
use crate::particles::Shader;
use num_traits::ToPrimitive;
use std::error::Error;

pub(super) fn complete(
    particles: &Particles,
    name: &str,
    generation: u32,
) -> Result<bool, Box<dyn Error>> {
    if particles.generation != generation {
        return Ok(false);
    }
    let list = *particles
        .actor_fx
        .get(name)
        .ok_or("missing original actor FX binding while checking flash retirement")?;
    let mut checked = false;
    for nugget in &particles.lists[list] {
        if let Nugget::AtBones { .. } = nugget {
            return Err("flash inspection requires direct actor FX nuggets".into());
        }
        let Nugget::Particle { template, .. } = nugget else {
            continue;
        };
        if !matches!(
            particles.templates[*template].drawing.shader,
            Shader::Additive
        ) {
            continue;
        }
        checked = true;
        // Include delayed and empty emitters: their native update may still emit.
        // Alpha dust and descendant smoke are intentionally left untouched.
        if particles
            .emitters
            .iter()
            .any(|emitter| emitter.template == *template)
        {
            return Ok(false);
        }
    }
    if !checked {
        return Err("actor FX has no original direct additive flash".into());
    }
    Ok(true)
}

#[cfg(test)]
mod aircraft_tests;

// Only the original positional collapse FX dependency closure is relevant.
// POST body/transition smoke belongs to independent repeating source systems.
const COLLAPSE_EFFECTS: [&str; 4] = [
    "FX_StructureCollapseInitial",
    "FX_StructureCollapseDelay",
    "FX_StructureCollapseBurst",
    "FX_StructureCollapseFinal",
];

pub(super) fn collapse_complete(
    particles: &Particles,
    generation: u32,
    final_frame: u64,
    now: u64,
) -> Result<bool, Box<dyn Error>> {
    if particles.generation != generation || now < final_frame {
        return Ok(false);
    }
    Ok(now - final_frame >= collapse_bound(particles)?)
}

fn collapse_bound(particles: &Particles) -> Result<u64, Box<dyn Error>> {
    let mut maximum = 0;
    let mut checked = false;
    for name in COLLAPSE_EFFECTS {
        let list = *particles
            .actor_fx
            .get(name)
            .ok_or("original collapse FX binding missing")?;
        for nugget in &particles.lists[list] {
            match nugget {
                Nugget::Particle {
                    template, delay, ..
                } => {
                    let override_delay = delay.map_or(0.0, |range| range[1]);
                    let delay = frame_bound(override_delay)?;
                    let span = system_bound(particles, *template, &mut Vec::new())?;
                    maximum = maximum.max(
                        delay
                            .checked_add(span)
                            .ok_or("collapse FX bound overflow")?,
                    );
                    checked = true;
                }
                Nugget::AtBones { .. } => {
                    return Err(
                        "original positional collapse FX unexpectedly uses bone traversal".into(),
                    );
                }
                _ => {}
            }
        }
    }
    if !checked {
        return Err("original collapse FX has no transient particle systems".into());
    }
    Ok(maximum)
}

fn frame_bound(frames: f32) -> Result<u64, Box<dyn Error>> {
    frames
        .ceil()
        .to_u64()
        .ok_or_else(|| "transient particle timing is not finite nonnegative frames".into())
}

fn system_bound(
    particles: &Particles,
    index: usize,
    path: &mut Vec<usize>,
) -> Result<u64, Box<dyn Error>> {
    if path.contains(&index) {
        return Err("cyclic transient particle dependency".into());
    }
    let rules = &particles.templates[index];
    if rules.system_lifetime <= 0.0 || rules.lifetime.low <= 0.0 {
        return Err(format!("transient {} has an unbounded source lifetime", rules.name).into());
    }
    path.push(index);
    // Delay counts down before emission; finite system lifetime bounds the last
    // possible burst. Every such particle then gets its full sampled lifetime.
    // Two extra updates cover the creation/removal boundary inclusively.
    let mut span = frame_bound(rules.initial_delay.high)?;
    for frames in [rules.system_lifetime, rules.lifetime.high, 2.0] {
        span = span
            .checked_add(frame_bound(frames)?)
            .ok_or("transient particle bound overflow")?;
    }
    let mut descendant = 0;
    for child in [rules.slave, rules.attached].into_iter().flatten() {
        descendant = descendant.max(system_bound(particles, child, path)?);
    }
    path.pop();
    span.checked_add(descendant)
        .ok_or_else(|| "transient particle descendant bound overflow".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{
        Pose,
        effects::rng,
        tests::{system, template},
    };

    #[ignore = "requires original game installation"]
    #[test]
    fn original_collapse_final_wait_outlives_all_transients_and_preserves_post_smoke() {
        use generals_assets::virtual_assets::VirtualAssets;
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let names = COLLAPSE_EFFECTS.into_iter().map(str::to_owned).collect();
        let mut particles =
            crate::particles::load_with_position_effects(&assets, &[], &names).unwrap();
        let mut transient = std::collections::BTreeSet::new();
        let mut nuggets = 0;
        for name in COLLAPSE_EFFECTS {
            for nugget in &particles.lists[particles.actor_fx[name]] {
                if let Nugget::Particle { template, .. } = nugget {
                    transient.insert(*template);
                    nuggets += 1;
                    assert!(particles.templates[*template].system_lifetime > 0.0);
                    assert!(particles.templates[*template].lifetime.low > 0.0);
                }
            }
        }
        assert_eq!(nuggets, 9);
        let shock = particles
            .templates
            .iter()
            .find(|template| template.name == "StructureCollapseFinalShockwave")
            .unwrap();
        assert_eq!(shock.lifetime.high.to_bits(), 400.0_f32.to_bits());
        assert_eq!(shock.system_lifetime.to_bits(), 20.0_f32.to_bits());
        assert_eq!(shock.initial_delay.high.to_bits(), 0.0_f32.to_bits());
        assert_eq!(collapse_bound(&particles).unwrap(), 422);
        let smoke = particles
            .templates
            .iter()
            .position(|template| template.name == "SmokeFactionSmall")
            .unwrap();
        assert!(!transient.contains(&smoke));
        assert_eq!(
            particles.templates[smoke].system_lifetime.to_bits(),
            0.0_f32.to_bits()
        );
        let final_frame = 100;
        particles.begin(1, final_frame);
        let mut random = |_, high| high;
        for name in COLLAPSE_EFFECTS {
            let list = particles.actor_fx[name];
            particles.fire(
                list,
                &Pose::at([100.0; 3]),
                &[],
                &|_| 0.0,
                0.0,
                &mut rng::real_only(|_, high| high),
            );
        }
        particles.create(smoke, Pose::at([100.0; 3]), None, &mut random);
        // Creation is not expiry: even empty/new delayed systems have source time left.
        assert!(!collapse_complete(&particles, 1, final_frame, final_frame).unwrap());
        for now in final_frame..final_frame + 422 {
            particles.begin(1, now);
            particles.logic(now, &mut random, &|_| 0.0);
        }
        assert!(!collapse_complete(&particles, 1, final_frame, final_frame + 421).unwrap());
        assert!(
            !particles
                .emitters
                .iter()
                .any(|emitter| transient.contains(&emitter.template))
        );
        assert!(
            particles
                .emitters
                .iter()
                .any(|emitter| emitter.template == smoke && !emitter.particles.is_empty())
        );
        particles.begin(1, final_frame + 422);
        particles.logic(final_frame + 422, &mut random, &|_| 0.0);
        assert!(collapse_complete(&particles, 1, final_frame, final_frame + 422).unwrap());
        assert!(!collapse_complete(&particles, 2, final_frame, final_frame + 422).unwrap());
        assert!(!collapse_complete(&particles, 1, final_frame + 1, final_frame + 422).unwrap());
    }

    #[test]
    fn impact_flash_waits_for_delayed_emission_but_not_child_smoke() {
        let mut parent = template("explosion");
        parent.drawing.shader = Shader::Additive;
        parent.initial_delay.low = 2.0;
        parent.initial_delay.high = 2.0;
        parent.lifetime.low = 4.0;
        parent.lifetime.high = 4.0;
        parent.attached = Some(1);
        let mut smoke = template("smoke");
        smoke.lifetime.low = 8.0;
        smoke.lifetime.high = 8.0;
        let mut particles = system(vec![parent, smoke]);
        particles.actor_fx.insert("impact".into(), 0);
        particles.lists.push(vec![Nugget::Particle {
            template: 0,
            count: 1,
            radius: None,
            height: None,
            oriented: false,
            ricochet: false,
            attached: false,
            offset: [0.0; 3],
            ground: false,
            caller_radius: false,
            delay: None,
        }]);
        let mut random = |_, high| high;
        particles.create(0, Pose::at([0.0; 3]), None, &mut random);
        assert!(!complete(&particles, "impact", 0).unwrap());
        particles.logic(0, &mut random, &|_| 0.0);
        assert!(!complete(&particles, "impact", 0).unwrap());
        let mut smoke_after_flash = false;
        for frame in 1..30 {
            particles.begin(0, frame);
            particles.logic(frame, &mut random, &|_| 0.0);
            if complete(&particles, "impact", 0).unwrap()
                && particles
                    .emitters
                    .iter()
                    .any(|emitter| emitter.template == 1 && !emitter.particles.is_empty())
            {
                smoke_after_flash = true;
            }
        }
        assert!(smoke_after_flash);
        assert!(complete(&particles, "impact", 0).unwrap());
        assert!(!complete(&particles, "impact", 1).unwrap());
        assert!(complete(&particles, "unloaded", 0).is_err());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_impact_flash_finishes_with_dust_remaining_before_hulk_final_deletion() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let game = &battle.simulation;
        let hulk = game
            .catalog
            .iter()
            .find(|unit| unit.name == "AmericaJetRaptorHulk")
            .unwrap();
        assert_eq!(hulk.behavior.lifetimes, [[45, 48]]);
        let physical = hulk.physical.as_ref().unwrap();
        let generals_sim::skirmish::physical_ocl::Death::Slow(death) = &physical.death else {
            panic!("hulk requires SlowDeath")
        };
        assert_eq!(death.sink_delay, [45, 0]);
        assert_eq!(death.destruction_delay, [240, 0]);
        let jet = game
            .catalog
            .iter()
            .find(|unit| unit.name == "AmericaJetRaptor")
            .unwrap()
            .jet_death_rules()
            .next()
            .unwrap();
        assert_eq!(jet.final_delay, 6);
        let mut particles = crate::particles::load(&assets, &game.catalog).unwrap();
        let list = particles.actor_fx["FX_JetDeathHitGround"];
        particles.begin(1, 0);
        particles.fire(
            list,
            &Pose::at([100.0, 100.0, 100.0]),
            &[],
            &|_| 0.0,
            0.0,
            &mut rng::real_only(|low, _| low),
        );
        let mut finished = None;
        for frame in 0..u64::from(death.destruction_delay[0]) {
            particles.begin(1, frame);
            particles.logic(frame, &mut |low, _| low, &|_| 0.0);
            if complete(&particles, "FX_JetDeathHitGround", 1).unwrap() {
                assert!(particles.emitters.iter().any(|emitter| {
                    particles.templates[emitter.template].name == "JetExplosionGroundDust"
                        && !emitter.particles.is_empty()
                }));
                finished = Some(frame);
                break;
            }
        }
        let frame = finished.expect("native flash must end before original hulk deletion");
        assert!(frame >= 60);
        println!(
            "Original impact flash retired at {frame} ticks; native dust remains; hulk sinking stays enabled (45-frame delay), final deletion is 240 frames after its death"
        );
    }
}
