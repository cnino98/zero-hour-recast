//! Asset/runtime checks, separate from an original-engine numeric comparison.
use super::{Particles, Pose, data};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::UnitType;
use std::error::Error;

pub fn verify(assets: &VirtualAssets, types: &[UnitType]) -> Result<(), Box<dyn Error>> {
    verify_runtime(data::load(assets, types)?)
}

fn verify_runtime(mut particles: Particles) -> Result<(), Box<dyn Error>> {
    let mut random = |low: f32, high: f32| {
        if low >= high {
            high
        } else {
            let width = high - low;
            let offset = width * 0.75;
            low + offset
        }
    };
    let names: Vec<_> = particles
        .templates
        .iter()
        .map(|template| template.name.clone())
        .collect();
    let mut created = vec![false; names.len()];
    for index in 0..names.len() {
        particles.create(index, Pose::at([100.0, 200.0, 30.0]), None, &mut random);
    }
    // Every authored system gets the same observation window after its actual
    // constructor delay, including the infantry fire sequence's delayed bursts.
    let mut delay = particles
        .emitters
        .iter()
        .map(|emitter| emitter.delay)
        .fold(0.0_f32, f32::max);
    let mut observation = 32;
    let mut frame = 0;
    while observation > 0 {
        particles.logic(frame, &mut random, &|_| 0.0);
        particles.draw(1.0);
        observe_particles(&particles, &mut created)?;
        frame += 1;
        if delay > 0.0 {
            delay -= 1.0;
        } else {
            observation -= 1;
        }
    }
    for (index, emitted) in created.into_iter().enumerate() {
        let rules = &particles.templates[index];
        let expected = rules.burst_count.high >= 1.0
            || particles
                .templates
                .iter()
                .any(|template| template.slave == Some(index));
        if emitted != expected {
            return Err(format!(
                "{} emission mismatch: expected={expected}, emitted={emitted}",
                names[index]
            )
            .into());
        }
    }
    for emitter in &mut particles.emitters {
        emitter.lifecycle.destroyed = true;
    }
    // Destruction retains existing particles. SCUD/Tomahawk trails live 250
    // frames, beyond the earlier rifle/missile fixture's fixed cutoff. Include
    // each current delay plus actual remaining lifetime and one retirement pass
    // per possible slave ancestor, without changing authored runtime lifetimes.
    let remaining = particles
        .emitters
        .iter()
        .map(|emitter| {
            let lifetime = emitter
                .particles
                .iter()
                .map(|particle| particle.remaining)
                .fold(0.0_f32, f32::max);
            f64::from(emitter.delay) + f64::from(lifetime)
        })
        .fold(0.0_f64, f64::max);
    let mut budget = particles
        .emitters
        .iter()
        .fold(remaining, |frames, _| frames + 1.0);
    while budget > 0.0 && !particles.emitters.is_empty() {
        particles.logic(frame, &mut random, &|_| 0.0);
        particles.draw(1.0);
        frame += 1;
        budget -= 1.0;
    }
    let indefinite = retained_indefinite(&particles)?;
    println!(
        "Particle assets: {} authored templates loaded, checked nonzero/zero bursts, finite particles and destruction drain; retained {indefinite} authored indefinite particles",
        names.len(),
    );
    Ok(())
}

fn observe_particles(particles: &Particles, created: &mut [bool]) -> Result<(), Box<dyn Error>> {
    for emitter in &particles.emitters {
        created[emitter.template] |= !emitter.particles.is_empty();
        for particle in &emitter.particles {
            if !particle
                .position
                .iter()
                .chain(&particle.color)
                .all(|value| value.is_finite())
                || !particle.size.is_finite()
            {
                return Err(format!(
                    "{} produced nonfinite particle",
                    particles.templates[emitter.template].name,
                )
                .into());
            }
        }
    }
    Ok(())
}

fn retained_indefinite(particles: &Particles) -> Result<usize, Box<dyn Error>> {
    let mut count = 0;
    for emitter in &particles.emitters {
        // Destroy stops emission; a zero-lifetime particle can retain its system.
        // Native finite slave systems also hold their empty master until retirement.
        let pending_slave = emitter.slave.is_some_and(|id| {
            particles.emitters.iter().any(|slave| {
                slave.id == id && particles.templates[slave.template].system_lifetime != 0.0
            })
        });
        if emitter
            .particles
            .iter()
            .any(|particle| particle.remaining != 0.0)
            || (emitter.particles.is_empty() && !pending_slave)
        {
            return Err(format!(
                "destroyed authored effect did not drain: {}",
                particles.templates[emitter.template].name
            )
            .into());
        }
        count += emitter.particles.len();
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_verification_observes_original_delays_beyond_the_initial_window() {
        let mut delayed = crate::particles::tests::template("FireInfantrySmallSub2 timing");
        delayed.initial_delay.low = 37.0;
        delayed.initial_delay.high = 37.0;
        let particles = crate::particles::tests::system(vec![delayed]);
        super::verify_runtime(particles).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_catalog_particle_dependencies_preserve_authored_runtime() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let map = generals_assets::map::MapData::parse(
            &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
        )
        .unwrap();
        let data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
        super::verify(&assets, &data.types).unwrap();
    }
}
