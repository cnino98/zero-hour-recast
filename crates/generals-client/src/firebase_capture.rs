//! Original Fire Base drawables, visible stationed infantry, shell and controls.
use crate::{
    control_bar::UiData,
    firebase_verification,
    model_animation::AnimatedPart,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    projectile_render::ProjectileVisual,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: firebase_verification::Setup,
    began: Option<u64>,
    done: bool,
    sound: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = firebase_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.firebase], true);
    app.insert_resource(Check {
        setup,
        began: None,
        done: false,
        sound: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let position =
        app.world().resource::<Match>().simulation.entities[check.setup.firebase].position;
    app.world_mut().resource_mut::<CameraRig>().focus = Vec2::from_array(position) * 0.1;
    app.add_systems(PreUpdate, inspect_audio);
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if check.began.is_some() && !check.done {
        check.sound |= audio.played([(check.setup.firebase, "StrategyCenter_ArtilleryRound")])[0];
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
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
    let Some(began) = check.began else {
        check.began = Some(battle.simulation.tick);
        time.unpause();
        return;
    };
    let result = (|| {
        if check.sound && view.verify(battle, &check.setup)? {
            check.done = true;
            time.pause();
            capture.finish_scenario();
            println!(
                "Original Fire Base rendered: paid building/four infantry, visible stations, passenger/evacuate/sell/stop icons, airborne shell and advancing original firing audio passed"
            );
        } else {
            require(
                battle.simulation.tick - began < 1200,
                "Fire Base visible infantry, shell or original audio timed out",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.done = true;
        error!("Fire Base native acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn verify(
        self,
        battle: &Match,
        setup: &firebase_verification::Setup,
    ) -> Result<bool, Box<dyn Error>> {
        for id in std::iter::once(&setup.firebase).chain(&setup.riders) {
            let visible = self.visuals.iter().any(|(visual, children)| {
                visual.index == *id
                    && visual.generation == battle.generation
                    && children.iter().any(|child| {
                        self.meshes
                            .get(child)
                            .is_ok_and(|(_, visible)| visible.get())
                    })
            });
            if !visible {
                return Ok(false);
            }
        }
        let game = &battle.simulation;
        for (slot, name) in [
            (1, "Command_FireBaseExit"),
            (2, "Command_FireBaseExit"),
            (3, "Command_FireBaseExit"),
            (4, "Command_FireBaseExit"),
            (6, "Command_Evacuate"),
            (12, "Command_Sell"),
            (14, "Command_Stop"),
        ] {
            let command = selected_command(battle, &self.data, slot)
                .ok_or("Fire Base original command missing")?;
            require(command.name == name, "Fire Base command set differs")?;
            let image = if slot <= 4 {
                &self.data.units[game.entities[setup.riders[slot - 1]].type_index].button_image
            } else {
                &command.image
            };
            let expected = self.data.icon(image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(index) if *index == slot)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "Fire Base original inventory/command icon not painted",
            )?;
        }
        Ok(self.projectiles.iter().any(|(visual, children)| {
            visual.generation == battle.generation
                && game
                    .projectiles
                    .iter()
                    .find(|projectile| projectile.id == visual.id)
                    .is_some_and(|projectile| projectile.source == setup.firebase)
                && children
                    .iter()
                    .any(|child| self.visible.get(child).is_ok_and(|visible| visible.get()))
        }))
    }
}
