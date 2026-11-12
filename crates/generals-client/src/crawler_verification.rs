//! Paid original Troop Crawler inventory, assault command and live troop rendering.
use crate::{
    control_bar::UiData,
    input::{self, Mouse, PickTarget},
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::{crawler, require},
    veterancy_feedback::VeterancyInspection,
};
use bevy::{app::AppExit, prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: crawler::Setup,
    phase: u8,
    began: u64,
    promoted: Option<usize>,
    promotion_position: [f32; 2],
    promotion_sample: Option<(usize, Vec3)>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = crawler::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.carrier], true);
    let data = app.world().resource::<UiData>();
    for slot in 1..=8 {
        require(
            selected_command(battle, data, slot)
                .is_some_and(|command| command.name == "Command_TransportExit"),
            "original crawler passenger slot missing",
        )?;
    }
    for (slot, name) in [
        (9, "Command_EmptyCrawler"),
        (11, "Command_AttackMove"),
        (14, "Command_Stop"),
    ] {
        require(
            selected_command(battle, data, slot).is_some_and(|command| command.name == name),
            "original crawler command missing",
        )?;
    }
    let began = battle.simulation.tick;
    app.insert_resource(Check {
        setup,
        phase: 0,
        began,
        promoted: None,
        promotion_position: [0.0; 2],
        promotion_sample: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let focus = {
        let check = app.world().resource::<Check>();
        let battle = app.world().resource::<Match>();
        let carrier = Vec2::from_array(battle.simulation.entities[check.setup.carrier].position);
        let target = Vec2::from_array(battle.simulation.entities[check.setup.target].position);
        (carrier + target) * 0.05
    };
    app.world_mut().resource_mut::<CameraRig>().focus = focus;
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct Presentation<'w, 's> {
    data: Res<'w, UiData>,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static ViewVisibility)>,
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
    veterancy: VeterancyInspection<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    presentation: Presentation,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == 3 || !capture.ready_for_scenario() {
        return;
    }
    let result =
        presentation.inspect_phase(&mut check, &mut battle, &mut mouse, &mut time, &mut capture);
    if let Err(error) = result {
        check.phase = 3;
        error!("Troop Crawler native acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl Presentation<'_, '_> {
    fn inspect_phase(
        self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<(), Box<dyn Error>> {
        let visible = |id| {
            self.visuals.iter().any(|(visual, visibility)| {
                visual.generation == battle.generation && visual.index == id && visibility.get()
            })
        };
        match check.phase {
            0 => {
                if !visible(check.setup.carrier) {
                    return Ok(());
                }
                require(
                    check.setup.riders.iter().all(|&id| !visible(id)),
                    "enclosed initial Red Guards were rendered",
                )?;
                for slot in 1..=8 {
                    let id = battle.simulation.passengers(check.setup.carrier)[slot - 1];
                    let image =
                        &self.data.units[battle.simulation.entities[id].type_index].button_image;
                    let expected = self.data.icon(image);
                    require(
                        self.buttons.iter().any(|(action, node, material)| {
                            matches!(action, Action::Slot(index) if *index == slot)
                                && node.size().min_element() > 0.0
                                && self.icons.get(&material.0).is_some_and(|icon| {
                                    icon.image == expected.image && icon.uv_rect == expected.uv_rect
                                })
                        }),
                        "crawler initial infantry portrait was not painted",
                    )?;
                }
                require(
                    input::context_order(
                        battle,
                        None,
                        Some(PickTarget::Unit(check.setup.target)),
                        mouse,
                    ),
                    "original crawler attack context command rejected",
                )?;
                time.unpause();
                check.phase = 1;
                println!("Crawler original eight-portrait inventory and context attack passed");
            }
            1 => {
                if battle.simulation.tick - check.began > 3000 {
                    return Err("native crawler assault timed out".into());
                }
                if battle.simulation.entities[check.setup.target].alive() {
                    return Ok(());
                }
                let promoted = hold_promoted(battle, &check.setup)?;
                check.promoted = Some(promoted);
                check.promotion_position = battle.simulation.entities[promoted].position;
                check.phase = 2;
            }
            2 => {
                let promoted = check
                    .promoted
                    .ok_or("crawler promotion actor was not retained")?;
                let actor = &battle.simulation.entities[promoted];
                require(
                    actor.selectable(&battle.simulation.catalog[actor.type_index])
                        && battle.selected.as_slice() == [promoted],
                    &format!(
                        "promoted Red Guard lost its ordinary Stop selection: id={promoted} alive={} contained={:?} selected={:?}",
                        actor.alive(),
                        actor.contained_by(),
                        battle.selected
                    ),
                )?;
                require(
                    visible(check.setup.carrier) && visible(promoted),
                    "original crawler or promoted deployed Red Guard was not visible",
                )?;
                self.veterancy.badge(battle, promoted)?;
                self.veterancy.no_badge(battle, check.setup.target)?;
                let sample = self.veterancy.promotion(battle, check.promotion_position)?;
                if let Some(previous) = check.promotion_sample {
                    if sample.0 == previous.0 || sample.1.y <= previous.1.y {
                        return Ok(());
                    }
                } else {
                    check.promotion_sample = Some(sample);
                    return Ok(());
                }
                time.pause();
                println!(
                    "Rendered Troop Crawler: original model, eight concealed starting Red Guards, original inventory, context-triggered assault, naturally promoted infantry, native SCVeter insignia and advancing/rising LevelGainedAnimation passed"
                );
                capture.finish_scenario();
                check.phase = 3;
            }
            _ => {}
        }
        Ok(())
    }
}

// An explicit player command removes this infantry from AssaultTransportAI's
// member list before its next automatic post-kill recall. Keep its stable ID;
// UI selection is mutable state, not ownership of the promotion event.
pub fn hold_promoted(battle: &mut Match, setup: &crawler::Setup) -> Result<usize, Box<dyn Error>> {
    let promoted = setup
        .riders
        .iter()
        .copied()
        .find(|&id| battle.simulation.entities[id].veterancy > 0)
        .ok_or("paid Ranger kill did not promote an original Red Guard")?;
    require(
        battle.simulation.entities[promoted].selectable(
            &battle.simulation.catalog[battle.simulation.entities[promoted].type_index],
        ),
        "naturally promoted Red Guard was not exposed for the player Stop handoff",
    )?;
    battle.simulation.stop(promoted)?;
    battle.select_objects(&[promoted], true);
    Ok(promoted)
}
