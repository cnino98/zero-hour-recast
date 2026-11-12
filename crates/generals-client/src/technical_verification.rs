//! Paid original Technical/salvage gameplay, command controls and live draw acceptance.
mod framing;
use crate::{
    control_bar::{CommandAction, UiData},
    input::{self, Mouse, PickTarget},
    model_animation::{Animations, Players},
    playtest::Match,
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::{Capture, UnitVisual, WorldCamera},
    projectile_render::ProjectileVisual,
    skirmish_verification::{advance_until, require, technical},
};
use bevy::{app::AppExit, prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: technical::Capture,
    crate_id: usize,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = technical::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.technicals[0]], true);
    let data = app.world().resource::<UiData>();
    for (slot, name) in [
        (1, "Command_TransportExit"),
        (10, "Command_Evacuate"),
        (11, "Command_AttackMove"),
        (13, "Command_Guard"),
        (14, "Command_Stop"),
    ] {
        require(
            selected_command(battle, data, slot).is_some_and(|command| command.name == name),
            &format!("Technical original slot {slot} requires {name}"),
        )?;
    }
    let mut mouse = app
        .world_mut()
        .remove_resource::<Mouse>()
        .ok_or("pointer resource absent")?;
    dispatch_command(battle, &mut mouse, CommandAction::ExitContainer(0));
    advance_until(
        &mut battle.simulation,
        300,
        "Technical original inventory exit",
        |game| game.entities[setup.rider].contained_by().is_none(),
    )?;
    battle.select_objects(&[setup.rider], true);
    require(
        input::context_order(
            battle,
            None,
            Some(PickTarget::Unit(setup.technicals[0])),
            &mut mouse,
        ),
        "Technical context boarding failed",
    )?;
    advance_until(
        &mut battle.simulation,
        3000,
        "Technical original context re-entry",
        |game| game.entities[setup.rider].contained_by() == Some(setup.technicals[0]),
    )?;
    println!(
        "Technical original controls and paid passenger exit/re-entry passed at frame {}",
        battle.simulation.tick
    );
    let crate_id = technical::capture_combat(&mut battle.simulation, &setup)
        .map_err(|error| format!("Technical visible combat: {error}"))?;
    battle.select_objects(&[setup.technicals[0]], true);
    verify_salvage_input(battle, &mut mouse, crate_id)?;
    mouse.voices.clear();
    app.insert_resource(mouse).insert_resource(Check {
        setup,
        crate_id,
        done: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        Startup,
        crate::playtest_view::crate_verification::verify.after(crate::playtest_view::setup),
    );
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    Ok(())
}
fn verify_salvage_input(
    battle: &mut Match,
    mouse: &mut Mouse,
    crate_id: usize,
) -> Result<(), Box<dyn Error>> {
    mouse.hovered = Some(crate_id);
    require(
        crate::attack_cursor::context(battle, mouse) == Some(9),
        "Salvage hint lacks original Move cursor",
    )?;
    let first = mouse.voices.len();
    require(
        input::context_order(battle, None, Some(PickTarget::Unit(crate_id)), mouse),
        "Original salvage context command failed",
    )?;
    require(
        mouse
            .voices
            .get(first)
            .is_some_and(|voice| matches!(voice.voice(), crate::source_audio::Voice::Salvage)),
        "Salvage context did not request original VoiceSalvage",
    )?;
    require(
        battle.selected.iter().all(|&id| {
            matches!(
                battle.simulation.entities[id].ai.order,
                generals_sim::skirmish::Order::Move(_)
            )
        }),
        "Salvage context did not retain source movement",
    )?;
    dispatch_command(battle, mouse, CommandAction::Stop);
    require(
        battle.simulation.entities[crate_id].crate_present(),
        "Unadvanced salvage command collected crate without contact",
    )?;
    mouse.hovered = None;
    println!(
        "Installed salvage context: original Move cursor, crate command, VoiceSalvage request and Stop cancellation passed"
    );
    Ok(())
}

