//! Original stationary Speaker Tower model, Sell-only controls and healing pulse.
use crate::{
    control_bar::UiData,
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
    speaker_verification,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: speaker_verification::Setup,
    health: f32,
    began: u64,
    started: bool,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = speaker_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.tower], true);
    app.insert_resource(Check {
        health: battle.simulation.entities[setup.worker].health,
        setup,
        began: battle.simulation.tick,
        started: false,
        done: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let position = app.world().resource::<Match>().simulation.entities[check.setup.tower].position;
    app.world_mut().resource_mut::<CameraRig>().focus = Vec2::from_array(position) * 0.1;
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
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

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    battle: Res<Match>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let battle = battle.into_inner();
    if check.done || !capture.ready_for_scenario() {
        return;
    }
    if !check.started {
        check.started = true;
        check.began = battle.simulation.tick;
        time.unpause();
        return;
    }
    let result = (|| {
        if view.verify(battle, check.setup.tower)?
            && battle.simulation.entities[check.setup.worker].health > check.health
        {
            check.done = true;
            time.pause();
            capture.finish_scenario();
            println!(
                "Original Speaker Tower rendered: paid prerequisite/build, original model, Sell-only command icon and actual healing SonicRange pulse passed"
            );
        } else {
            require(
                battle.simulation.tick - check.began < 600,
                "stationary speaker native render timed out",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.done = true;
        error!("Speaker Tower acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn verify(self, battle: &Match, tower: usize) -> Result<bool, Box<dyn Error>> {
        let Some((_, children)) = self
            .visuals
            .iter()
            .find(|(visual, _)| visual.index == tower && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        if !children.iter().any(|child| {
            self.meshes
                .get(child)
                .is_ok_and(|(_, visible)| visible.get())
        }) {
            return Ok(false);
        }
        let command = selected_command(battle, &self.data, 14)
            .ok_or("speaker original Sell command missing")?;
        require(
            command.name == "Command_Sell",
            "stationary speaker has wrong native command",
        )?;
        require(
            (1..14).all(|slot| selected_command(battle, &self.data, slot).is_none()),
            "stationary speaker exposes commands absent from original command set",
        )?;
        let expected = self.data.icon(&command.image);
        require(
            self.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(14))
                    && node.size().min_element() > 0.0
                    && self.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
            }),
            "speaker Sell icon is not rendered from original art",
        )?;
        self.particles.propaganda(battle.generation)
    }
}
