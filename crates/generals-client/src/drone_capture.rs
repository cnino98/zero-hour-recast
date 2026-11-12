//! Paid drone repair and Hellfire combat, with separate original-art captures.
mod hellfire;

use crate::{
    control_bar::UiData,
    drone_verification,
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: drone_verification::Setup,
    began: u64,
    initial_health: f32,
    stage: Stage,
    sounds: [bool; 2],
}

enum Stage {
    Waiting,
    Repair,
    RepairCaptured,
    Hellfire,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = drone_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.parent], true);
    app.insert_resource(Check {
        initial_health: battle.simulation.entities[setup.parent].health,
        setup,
        began: battle.simulation.tick,
        stage: Stage::Waiting,
        sounds: [false; 2],
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let focus = Vec2::from_array(game.entities[check.setup.parent].position) * 0.1;
    app.world_mut().resource_mut::<CameraRig>().focus = focus;
    hellfire::register(app);
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
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
    particles: ParticleInspection,
    view: View,
    battle: Res<Match>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let battle = battle.into_inner();
    if !matches!(check.stage, Stage::Waiting | Stage::Repair) || !capture.ready_for_scenario() {
        return;
    }
    if matches!(check.stage, Stage::Waiting) {
        check.stage = Stage::Repair;
        check.began = battle.simulation.tick;
        time.unpause();
        return;
    }
    let result = (|| {
        require(
            battle.simulation.entities[check.setup.parent].alive(),
            "drone capture parent died",
        )?;
        if particles.drone_welding(battle.generation)?
            && battle.simulation.entities[check.setup.parent].health > check.initial_health
        {
            view.verify(battle, &check.setup)?;
            check.stage = Stage::RepairCaptured;
            time.pause();
            capture.intermediate("drones-welding.png");
            println!(
                "Original repair drone capture: paid Humvee/Battle Drone bodies, native arm/welding particles, genuine combat healing and both vehicle drone purchase icons passed"
            );
        } else {
            require(
                battle.simulation.tick - check.began < 1800,
                "native drone repair/render acceptance timed out",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.stage = Stage::Done;
        error!("Original drones acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn verify(
        self,
        battle: &Match,
        setup: &drone_verification::Setup,
    ) -> Result<(), Box<dyn Error>> {
        self.bodies(battle, &[setup.parent, setup.battle])?;
        let mut icons = 0;
        for slot in 1..=14 {
            let Some(command) = selected_command(battle, &self.data, slot) else {
                continue;
            };
            if !command.image.contains("Drone") && !command.image.contains("Hellfire") {
                continue;
            }
            let expected = self.data.icon(&command.image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(index) if *index == slot)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "native vehicle drone purchase icon is not rendered",
            )?;
            icons += 1;
        }
        require(
            icons == 2,
            "vehicle command set lacks the two supported original drone upgrades",
        )?;
        Ok(())
    }

    fn bodies(&self, battle: &Match, ids: &[usize]) -> Result<(), Box<dyn Error>> {
        for &id in ids {
            let children = self
                .visuals
                .iter()
                .find_map(|(visual, children)| {
                    (visual.index == id && visual.generation == battle.generation)
                        .then_some(children)
                })
                .ok_or("native drone or parent drawable missing")?;
            require(
                children.iter().any(|child| {
                    self.meshes
                        .get(child)
                        .is_ok_and(|(_, visible)| visible.get())
                }),
                "native drone or parent model is not visible",
            )?;
        }
        Ok(())
    }
}
