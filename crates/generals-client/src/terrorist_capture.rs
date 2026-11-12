//! Original Terrorist controls, visible suicide animation, explosion and sound.
use crate::{
    control_bar::UiData,
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
    terrorist_verification,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: terrorist_verification::Setup,
    began: Option<u64>,
    death: Option<u64>,
    scorch: Option<[f32; 3]>,
    sound: bool,
    done: bool,
    real_began: Option<f64>,
    reported_at: f64,
    visuals: [bool; 3],
    precommand_snapshot: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = terrorist_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.actor], true);
    app.insert_resource(Check {
        setup,
        began: None,
        death: None,
        scorch: None,
        sound: false,
        done: false,
        real_began: None,
        reported_at: 0.0,
        visuals: [false; 3],
        precommand_snapshot: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let actor = Vec2::from_array(game.entities[check.setup.actor].position);
    let target = Vec2::from_array(game.entities[check.setup.target].position);
    // Include the real approach and the building contact point at the original
    // camera height/yaw. Centering the large building hides the waiting unit.
    app.world_mut().resource_mut::<CameraRig>().focus = (actor + target) * 0.05;
    app.add_systems(FixedUpdate, observe.after(crate::particles::step));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn observe(mut check: ResMut<Check>, battle: Res<Match>, mut exit: MessageWriter<AppExit>) {
    let battle = battle.into_inner();
    if check.done || check.death.is_some() || battle.simulation.entities[check.setup.actor].alive()
    {
        return;
    }
    match terrorist_verification::verify_explosion(&battle.simulation, &check.setup) {
        Ok(frame) => {
            check.death = Some(frame);
            check.scorch = battle
                .simulation
                .presentation
                .shots
                .iter()
                .find(|shot| shot.source == check.setup.actor && shot.weapon_index >= 7)
                .map(|shot| shot.source_position);
        }
        Err(error) => {
            error!("Terrorist native combat acceptance: {error}");
            check.done = true;
            exit.write(AppExit::error());
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    camera: Res<'w, CameraRig>,
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
    particles: ParticleInspection<'w, 's>,
    audio: FiringAudioInspection<'w, 's>,
    scorch: crate::scorch::Inspection<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.done {
        return;
    }
    if view.timed_out(&mut check, &battle, &time, &capture) {
        check.done = true;
        error!("Terrorist rendered acceptance exceeded 60 real seconds; see capture state above");
        exit.write(AppExit::error());
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        if check.began.is_none() {
            if !view.actor_visible(&battle, check.setup.actor) {
                if !check.precommand_snapshot
                    && view.clock.elapsed_secs_f64() - check.real_began.unwrap_or(0.0) >= 5.0
                {
                    view.report(&check, &battle, &time, &capture);
                    check.precommand_snapshot = true;
                    capture.intermediate("terrorist-before-command.png");
                }
                return Ok(());
            }
            view.controls(&battle)?;
            battle
                .simulation
                .attack(check.setup.actor, check.setup.target)?;
            check.began = Some(battle.simulation.tick);
            time.unpause();
            return Ok(());
        }
        let visible = view.actor_visible(&battle, check.setup.actor);
        let particles = if let Some(frame) = check.death {
            view.particles
                .actor_fx("FX_TerroristExplode", frame, battle.generation)?
        } else {
            false
        };
        check.sound |= view.audio.object_fx("TerroristSuicides");
        let scorch = check
            .scorch
            .map(|position| view.scorch.explosion(position))
            .transpose()?
            .unwrap_or(false);
        check.visuals = [visible, particles, scorch];
        if visible && particles && scorch && check.sound && check.death.is_some() {
            require(
                battle.simulation.entities[check.setup.actor].corpse_present(),
                "Terrorist corpse draw disappeared before native slow death completed",
            )?;
            check.done = true;
            time.pause();
            capture.finish_scenario();
            println!(
                "Original Terrorist rendered: paid unit/source command icons, contact attack, visible dying/flailing mesh, original explosion particles/terrain scorch and advancing TerroristSuicides audio passed"
            );
        } else {
            require(
                battle.simulation.tick - check.began.unwrap_or(0) < 1200,
                "Terrorist native animation/explosion/audio timed out",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.done = true;
        error!("Terrorist rendered acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn timed_out(
        &self,
        check: &mut Check,
        battle: &Match,
        time: &Time<Virtual>,
        capture: &Capture,
    ) -> bool {
        let now = self.clock.elapsed_secs_f64();
        let began = *check.real_began.get_or_insert(now);
        if now - began >= 15.0 && now - check.reported_at >= 15.0 {
            self.report(check, battle, time, capture);
            check.reported_at = now;
        }
        now - began >= 60.0
    }

    fn report(&self, check: &Check, battle: &Match, time: &Time<Virtual>, capture: &Capture) {
        let actor = &battle.simulation.entities[check.setup.actor];
        let drawable = self.visuals.iter().find(|(visual, _)| {
            visual.index == check.setup.actor && visual.generation == battle.generation
        });
        let meshes = drawable.map(|(_, children)| {
            let total = children
                .iter()
                .filter(|&child| self.meshes.get(child).is_ok())
                .count();
            (children.len(), total)
        });
        warn!(
            "Terrorist capture state: frame={} began={:?} death={:?} virtual_paused={} ready={} pipelines={} actor={} health={} position={:?} corpse={} masked={} camera={:?} drawable_children_meshes={meshes:?} body_visible={} last_body_particles_scorch={:?} audio={}",
            battle.simulation.tick,
            check.began,
            check.death,
            time.is_paused(),
            capture.ready_for_scenario(),
            capture.pipelines_ready(),
            check.setup.actor,
            actor.health,
            battle.simulation.object_position(check.setup.actor),
            actor.corpse_present(),
            actor.masked(),
            self.camera.focus,
            self.actor_visible(battle, check.setup.actor),
            check.visuals,
            check.sound
        );
    }

    fn actor_visible(&self, battle: &Match, actor: usize) -> bool {
        self.visuals.iter().any(|(visual, children)| {
            visual.index == actor
                && visual.generation == battle.generation
                && children.iter().any(|child| {
                    self.meshes
                        .get(child)
                        .is_ok_and(|(_, visible)| visible.get())
                })
        })
    }

    fn controls(&self, battle: &Match) -> Result<(), Box<dyn Error>> {
        for slot in [1, 13, 14] {
            let command = selected_command(battle, &self.data, slot)
                .ok_or("Terrorist original command missing")?;
            let expected = self.data.icon(&command.image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(index) if *index == slot)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "Terrorist source command icon not painted",
            )?;
        }
        Ok(())
    }
}
