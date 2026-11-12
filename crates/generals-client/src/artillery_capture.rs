//! Paid source artillery, command icons, airborne missile geometry and firing audio.
use crate::{
    artillery_verification,
    control_bar::UiData,
    input::{self, Mouse, PickTarget},
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    projectile_render::ProjectileVisual,
    skirmish_verification::require,
    source_audio::{FiringAudioInspection, Voice},
    strategy_verification::research,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    actor: usize,
    target: usize,
    kind: u8,
    began: u64,
    started: bool,
    done: bool,
    projectile: bool,
    sounds: [bool; 2],
}

pub fn prepare(app: &mut App, battle: &mut Match, kind: u8) -> Result<(), Box<dyn Error>> {
    let name = match kind {
        0 => "AmericaVehicleTomahawk",
        1 => "GLAVehicleRocketBuggy",
        2 => "GLAVehicleScudLauncher",
        _ => return Err("unknown artillery capture".into()),
    };
    let setup = artillery_verification::prepare(&mut battle.simulation, name)?;
    if kind == 2 {
        research(&mut battle.simulation, setup.tech, "Upgrade_GLAAnthraxBeta")?;
        battle.simulation.switch_weapon(setup.actor, 1)?;
    }
    battle.select_objects(&[setup.actor], true);
    app.insert_resource(Check {
        actor: setup.actor,
        target: setup.target,
        kind,
        began: battle.simulation.tick,
        started: false,
        done: false,
        projectile: false,
        sounds: [false; 2],
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let from = Vec2::from_array(game.entities[check.actor].position);
    let to = Vec2::from_array(game.entities[check.target].position);
    let mut rig = app.world_mut().resource_mut::<CameraRig>();
    rig.focus = (from + to) * 0.05;
    rig.settings.yaw = (to.y - from.y).atan2(to.x - from.x);
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
    let names = match check.kind {
        0 => ["TomahawkWeapon", "TomahawkVoiceAttack"],
        1 => ["RocketBuggyWeapon", "RocketBuggyVoiceAttack"],
        _ => ["ScudLauncherWeapon", "ScudLauncherVoiceAttack"],
    };
    let events = names.map(|name| (check.actor, name));
    for (heard, played) in check.sounds.iter_mut().zip(audio.played(events)) {
        *heard |= played;
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
        Some(PickTarget::Unit(check.target)),
        &mut control.mouse,
    ) || control
        .mouse
        .voices
        .last()
        .is_none_or(|request| !matches!(request.voice(), Voice::Attack))
    {
        check.done = true;
        error!("Original artillery attack or VoiceAttack command rejected");
        control.exit.write(AppExit::error());
        return;
    }
    check.started = true;
    check.began = control.battle.simulation.tick;
    control.time.unpause();
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    particles: ParticleInspection<'w, 's>,
    data: Res<'w, UiData>,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    meshes: Query<'w, 's, (&'static AnimatedPart, &'static ViewVisibility)>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    visible: Query<'w, 's, &'static ViewVisibility>,
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

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w> {
    battle: Res<'w, Match>,
    time: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: View, completion: Completion) {
    let Completion {
        battle,
        mut time,
        mut capture,
        mut exit,
    } = completion;
    if check.done || !check.started || !capture.ready_for_scenario() {
        return;
    }
    let battle = battle.into_inner();
    let result = (|| {
        let visible = view.visible_projectile(battle, check.actor);
        check.projectile |= visible;
        let effects = if check.kind == 2 {
            view.particles.poison_field(battle.generation)?
        } else {
            visible
        };
        if check.projectile && effects && check.sounds.into_iter().all(|heard| heard) {
            view.verify(battle, check.actor, check.kind)?;
            check.done = true;
            time.pause();
            capture.finish_scenario();
            println!(
                "Original artillery: paid technology/production, original model/commands, visible missile and advancing attack/fire audio passed (kind {})",
                check.kind
            );
        } else {
            require(
                battle.simulation.tick - check.began < 1800,
                "artillery original projectile, field or audio acceptance timed out",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.done = true;
        error!("Artillery native acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn visible_projectile(&self, battle: &Match, actor: usize) -> bool {
        self.projectiles.iter().any(|(visual, children)| {
            visual.generation == battle.generation
                && battle
                    .simulation
                    .projectiles
                    .iter()
                    .any(|projectile| projectile.id == visual.id && projectile.source == actor)
                && children
                    .iter()
                    .any(|child| self.visible.get(child).is_ok_and(|visible| visible.get()))
        })
    }

    fn verify(self, battle: &Match, actor: usize, kind: u8) -> Result<(), Box<dyn Error>> {
        let children = self
            .visuals
            .iter()
            .find_map(|(visual, children)| {
                (visual.index == actor && visual.generation == battle.generation)
                    .then_some(children)
            })
            .ok_or("original artillery drawable missing")?;
        require(
            children.iter().any(|child| {
                self.meshes
                    .get(child)
                    .is_ok_and(|(_, visible)| visible.get())
            }),
            "original artillery geometry is not visible",
        )?;
        let slots: &[usize] = if kind == 2 {
            &[1, 3, 11, 13, 14]
        } else {
            &[11, 13, 14]
        };
        for &slot in slots {
            let command = selected_command(battle, &self.data, slot)
                .ok_or("original artillery command slot missing")?;
            let expected = self.data.icon(&command.image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(index) if *index == slot)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "original artillery command icon is not painted",
            )?;
        }
        Ok(())
    }
}
