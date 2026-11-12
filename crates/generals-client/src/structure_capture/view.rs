use super::{Check, require};
use crate::{
    capture_framing::AircraftFrame,
    particles::ParticleInspection,
    playtest::Match,
    playtest_view::{Capture, UnitVisual},
    source_audio::FiringAudioInspection,
    structure_view::Structures,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(SystemParam)]
pub(super) struct View<'w, 's> {
    battle: Res<'w, Match>,
    framing: AircraftFrame<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    audio: FiringAudioInspection<'w, 's>,
    structures: Res<'w, Structures>,
    roots: Query<'w, 's, (&'static UnitVisual, &'static GlobalTransform)>,
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
}

pub(super) fn inspect(mut check: ResMut<Check>, mut view: View) {
    if !matches!(check.phase, 1 | 3) || !view.capture.ready_for_scenario() {
        return;
    }
    let names = if check.setup.topple {
        [
            "BuildingToppleDelayStone",
            "BuildingFallingStone",
            "BuildingDestroyStone",
        ]
    } else {
        ["BuildingDamage", "BuildingCollapse2", "BuildingCollapse3"]
    };
    // Playback progression is machine evidence, not a claim of physical audibility.
    check.audio |= view.audio.ambient(names).into_iter().any(|playing| playing);
    let result = inspect_scene(
        &mut check,
        &view.battle,
        &view.framing,
        &view.particles,
        &view.structures,
        &view.roots,
    );
    match result {
        Ok(true) if check.phase == 1 => {
            view.time.pause();
            view.capture.intermediate(if check.setup.topple {
                "structure-topple-falling.png"
            } else {
                "structure-collapse-falling.png"
            });
            check.phase = 2;
        }
        Ok(true) => {
            view.time.pause();
            view.capture.finish_scenario();
            check.phase = 255;
            println!(
                "Original paid structure capture passed geometry/effect/playback gates: topple={}, authored target={}, retained POST_COLLAPSE; final pixels and physical audio require review",
                check.setup.topple, check.setup.target
            );
        }
        Ok(false) => {}
        Err(error) => {
            let effect = check.effect.as_ref().map_or_else(
                || "no PositionFx event recorded".into(),
                |(name, frame)| {
                    view.particles
                        .actor_fx_diagnostic(name, *frame, view.battle.generation)
                        .unwrap_or_else(|error| format!("effect diagnostic failed: {error}"))
                },
            );
            error!(
                "Structure capture phase {}: {error}; tick={} dead={:?} falling_frames={} visible_falling_frames={} effect_falling_frames={} particles={} bone={} audio={} latest_effect={:?}; offset={:?} source_height={} falling_offset_interval=({},{}); {effect}; {}",
                check.phase,
                view.battle.simulation.tick,
                check.dead_frame,
                check.falling_frames,
                check.visible_falling_frames,
                check.effect_falling_frames,
                check.particles,
                check.bone,
                check.audio,
                check.effect,
                view.structures
                    .offset(view.battle.generation, check.setup.target),
                check.setup.height,
                -check.setup.height * 0.8,
                -check.setup.height * 0.2,
                view.framing
                    .visibility_diagnostic(&view.battle, check.setup.target)
                    .unwrap_or_else(|error| format!("visibility diagnostic failed: {error}"))
            );
            check.phase = 255;
            view.exit.write(AppExit::error());
        }
    }
}

fn inspect_scene(
    check: &mut Check,
    battle: &Match,
    framing: &AircraftFrame,
    particles: &ParticleInspection,
    structures: &Structures,
    roots: &Query<(&UnitVisual, &GlobalTransform)>,
) -> Result<bool, Box<dyn Error>> {
    let game = &battle.simulation;
    let target = check.setup.target;
    let actor = &game.entities[target];
    require(
        actor.exists(),
        "authored structure was removed instead of retained",
    )?;
    require(
        game.object_position(target).map(f32::to_bits) == check.setup.position.map(f32::to_bits),
        "structure collapse/topple moved the original collision position",
    )?;
    if let Some((name, frame)) = &check.effect {
        check.particles |= particles.actor_fx(name, *frame, battle.generation)?;
    }
    let smoke = if check.setup.topple {
        "StructureToppleSmokeTrail"
    } else {
        "StructureTransitionMediumSmoke"
    };
    check.bone |= particles.object_attached(target, smoke, battle.generation)?;
    if check.dead_frame.is_none() {
        return Ok(false);
    }
    require(
        check
            .attacker
            .is_some_and(|id| check.setup.tanks.contains(&id)),
        "structure death did not come from a paid capture tank",
    )?;
    let offset = structures.offset(battle.generation, target);
    let falling = if check.setup.topple {
        actor
            .structure_basis()
            .is_some_and(|basis| basis[2][2] < 0.95 && basis[2][2] > 0.2)
    } else {
        offset[2] < -check.setup.height * 0.2 && offset[2] > -check.setup.height * 0.8
    };
    let visible = framing.visible(battle, target)?;
    if check.phase == 1 && falling {
        check.falling_frames += 1;
        check.visible_falling_frames += u64::from(visible);
        check.effect_falling_frames += u64::from(check.particles);
        if check.falling_frames == 1 {
            let effect = check.effect.as_ref().map_or_else(
                || "no PositionFx event recorded".into(),
                |(name, frame)| {
                    particles
                        .actor_fx_diagnostic(name, *frame, battle.generation)
                        .unwrap_or_else(|error| format!("effect diagnostic failed: {error}"))
                },
            );
            println!(
                "Structure capture first falling pose: tick={} offset={offset:?} source_height={} visible={visible} particles={} latest_effect={:?}; {effect}; {}",
                game.tick,
                check.setup.height,
                check.particles,
                check.effect,
                framing
                    .visibility_diagnostic(battle, target)
                    .unwrap_or_else(|error| format!("visibility diagnostic failed: {error}"))
            );
        }
    }
    if !visible {
        return Ok(false);
    }
    if check.phase == 3 {
        return post_ready(check, battle, particles);
    }
    if actor.post_collapse() {
        return Err("structure completed without a captured visible falling pose".into());
    }
    if check.setup.topple {
        let tilted = actor
            .structure_basis()
            .is_some_and(|basis| basis[2][2] < 0.95 && basis[2][2] > 0.2);
        Ok(tilted && framing.physical_pose(battle, target)? && check.particles)
    } else {
        let Some((_, pose)) = roots
            .iter()
            .find(|(visual, _)| visual.index == target && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        let height = (check.setup.position[2] + offset[2]) * 0.1;
        require(
            (pose.translation().y - height).abs() < 0.000_1,
            "collapse drawable does not contain the original vertical translation",
        )?;
        Ok(offset[2] < -check.setup.height * 0.2
            && offset[2] > -check.setup.height * 0.8
            && check.particles)
    }
}

fn post_ready(
    check: &Check,
    battle: &Match,
    particles: &ParticleInspection,
) -> Result<bool, Box<dyn Error>> {
    let game = &battle.simulation;
    if !game.entities[check.setup.target].post_collapse()
        || !check.audio
        || !check.particles
        || !check.bone
    {
        return Ok(false);
    }
    if check.setup.topple {
        return Ok(true);
    }
    let Some((name, frame)) = &check.effect else {
        return Ok(false);
    };
    if name != "FX_StructureCollapseFinal" {
        return Ok(false);
    }
    // Final is a real ordered callback. Give all finite original transient
    // systems and particles their conservative source lifetime in ordinary ticks.
    particles.collapse_dust_finished(battle.generation, *frame, game.tick)
}
