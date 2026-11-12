//! Paid original disable command, live beam, binary shower and advancing audio.
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, CommandMode, Mouse},
    laser_beam::verification::HackerBeamInspection,
    particles::ParticleInspection,
    playtest::Match,
    playtest_ui::{Action, PromotionIcon, dispatch_command},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
    source_audio::HackerAudioInspection,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    actor: usize,
    target: usize,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let [actor, target] = crate::income_verification::disable::prepare(&mut battle.simulation)?;
    battle.select_objects(&[actor], true);
    let data = app.world().resource::<UiData>();
    let commands = &data.units[battle.simulation.entities[actor].type_index].commands;
    require(
        commands.iter().any(|command| {
            command.slot == 1
                && command.image == "SSBuildingHack"
                && matches!(command.action, CommandAction::DisableBuilding)
        }),
        "Hacker lacks original Disable Building slot and artwork",
    )?;
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("pointer state missing")?;
    mixed_context(battle, &mut mouse, actor, target)?;
    dispatch_command(battle, &mut mouse, CommandAction::DisableBuilding);
    require(
        matches!(mouse.pending_command, CommandMode::DisableBuilding),
        "Disable Building button did not enter its original target mode",
    )?;
    mouse.hovered = Some(target);
    require(
        crate::attack_cursor::context(battle, &mouse) == Some(6),
        "Eligible original structure lacks Hack cursor",
    )?;
    mouse.hovered = Some(actor);
    require(
        crate::attack_cursor::context(battle, &mouse) == Some(3),
        "Friendly infantry incorrectly accepts Disable Building",
    )?;
    mouse.hovered = Some(target);
    require(
        input::hacker::issue(battle, &mut mouse, target, false),
        "Original targeted disable command was rejected",
    )?;
    require(
        matches!(mouse.pending_command, CommandMode::Normal),
        "Accepted disable command retained its target mode",
    )?;
    app.insert_resource(mouse).insert_resource(Check {
        actor,
        target,
        done: false,
    });
    println!(
        "Installed Hacker UI: paid construction, original slot 1 artwork, valid Hack/invalid friendly cursor and native explicit target dispatch passed"
    );
    Ok(())
}

fn mixed_context(
    battle: &mut Match,
    mouse: &mut Mouse,
    actor: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    let builder = crate::skirmish_verification::friendly_entity(
        &battle.simulation,
        generals_sim::skirmish::Kind::Builder,
        0,
    )?;
    battle.select_objects(&[builder, actor], true);
    mouse.hovered = Some(target);
    require(
        crate::attack_cursor::context(battle, mouse) == Some(6)
            && input::hacker::context_target(battle, mouse, Some(target)) == Some(target),
        "Mixed selection must hint Hack from any eligible Hacker",
    )?;
    require(
        !input::hacker::issue(battle, mouse, target, true),
        "Context Hack must use the first selected object's command set",
    )?;
    battle.select_objects(&[actor], true);
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let battle = app.world().resource::<Match>();
    let focus = (Vec2::from_array(battle.simulation.entities[check.actor].position)
        + Vec2::from_array(battle.simulation.entities[check.target].position))
        * 0.05;
    {
        let mut rig = app.world_mut().resource_mut::<crate::playtest::CameraRig>();
        rig.focus = focus;
        rig.distance = rig.settings.min_height;
        rig.settings.yaw += std::f32::consts::PI;
    }
    app.world_mut()
        .resource_mut::<Capture>()
        .wait_for_scenario();
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w, 's> {
    capture: ResMut<'w, Capture>,
    clock: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    exit: MessageWriter<'w, AppExit>,
    beam: HackerBeamInspection<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    audio: HackerAudioInspection<'w, 's>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Presentation<'w, 's> {
    data: Res<'w, UiData>,
    icons: Res<'w, Assets<PromotionIcon>>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static Node,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Visibility)>,
}

fn inspect(
    battle: Res<Match>,
    presentation: Presentation,
    mut check: ResMut<Check>,
    mut completion: Completion,
) {
    let battle = battle.into_inner();
    let Presentation {
        data,
        icons,
        buttons,
        visuals,
    } = presentation;
    let data = data.into_inner();
    let icons = icons.into_inner();
    if check.done {
        return;
    }
    if completion.real.elapsed_secs() > 75.0 {
        check.done = true;
        error!(
            "Hacker disable capture timed out before native UI, beam, particles and audio were present"
        );
        completion.exit.write(AppExit::error());
        return;
    }
    if !completion.capture.ready_for_scenario() {
        return;
    }
    let game = &battle.simulation;
    let actor = &game.entities[check.actor];
    if actor.hacker_disable_target() != Some(check.target)
        || !game.model_flags(actor, game.catalog[actor.type_index].body_damage_thresholds).contains("FIRING_A")
        || !game.entities[check.target].hacked()
        || !completion.audio.running(check.actor, &game.catalog[actor.type_index].behavior.special_abilities.iter().find(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialHackerDisableBuilding)).expect("installed hacker ability").feedback.sounds[2])
    {
        return;
    }
    let expected = data.icon("SSBuildingHack");
    let button = buttons.iter().any(|(action, node, material)| {
        matches!(action, Action::Slot(1))
            && node.display != Display::None
            && icons.get(&material.0).is_some_and(|icon| {
                icon.image == expected.image
                    && icon.uv_rect == expected.uv_rect
                    && icon.state.x < 0.5
            })
    });
    let objects = [check.actor, check.target].into_iter().all(|id| {
        visuals.iter().any(|(visual, visibility)| {
            visual.index == id
                && visual.generation == battle.generation
                && *visibility == Visibility::Visible
        })
    });
    if !button || !objects {
        return;
    }
    let result = completion.beam.visible(check.actor).and_then(|visible| {
        if visible {
            completion
                .particles
                .disabled(check.target, battle.generation)
        } else {
            Ok(false)
        }
    });
    match result {
        Ok(true) => {
            check.done = true;
            completion.clock.pause();
            completion.capture.finish_scenario();
            println!(
                "Rendered Hacker Disable Building: original command artwork, FIRING_A model condition, live additive BinaryDataStream arc, attached master/slave binary shower, disabled producer and advancing native preparation-loop audio sink passed"
            );
        }
        Ok(false) => {}
        Err(error) => {
            check.done = true;
            error!("Hacker rendered diagnostic failed: {error}");
            completion.exit.write(AppExit::error());
        }
    }
}