pub fn frame_camera(app: &mut App) {
    framing::frame(app);
}
#[derive(bevy::ecs::system::SystemParam)]
struct Presentation<'w, 's> {
    visuals: Query<
        'w,
        's,
        (
            &'static UnitVisual,
            &'static Players,
            &'static Transform,
            &'static Children,
        ),
    >,
    meshes: Query<
        'w,
        's,
        (
            &'static ViewVisibility,
            Option<&'static crate::model_animation::AnimatedPart>,
        ),
        With<Mesh3d>,
    >,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    animations: Res<'w, Animations>,
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
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
}
fn inspect(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    mut view: Presentation,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.done || !capture.ready_for_scenario() {
        return;
    }
    let battle = battle.into_inner();
    match verify(&check, battle, &mut view) {
        Ok(()) => {
            check.done = true;
            capture.finish_scenario();
            println!(
                "Rendered Technical salvage: original three weapon/gunner states, attached live gunner geometry, Marauder second upgrade, original crate model/science visibility, original RPG projectile and transport inventory passed"
            );
        }
        Err(message) => {
            check.done = true;
            error!("Technical native validation: {message}");
            exit.write(AppExit::error());
        }
    }
}
fn verify(check: &Check, battle: &Match, view: &mut Presentation) -> Result<(), Box<dyn Error>> {
    let game = &battle.simulation;
    let present = |id| {
        view.visuals
            .iter()
            .find(|(visual, _, _, _)| visual.index == id && visual.generation == battle.generation)
    };
    let (camera, transform) = *view.camera;
    let viewport = camera
        .logical_viewport_rect()
        .ok_or("capture viewport absent")?;
    for id in check
        .setup
        .technicals
        .into_iter()
        .chain([check.setup.marauder, check.crate_id])
    {
        let (_, players, position, children) =
            present(id).ok_or("source salvage object drawable absent")?;
        let projected = camera.world_to_viewport(transform, position.translation);
        let visible = children.iter().any(|child| {
            view.meshes
                .get(child)
                .is_ok_and(|(visible, _)| visible.get())
        });
        require(
            visible
                && projected
                    .as_ref()
                    .is_ok_and(|&point| viewport.contains(point)),
            &format!(
                "salvage object {id} outside capture: world {:?}, projected {projected:?}, viewport {viewport:?}, mesh visible {visible}",
                position.translation
            ),
        )?;
        let actor = &game.entities[id];
        if id != check.crate_id
            && let Some(model) = view.animations.verify_salvage_draw(actor, players)?
        {
            require(
                children.iter().any(|child| {
                    view.meshes.get(child).is_ok_and(|(visible, part)| {
                        visible.get() && part.is_some_and(|part| part.model == model)
                    })
                }),
                "Technical attached gunner has no visible original mesh",
            )?;
        }
    }
    require(
        !view.visuals.iter().any(|(visual, _, _, _)| {
            visual.generation == battle.generation
                && game.catalog[game.entities[visual.index].type_index].kind
                    == generals_sim::skirmish::Kind::Crate
                && !game.entities[visual.index].crate_present()
        }),
        "consumed salvage crate retained its root drawable",
    )?;
    require(
        game.crate_visible_to(0, check.crate_id),
        "original GLA science crate visibility failed",
    )?;
    require(
        game.entities[check.setup.rider].contained_by() == Some(check.setup.technicals[0]),
        "Technical passenger left original enclosing transport",
    )?;
    verify_inventory(check, battle, view)?;
    require(
        view.projectiles.iter().any(|(visual, children)| {
            children.iter().any(|child| {
                view.meshes
                    .get(child)
                    .is_ok_and(|(visible, _)| visible.get())
            }) && visual.generation == battle.generation
                && game.projectiles.iter().any(|projectile| {
                    projectile.id == visual.id
                        && projectile.source == check.setup.technicals[2]
                        && projectile.weapon_index == 4
                })
        }),
        "source Technical RPG projectile mesh absent",
    )?;
    Ok(())
}

fn verify_inventory(
    check: &Check,
    battle: &Match,
    view: &Presentation,
) -> Result<(), Box<dyn Error>> {
    let rider = &battle.simulation.entities[check.setup.rider];
    for slot in 1..=5 {
        let image = if slot == 1 {
            &view.data.units[rider.type_index].button_image
        } else {
            // Installed GLA ControlBarScheme::CommandMarkerImage; the command
            // itself supplies SSGenericBuilding before the native scheme override.
            "SUEmptyFrame"
        };
        let expected = view.data.icon(image);
        let disabled = if slot == 1 { 0.0_f32 } else { 1.0 };
        require(
            view.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(value) if *value == slot)
                    && node.display != Display::None
                    && view.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image
                            && icon.uv_rect == expected.uv_rect
                            && icon.state.x.to_bits() == disabled.to_bits()
                    })
            }),
            &format!("Technical inventory slot {slot} lacks original image/disabled state"),
        )?;
    }
    Ok(())
}
