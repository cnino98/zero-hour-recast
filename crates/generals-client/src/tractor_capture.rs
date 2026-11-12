//! Paid Tractor stream, original ground-command cursor, contamination and audio.
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, CommandMode, Mouse, PickTarget},
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::{Capture, UnitVisual, WorldCamera},
    projectile_stream::ProjectileStreamInspection,
    skirmish_verification::require,
    source_audio::{FiringAudioInspection, Voice},
    terrain::Terrain,
    tractor_verification::{self, Setup},
};
use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    ui_render::ui_material::MaterialNode,
    window::PrimaryWindow,
};
use generals_sim::skirmish::{Order, hazards::Origin};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    began: u64,
    health: f32,
    audio: [bool; 4],
    attack_finished: bool,
    puddle_visible: bool,
    secondary_shot: Option<u64>,
    reported: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = tractor_verification::prepare(&mut battle.simulation)?;
    let health = battle.simulation.entities[setup.target].health;
    battle.select_objects(&[setup.actor], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        began: battle.simulation.tick,
        health,
        audio: [false; 4],
        attack_finished: false,
        puddle_visible: false,
        secondary_shot: None,
        reported: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let from = Vec2::from_array(game.entities[check.setup.actor].position);
    let to = Vec2::from_array(game.entities[check.setup.target].position);
    let mut rig = app.world_mut().resource_mut::<CameraRig>();
    rig.focus = (from + to) * 0.05;
    rig.settings.yaw = (to.y - from.y).atan2(to.x - from.x);
    app.add_systems(PreUpdate, inspect_audio);
    app.add_systems(
        Update,
        update
            .after(input::pointer)
            .after(crate::attack_cursor::update),
    );
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    // Both responses lack INTERRUPT. Wait for the actual first voice to finish
    // before the fixture requests another voice for the same original object.
    check.attack_finished |=
        check.audio[0] && audio.finished(check.setup.actor, "ToxinTractorVoiceAttack");
    let events = [
        "ToxinTractorVoiceAttack",
        "ToxinTractorWeaponLoop",
        "ToxinTractorVoiceAttackContam",
        "ToxinTractorContaminate",
    ]
    .map(|name| (check.setup.actor, name));
    for (heard, played) in check.audio.iter_mut().zip(audio.played(events)) {
        *heard |= played;
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    stream: ProjectileStreamInspection<'w, 's>,
    particles: ParticleInspection<'w, 's>,
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

#[derive(SystemParam)]
struct Control<'w, 's> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    clock: ResMut<'w, Time<Virtual>>,
    time: Res<'w, Time<Real>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
    window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    terrain: Res<'w, Terrain>,
    events: MessageWriter<'w, MouseButtonInput>,
    pointer: Single<'w, 's, &'static Node, With<crate::attack_cursor::AttackPointer>>,
}

fn update(mut check: ResMut<Check>, mut control: Control) {
    if check.phase == 255 {
        return;
    }
    if control.time.elapsed_secs() >= 120.0 {
        error!(
            "Tractor native timeout: {}",
            details(&check, &control.battle)
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if control
        .battle
        .simulation
        .presentation
        .shots
        .iter()
        .any(|shot| shot.source == check.setup.actor && shot.weapon_slot == 1)
    {
        check.secondary_shot = Some(control.battle.simulation.tick);
    }
    if !control.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        match check.phase {
            0 => begin_attack(&mut check, &mut control)?,
            2 if check.attack_finished => begin_contamination(&mut check, &mut control)?,
            3 => {
                require(
                    crate::attack_cursor::context(&mut control.battle, &control.mouse) == Some(5)
                        && control.pointer.display == Display::Flex,
                    "original Tractor position command did not paint the CROSS cursor",
                )?;
                button(&mut control, ButtonState::Pressed);
                check.phase = 4;
            }
            4 => {
                button(&mut control, ButtonState::Released);
                check.phase = 5;
            }
            5 => accept_ground_order(&mut check, &mut control)?,
            7 => {
                dispatch_command(&mut control.battle, &mut control.mouse, CommandAction::Stop);
                control.clock.unpause();
                check.phase = 8;
            }
            _ => {}
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Tractor native command phase {}: {error}", check.phase);
        check.phase = 255;
        control.exit.write(AppExit::error());
    }
}

#[derive(SystemParam)]
struct Completion<'w> {
    battle: Res<'w, Match>,
    clock: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: View, mut completion: Completion) {
    if check.phase == 255 || !completion.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        match check.phase {
            1 => {
                verify_controls(&view, &completion.battle, check.setup.actor)?;
                let index =
                    completion.battle.simulation.entities[check.setup.actor].weapon_index(0);
                if view.stream.visible(check.setup.actor, index)?
                    && check.audio[..2].iter().all(|heard| *heard)
                    && completion.battle.simulation.entities[check.setup.target].health
                        < check.health
                {
                    completion.clock.pause();
                    completion.capture.intermediate("tractor-stream.png");
                    check.phase = 2;
                }
            }
            6 => {
                check.puddle_visible = view
                    .particles
                    .contamination(completion.battle.generation, true)?;
                let elapsed = completion.battle.simulation.tick - check.began >= 60;
                if elapsed && !check.reported {
                    eprintln!(
                        "Tractor native contamination: {}",
                        details(&check, &completion.battle)
                    );
                    check.reported = true;
                }
                if elapsed && check.audio[2..].iter().all(|heard| *heard) && check.puddle_visible {
                    completion.clock.pause();
                    completion.capture.intermediate("tractor-contamination.png");
                    check.phase = 7;
                }
            }
            8 => finish(&mut check, &view.particles, &mut completion)?,
            _ => {}
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Tractor native rendering phase {}: {error}", check.phase);
        check.phase = 255;
        completion.exit.write(AppExit::error());
    }
}

fn details(check: &Check, battle: &Match) -> String {
    let game = &battle.simulation;
    let actor = &game.entities[check.setup.actor];
    let weapon = actor.active_weapon(&game.catalog[actor.type_index]);
    let fields = game
        .hazards
        .iter()
        .filter(|field| {
            field.source == check.setup.actor
                && field.created >= check.began
                && matches!(field.origin, Origin::Cooldown(_))
        })
        .count();
    format!(
        "phase={} frame={} phase_frames={} attack_voice_finished={} audio_attack={} audio_stream={} audio_poison_voice={} audio_contaminate={} puddle_visible={} last_secondary_shot={:?} weapon={} slot={} cooldown_fields={fields}; {}",
        check.phase,
        game.tick,
        game.tick - check.began,
        check.attack_finished,
        check.audio[0],
        check.audio[1],
        check.audio[2],
        check.audio[3],
        check.puddle_visible,
        check.secondary_shot,
        weapon.name,
        actor.weapon_slot(),
        generals_sim::skirmish::diagnostics::movement_details(game, check.setup.actor)
    )
}

fn begin_attack(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let destination =
        Vec2::from_array(control.battle.simulation.entities[check.setup.target].position);
    require(
        input::context_order(
            &mut control.battle,
            Some(destination),
            Some(PickTarget::Unit(check.setup.target)),
            &mut control.mouse,
        ) && control
            .mouse
            .voices
            .last()
            .is_some_and(|voice| matches!(voice.voice(), Voice::Attack)),
        "original Tractor attack command or voice request rejected",
    )?;
    check.phase = 1;
    control.clock.unpause();
    Ok(())
}

fn begin_contamination(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    dispatch_command(&mut control.battle, &mut control.mouse, CommandAction::Stop);
    dispatch_command(
        &mut control.battle,
        &mut control.mouse,
        CommandAction::FireWeapon(1),
    );
    require(
        matches!(control.mouse.pending_command, CommandMode::FireWeapon(1))
            && control
                .mouse
                .voices
                .last()
                .is_some_and(|voice| matches!(voice.voice(), Voice::ContaminateMode)),
        "original contamination button did not enter secondary-weapon targeting",
    )?;
    // Original mode voice has an empty Sounds list; no audible substitute.
    let actor = control.battle.simulation.entities[check.setup.actor].position;
    let ground = Vec2::new(actor[0] + 4.0, actor[1]) * 0.1;
    let world = Vec3::new(ground.x, control.terrain.sample(ground), -ground.y);
    let screen = control
        .camera
        .0
        .world_to_viewport(control.camera.1, world)?;
    control.window.1.set_cursor_position(Some(screen));
    check.phase = 3;
    Ok(())
}

fn button(control: &mut Control, state: ButtonState) {
    control.events.write(MouseButtonInput {
        button: MouseButton::Left,
        state,
        window: control.window.0,
    });
}

fn accept_ground_order(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let actor = &control.battle.simulation.entities[check.setup.actor];
    require(
        matches!(control.mouse.pending_command, CommandMode::Normal)
            && matches!(actor.ai.order, Order::FireAt { .. })
            && actor.weapon_slot() == 1,
        "original ground click did not issue the locked secondary weapon command",
    )?;
    check.began = control.battle.simulation.tick;
    check.phase = 6;
    control.clock.unpause();
    Ok(())
}

fn finish(
    check: &mut Check,
    particles: &ParticleInspection,
    control: &mut Completion,
) -> Result<(), Box<dyn Error>> {
    let field = control.battle.simulation.hazards.iter().any(|field| {
        field.source == check.setup.actor
            && field.created >= check.began
            && matches!(field.origin, Origin::Cooldown(_))
    });
    if field && particles.contamination(control.battle.generation, false)? {
        check.phase = 255;
        control.clock.pause();
        control.capture.finish_scenario();
        println!(
            "Original Tractor: paid unit, visible textured stream and damage, four original command icons, CROSS ground targeting, continuous toxin puddle, post-Stop field and advancing original attack/contamination audio passed"
        );
    }
    Ok(())
}

fn verify_controls(view: &View, battle: &Match, actor: usize) -> Result<(), Box<dyn Error>> {
    let children = view
        .visuals
        .iter()
        .find_map(|(visual, children)| {
            (visual.index == actor && visual.generation == battle.generation).then_some(children)
        })
        .ok_or("original Tractor drawable missing")?;
    require(
        children.iter().any(|child| {
            view.meshes
                .get(child)
                .is_ok_and(|(_, visible)| visible.get())
        }),
        "original Tractor mesh is not visible",
    )?;
    for slot in [1, 11, 13, 14] {
        let command =
            selected_command(battle, &view.data, slot).ok_or("original Tractor command missing")?;
        let expected = view.data.icon(&command.image);
        require(
            view.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(index) if *index == slot)
                    && node.size().min_element() > 0.0
                    && view.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
            }),
            "original Tractor command art is not painted",
        )?;
    }
    Ok(())
}
