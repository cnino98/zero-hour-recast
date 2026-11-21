//! Paid Lotus controls, cloak, three finite powers, source art and audible callbacks.
use crate::{
    control_bar::{CommandAction, UiData},
    infiltrator_verification::{self, Setup},
    input::{self, CommandMode, Mouse},
    laser_beam::verification::HackerBeamInspection,
    model_animation::{AnimatedPart, Animations, Players},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, dispatch_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
    source_audio::LotusAudioInspection,
    source_material::SourceMaterial,
    stealth_view::Cloaks,
};
use bevy::{ecs::system::SystemParam, prelude::*, ui_render::ui_material::MaterialNode};
use generals_sim::skirmish::special_ability::FeedbackEffect;
use std::error::Error;

#[cfg(test)]
mod tests;

const ABILITIES: [usize; 3] = [1, 2, 0];

struct Heard {
    sounds: [bool; 4],
    mode_finished: bool,
}

#[derive(Resource)]
struct Check {
    setup: Setup,
    stage: u8,
    power: usize,
    audio: Heard,
    frame: Option<f32>,
    advanced: bool,
    cash: bool,
    shower: bool,
    triggered: Option<u64>,
    stolen: Option<u32>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = infiltrator_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.actor], true);
    // Start then cancel a genuine finite command so the client sees reveal and
    // re-conceal. Lotus remains stealthed during ordinary movement in source.
    battle.simulation.hack(setup.actor, setup.vehicle, 1)?;
    app.insert_resource(Check {
        setup,
        stage: 0,
        power: 0,
        audio: Heard {
            sounds: [false; 4],
            mode_finished: false,
        },
        frame: None,
        advanced: false,
        cash: false,
        shower: false,
        triggered: None,
        stolen: None,
    });
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let focus = Vec2::from_array(game.entities[check.setup.actor].position) * 0.1;
    app.world_mut().resource_mut::<CameraRig>().focus = focus;
    register_systems(app);
}

fn register_systems(app: &mut App) {
    app.add_systems(PreUpdate, inspect_audio);
    app.add_systems(FixedUpdate, record_effects.after(crate::laser_beam::record));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, battle: Res<Match>, audio: LotusAudioInspection) {
    let battle = battle.into_inner();
    if check.power >= ABILITIES.len() {
        return;
    }
    let actor = check.setup.actor;
    let (heard, finished) = audio.sample(
        actor,
        battle.simulation.entities[actor].type_index,
        ABILITIES[check.power],
    );
    // BlackLotusTrigger is an authored empty AudioEvent. Require its actual
    // Triggered callback separately; only these four callbacks have samples.
    for (heard_before, current) in check
        .audio
        .sounds
        .iter_mut()
        .zip([heard[0], heard[1], heard[2], heard[4]])
    {
        *heard_before |= current;
    }
    check.audio.mode_finished = check.audio.sounds[0] && finished;
}

