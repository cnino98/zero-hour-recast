//! Original Guard button, terrain/radar targeting, accepted order and cancellation.
use super::{RadiusCursor, RadiusVisual};
use crate::{
    control_bar::{CommandAction, UiData},
    input::{CommandMode, Mouse},
    playtest::Match,
    playtest_ui::Action,
    playtest_view::{Capture, WorldCamera},
    radar::Radar,
    skirmish_verification::{construct, friendly_entity, produce},
    source_material::SourceMaterial,
    terrain::Terrain,
};
use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    window::PrimaryWindow,
};
use generals_sim::skirmish::{Faction, Kind, Order};
use std::error::Error;

#[derive(Resource)]
pub struct Scenario {
    actor: usize,
    phase: u8,
    ground: Option<Vec2>,
}

pub fn prepare(battle: &mut Match) -> Result<Scenario, Box<dyn Error>> {
    battle.start(Faction::Usa);
    let builder = friendly_entity(&battle.simulation, Kind::Builder, 0)?;
    construct(&mut battle.simulation, builder, Kind::PowerPlant, 0)?;
    let barracks = construct(&mut battle.simulation, builder, Kind::Barracks, 0)?;
    let actor = produce(&mut battle.simulation, barracks, Kind::Infantry, 0)?;
    battle.select_objects(&[actor], true);
    Ok(Scenario {
        actor,
        phase: 0,
        ground: None,
    })
}

pub fn register(app: &mut App, scenario: Scenario) {
    app.world_mut()
        .resource_mut::<Capture>()
        .wait_for_scenario();
    app.insert_resource(scenario)
        .add_systems(Update, update.after(super::update));
}

#[derive(SystemParam)]
struct Scene<'w, 's> {
    battle: ResMut<'w, Match>,
    mouse: Res<'w, Mouse>,
    data: Res<'w, UiData>,
    radar: Res<'w, Radar>,
    terrain: Res<'w, Terrain>,
    cursor: Res<'w, RadiusCursor>,
    pointer: Single<'w, 's, &'static Node, With<crate::attack_cursor::AttackPointer>>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static Node,
            &'static UiGlobalTransform,
            &'static Interaction,
        ),
    >,
    window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    visual: Single<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static Visibility,
        ),
        With<RadiusVisual>,
    >,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    images: Res<'w, Assets<Image>>,
    events: MessageWriter<'w, MouseButtonInput>,
}

fn update(
    mut scenario: ResMut<Scenario>,
    mut scene: Scene,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Time<Virtual>>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let time = time.into_inner();
    if scenario.phase > 20 {
        return;
    }
    if time.elapsed_secs() > 90.0 {
        error!(
            "Guard radius input/render verification timed out at phase {}",
            scenario.phase
        );
        exit.write(AppExit::error());
        scenario.phase = 21;
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    clock.pause();
    match step(&mut scenario, &mut scene) {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            error!(
                "Guard radius verification phase {}: {error}",
                scenario.phase
            );
            exit.write(AppExit::error());
            scenario.phase = 21;
            return;
        }
    }
    scenario.phase += 1;
    if scenario.phase == 21 {
        println!(
            "Original Guard radius: paid Ranger, real original button, source terrain decal/texture/pulse, opaque HUD suppression, radar projection, accepted Guard order and right-click cancellation passed"
        );
        capture.finish_scenario();
    }
}

