//! Original independent drone firing while its paid parent attacks a vehicle.
use super::{Check, Stage, View};
use crate::{
    input::{self, Mouse, PickTarget},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    projectile_render::ProjectileVisual,
    skirmish_verification::require,
    source_audio::{FiringAudioInspection, Voice},
};
use bevy::prelude::*;
use std::error::Error;

pub(super) fn register(app: &mut App) {
    app.add_systems(PreUpdate, (inspect_audio, begin).chain());
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if !matches!(check.stage, Stage::Hellfire) {
        return;
    }
    let events = [
        (check.setup.second, "CrusaderTankVoiceAttack"),
        (check.setup.hellfire, "MissileDefenderWeapon"),
    ];
    for (heard, played) in check.sounds.iter_mut().zip(audio.played(events)) {
        *heard |= played;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    camera: ResMut<'w, CameraRig>,
    time: ResMut<'w, Time<Virtual>>,
    capture: Res<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if matches!(check.stage, Stage::RepairCaptured) && control.capture.ready_for_scenario() {
        // Human Hellfire auto-acquisition excludes unarmed buildings. Preserve
        // the real paid Battlemaster and command only its drone's Crusader.
        let target = check.setup.enemy;
        if !control.battle.simulation.entities[target].alive() {
            check.stage = Stage::Done;
            error!("Hellfire capture paid Battlemaster died before the attack command");
            control.exit.write(AppExit::error());
            return;
        }
        control.battle.select_objects(&[check.setup.second], true);
        let destination = Vec2::from_array(control.battle.simulation.entities[target].position);
        if !input::context_order(
            &mut control.battle,
            Some(destination),
            Some(PickTarget::Unit(target)),
            &mut control.mouse,
        ) || control
            .mouse
            .voices
            .last()
            .is_none_or(|request| !matches!(request.voice(), Voice::Attack))
        {
            check.stage = Stage::Done;
            error!("Original Crusader attack or VoiceAttack command rejected");
            control.exit.write(AppExit::error());
            return;
        }
        check.began = control.battle.simulation.tick;
        check.stage = Stage::Hellfire;
        control.time.unpause();
    }
    if matches!(check.stage, Stage::Hellfire) {
        let game = &control.battle.simulation;
        let from = Vec2::from_array(game.entities[check.setup.hellfire].position);
        let to = Vec2::from_array(game.entities[check.setup.enemy].position);
        control.camera.focus = (from + to) * 0.05;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct MissileView<'w, 's> {
    particles: ParticleInspection<'w, 's>,
    actors: View<'w, 's>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    visible: Query<'w, 's, &'static ViewVisibility>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w> {
    battle: Res<'w, Match>,
    time: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: MissileView, completion: Completion) {
    let MissileView {
        particles,
        actors,
        projectiles,
        visible,
    } = view;
    let Completion {
        battle,
        mut time,
        mut capture,
        mut exit,
    } = completion;
    if !matches!(check.stage, Stage::Hellfire) || !capture.ready_for_scenario() {
        return;
    }
    let battle = battle.into_inner();
    let result = (|| {
        let missile = visible_missile(
            battle,
            check.setup.hellfire,
            &particles,
            &projectiles,
            &visible,
        )?;
        if missile && check.sounds.into_iter().all(|heard| heard) {
            actors.bodies(battle, &[check.setup.second, check.setup.hellfire])?;
            check.stage = Stage::Done;
            time.pause();
            capture.finish_scenario();
            println!(
                "Original Hellfire capture: ordinary paid Crusader attack, independent drone missile, original STREAK/slave exhaust, and advancing attack/fire audio passed"
            );
        } else {
            require(
                battle.simulation.entities[check.setup.second].alive()
                    && battle.simulation.entities[check.setup.hellfire].alive()
                    && battle.simulation.entities[check.setup.enemy].alive(),
                "Hellfire capture combatant died before visible missile/audio acceptance",
            )?;
            require(
                battle.simulation.tick - check.began < 1800,
                "Hellfire original missile, exhaust or firing audio acceptance timed out",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.stage = Stage::Done;
        error!("Original Hellfire acceptance: {error}");
        exit.write(AppExit::error());
    }
}

fn visible_missile(
    battle: &Match,
    actor: usize,
    particles: &ParticleInspection,
    projectiles: &Query<(&ProjectileVisual, &Children)>,
    visible: &Query<&ViewVisibility>,
) -> Result<bool, Box<dyn Error>> {
    let game = &battle.simulation;
    let unit = &game.catalog[game.entities[actor].type_index];
    for (visual, children) in projectiles {
        if visual.generation != battle.generation
            || !game.projectiles.iter().any(|projectile| {
                projectile.id == visual.id
                    && projectile.source == actor
                    && unit
                        .weapon_template(projectile.weapon_index)
                        .is_some_and(|weapon| weapon.name == "HellfireMissileWeapon")
            })
            || !children
                .iter()
                .any(|child| visible.get(child).is_ok_and(|visible| visible.get()))
        {
            continue;
        }
        if particles.missile(visual.id, battle.generation)? {
            return Ok(true);
        }
    }
    Ok(false)
}