fn record_effects(mut check: ResMut<Check>, battle: Res<Match>) {
    if !(3..=5).contains(&check.stage) || check.power >= ABILITIES.len() {
        return;
    }
    let battle = battle.into_inner();
    let actor = check.setup.actor;
    let kind = match ABILITIES[check.power] {
        0 => generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding,
        1 => {
            generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusDisableVehicleHack
        }
        _ => generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusStealCashHack,
    };
    let sound = &battle.simulation.catalog[battle.simulation.entities[actor].type_index]
        .behavior
        .special_abilities
        .iter()
        .find(|rules| rules.power.as_ref().is_some_and(|power| power.kind == kind))
        .expect("installed Lotus ability")
        .feedback
        .sounds[3];
    if battle
        .simulation
        .presentation
        .object_sounds
        .iter()
        .any(|event| event.object == actor && event.name == *sound)
    {
        check.triggered = Some(battle.simulation.tick);
    }
    for event in &battle.simulation.presentation.ability_feedback {
        if event.object == actor
            && event.target == target(&check)
            && let FeedbackEffect::CashStolen(amount) = event.effect
        {
            check.stolen = Some(amount);
        }
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    data: Res<'w, UiData>,
    icons: Res<'w, Assets<PromotionIcon>>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static ComputedNode,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Players, &'static Children)>,
    meshes: Query<
        'w,
        's,
        (
            &'static AnimatedPart,
            &'static ViewVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
    >,
    materials: Res<'w, Assets<SourceMaterial>>,
    animations: Res<'w, Animations>,
    cloaks: Res<'w, Cloaks>,
    beam: HackerBeamInspection<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    floating: Query<'w, 's, (&'static Text, &'static Node), With<crate::income::Floating>>,
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    clock: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    camera: ResMut<'w, CameraRig>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: View, mut control: Control) {
    if check.stage == 255 {
        return;
    }
    if control.real.elapsed_secs() > 180.0 {
        error!(
            "Lotus native timeout: stage={} power={} audio_mode_initiate_prepare_complete={:?} advanced={} cash={} shower={} trigger_frame={:?} stolen={:?} condition={:?} target_hacked={} target_owner={} {}",
            check.stage,
            check.power,
            check.audio.sounds,
            check.advanced,
            check.cash,
            check.shower,
            check.triggered,
            check.stolen,
            control.battle.simulation.entities[check.setup.actor].capture_condition(),
            control.battle.simulation.entities[target(&check)].hacked(),
            control.battle.simulation.entities[target(&check)].owner,
            generals_sim::skirmish::diagnostics::movement_details(
                &control.battle.simulation,
                check.setup.actor
            )
        );
        check.stage = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() {
        return;
    }
    let result = view.progress(&mut check, &mut control);
    if let Err(error) = result {
        error!("Lotus native stage {}: {error}", check.stage);
        check.stage = 255;
        control.exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn progress(self, check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
        let view = &self;
        match check.stage {
            0 => {
                if !control.battle.simulation.entities[check.setup.actor].stealthed() {
                    dispatch_command(&mut control.battle, &mut control.mouse, CommandAction::Stop);
                    check.stage = 9;
                }
            }
            9 => {
                let game = &control.battle.simulation;
                if game.entities[check.setup.actor].stealthed()
                    && view
                        .cloaks
                        .base_opacity(check.setup.actor)
                        .is_some_and(|value| value < 0.8)
                    && view.visible_model(&control.battle, check.setup.actor, true)?
                {
                    view.controls()?;
                    control.clock.pause();
                    control.capture.intermediate("infiltrator-stealth.png");
                    check.audio.sounds = [false; 4];
                    check.stage = 1;
                }
            }
            1 => begin_mode(check, view, control)?,
            2 if check.audio.mode_finished => issue(check, control)?,
            3 => {
                let game = &control.battle.simulation;
                let actor = &game.entities[check.setup.actor];
                if actor.capture_condition() == Some("FIRING_A") {
                    check.advanced |=
                        view.advancing(&control.battle, check.setup.actor, &mut check.frame)?;
                    if check.advanced
                        && check.audio.sounds[..3].iter().all(|heard| *heard)
                        && view.visible_model(&control.battle, check.setup.actor, false)?
                        && view.beam.visible(check.setup.actor)?
                    {
                        control.clock.pause();
                        control.capture.intermediate(
                            ["lotus-disable.png", "lotus-cash.png", "lotus-capture.png"]
                                [check.power],
                        );
                        check.stage = 4;
                    }
                }
            }
            4 => {
                control.clock.unpause();
                check.stage = 5;
            }
            5 => complete_power(check, view, control)?,
            6 => {
                let game = &control.battle.simulation;
                let from = Vec2::from_array(game.entities[check.setup.actor].position);
                let to = Vec2::from_array(game.entities[target(check)].position);
                control.camera.focus = (from + to) * 0.05;
                if from.distance(to) < 190.0 && game.logically_visible(0, target(check)) {
                    dispatch_command(&mut control.battle, &mut control.mouse, CommandAction::Stop);
                    control.clock.pause();
                    check.stage = 1;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

const fn target(check: &Check) -> usize {
    match check.power {
        0 => check.setup.vehicle,
        1 => check.setup.cash,
        _ => check.setup.building,
    }
}

fn begin_mode(check: &mut Check, view: &View, control: &mut Control) -> Result<(), Box<dyn Error>> {
    view.controls()?;
    let ability = ABILITIES[check.power];
    let action = if ability == 0 {
        CommandAction::Capture
    } else {
        CommandAction::Hack(ability)
    };
    dispatch_command(&mut control.battle, &mut control.mouse, action);
    require(
        matches!(control.mouse.pending_command, CommandMode::Capture) && ability == 0
            || matches!(control.mouse.pending_command, CommandMode::Hack(value) if value == ability),
        "Lotus original button did not enter native target mode",
    )?;
    control.mouse.hovered = Some(target(check));
    require(
        crate::attack_cursor::context(&mut control.battle, &control.mouse) == Some(6),
        &format!(
            "Lotus power {ability} lacks Hack cursor: ready={} visible={} logically_visible={} can_capture={}",
            control.battle.simulation.capture_ready(check.setup.actor),
            control.battle.simulation.visible_to(0, target(check)),
            control
                .battle
                .simulation
                .logically_visible(0, target(check)),
            control
                .battle
                .simulation
                .can_capture(check.setup.actor, target(check))
        ),
    )?;
    control.mouse.hovered = Some(check.setup.actor);
    require(
        crate::attack_cursor::context(&mut control.battle, &control.mouse) == Some(3),
        "Lotus original command admits friendly self",
    )?;
    check.stage = 2;
    Ok(())
}

fn issue(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let target = target(check);
    let point = Vec2::from_array(control.battle.simulation.entities[target].position);
    input::targeted_order(&mut control.battle, &mut control.mouse, point, Some(target));
    require(
        matches!(control.mouse.pending_command, CommandMode::Normal),
        "Lotus original targeted command was rejected",
    )?;
    let game = &control.battle.simulation;
    let from = Vec2::from_array(game.entities[check.setup.actor].position);
    let to = Vec2::from_array(game.entities[target].position);
    control.camera.focus = (from + to) * 0.05;
    control.clock.unpause();
    check.stage = 3;
    Ok(())
}

fn complete_power(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    let game = &control.battle.simulation;
    let effect = match check.power {
        0 => {
            check.shower |= view
                .particles
                .disabled(check.setup.vehicle, control.battle.generation)?;
            game.entities[check.setup.vehicle].hacked() && check.shower
        }
        1 => {
            if let Some(amount) = check.stolen {
                let texts = ["GUI:AddCash", "GUI:LoseCash"].map(|label| {
                    view.data
                        .label(label)
                        .replacen("%d", &amount.to_string(), 1)
                });
                check.cash |= texts.iter().all(|expected| {
                    view.floating
                        .iter()
                        .any(|(text, node)| text.0 == *expected && node.display != Display::None)
                });
            }
            check.cash
        }
        _ => game.entities[check.setup.building].owner == 0,
    };
    if !effect
        || check.triggered.is_none()
        || game.entities[check.setup.actor]
            .capture_condition()
            .is_some()
        || !check.audio.sounds.iter().all(|heard| *heard)
    {
        return Ok(());
    }
    println!(
        "Rendered Lotus power {}: original controls/cursor, advancing NIHero clip, binary stream, real effect/trigger and four audible original callbacks passed",
        ABILITIES[check.power]
    );
    check.power += 1;
    control.clock.pause();
    if check.power == ABILITIES.len() {
        control.capture.finish_scenario();
        check.stage = 255;
    } else {
        check.audio.sounds = [false; 4];
        check.audio.mode_finished = false;
        check.frame = None;
        check.advanced = false;
        check.triggered = None;
        check.stolen = None;
        // Each next target must be revealed through ordinary movement before
        // checking its context cursor. The capture target can leave sight while
        // Lotus approaches the separate supply building for the preceding power.
        let point = control.battle.simulation.entities[target(check)].position;
        control
            .battle
            .simulation
            .move_to(check.setup.actor, point)?;
        control.clock.unpause();
        check.stage = 6;
    }
    Ok(())
}

impl View<'_, '_> {
    fn controls(&self) -> Result<(), Box<dyn Error>> {
        for (slot, image) in [(1, "SSBLCapture"), (3, "SSVehicleHack"), (5, "SSCashHack")] {
            let expected = self.data.icon(image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(value) if *value == slot)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "Lotus original command icon/slot missing",
            )?;
        }
        Ok(())
    }

    fn visible_model(
        &self,
        battle: &Match,
        id: usize,
        cloaked: bool,
    ) -> Result<bool, Box<dyn Error>> {
        let Some((_, _, children)) = self
            .visuals
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        for child in children.iter() {
            if let Ok((_, visible, material)) = self.meshes.get(child)
                && visible.get()
                && let Some(material) = self.materials.get(&material.0)
            {
                require(
                    material.parameters.opacity.is_finite(),
                    "Lotus cloak produced nonfinite opacity",
                )?;
                if !cloaked
                    || (material.parameters.opacity > 0.0
                        && material.parameters.opacity < 1.0
                        && material.alpha_mode == AlphaMode::Blend)
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn advancing(
        &self,
        battle: &Match,
        id: usize,
        previous: &mut Option<f32>,
    ) -> Result<bool, Box<dyn Error>> {
        let Some((_, players, _)) = self
            .visuals
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        let samples = self
            .animations
            .inspect(players, battle.simulation.entities[id].type_index)?;
        let Some(sample) = samples
            .iter()
            .find(|sample| sample.clip == "nihero_skl.nihero_ata2")
        else {
            return Ok(false);
        };
        let advanced = previous.is_some_and(|frame| sample.frame.to_bits() != frame.to_bits());
        *previous = Some(sample.frame);
        Ok(advanced)
    }
}

#[cfg(test)]
mod schedule_tests {
    #[test]
    fn actual_capture_graph_initializes_after_current_tick_effects() {
        crate::playtest::schedule_tests::assert_capture_graph(
            super::register_systems,
            "infiltrator_capture::record_effects",
        );
    }
}
