//! Quarter views of one paused original body; simulation and effects stay intact.
use super::{Check, Stage, View, physical};
use crate::{playtest::Match, playtest_view::Capture, skirmish_verification::require};
use bevy::prelude::*;
use std::{error::Error, time::Duration};

pub(super) struct Snapshot {
    pub yaw: f32,
    body: usize,
    generation: u32,
    tick: u64,
    milliseconds: u32,
    elapsed: Duration,
    position: [f32; 3],
    basis: [[f32; 3]; 3],
    blocked_view: Option<(u8, f64)>,
}

impl Snapshot {
    pub(super) fn new(
        battle: &Match,
        clock: &crate::model_animation::Clock,
        time: &Time<Virtual>,
        body: usize,
        yaw: f32,
    ) -> Self {
        let (position, basis) = battle.simulation.object_transform(body);
        Self {
            yaw,
            body,
            generation: battle.generation,
            tick: battle.simulation.tick,
            milliseconds: clock.milliseconds(),
            elapsed: time.elapsed(),
            position,
            basis,
            blocked_view: None,
        }
    }

    pub(super) fn unchanged(
        &self,
        battle: &Match,
        clock: &crate::model_animation::Clock,
        time: &Time<Virtual>,
    ) -> Result<(), Box<dyn Error>> {
        let (position, basis) = battle.simulation.object_transform(self.body);
        require(
            time.is_paused()
                && battle.generation == self.generation
                && battle.simulation.tick == self.tick
                && clock.milliseconds() == self.milliseconds
                && time.elapsed() == self.elapsed
                && position.map(f32::to_bits) == self.position.map(f32::to_bits)
                && basis.map(|row| row.map(f32::to_bits))
                    == self.basis.map(|row| row.map(f32::to_bits)),
            "aircraft quarter view changed the original paused body snapshot",
        )
    }
}

impl Check {
    pub(super) fn freeze(
        &mut self,
        battle: &Match,
        view: &View,
        time: &Time<Virtual>,
        body: usize,
    ) {
        self.view_index = 0;
        self.snapshot = Some(Snapshot::new(
            battle,
            &view.animation_clock,
            time,
            body,
            view.camera.settings.yaw,
        ));
    }

    pub(super) fn next_view(&mut self, time: &mut Time<Virtual>) {
        if matches!(self.stage, Stage::FallingCaptured) && self.view_index == 3 {
            self.stage = Stage::Ground;
            self.snapshot = None;
            time.unpause();
        } else {
            self.view_index += 1;
            self.stage = if matches!(self.stage, Stage::FallingCaptured) {
                Stage::FallingView
            } else {
                Stage::HulkView
            };
        }
    }

    pub(super) fn log_view(
        &self,
        battle: &Match,
        view: &View,
        time: &Time<Virtual>,
    ) -> Result<(), Box<dyn Error>> {
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or("missing paused aircraft snapshot")?;
        snapshot.unchanged(battle, &view.animation_clock, time)?;
        let actor = &battle.simulation.entities[snapshot.body];
        println!(
            "Original aircraft paused view: quarter={} yaw={} tick={} virtual_ms={} animation_ms={} actor={} producer={:?} template={} physical_model={:?} position={:?} basis={:?}; {}",
            self.view_index,
            view.camera.settings.yaw,
            snapshot.tick,
            snapshot.elapsed.as_millis(),
            snapshot.milliseconds,
            snapshot.body,
            actor.producer(),
            battle.simulation.catalog[actor.type_index].name,
            actor.physical_model(),
            snapshot.position,
            snapshot.basis,
            view.framing.visibility_diagnostic(battle, snapshot.body)?
        );
        if snapshot.body == self.setup.aircraft.raptor {
            for (id, actor) in battle.simulation.entities.iter().enumerate() {
                if actor.producer() == Some(snapshot.body)
                    && actor.physical_model() == Some("AVRaptor_D4")
                {
                    println!(
                        "Original aircraft separate D4: {}",
                        view.framing.visibility_diagnostic(battle, id)?
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn capture_view(
        &mut self,
        battle: &Match,
        view: &View,
        time: &Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<(), Box<dyn Error>> {
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or("missing paused aircraft snapshot")?;
        snapshot.unchanged(battle, &view.animation_clock, time)?;
        let body_visible = view.framing.visible(battle, snapshot.body)?;
        let body_pose = view.framing.physical_pose(battle, snapshot.body)?;
        let debris_ready = !matches!(self.stage, Stage::FallingView)
            || physical(
                battle,
                snapshot.body,
                Some("AVRaptor_D4"),
                "GenericDebris",
                &view.framing,
            )?;
        if !body_visible || !body_pose || !debris_ready {
            let now = view.clock.elapsed_secs_f64();
            let blocked = &mut self
                .snapshot
                .as_mut()
                .expect("paused snapshot")
                .blocked_view;
            let report = match blocked {
                Some((quarter, began)) if *quarter == self.view_index => {
                    if now - *began >= 2.0 {
                        *began = f64::INFINITY;
                        true
                    } else {
                        false
                    }
                }
                _ => {
                    *blocked = Some((self.view_index, now));
                    false
                }
            };
            if report {
                println!(
                    "Aircraft paused view waiting: quarter={} body_visible={body_visible} body_pose={body_pose} debris_ready={debris_ready}",
                    self.view_index
                );
                self.log_view(battle, view, time)?;
            }
            return Ok(());
        }
        self.log_view(battle, view, time)?;
        if matches!(self.stage, Stage::FallingView) {
            let filename = match self.view_index {
                1 => "aircraft-crash-falling-view-1.png",
                2 => "aircraft-crash-falling-view-2.png",
                3 => "aircraft-crash-falling-view-3.png",
                _ => return Err("invalid falling aircraft quarter view".into()),
            };
            self.stage = Stage::FallingCaptured;
            capture.intermediate(filename);
        } else if self.view_index == 3 {
            self.stage = Stage::Done;
            capture.finish_scenario();
        } else {
            let filename = match self.view_index {
                1 => "aircraft-crash-hulk-view-1.png",
                2 => "aircraft-crash-hulk-view-2.png",
                _ => return Err("invalid hulk quarter view".into()),
            };
            self.stage = Stage::HulkCaptured;
            capture.intermediate(filename);
        }
        Ok(())
    }
}
