//! `W3DDebrisDraw::doDrawModule` and `Animatable3DObjClass`, SH 2bfaea60.
use super::{Animations, Clock, Draw, Models};
use crate::{
    model_animation::AnimatedPart, model_render, playtest::Match, playtest_view::UnitVisual,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_assets::w3d::{MeshPose, animation::Clip};
use generals_sim::skirmish::physical_ocl::DebrisDraw;
use num_traits::ToPrimitive;

pub(super) struct Playback {
    phase: usize,
    frames: u32,
    playing: Option<usize>,
    frame: f32,
    milliseconds: u32,
}

impl Playback {
    pub(super) const fn new() -> Self {
        Self {
            phase: 0,
            frames: 0,
            playing: None,
            frame: 0.0,
            milliseconds: 0,
        }
    }

    fn draw<'a>(
        &mut self,
        setup: &DebrisDraw,
        models: &'a Models,
        above: bool,
        now: u32,
    ) -> Option<(&'a Clip, f32, bool)> {
        let names = setup.animations.as_ref()?;
        let final_stop = names[2].eq_ignore_ascii_case("STOP");
        let clip_at = |phase| {
            models
                .1
                .get(&names[if phase == 2 && final_stop { 1 } else { phase }])
        };
        // The no-animation BASE_POSE mode is not animation-complete. Only a
        // SINGLE_ANIM playing ONCE can advance INITIAL to FLYING.
        let complete = self.playing.is_some_and(|phase| {
            phase != 1
                && !(phase == 2 && final_stop)
                && clip_at(phase).is_some_and(|clip| {
                    self.frame.to_bits() == f32::from(clip.frame_count - 1).to_bits()
                })
        });
        let old = self.phase;
        if self.phase != 2 && !above && self.frames > 3 {
            self.phase = 2;
        } else if self.phase < 2 && complete {
            self.phase += 1;
        }
        let selected = clip_at(self.phase);
        let final_fx =
            if selected.is_some() && (self.playing != Some(self.phase) || old != self.phase) {
                self.playing = Some(self.phase);
                self.frame = 0.0;
                self.milliseconds = now;
                self.phase == 2
            } else {
                false
            };
        self.frames = self.frames.wrapping_add(1);
        let playing = self.playing?;
        let clip = clip_at(playing)?;
        if !(playing == 2 && final_stop) {
            let elapsed = now
                .wrapping_sub(self.milliseconds)
                .to_f32()
                .expect("native milliseconds");
            let advance = f32::from(clip.frame_rate) * elapsed * 0.001;
            self.frame += advance;
            let end = f32::from(clip.frame_count - 1);
            if playing == 1 {
                // Native LOOP subtracts the last-frame index once, then resets
                // after a second whole interval; it does not use modulo.
                if self.frame >= end {
                    self.frame -= end;
                    if self.frame >= end {
                        self.frame = 0.0;
                    }
                }
            } else if self.frame >= end {
                self.frame = end;
            }
        }
        self.milliseconds = now;
        Some((clip, self.frame, final_fx))
    }
}

#[derive(SystemParam)]
pub struct Resources<'w> {
    models: Res<'w, Models>,
    clock: Res<'w, Clock>,
    animations: ResMut<'w, Animations>,
    particles: ResMut<'w, crate::particles::Particles>,
    audio: ResMut<'w, crate::source_audio::SourceAudio>,
    meshes: ResMut<'w, Assets<Mesh>>,
}

type Part = (
    &'static AnimatedPart,
    &'static Mesh3d,
    &'static ChildOf,
    &'static mut Visibility,
    Option<&'static mut crate::sorted_view::submission::ModelSubmission>,
);

pub fn update(
    battle: Res<Match>,
    mut resources: Resources,
    mut draws: Query<(Entity, &UnitVisual, &mut Draw)>,
    mut parts: Query<Part>,
) {
    let battle = battle.into_inner();
    let now = resources.clock.milliseconds();
    let mut poses = std::collections::BTreeMap::new();
    for (entity, visual, mut draw) in &mut draws {
        if visual.generation != battle.generation
            || !crate::playtest_view::drawable_present(&battle.simulation, visual.index)
            || !matches!(
                battle
                    .simulation
                    .visibility_status(battle.observer_player.unwrap_or(0), visual.index),
                generals_sim::skirmish::visibility::Status::Clear
                    | generals_sim::skirmish::visibility::Status::Partial
            )
        {
            continue;
        }
        let actor = &battle.simulation.entities[visual.index];
        let Some(setup) = actor.debris_draw() else {
            continue;
        };
        let Some((clip, frame, final_fx)) = draw.1.draw(
            setup,
            &resources.models,
            battle.simulation.height_above_ground(visual.index) > 0.0,
            now,
        ) else {
            continue;
        };
        let model = resources.models.0[&setup.model.to_ascii_lowercase()];
        match resources.animations.debris_pose(model, Some(clip), frame) {
            Ok(pose) => {
                poses.insert(entity, pose);
            }
            Err(error) => error!("Original debris animation: {error}"),
        }
        if final_fx {
            // Attached Drawable::getTransformMatrix returns Object's matrix;
            // the mesh's incoming draw transform alone includes m_instance.
            let (position, basis) = battle.simulation.object_transform(visual.index);
            resources.particles.debris_final_fx(
                battle,
                &setup.final_fx,
                position,
                basis,
                &mut resources.animations,
            );
        }
    }
    resources
        .particles
        .flush_effect_sounds(&mut resources.audio);
    apply_poses(&poses, &mut parts, &mut resources.meshes);
}

