//! Original emitter data and the actual renderer geometry at repeatable populations.
use super::{Particles, Pose, render};
use crate::render_performance::Digest;
use bevy::math::Affine3A;
use std::{
    error::Error,
    time::{Duration, Instant},
};

impl Particles {
    pub fn benchmark(&mut self, count: usize, mixed: bool) -> Result<(), Box<dyn Error>> {
        self.emitters.clear();
        self.next_id = 0;
        let mut random = |low: f32, high: f32| {
            let offset = (high - low) * 0.375;
            low + offset
        };
        let names = [
            "GenericLaserFlare",
            "FlashBangNormalSpark",
            "MissileDefenderMissileExhaust",
            "MissileBackblastClouds",
        ];
        let templates: Vec<_> = names
            .iter()
            .map(|name| {
                self.templates
                    .iter()
                    .position(|template| template.name == *name)
                    .ok_or("particle benchmark original template missing")
            })
            .collect::<Result<_, _>>()?;
        for index in 0..count {
            let template = templates[if mixed {
                (index / 16) % templates.len()
            } else {
                0
            }];
            self.create(template, Pose::at([100.0, 200.0, 30.0]), None, &mut random);
        }
        let mut elapsed = Duration::ZERO;
        let mut vertices = 0usize;
        let mut meshes = 0usize;
        let mut digest = Digest::new();
        let mut durations = Vec::with_capacity(20);
        for frame in 0..20u16 {
            let mut frame_elapsed = Duration::ZERO;
            if frame % 2 == 0 {
                self.logic(u64::from(frame / 2), &mut random, &|_| 0.0);
            }
            self.draw(0.5);
            for emitter in &self.emitters {
                if emitter.particles.is_empty() {
                    continue;
                }
                let start = Instant::now();
                let mesh = render::mesh(
                    emitter,
                    &self.templates[emitter.template],
                    Affine3A::IDENTITY,
                    Affine3A::IDENTITY,
                )?;
                frame_elapsed += start.elapsed();
                vertices += mesh.count_vertices();
                meshes += 1;
                digest.mesh(&mesh, true);
            }
            elapsed += frame_elapsed;
            durations.push(frame_elapsed);
        }
        println!(
            "particles emitters={count} mode={} cpu_ms={:.3} {} meshes={meshes} vertices={vertices} hash={:016x}",
            if mixed { "mixed" } else { "flares" },
            elapsed.as_secs_f64() * 1000.0,
            crate::render_performance::frame_summary(&mut durations),
            digest.value()
        );
        Ok(())
    }
}
