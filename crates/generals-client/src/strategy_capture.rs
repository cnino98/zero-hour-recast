//! Actual original Strategy Center commands, deployment art and announcement playback.
use crate::{
    control_bar::{CommandAction, UiData},
    input::Mouse,
    model_animation::{AnimatedPart, Animations, Players},
    playtest::{CameraRig, Match},
    playtest_ui::{self, Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
    source_audio::StrategyAudioInspection,
    strategy_verification,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use generals_sim::skirmish::battle_plan::Plan;
use std::error::Error;

const PLANS: [Plan; 3] = [Plan::Bombardment, Plan::HoldTheLine, Plan::SearchAndDestroy];

#[derive(Resource)]
struct Check {
    center: usize,
    mode: usize,
    began: u64,
    issued: bool,
    frame: Option<f32>,
    advanced: bool,
    heard: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = strategy_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.center], true);
    let began = battle.simulation.tick;
    app.insert_resource(Check {
        center: setup.center,
        mode: 0,
        began,
        issued: false,
        frame: None,
        advanced: false,
        heard: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let center = app.world().resource::<Check>().center;
    let focus =
        Vec2::from_array(app.world().resource::<Match>().simulation.entities[center].position)
            * 0.1;
    app.world_mut().resource_mut::<CameraRig>().focus = focus;
    app.add_systems(PreUpdate, inspect_audio);
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
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Players, &'static Children)>,
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
    animations: Res<'w, Animations>,
}

fn inspect_audio(mut check: ResMut<Check>, battle: Res<Match>, audio: StrategyAudioInspection) {
    let battle = battle.into_inner();
    // Observe before playback retires completed sinks, including while new draw
    // pipelines compile. Audio output uses real time and continues during stalls.
    if check.issued && check.mode < PLANS.len() {
        let type_index = battle.simulation.entities[check.center].type_index;
        check.heard |= audio.played(type_index, check.mode);
    }
}

fn inspect(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    view: View,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.mode == 3 || !capture.ready_for_scenario() {
        return;
    }
    if let Err(error) = view.verify(&mut check, &mut battle, &mut mouse, &mut time, &mut capture) {
        check.mode = 3;
        error!("Strategy Center native acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn verify(
        self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<(), Box<dyn Error>> {
        let actor = &battle.simulation.entities[check.center];
        let type_index = actor.type_index;
        let (_, players, children) = self
            .visuals
            .iter()
            .find(|(visual, _, _)| {
                let selected = visual.index == check.center;
                selected && visual.generation == battle.generation
            })
            .ok_or("original Strategy Center drawable missing")?;
        if !check.issued {
            self.verify_buttons(battle)?;
            playtest_ui::dispatch_command(
                battle,
                mouse,
                CommandAction::BattlePlan(PLANS[check.mode]),
            );
            require(
                battle.simulation.entities[check.center].battle_plan_desired()
                    == Some(PLANS[check.mode]),
                "original battle-plan control did not dispatch",
            )?;
            check.issued = true;
            time.unpause();
            return Ok(());
        }
        require(
            battle.simulation.tick - check.began < 3000,
            "native battle plan timed out",
        )?;
        let suffix = ["abstrategy_a1", "abstrategy_a7", "abstrategy_a2"][check.mode];
        if actor
            .battle_plan_condition()
            .is_some_and(|name| name.ends_with("OPENING"))
        {
            let sample = self
                .animations
                .inspect(players, type_index)?
                .into_iter()
                .find(|sample| sample.clip.contains(suffix))
                .ok_or("original deployment animation not running")?;
            if let Some(frame) = check.frame {
                check.advanced |= sample.frame > frame;
            }
            check.frame = Some(sample.frame);
        }
        if actor.battle_plan() != Some(PLANS[check.mode]) {
            return Ok(());
        }
        self.verify_models(players, type_index, check.mode, children)?;
        require(
            check.advanced,
            "original deployment animation did not advance",
        )?;
        require(
            check.heard,
            "original plan announcement did not reach an advancing audio sink",
        )?;
        println!(
            "Rendered original battle plan {}: original button, advancing deployment, active geometry and announcement sink passed",
            check.mode + 1
        );
        check.mode += 1;
        check.frame = None;
        check.advanced = false;
        check.heard = false;
        check.issued = false;
        if check.mode == 3 {
            time.pause();
            capture.finish_scenario();
        }
        Ok(())
    }

    fn verify_models(
        &self,
        players: &Players,
        type_index: usize,
        mode: usize,
        children: &Children,
    ) -> Result<(), Box<dyn Error>> {
        let model = self
            .animations
            .strategy_active_model(players, type_index, PLANS[mode])
            .ok_or("active original cannon/walls/radar model missing")?;
        require(
            children.iter().any(|child| {
                self.meshes
                    .get(child)
                    .is_ok_and(|(part, visible)| part.model == model && visible.get())
            }),
            "original active battle-plan geometry is not visible",
        )?;
        if mode != 0 {
            let cannon = self
                .animations
                .strategy_active_model(players, type_index, Plan::Bombardment)
                .ok_or("original packed cannon model missing")?;
            require(
                !children.iter().any(|child| {
                    self.meshes
                        .get(child)
                        .is_ok_and(|(part, visible)| part.model == cannon && visible.get())
                }),
                "packed cannon retained visible descendant geometry",
            )?;
        }
        Ok(())
    }

    fn verify_buttons(&self, battle: &Match) -> Result<(), Box<dyn Error>> {
        for slot in [1, 3, 5, 8, 9] {
            let command = selected_command(battle, &self.data, slot).ok_or_else(|| {
                format!("original battle-plan/research command missing at slot {slot}")
            })?;
            let expected = self.data.icon(&command.image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(index) if *index == slot)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "original Strategy Center command art not painted",
            )?;
        }
        Ok(())
    }
}
