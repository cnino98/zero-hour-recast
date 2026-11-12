//! Original paid Paladin model, command art, independent infantry laser and sound.
use crate::{
    control_bar::UiData,
    input::{self, Mouse},
    laser_beam::PulseInspection,
    model_animation::AnimatedPart,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    point_defense_verification,
    skirmish_verification::require,
    source_audio::{FiringAudioInspection, Voice},
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    actor: usize,
    target: usize,
    began: u64,
    started: bool,
    done: bool,
    sounds: [bool; 2],
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (setup, target) = point_defense_verification::prepare_paladin(&mut battle.simulation)?;
    battle.select_objects(&[setup.paladin], true);
    app.insert_resource(Check {
        actor: setup.paladin,
        target,
        began: battle.simulation.tick,
        started: false,
        done: false,
        sounds: [false; 2],
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let focus = (Vec2::from_array(game.entities[check.actor].position)
        + Vec2::from_array(game.entities[check.target].position))
        * 0.05;
    app.world_mut().resource_mut::<CameraRig>().focus = focus;
    app.add_systems(PreUpdate, (inspect_audio, begin).chain());
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if !check.started || check.done {
        return;
    }
    let played = audio.played_events([
        (None, "PaladinPointDefenseLaserPulse"),
        (Some(check.actor), "PaladinTankVoiceMove"),
    ]);
    for (heard, playing) in check.sounds.iter_mut().zip(played) {
        *heard |= playing;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    time: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if check.started || !control.capture.ready_for_scenario() {
        return;
    }
    let destination = Vec2::from_array(control.battle.simulation.entities[check.target].position);
    if !input::context_order(
        &mut control.battle,
        Some(destination),
        None,
        &mut control.mouse,
    ) || control
        .mouse
        .voices
        .last()
        .is_none_or(|request| !matches!(request.voice(), Voice::Move))
    {
        check.done = true;
        error!("Paladin original ground move or VoiceMove command rejected");
        control.exit.write(AppExit::error());
        return;
    }
    check.started = true;
    check.began = control.battle.simulation.tick;
    control.time.unpause();
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    pulses: PulseInspection<'w, 's>,
    data: Res<'w, UiData>,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    meshes: Query<'w, 's, (&'static AnimatedPart, &'static ViewVisibility)>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static ComputedNode,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    icons: Res<'w, Assets<PromotionIcon>>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    battle: Res<Match>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let battle = battle.into_inner();
    if check.done || !check.started || !capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        let visible = view
            .pulses
            .visible(check.actor, "PaladinPointDefenseLaser")?;
        if visible && check.sounds.into_iter().all(|heard| heard) {
            view.verify(battle, check.actor)?;
            check.done = true;
            time.pause();
            capture.finish_scenario();
            println!(
                "Original Paladin: paid science/production, visible original tank and command icons, independent infantry laser and advancing pulse/Move audio passed"
            );
        } else {
            require(
                battle.simulation.tick - check.began < 1800,
                "Paladin native beam/audio acceptance timed out",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.done = true;
        error!("Paladin native acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn verify(self, battle: &Match, actor: usize) -> Result<(), Box<dyn Error>> {
        let children = self
            .visuals
            .iter()
            .find_map(|(visual, children)| {
                (visual.index == actor && visual.generation == battle.generation)
                    .then_some(children)
            })
            .ok_or("original Paladin drawable missing")?;
        require(
            children.iter().any(|child| {
                self.meshes
                    .get(child)
                    .is_ok_and(|(_, visible)| visible.get())
            }),
            "original Paladin geometry is not visible",
        )?;
        for slot in [11, 13, 14] {
            let command = selected_command(battle, &self.data, slot)
                .ok_or("original Paladin command slot missing")?;
            let expected = self.data.icon(&command.image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(index) if *index == slot)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "original Paladin command icon is not painted",
            )?;
        }
        Ok(())
    }
}
