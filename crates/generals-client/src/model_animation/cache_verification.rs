//! Installed-content differential checks for pure geometry reuse.
#[cfg(test)]
mod part_upload_tests;
use super::{Animations, Player, turret};
use crate::render_performance::Digest;
use generals_assets::w3d::MeshPose;
use generals_sim::skirmish::Entity;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn verify_pose_reuse(&self, unit: &mut Entity) -> Result<(), Box<dyn Error>> {
        let mut checked = 0;
        let mut attached_modules = 0;
        for (type_index, draws) in self.draws.iter().enumerate() {
            unit.type_index = type_index;
            for (module, draw) in draws.iter().enumerate() {
                attached_modules += usize::from(draw.attach_bone.is_some());
                let mut player = Player::new([0.0; 2], 0);
                let mut tick = 0;
                for (state_index, state) in draw.states.iter().enumerate() {
                    if state.model.is_none() {
                        continue;
                    }
                    for choice in 0..state.motions.len().max(1) {
                        player.state = state_index;
                        player.choice = choice;
                        player.frame = 0.0;
                        unit.ai.turret.angle = 0.0;
                        unit.ai.turret.pitch = 0.0;
                        let mut previous = None;
                        for stage in 0..8 {
                            match stage {
                                2 => player.frame = 0.5,
                                3 => unit.ai.turret.angle = 0.35,
                                4 => unit.ai.turret.pitch = 0.15,
                                5 => unit.owner ^= 1,
                                6 => player.model = None,
                                _ => {}
                            }
                            let fired = (stage == 7).then_some((0, 0));
                            if stage != 1 {
                                tick += 1;
                            }
                            let pose = self.pose(&mut player, unit, module, tick, fired);
                            if stage == 1 && pose.is_some() {
                                return Err("unchanged rendered sample reevaluated geometry".into());
                            }
                            if matches!(stage, 5 | 6) && pose.is_none() {
                                return Err("new child set lost its initial posed geometry".into());
                            }
                            if let Some(pose) = pose {
                                previous = Some(pose);
                            }
                            let expected = self.uncached_pose(&player, unit, module)?;
                            if previous.as_ref().map(|pose| digest(pose)) != Some(digest(&expected))
                            {
                                return Err(format!("cached pose mismatch: type {type_index}, module {module}, state {state_index}, choice {choice}, stage {stage}").into());
                            }
                            player.model = state.render_model;
                            player.owner = Some(unit.owner);
                            checked += 1;
                        }
                        // Recoil/flash settling remains active after a shot, even
                        // with no animation-frame or turret-angle changes.
                        for _ in 0..12 {
                            tick += 1;
                            if let Some(pose) = self.pose(&mut player, unit, module, tick, None) {
                                previous = Some(pose);
                            }
                            let expected = self.uncached_pose(&player, unit, module)?;
                            if previous.as_ref().map(|pose| digest(pose)) != Some(digest(&expected))
                            {
                                return Err("cached recoil/flash geometry failed to settle".into());
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
        println!(
            "Original draw reuse: {checked} exact-bit state/clip/frame/turret/recoil/owner comparisons passed, including {attached_modules} pristine-bone attached modules"
        );
        Ok(())
    }

    pub fn verify_world_pose_reuse(&self) -> Result<(), Box<dyn Error>> {
        let mut checked = 0;
        for draw in self.world_draws.values() {
            let mut player = Player::new([0.0; 2], 0);
            for (index, state) in draw.states.iter().enumerate() {
                let Some(model) = state.model else { continue };
                for (choice, motion) in state.motions.iter().enumerate() {
                    player.state = index;
                    player.choice = choice;
                    let mut previous = None;
                    for frame in [0.0, 0.0, 0.5, 1.0] {
                        player.frame = frame;
                        if let Some(pose) =
                            super::world_pose(&mut player, state, &self.models, &self.clips)
                        {
                            previous = Some(pose);
                        }
                        let expected = self.models[model].pose(&self.clips[&motion.name], frame)?;
                        if previous.as_ref().map(|pose| digest(pose)) != Some(digest(&expected)) {
                            return Err(
                                "world geometry reuse changed original supply/ambient pose".into(),
                            );
                        }
                        player.model = Some(model);
                        checked += 1;
                    }
                }
            }
        }
        println!(
            "Original world draw reuse: {checked} exact-bit supply/ambient comparisons passed"
        );
        Ok(())
    }

    fn uncached_pose(
        &self,
        player: &Player,
        unit: &Entity,
        module: usize,
    ) -> Result<Vec<MeshPose>, generals_assets::w3d::Error> {
        let draw = &self.draws[unit.type_index][module];
        let state = &draw.states[player.state];
        let model = &self.models[state.model.expect("checked model")];
        let clip = state
            .motions
            .get(player.choice)
            .and_then(|motion| self.clips.get(&motion.name));
        let mut controls = turret::controls(
            &state.fields,
            unit.ai.turret.angle,
            unit.ai.turret.pitch,
            unit.ai.secondary_turret.as_ref(),
        );
        player.muzzle.controls(&mut controls);
        let mut pose = model.controlled_pose(clip, player.frame, &controls)?;
        super::render_objects::hide_subobjects(model, &mut pose);
        player.muzzle.apply(&mut pose);
        let clips = std::array::from_fn(|slot| {
            let maximum = self
                .weapon_clips
                .get(unit.type_index)
                .and_then(|clips| clips.get(unit.weapon_index(slot)))
                .copied()
                .unwrap_or(0);
            (unit.weapon_rounds_remaining(slot), maximum)
        });
        for (name, visible) in
            super::muzzle::projectile_visibility(&state.fields, clips, draw.recoil.projectile_slots)
        {
            for (mesh, posed) in model.meshes.iter().zip(&mut pose) {
                if mesh
                    .name
                    .rsplit('.')
                    .next()
                    .is_some_and(|short| short.eq_ignore_ascii_case(&name))
                {
                    posed.visible = visible;
                }
            }
        }
        if let Some(bone) = &draw.attach_bone {
            // W3DModelDraw's enabled CACHE_ATTACH_BONE branch adds the pristine
            // translation after posing. Re-resolve its original model input here
            // instead of copying the runtime's cached attach_offset.
            let offset = self.draws[unit.type_index][..module]
                .iter()
                .find_map(|prior| {
                    self.models
                        .get(prior.states[prior.select(&BTreeSet::new())].model?)
                        .and_then(|model| model.bone_position(bone))
                })
                .unwrap_or([0.0; 3]);
            for mesh in &mut pose {
                for point in &mut mesh.positions {
                    for (coordinate, offset) in point.iter_mut().zip(offset) {
                        *coordinate += offset;
                    }
                }
            }
        }
        Ok(pose)
    }
}

fn digest(poses: &[MeshPose]) -> u64 {
    let mut digest = Digest::new();
    for pose in poses {
        digest.word(u32::from(pose.visible));
        for point in pose.positions.iter().chain(pose.normals.iter().flatten()) {
            for value in point {
                digest.word(value.to_bits());
            }
        }
    }
    digest.value()
}