fn step(scenario: &mut Scenario, scene: &mut Scene) -> Result<bool, &'static str> {
    match scenario.phase {
        0 | 9 | 16 => {
            if scenario.phase == 9 {
                hidden(scene)?;
                accepted(scene, scenario)?;
            }
            aim_guard(scene, scenario.actor)?;
        }
        1 | 10 | 17 => {
            if !aim_guard(scene, scenario.actor)? {
                return Ok(false);
            }
            button(scene, MouseButton::Left, ButtonState::Pressed);
        }
        2 | 8 | 11 | 18 => button(scene, MouseButton::Left, ButtonState::Released),
        3 | 12 | 19 => {
            if !matches!(scene.mouse.pending_command, CommandMode::Guard) {
                return Err("original Guard button did not activate targeting");
            }
            move_ground_pointer(scene, scenario)?;
        }
        4 => {
            visible(
                scene,
                scenario.actor,
                scenario.ground.ok_or("no terrain target")?,
            )?;
            move_source_pointer(scene, Vec2::new(760.0, 570.0));
        }
        5 => {
            hidden(scene)?;
            let point = scene.data.windows["ControlBar.wnd:LeftHUD"].center();
            move_source_pointer(scene, point);
        }
        6 => {
            let point = scene
                .radar
                .mouse_world(
                    &scene.data,
                    &scene.window.1,
                    scene
                        .window
                        .1
                        .cursor_position()
                        .ok_or("missing radar pointer")?,
                )
                .ok_or("radar projection missed the original HUD")?;
            visible(scene, scenario.actor, point)?;
            move_ground_pointer(scene, scenario)?;
        }
        7 => {
            visible(
                scene,
                scenario.actor,
                scenario.ground.ok_or("missing terrain point")?,
            )?;
            button(scene, MouseButton::Left, ButtonState::Pressed);
        }
        13 => {
            visible(
                scene,
                scenario.actor,
                scenario.ground.ok_or("missing terrain point")?,
            )?;
            button(scene, MouseButton::Right, ButtonState::Pressed);
        }
        14 => button(scene, MouseButton::Right, ButtonState::Released),
        15 => {
            hidden(scene)?;
            if !matches!(scene.mouse.pending_command, CommandMode::Normal) {
                return Err("right click did not cancel Guard targeting");
            }
        }
        20 => visible(
            scene,
            scenario.actor,
            scenario.ground.ok_or("missing terrain target")?,
        )?,
        _ => unreachable!("bounded guard input sequence"),
    }
    Ok(true)
}

fn aim_guard(scene: &mut Scene, actor: usize) -> Result<bool, &'static str> {
    let command = scene
        .data
        .commands(&scene.battle.simulation, actor)
        .iter()
        .find(|command| matches!(command.action, CommandAction::Guard))
        .ok_or("Ranger has no original Guard command")?;
    let (_, node, transform, interaction) = scene
        .buttons
        .iter()
        .find(|(action, _, _, _)| matches!(action, Action::Slot(slot) if *slot == command.slot))
        .ok_or("Guard command button missing")?;
    if node.display == Display::None {
        return Err("Original Guard command button is hidden");
    }
    let point = transform.translation / scene.window.1.scale_factor();
    let authored =
        scene.data.windows[&format!("ControlBar.wnd:ButtonCommand{:02}", command.slot)].center();
    let scale = scene.window.1.size() / Vec2::new(800.0, 600.0);
    if point.distance(authored * scale) > 1.0 {
        error!(
            "Guard slot {} authored {authored:?}, layout {point:?}, window {:?}, scale factor {}",
            command.slot,
            scene.window.1.size(),
            scene.window.1.scale_factor()
        );
        return Err("Guard button layout differs from its original WND rectangle");
    }
    let ready = *interaction == Interaction::Hovered
        && scene
            .window
            .1
            .cursor_position()
            .is_some_and(|position| position.distance(point) < 1.0);
    if !ready {
        scene.window.1.set_cursor_position(Some(point));
    }
    Ok(ready)
}

fn move_source_pointer(scene: &mut Scene, point: Vec2) {
    let window = &mut scene.window.1;
    let scale = Vec2::new(window.width() / 800.0, window.height() / 600.0);
    window.set_cursor_position(Some(point * scale));
}

fn move_ground_pointer(scene: &mut Scene, scenario: &mut Scenario) -> Result<(), &'static str> {
    let point = scene.window.1.size() * Vec2::new(0.5, 0.35);
    let ray = scene
        .camera
        .0
        .viewport_to_world(scene.camera.1, point)
        .map_err(|_| "guard screen ray missing")?;
    scenario.ground = scene.terrain.ray_point(ray).map(|point| point * 10.0);
    if scenario.ground.is_none() {
        return Err("guard cursor has no terrain intersection");
    }
    scene.window.1.set_cursor_position(Some(point));
    Ok(())
}

