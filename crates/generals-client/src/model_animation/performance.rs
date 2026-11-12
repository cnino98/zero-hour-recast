//! The same source pose/deformation calls used by unit drawables, at fixed populations.
use super::{Animations, Player, clock::FrameTime};
use crate::{model_render, render_performance::Digest};
use generals_sim::skirmish::{Entity, UnitType};
use std::{
    collections::BTreeSet,
    error::Error,
    time::{Duration, Instant},
};

struct Sample {
    type_index: usize,
    player: Player,
    parts: Vec<model_render::Part>,
    visible: Vec<bool>,
}

impl Animations {
    pub fn benchmark(
        &mut self,
        unit: &mut Entity,
        types: &[UnitType],
        count: usize,
        moving: bool,
    ) -> Result<(), Box<dyn Error>> {
        let names = [
            "AmericaInfantryRanger",
            "ChinaInfantryRedguard",
            "GLAInfantryRebel",
            "AmericaTankCrusader",
            "ChinaTankBattleMaster",
            "AmericaPowerPlant",
        ];
        let types: Vec<_> = names
            .iter()
            .map(|name| {
                types
                    .iter()
                    .position(|unit| unit.name == *name)
                    .ok_or("render benchmark model missing")
            })
            .collect::<Result<_, _>>()?;
        let flags: BTreeSet<String> = if moving {
            ["MOVING".into()].into()
        } else {
            BTreeSet::new()
        };
        let mut samples = Vec::with_capacity(count);
        for index in 0..count {
            let type_index = types[index % types.len()];
            let draw = &self.draws[type_index][0];
            let state = draw.select(&flags);
            let model = draw.states[state]
                .model
                .ok_or("render benchmark model absent")?;
            let parts = model_render::parts(&self.models[model])?;
            samples.push(Sample {
                type_index,
                player: Player::new([0.0; 2], 100),
                visible: vec![true; parts.len()],
                parts,
            });
        }
        self.random = super::playback::Random::new(17);
        let mut elapsed = Duration::ZERO;
        let mut evaluated = 0usize;
        let mut vertices = 0usize;
        let mut mutations = 0usize;
        let mut digest = Digest::new();
        let mut durations = Vec::with_capacity(20);
        for frame in 0..20u16 {
            let tick = 100 + u64::from(frame / 2);
            let start = Instant::now();
            for sample in &mut samples {
                unit.type_index = sample.type_index;
                unit.position = [f32::from(frame / 2), 0.0];
                unit.ai.turret.angle = if moving {
                    f32::from(frame / 2) * 0.025
                } else {
                    0.0
                };
                unit.ai.turret.pitch = 0.0;
                self.advance(
                    &mut sample.player,
                    unit,
                    0,
                    &flags,
                    &FrameTime::logic_tick(tick),
                    false,
                );
                let fired = (moving && frame % 6 == 0).then_some((0, 0));
                if let Some(pose) = self.pose(&mut sample.player, unit, 0, tick, fired) {
                    evaluated += 1;
                    for (part, visible) in sample.parts.iter_mut().zip(&mut sample.visible) {
                        let binding = part
                            .binding
                            .as_ref()
                            .ok_or("benchmark part lacks binding")?;
                        *visible = model_render::deform(&mut part.mesh, binding, &pose)?;
                        vertices += binding.vertices.len();
                        mutations += 1;
                    }
                }
                sample.player.model =
                    self.draws[sample.type_index][0].states[sample.player.state].render_model;
                sample.player.owner = Some(unit.owner);
            }
            let duration = start.elapsed();
            elapsed += duration;
            durations.push(duration);
            // Hash all output frames outside the timed region, including unchanged samples.
            for sample in &samples {
                for (part, visible) in sample.parts.iter().zip(&sample.visible) {
                    digest.mesh(&part.mesh, *visible);
                }
            }
        }
        println!(
            "models count={count} mode={} cpu_ms={:.3} {} poses={evaluated} mesh_writes={mutations} vertices={vertices} hash={:016x}",
            if moving { "motion" } else { "idle" },
            elapsed.as_secs_f64() * 1000.0,
            crate::render_performance::frame_summary(&mut durations),
            digest.value()
        );
        Ok(())
    }
}