fn apply_poses(
    poses: &std::collections::BTreeMap<Entity, Vec<MeshPose>>,
    parts: &mut Query<Part>,
    meshes: &mut Assets<Mesh>,
) {
    for (part, mesh, parent, mut visibility, submission) in parts {
        let Some(pose) = poses.get(&parent.parent()) else {
            continue;
        };
        if let Some(mut submission) = submission {
            submission.set_pose(&pose[part.binding.mesh]);
        }
        let Some(mut mesh) = meshes.get_mut(&mesh.0) else {
            continue;
        };
        match model_render::deform(&mut mesh, &part.binding, pose) {
            Ok(visible) => {
                visibility.set_if_neq(if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            Err(error) => error!("Original debris mesh binding: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> (Models, Animations, DebrisDraw) {
        let (assets, _) = crate::cleanup_verification::tests::installed();
        let (_, mut unit, _) =
            crate::skirmish_data::physical_ocl::tests::installed_outpost(&assets);
        let list = crate::skirmish_data::physical_ocl::tests::technical_list(&assets);
        let nugget = &list.nuggets[0];
        let setup = DebrisDraw {
            model: nugget.names[0].clone(),
            animations: Some(nugget.draw.animations[0].clone()),
            final_fx: nugget.draw.final_fx.clone(),
            color: None,
        };
        unit.death_lifecycle.as_mut().unwrap().slow[0].objects[0].push(
            generals_sim::skirmish::death_lifecycle::Spawn::Physical(list),
        );
        let mut models = Vec::new();
        let units = vec![unit];
        let prepared = super::super::load(&assets, &units, &mut models).unwrap();
        let mut animations = Animations::effect_fixture(0);
        animations.retain_models(models, 0);
        (prepared, animations, setup)
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_debris_plays_initial_then_flying_and_ground_final_only_once() {
        let (models, animations, setup) = installed();
        let mut playback = Playback::new();
        let (initial, frame, fx) = playback.draw(&setup, &models, true, 100).unwrap();
        assert!(initial.name.ends_with("DTA1"));
        assert_eq!(frame.to_bits(), 0.0_f32.to_bits());
        assert!(!fx);
        let model = models.0["uitech_man_skn"];
        let first = animations.debris_pose(model, Some(initial), frame).unwrap();
        let (_, final_initial_frame, _) = playback.draw(&setup, &models, true, 10_000).unwrap();
        assert_eq!(
            final_initial_frame.to_bits(),
            f32::from(initial.frame_count - 1).to_bits()
        );
        let (flying, _, fx) = playback.draw(&setup, &models, true, 10_001).unwrap();
        assert!(flying.name.ends_with("DTA2"));
        assert!(!fx);
        playback.draw(&setup, &models, true, 10_002).unwrap();
        let (landing, _, fx) = playback.draw(&setup, &models, false, 10_003).unwrap();
        assert!(landing.name.ends_with("DTA3"));
        assert!(fx);
        let last = animations.debris_pose(model, Some(landing), 0.0).unwrap();
        assert!(first.iter().zip(last).any(|(left, right)| {
            left.positions
                .iter()
                .flatten()
                .map(|value| value.to_bits())
                .ne(right
                    .positions
                    .iter()
                    .flatten()
                    .map(|value| value.to_bits()))
        }));
        assert!(!playback.draw(&setup, &models, false, 10_004).unwrap().2);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn early_ground_waits_four_draws_and_stop_freezes_flying_clip_at_frame_zero() {
        let (models, _, mut setup) = installed();
        setup.animations.as_mut().unwrap()[2] = "STOP".into();
        let mut playback = Playback::new();
        for now in 0..4 {
            assert!(!playback.draw(&setup, &models, false, now).unwrap().2);
            assert_eq!(playback.phase, 0);
        }
        let (clip, frame, fx) = playback.draw(&setup, &models, false, 4).unwrap();
        assert!(clip.name.ends_with("DTA2"));
        assert_eq!(frame.to_bits(), 0.0_f32.to_bits());
        assert!(fx);
        let (_, frame, fx) = playback.draw(&setup, &models, false, 100_000).unwrap();
        assert_eq!(frame.to_bits(), 0.0_f32.to_bits());
        assert!(!fx);
    }
}