fn accepted(scene: &Scene, scenario: &Scenario) -> Result<(), &'static str> {
    let order = scene.battle.simulation.entities[scenario.actor].ai.order;
    if matches!(scene.mouse.pending_command, CommandMode::Normal)
        && matches!(order, Order::Guard(point) if scenario.ground.is_some_and(|expected| Vec2::from_array(point).distance(expected) < 0.001))
    {
        return Ok(());
    }
    let pending = match scene.mouse.pending_command {
        CommandMode::Normal => "Normal",
        CommandMode::Guard => "Guard",
        CommandMode::GuardFlying => "GuardFlying",
        CommandMode::Rally => "Rally",
        CommandMode::AttackMove => "AttackMove",
        CommandMode::CombatDrop => "CombatDrop",
        CommandMode::Capture => "Capture",
        CommandMode::LaserLock => "LaserLock",
        CommandMode::DisableBuilding => "DisableBuilding",
        CommandMode::Hack(_) => "Hack",
        CommandMode::Cleanup => "Cleanup",
        CommandMode::FireWeapon(_) => "FireWeapon",
        CommandMode::ClearMines(_) => "ClearMines",
        CommandMode::Hijack => "Hijack",
        CommandMode::HelixBomb => "HelixBomb",
        CommandMode::CarBomb => "CarBomb",
        CommandMode::Sabotage => "Sabotage",
        CommandMode::SnipeVehicle(_) => "SnipeVehicle",
        CommandMode::Burton(_) => "Burton",
        CommandMode::BombTruckDisguise => "BombTruckDisguise",
        CommandMode::Power { .. } => "Power",
    };
    let (name, destination) = match order {
        Order::Guard(point) => ("Guard", Some(point)),
        Order::Move(point) => ("Move", Some(point)),
        Order::AttackMove(point) => ("AttackMove", Some(point)),
        Order::Exit(point) => ("Exit", Some([point[0], point[1]])),
        Order::Idle => ("Idle", None),
        Order::Attack(_) => ("Attack", None),
        Order::AttackArea => ("AttackArea", None),
        Order::Build(_) => ("Build", None),
        Order::Repair(_) => ("Repair", None),
        Order::RepairAt(_) => ("RepairAt", None),
        Order::Enter(_) => ("Enter", None),
        Order::ExitContainer => ("ExitContainer", None),
        Order::HackInternet => ("HackInternet", None),
        Order::FollowPath => ("FollowPath", None),
        Order::Harvest(_) => ("Harvest", None),
        Order::ReturnSupply => ("ReturnSupply", None),
        Order::CleanHazard(_) => ("CleanHazard", None),
        Order::FireAt {
            position: point, ..
        } => ("FireAt", Some(point)),
    };
    error!(
        "Guard accepted click: pending={pending}, order={name}, destination={destination:?}, raw target={:?}, cursor={:?}",
        scenario.ground,
        scene.window.1.cursor_position()
    );
    Err("accepted click failed to clear targeting or issue the original Guard order")
}

fn button(scene: &mut Scene, button: MouseButton, state: ButtonState) {
    scene.events.write(MouseButtonInput {
        button,
        state,
        window: scene.window.0,
    });
}

fn hidden(scene: &Scene) -> Result<(), &'static str> {
    if *scene.visual.2 != Visibility::Hidden || scene.cursor.radius.is_some() {
        return Err("radius decal remained visible outside the native targeting context");
    }
    Ok(())
}

fn visible(scene: &mut Scene, actor: usize, expected: Vec2) -> Result<(), &'static str> {
    let cursor = &scene.cursor;
    let material = scene
        .materials
        .get(&cursor.styles[0].material)
        .ok_or("radius material missing")?;
    let mesh = scene
        .meshes
        .get(&cursor.mesh)
        .ok_or("radius mesh missing")?;
    let image = material
        .texture
        .as_ref()
        .and_then(|handle| scene.images.get(handle))
        .ok_or("radius texture missing")?;
    if *scene.visual.2 == Visibility::Hidden
        || scene.pointer.display != Display::Flex
        || crate::attack_cursor::context(&mut scene.battle, &scene.mouse) != Some(5)
        || scene.visual.0.0 != cursor.mesh
        || scene.visual.1.0 != cursor.styles[0].material
        || cursor.position.distance(expected) >= 0.001
        || cursor.radius.map(f32::to_bits)
            != Some(scene.battle.simulation.guard_radius(actor).to_bits())
        || cursor.styles[0].template.texture != "SCCGuard_Generic"
        || material.parameters.vertex_diffuse != 3
        || material.depth_write
        || image.width() == 0
        || mesh.indices().is_none_or(|indices| indices.len() < 6)
        || (material.parameters.opacity
            - cursor.styles[0]
                .template
                .opacity(scene.battle.simulation.tick))
        .abs()
            > f32::EPSILON
    {
        return Err(
            "radius texture, geometry, pulse, original selected-unit range or projected position differs",
        );
    }
    Ok(())
}
