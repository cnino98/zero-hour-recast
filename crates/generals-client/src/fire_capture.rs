//! Original fire-wall ground command and paid Inferno firestorm presentation.
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, CommandMode, Mouse, PickTarget},
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::{Capture, UnitVisual, WorldCamera},
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    source_audio::FiringAudioInspection,
    strategy_verification::{research, template},
    terrain::Terrain,
};
use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    ui_render::ui_material::MaterialNode,
    window::PrimaryWindow,
};
use generals_sim::skirmish::{Faction, Kind, Order};
use std::error::Error;

#[derive(Resource)]
struct Check {
    storm: bool,
    actors: Vec<usize>,
    target: usize,
    point: Vec2,
    phase: u8,
    began: u64,
    sounds: [bool; 4],
    trail: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match, storm: bool) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    research(game, factory, "Upgrade_ChinaBlackNapalm")?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let mut actors = Vec::new();
    if storm {
        construct_type(game, worker, template(game, "ChinaPropagandaCenter")?, 0)?;
        for _ in 0..3 {
            actors.push(produce_type(
                game,
                factory,
                template(game, "ChinaVehicleInfernoCannon")?,
                0,
            )?);
        }
        let destination = Vec2::from_array(game.entities[target].position);
        let direction =
            (destination - Vec2::from_array(game.entities[actors[0]].position)).normalize();
        let center = destination - direction * 220.0;
        for (&actor, offset) in actors.iter().zip([-30.0, 0.0, 30.0]) {
            let point = center + direction.perp() * offset;
            game.move_to(actor, point.to_array())?;
        }
        advance_until(
            game,
            18000,
            "paid Infernos reaching ordinary firing positions",
            |game| {
                actors
                    .iter()
                    .all(|&id| matches!(game.entities[id].ai.order, Order::Idle))
            },
        )?;
    } else {
        actors.push(produce_type(
            game,
            factory,
            template(game, "ChinaTankDragon")?,
            0,
        )?);
    }
    let point = if storm {
        Vec2::from_array(game.entities[target].position)
    } else {
        Vec2::from_array(game.entities[actors[0]].position) + Vec2::Y * 65.0
    };
    let began = game.tick;
    battle.select_objects(&actors, true);
    app.insert_resource(Check {
        storm,
        actors,
        target,
        point,
        phase: 0,
        began,
        sounds: [false; 4],
        trail: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let from = Vec2::from_array(game.entities[check.actors[0]].position);
    let point = check.point;
    let mut rig = app.world_mut().resource_mut::<CameraRig>();
    rig.focus = (from + point) * 0.05;
    rig.settings.yaw = (point.y - from.y).atan2(point.x - from.x);
    app.add_systems(PreUpdate, (actor_audio, ambient_audio));
    app.add_systems(Update, control.after(input::pointer));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn actor_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    let names = if check.storm {
        ["InfernoCannonVoiceAttack", "InfernoCannonWeapon"]
    } else {
        ["DragonTankVoiceFireStorm", "DragonTankWeaponLoop"]
    };
    let events = names.map(|name| (check.actors[0], name));
    for (heard, actual) in check.sounds.iter_mut().zip(audio.played(events)) {
        *heard |= actual;
    }
}

fn ambient_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if check.storm {
        for (heard, played) in check.sounds[2..]
            .iter_mut()
            .zip(audio.ambient(["InfernoCannonIncomingWhistle", "FireStormLoop"]))
        {
            *heard |= played;
        }
    }
}

#[derive(SystemParam)]
struct Control<'w, 's> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    clock: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    time: Res<'w, Time<Real>>,
    exit: MessageWriter<'w, AppExit>,
    window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    terrain: Res<'w, Terrain>,
    events: MessageWriter<'w, MouseButtonInput>,
}

