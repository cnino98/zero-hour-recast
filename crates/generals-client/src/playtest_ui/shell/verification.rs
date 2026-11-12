//! Bounded native traversal through actual shell, chooser, error and load controls.
use crate::{
    map_selection::{Admission, MapSelection},
    playtest::{Match, startup::Shell},
    playtest_ui::{Action, Menu},
    playtest_view::Capture,
};
use bevy::{
    input::{
        ButtonState,
        mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    window::PrimaryWindow,
};

#[derive(Resource)]
struct Check {
    stage: u8,
    phase: u8,
    began: Option<f64>,
    build: Option<BuildCheck>,
}

struct BuildCheck {
    builder: usize,
    product: usize,
    slot: usize,
    point: Option<Vec2>,
    site: Option<Vec2>,
    cash: u32,
    foundation: Option<usize>,
}

pub fn register(app: &mut App) {
    app.insert_resource(Check {
        stage: 0,
        phase: 0,
        began: None,
        build: None,
    });
    app.world_mut()
        .resource_mut::<Capture>()
        .wait_for_scenario();
    app.add_systems(First, step);
}

fn step(world: &mut World) {
    world.resource_scope(|world, mut check: Mut<Check>| {
        if check.stage == 26 {
            return;
        }
        if let Err(reason) = advance(world, &mut check) {
            error!(
                "Native shell flow failed: stage={} phase={}: {reason}",
                check.stage, check.phase
            );
            check.stage = 26;
            world.write_message(AppExit::error());
        }
    });
}

fn next(check: &mut Check) {
    check.stage += 1;
    check.phase = 0;
    eprintln!("Native shell flow: stage={}", check.stage);
}

fn snapshot(world: &mut World, check: &mut Check, name: &'static str) {
    world.resource_mut::<Capture>().intermediate(name);
    next(check);
}

fn advance(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    if now - *check.began.get_or_insert(now) > 180.0 {
        return Err("shell traversal timed out");
    }
    if check.stage <= 18 && world.contains_resource::<Match>() {
        return Err("a gameplay world was installed before the accepted Start");
    }
    if !world.resource::<Capture>().ready_for_scenario() {
        return Ok(());
    }
    match check.stage {
        0 => snapshot(world, check, "shell-flow-menu.png"),
        1 | 3 | 4 | 5 | 7 | 9 | 10 | 12 | 13 | 15 | 17 => {
            click_stage(world, check)?;
        }
        2 if world.resource::<Menu>().options_open => {
            snapshot(world, check, "shell-flow-options.png");
        }
        6 if world.contains_resource::<MapSelection>() => {
            world.resource_mut::<Menu>().factions[0] = Some(generals_sim::skirmish::Faction::Usa);
            if world
                .resource::<MapSelection>()
                .maps
                .iter()
                .any(|entry| !matches!(entry.admission, Admission::Unknown))
            {
                return Err("metadata-only setup admitted a map");
            }
            snapshot(world, check, "shell-flow-setup.png");
        }
        8 | 14 => {
            let path = if check.stage == 8 {
                "maps/thefrontline/thefrontline.map".to_owned()
            } else {
                crate::playtest::MAP_NAME.replace('\\', "/").to_lowercase()
            };
            if select_map(world, check, &path)? {
                next(check);
            }
        }
        11 if world.resource::<Menu>().confirmation
            == Some(super::super::menu::Confirmation::MapUnavailable) =>
        {
            if world.resource::<Shell>().loading() {
                return Err("rejected map retained a worker");
            }
            let maps = world.resource::<MapSelection>();
            if !maps.maps[maps.current]
                .admission
                .rejection()
                .is_some_and(|reason| reason.contains("unsupported second texture stage"))
            {
                return Err("original invalid-map error did not reach the dialog");
            }
            snapshot(world, check, "shell-flow-error.png");
        }
        16 if !world.resource::<MapSelection>().open => {
            snapshot(world, check, "shell-flow-selected.png");
        }
        18 if super::loading::ready(world) => {
            eprintln!("SHELL_FLOW_LOADING original_layout=MultiplayerLoadScreen.wnd");
            snapshot(world, check, "shell-flow-loading.png");
        }
        19 => finish(world, check)?,
        20..=25 => advance_build(world, check)?,
        _ => {}
    }
    Ok(())
}

fn finish(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let Some(battle) = world.get_resource::<Match>() else {
        return Ok(());
    };
    if !battle.started {
        return Err("accepted map did not start its real configured match");
    }
    let resolved = world
        .resource::<Shell>()
        .loading_slots()
        .ok_or("original loading slots absent")?;
    if battle.simulation.player_colors != resolved.colors
        || battle.simulation.start_positions != resolved.start_positions
        || battle
            .simulation
            .players
            .iter()
            .zip(&resolved.factions)
            .any(|(player, &faction)| player.faction != faction)
    {
        return Err("original loading choices differ from the started match");
    }
    let maps = world.resource::<MapSelection>();
    if !maps.maps[maps.current].admission.is_verified() {
        return Err("real match lacks strict admission");
    }
    if world
        .query_filtered::<Entity, With<crate::playtest_view::WorldCamera>>()
        .iter(world)
        .next()
        .is_none()
    {
        return Err("committed match lacks its original world camera");
    }
    let models = world
        .query_filtered::<Entity, With<crate::model_animation::AnimatedPart>>()
        .iter(world)
        .count();
    if models == 0 {
        return Ok(());
    }
    eprintln!(
        "SHELL_FLOW_RENDERED actual_mouse=true options=true metadata_without_match=true rejected_map_dialog=true rejected_map_retry=true original_loading_screen=true resolved_slots=true original_models={models}"
    );
    next(check);
    Ok(())
}

fn advance_build(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    match check.stage {
        20 => prepare_build(world, check)?,
        21 => {
            let slot = check
                .build
                .as_ref()
                .ok_or("missing fresh builder check")?
                .slot;
            if click(
                world,
                &mut check.phase,
                |action| matches!(action, Action::Slot(index) if *index == slot),
            )? {
                next(check);
            }
        }
        22 => plan_build(world, check)?,
        23 => click_build_ground(world, check)?,
        24 => {
            verify_build(world, check)?;
            snapshot(world, check, "shell-flow-paid-build.png");
        }
        25 => {
            let id = check
                .build
                .as_ref()
                .and_then(|build| build.foundation)
                .ok_or("missing paid foundation")?;
            let game = &world.resource::<Match>().simulation;
            if !game.entities[id].alive() {
                return Err("paid Power Plant was destroyed before completion");
            }
            if game.entities[id].complete() {
                eprintln!(
                    "SHELL_FLOW_BUILD_FINISHED template=AmericaPowerPlant object={id} tick={}",
                    game.tick
                );
                world.resource_mut::<Capture>().finish_scenario();
                next(check);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn prepare_build(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let builder = {
        let mut battle = world.resource_mut::<Match>();
        if battle.simulation.players[0].faction != generals_sim::skirmish::Faction::Usa {
            return Err("fresh shell human is not USA");
        }
        let builder = battle
            .simulation
            .entities
            .iter()
            .position(|unit| {
                unit.owner == 0
                    && unit.alive()
                    && battle.simulation.catalog[unit.type_index].name == "AmericaVehicleDozer"
            })
            .ok_or("fresh shell lacks owned USA Dozer")?;
        battle.select_objects(&[builder], true);
        builder
    };
    world.resource_scope(
        |world, mut battle: Mut<Match>| -> Result<(), &'static str> {
            let data = world.resource::<crate::control_bar::UiData>();
            let (slot, product) = (1..=14)
                .find_map(|slot| {
                    let command = super::super::selected_command(&battle, data, slot)?;
                    let crate::control_bar::CommandAction::Produce(product) = command.action else {
                        return None;
                    };
                    (battle.simulation.catalog[product].name == "AmericaPowerPlant")
                        .then_some((slot, product))
                })
                .ok_or("fresh USA Dozer lacks original Power Plant command")?;
            let command = super::super::selected_command(&battle, data, slot)
                .ok_or("Power Plant command disappeared")?;
            if !super::super::command_icons::available(&mut battle, command) {
                return Err("fresh USA Power Plant is unavailable");
            }
            check.build = Some(BuildCheck {
                builder,
                product,
                slot,
                point: None,
                site: None,
                foundation: None,
                cash: battle.simulation.players[0].cash,
            });
            Ok(())
        },
    )?;
    next(check);
    Ok(())
}

fn plan_build(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let build = check.build.as_mut().ok_or("missing fresh builder check")?;
    let mut window_query = world.query_filtered::<&Window, With<PrimaryWindow>>();
    let mut camera_query = world
        .query_filtered::<(&Camera, &GlobalTransform), With<crate::playtest_view::WorldCamera>>();
    let battle = world.resource::<Match>();
    let placement = battle
        .placing
        .as_ref()
        .ok_or("real Power Plant HUD click did not enter placement")?;
    if placement.type_index != build.product || battle.selected != [build.builder] {
        return Err("HUD click bound placement to the wrong USA object");
    }
    let terrain = world.resource::<crate::terrain::Terrain>();
    let data = world.resource::<crate::control_bar::UiData>();
    let promotions = world.resource::<super::super::PromotionTransition>();
    let window = window_query
        .single(world)
        .map_err(|_| "primary window missing")?;
    let (camera, transform) = camera_query
        .single(world)
        .map_err(|_| "world camera missing")?;
    let mut nearest: Option<(f32, Vec2, Vec2)> = None;
    let home = Vec2::from_array(battle.simulation.entities[build.builder].position);
    let angle = battle.simulation.catalog[build.product].placement_angle;
    for y in (2_u16..100)
        .map(|value| f32::from(value) * 20.0)
        .take_while(|&value| value < window.height() - 40.0)
    {
        for x in (2_u16..200)
            .map(|value| f32::from(value) * 20.0)
            .take_while(|&value| value < window.width() - 40.0)
        {
            let point = (Vec2::new(x, y) * window.scale_factor())
                .as_ivec2()
                .as_vec2()
                / window.scale_factor();
            if super::super::captures_pointer(battle, window, point, data, promotions) {
                continue;
            }
            let Ok(ray) = camera.viewport_to_world(transform, point) else {
                continue;
            };
            let Some(site) = terrain.ray_point(ray).map(|point| point * 10.0) else {
                continue;
            };
            if battle.simulation.can_build_facing(
                build.builder,
                build.product,
                site.to_array(),
                angle,
            ) {
                let distance = site.distance_squared(home);
                if nearest.is_none_or(|(old, _, _)| distance < old) {
                    nearest = Some((distance, point, site));
                }
            }
        }
    }
    let (_, point, site) = nearest.ok_or("fresh USA has no visible legal reachable build site")?;
    build.point = Some(point);
    build.site = Some(site);
    world
        .query_filtered::<&mut Window, With<PrimaryWindow>>()
        .single_mut(world)
        .map_err(|_| "primary window missing")?
        .set_cursor_position(Some(point));
    next(check);
    Ok(())
}

fn click_build_ground(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let build = check.build.as_mut().ok_or("missing fresh builder check")?;
    if check.phase == 0 {
        let battle = world.resource::<Match>();
        let Some((position, _, legal)) = battle
            .placing
            .as_ref()
            .and_then(crate::input::placement::Placement::pose)
        else {
            return Ok(());
        };
        if !legal || position.distance(build.site.ok_or("missing build site")?) > 0.1 {
            return Err("normal pointer did not show the legal Power Plant preview");
        }
        build.cash = battle.simulation.players[0].cash;
    }
    let (entity, mut window) = world
        .query_filtered::<(Entity, &mut Window), With<PrimaryWindow>>()
        .single_mut(world)
        .map_err(|_| "primary window missing")?;
    window.set_cursor_position(build.point);
    world.write_message(MouseButtonInput {
        button: MouseButton::Left,
        state: if check.phase == 0 {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        window: entity,
    });
    check.phase += 1;
    if check.phase == 2 {
        next(check);
    }
    Ok(())
}

fn verify_build(world: &World, check: &mut Check) -> Result<(), &'static str> {
    let build = check.build.as_mut().ok_or("missing fresh builder check")?;
    let battle = world.resource::<Match>();
    let game = &battle.simulation;
    if battle.placing.is_some() {
        return Err("ordinary ground release did not commit Power Plant placement");
    }
    let site = build.site.ok_or("missing build site")?;
    let mut foundations = game.entities.iter().enumerate().filter(|(_, unit)| {
        unit.alive()
            && unit.owner == 0
            && unit.type_index == build.product
            && Vec2::from_array(unit.position).distance(site) < 0.1
    });
    let (id, unit) = foundations
        .next()
        .ok_or("no Power Plant at the clicked site")?;
    if unit.complete()
        || foundations.next().is_some()
        || build.cash.checked_sub(game.build_cost(0, build.product)) != Some(game.players[0].cash)
    {
        return Err("ordinary USA build did not create one paid human Power Plant foundation");
    }
    build.foundation = Some(id);
    eprintln!(
        "SHELL_FLOW_PAID_BUILD actual_mouse=true faction=USA template=AmericaPowerPlant builder={} site={:?} cost={} cash_before={} cash_after={}",
        build.builder,
        site,
        game.build_cost(0, build.product),
        build.cash,
        game.players[0].cash
    );
    Ok(())
}

fn click_stage(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let stage = check.stage;
    if click(world, &mut check.phase, |action| {
        matches!(
            (stage, action),
            (1, Action::Options)
                | (3, Action::OptionsBack)
                | (4, Action::SinglePlayer)
                | (5, Action::Skirmish)
                | (7 | 13, Action::SelectMap)
                | (9 | 15, Action::MapAccept)
                | (10 | 17, Action::Start)
                | (12, Action::CancelConfirmation)
        )
    })? {
        next(check);
    }
    Ok(())
}

fn select_map(world: &mut World, check: &mut Check, path: &str) -> Result<bool, &'static str> {
    if !world.resource::<MapSelection>().open {
        return Ok(false);
    }
    let wanted = world
        .resource::<MapSelection>()
        .maps
        .iter()
        .filter(|entry| !entry.path.starts_with("user-maps/"))
        .position(|entry| entry.path == path)
        .ok_or("native shell map fixture absent")?;
    let mut rows = world.query::<(&Action, &InheritedVisibility, &ComputedNode)>();
    let visible: Vec<_> = rows
        .iter(world)
        .filter_map(|(action, visible, node)| {
            if let Action::MapRow(row) = action {
                (visible.get() && !node.is_empty()).then_some(*row)
            } else {
                None
            }
        })
        .collect();
    if visible.contains(&wanted) {
        return click(
            world,
            &mut check.phase,
            |action| matches!(action, Action::MapRow(row) if *row == wanted),
        );
    }
    if let Some(first) = visible.iter().min() {
        let area = world.resource::<crate::control_bar::UiData>().windows["SkirmishMapSelectMenu.wnd:ListboxMap"];
        let (entity, mut window) = world
            .query_filtered::<(Entity, &mut Window), With<PrimaryWindow>>()
            .single_mut(world)
            .map_err(|_| "primary window missing")?;
        let point = area.center() * Vec2::new(window.width() / 800.0, window.height() / 600.0);
        window.set_cursor_position(Some(point));
        world.write_message(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: if wanted < *first { 1.0 } else { -1.0 },
            window: entity,
            phase: bevy::input::touch::TouchPhase::Moved,
        });
    }
    Ok(false)
}

fn click(
    world: &mut World,
    phase: &mut u8,
    target: impl Fn(&Action) -> bool,
) -> Result<bool, &'static str> {
    if *phase == 3 {
        *phase = 0;
        return Ok(true);
    }
    let point = world
        .query::<(
            &Action,
            &Interaction,
            &ComputedNode,
            &UiGlobalTransform,
            &InheritedVisibility,
        )>()
        .iter(world)
        .find_map(|(action, interaction, node, transform, visible)| {
            (target(action) && visible.get() && !node.is_empty())
                .then_some((transform.translation, *interaction))
        });
    let Some((point, interaction)) = point else {
        return Ok(false);
    };
    let (entity, mut window) = world
        .query_filtered::<(Entity, &mut Window), With<PrimaryWindow>>()
        .single_mut(world)
        .map_err(|_| "primary window missing")?;
    let scale = window.scale_factor();
    window.set_cursor_position(Some(point / scale));
    let state = match *phase {
        0 => {
            *phase = 1;
            return Ok(false);
        }
        1 if interaction == Interaction::Hovered => ButtonState::Pressed,
        2 if interaction == Interaction::Pressed => ButtonState::Released,
        _ => return Ok(false),
    };
    world.write_message(MouseButtonInput {
        button: MouseButton::Left,
        state,
        window: entity,
    });
    *phase += 1;
    Ok(false)
}

#[derive(Resource)]
struct MenuProbes {
    next: usize,
    output: std::path::PathBuf,
}

pub fn register_menu_probes(app: &mut App, output: std::path::PathBuf) {
    app.insert_resource(MenuProbes { next: 0, output });
    app.add_systems(Last, probe_menu);
}

fn probe_menu(mut commands: Commands, shell: Res<Shell>, mut probes: ResMut<MenuProbes>) {
    use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
    const TARGETS: [(f64, &str); 3] = [
        (750.0, "shell-menu-probe-0750ms.png"),
        (1500.0, "shell-menu-probe-1500ms.png"),
        (3000.0, "shell-menu-probe-3000ms.png"),
    ];
    let Some(&(target_ms, filename)) = TARGETS.get(probes.next) else {
        return;
    };
    let requested_ms = shell.into_inner().startup_elapsed_ms();
    if requested_ms < target_ms {
        return;
    }
    probes.next += 1;
    let path = probes.output.join(filename);
    eprintln!(
        "EARLY_SHELL_PROBE_REQUEST target_ms={target_ms:.0} actual_ms={requested_ms:.3} path={} verified=false",
        path.display()
    );
    // These readbacks are explicitly unverified observations. They never change
    // Capture state or contribute to the strict final screenshot's acceptance.
    commands.spawn(Screenshot::primary_window()).observe(
        move |mut event: On<ScreenshotCaptured>, shell: Res<Shell>| {
            let captured_ms = shell.startup_elapsed_ms();
            let result = std::mem::take(&mut event.image).try_into_dynamic()
                .map_err(|error| error.to_string())
                .and_then(|image| image.to_rgb8().save(&path).map_err(|error| error.to_string()));
            match result {
                Ok(()) => eprintln!("EARLY_SHELL_PROBE_SAVED target_ms={target_ms:.0} requested_ms={requested_ms:.3} callback_ms={captured_ms:.3} path={} verified=false", path.display()),
                Err(error) => eprintln!("EARLY_SHELL_PROBE_FAILED target_ms={target_ms:.0} requested_ms={requested_ms:.3} callback_ms={captured_ms:.3}: {error}"),
            }
        },
    );
}