fn control(mut check: ResMut<Check>, mut control: Control) {
    if check.phase == 255 {
        return;
    }
    if control.time.elapsed_secs() > 120.0 {
        error!(
            "Original fire capture timed out phase={} frame={} sounds={:?} trail={}",
            check.phase, control.battle.simulation.tick, check.sounds, check.trail
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        match check.phase {
            0 if check.storm => {
                require(
                    input::context_order(
                        &mut control.battle,
                        Some(check.point),
                        Some(PickTarget::Unit(check.target)),
                        &mut control.mouse,
                    ),
                    "original Inferno context attack rejected",
                )?;
                check.phase = 4;
                control.clock.unpause();
            }
            0 => {
                dispatch_command(
                    &mut control.battle,
                    &mut control.mouse,
                    CommandAction::FireWeapon(1),
                );
                require(
                    matches!(control.mouse.pending_command, CommandMode::FireWeapon(1)),
                    "original Dragon wall button rejected",
                )?;
                let ground = check.point * 0.1;
                let point = Vec3::new(ground.x, control.terrain.sample(ground), -ground.y);
                let screen = control
                    .camera
                    .0
                    .world_to_viewport(control.camera.1, point)?;
                control.window.1.set_cursor_position(Some(screen));
                check.phase = 1;
            }
            1 | 2 => {
                let state = if check.phase == 1 {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                };
                let window = control.window.0;
                control.events.write(MouseButtonInput {
                    button: MouseButton::Left,
                    state,
                    window,
                });
                check.phase += 1;
            }
            3 => {
                require(
                    matches!(control.mouse.pending_command, CommandMode::Normal)
                        && matches!(
                            control.battle.simulation.entities[check.actors[0]].ai.order,
                            Order::FireAt { .. }
                        ),
                    "original Dragon target click did not start location fire",
                )?;
                check.phase = 4;
                control.clock.unpause();
            }
            _ => {}
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Original fire capture: {error}");
        check.phase = 255;
        control.exit.write(AppExit::error());
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
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
    clock: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if check.phase != 4 || !view.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        let generation = view.battle.generation;
        check.trail |= view
            .particles
            .incendiary("InfernoTrailUpgraded", generation)?;
        let name = if check.storm {
            "FireStormMainUpgraded"
        } else {
            "FireWallSegmentUpgradedSystem"
        };
        let count = if check.storm { 4 } else { 2 };
        if view.particles.incendiary(name, generation)?
            && check.sounds[..count].iter().all(|heard| *heard)
            && (!check.storm || check.trail)
        {
            verify_controls(&view, &check)?;
            view.clock.pause();
            view.capture.finish_scenario();
            check.phase = 255;
            println!(
                "Original fire capture PASSED: paid units/research, original commands/model/particles and advancing source audio; storm={}",
                check.storm
            );
        } else {
            require(
                view.battle.simulation.tick - check.began < 3600,
                "original incendiary particles/audio did not become visible",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Original fire capture: {error}");
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}

fn verify_controls(view: &View, check: &Check) -> Result<(), Box<dyn Error>> {
    let actor = check.actors[0];
    let children = view
        .visuals
        .iter()
        .find_map(|(visual, children)| {
            (visual.index == actor && visual.generation == view.battle.generation)
                .then_some(children)
        })
        .ok_or("missing original fire unit drawable")?;
    require(
        children.iter().any(|child| {
            view.meshes
                .get(child)
                .is_ok_and(|(_, visible)| visible.get())
        }),
        "original fire unit geometry not visible",
    )?;
    let slots: &[usize] = if check.storm {
        &[11, 13, 14]
    } else {
        &[1, 11, 13, 14]
    };
    for &slot in slots {
        let command = selected_command(&view.battle, &view.data, slot)
            .ok_or("missing original fire unit command")?;
        let expected = view.data.icon(&command.image);
        require(
            view.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(index) if *index == slot)
                    && node.size().min_element() > 0.0
                    && view.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
            }),
            "original fire unit icon not painted",
        )?;
    }
    Ok(())
}
