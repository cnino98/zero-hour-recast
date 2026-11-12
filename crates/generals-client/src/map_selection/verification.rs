//! Native pointer traversal through the original chooser and retained restart.
use super::MapSelection;
mod controls;
use crate::{
    playtest::Match,
    playtest_ui::Action,
    playtest_view::{Capture, UnitVisual},
};
use bevy::{
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    window::PrimaryWindow,
};
use generals_sim::skirmish::{Faction, PlayerSlot};

#[derive(Resource)]
struct Check {
    stage: u8,
    phase: u8,
    scrolling: bool,
    controls: u8,
    scroll_top: usize,
    capacity: usize,
    custom: bool,
    category_pending: Option<bool>,
    target: Option<usize>,
    began: Option<f64>,
    preview_saved: bool,
    generation: u32,
    seed: u32,
    choices: Vec<PlayerSlot>,
    factions: Vec<Faction>,
    starts: Vec<Option<usize>>,
    colors: Vec<Option<usize>>,
}

pub fn enable_capacity(app: &mut App, capacity: usize) {
    enable(app, capacity, false);
}

pub fn enable_custom(app: &mut App, capacity: usize) {
    enable(app, capacity, true);
}

fn enable(app: &mut App, capacity: usize, custom: bool) {
    app.insert_resource(Check {
        stage: 0,
        phase: 0,
        scrolling: false,
        controls: 0,
        scroll_top: 0,
        capacity,
        custom,
        category_pending: custom.then_some(true),
        target: None,
        began: None,
        preview_saved: false,
        generation: 0,
        seed: 0,
        choices: Vec::new(),
        factions: Vec::new(),
        starts: Vec::new(),
        colors: Vec::new(),
    });
    app.add_systems(First, step);
}

fn step(world: &mut World) {
    world.resource_scope(|world, mut check: Mut<Check>| {
        if check.stage == 12 {
            return;
        }
        if let Err(reason) = advance(world, &mut check) {
            error!("Original map chooser native verification: {reason}");
            check.stage = 12;
            world.write_message(AppExit::error());
        }
    });
}

fn advance(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let began = *check.began.get_or_insert(now);
    if now - began > 120.0 {
        let selection = world.resource::<MapSelection>();
        error!(
            "Map chooser timeout stage={} pointer_phase={} scrolling={} current={} target={:?} open={} requested={:?} top={} highlighted={:?}",
            check.stage,
            check.phase,
            check.scrolling,
            selection.current,
            check.target,
            selection.open,
            selection.requested,
            selection.top,
            selection.highlighted
        );
        return Err("pointer selection/reload/start/restart timed out");
    }
    if check.target.is_none() {
        check.target = Some(choose_target(world.resource::<MapSelection>(), check)?);
    }
    let target = check.target.expect("native target selected");
    if check.stage == 3
        && matches!(
            world.resource::<MapSelection>().catalog_state,
            super::CatalogState::RefreshRequested
        )
    {
        return Ok(());
    }
    if check.stage == 3 && check.category_pending.is_none() && !controls::scroll(world, check)? {
        return Ok(());
    }
    if check.stage == 4
        && world.resource::<MapSelection>().current == target
        && !world.resource::<MapSelection>().open
    {
        check.stage = 5;
        check.phase = 0;
    }
    if check.stage == 4 {
        if let Some(reason) = world.resource::<MapSelection>().maps[target]
            .admission
            .rejection()
        {
            error!("Native map-selection fixture rejected: {reason}");
            return Err("native map-selection fixture failed mandatory validation");
        }
        let capture = world.resource::<Capture>();
        if !capture.ready_for_scenario() {
            return Ok(());
        }
        if !check.preview_saved {
            check.preview_saved = true;
            world
                .resource_mut::<Capture>()
                .intermediate("map-selection.png");
            return Ok(());
        }
        if !controls::keyboard(world, check, target)? {
            return Ok(());
        }
        if !world.contains_resource::<super::DoubleClickInterval>() {
            return Err("system double-click interval unavailable for native acceptance");
        }
    }
    if check.stage == 5 {
        let selection = world.resource::<MapSelection>();
        if selection.requested.is_some() || selection.open {
            return Ok(());
        }
        if selection.current != target || !selection.maps[target].admission.is_verified() {
            return Err("accept did not commit and validate requested map");
        }
        let game = &world.resource::<Match>().simulation;
        if game.slots.len() != check.capacity {
            return Err("committed map retained previous slot capacity");
        }
        check.stage = 6;
    }
    if check.stage == 7 {
        let battle = world.resource::<Match>();
        if !battle.started {
            return Ok(());
        }
        let game = &battle.simulation;
        check.generation = battle.generation;
        check.seed = game.random_seed();
        check.choices.clone_from(&game.slots);
        check.factions = game.players.iter().map(|player| player.faction).collect();
        check.starts.clone_from(&game.start_positions);
        check.colors.clone_from(&game.player_colors);
        check.stage = 8;
    }
    if check.stage == 11 {
        return complete_restart(world, check, target);
    }
    pointer(world, check, target)
}

fn choose_target(selection: &MapSelection, check: &Check) -> Result<usize, &'static str> {
    // Deliberate native acceptance fixtures; unknown catalog entries are not
    // evidence of gameplay support. Normal Accept still validates this map.
    let path = match (check.custom, check.capacity) {
        (false, 2) => "maps/flash fire/flash fire.map",
        (false, 4) => "maps/dark mountain/dark mountain.map",
        (false, 6) => "maps/defcon6/defcon6.map",
        (false, 8) => "maps/whiteout/whiteout.map",
        (true, 2) => "user-maps/custom desert 01/custom desert 01.map",
        _ => return Err("no native map-selection fixture for requested category/capacity"),
    };
    let index = selection
        .maps
        .iter()
        .position(|map| map.path == path)
        .ok_or("native map-selection fixture is absent from catalog")?;
    let map = &selection.maps[index];
    if index == selection.current || map.capacity != check.capacity || map.custom() != check.custom
    {
        return Err("native map-selection fixture does not offer a different requested map");
    }
    if map.admission.rejection().is_some() {
        return Err("native map-selection fixture has rejected mandatory data");
    }
    Ok(index)
}

fn complete_restart(
    world: &mut World,
    check: &mut Check,
    target: usize,
) -> Result<(), &'static str> {
    let battle = world.resource::<Match>();
    if battle.generation == check.generation {
        return Ok(());
    }
    let game = &battle.simulation;
    if world.resource::<MapSelection>().current != target
        || game.slots.len() != check.capacity
        || game.random_seed() != check.seed
        || game.start_positions != check.starts
        || game.player_colors != check.colors
        || !game
            .players
            .iter()
            .map(|player| player.faction)
            .eq(check.factions.iter().copied())
        || !game.slots.iter().zip(&check.choices).all(|(left, right)| {
            left.faction == right.faction
                && left.occupied == right.occupied
                && left.difficulty.as_ref().map(std::mem::discriminant)
                    == right.difficulty.as_ref().map(std::mem::discriminant)
                && left.team == right.team
                && left.start == right.start
                && left.color == right.color
        })
    {
        return Err("restart lost map/seed/original choices/resolved starts/colors/factions");
    }
    let generation = battle.generation;
    let alive = world
        .query::<(&UnitVisual, &Children)>()
        .iter(world)
        .filter(|(visual, _)| visual.generation == generation)
        .flat_map(|(_, children)| children.iter())
        .any(|child| {
            world
                .get::<ViewVisibility>(child)
                .is_some_and(|visibility| visibility.get())
        });
    if !alive || !world.resource::<Capture>().ready_for_scenario() {
        return Ok(());
    }
    let selection = world.resource::<MapSelection>();
    println!(
        "MAP_SELECTION_RENDERED path={} capacity={} actual_pointer=true original_double_click_accept=true wheel_thumb_keyboard=true full_reload=true restart_retained=true original_unit_mesh_visible=true",
        selection.maps[target].path, check.capacity
    );
    world.resource_mut::<Time<Virtual>>().pause();
    world.resource_mut::<Capture>().finish_scenario();
    check.stage = 12;
    Ok(())
}

fn pointer(world: &mut World, check: &mut Check, target: usize) -> Result<(), &'static str> {
    // LEFT_UP can hide/despawn its gadget or reload the whole map. Complete the
    // emitted click before looking for that gadget in the next screen's layout.
    if check.phase == 3 {
        check.phase = 0;
        if check.stage == 3 && check.category_pending.is_some() {
            check.category_pending = None;
        } else if !check.scrolling && check.stage != 4 {
            check.stage += 1;
        }
        return Ok(());
    }
    let mut scroll = false;
    let point = {
        let mut nodes = world.query::<(
            &Action,
            &ComputedNode,
            &UiGlobalTransform,
            &InheritedVisibility,
            Option<&CalculatedClip>,
        )>();
        let target_row = world.resource::<MapSelection>().row(target);
        let find = |action: &Action| match (check.stage, action) {
            (0, Action::SinglePlayer)
            | (1, Action::Skirmish)
            | (2, Action::SelectMap)
            | (6, Action::Start)
            | (8, Action::Pause)
            | (9, Action::Restart)
            | (10, Action::ConfirmRestart) => true,
            (3, Action::MapCategory(user)) if check.category_pending == Some(*user) => true,
            (3 | 4, Action::MapRow(index)) if check.category_pending.is_none() => {
                Some(*index) == target_row
            }
            _ => false,
        };
        let point = nodes
            .iter(world)
            .find_map(|(action, node, transform, visibility, clip)| {
                if !visibility.get() || node.is_empty() || !find(action) {
                    return None;
                }
                visible_point(
                    node.size(),
                    transform.translation,
                    clip.map(|clip| clip.clip),
                )
            });
        if point.is_none() && check.stage == 3 {
            scroll = true;
            nodes.iter(world).find_map(|(action,node,transform,visibility,clip)| (visibility.get() && !node.is_empty() && matches!(action,Action::MapScroll(delta) if *delta == if target_row.is_some_and(|row| row < world.resource::<MapSelection>().top) { -1 } else { 1 })).then(|| visible_point(node.size(), transform.translation, clip.map(|clip| clip.clip))).flatten())
        } else {
            point
        }
    };
    let Some(point) = point else {
        return Ok(());
    };
    if check.phase == 0 {
        check.scrolling = scroll;
    }
    let (window, factor) = {
        let mut windows = world.query_filtered::<(Entity, &Window), With<PrimaryWindow>>();
        let (entity, window) = windows
            .single(world)
            .map_err(|_| "native primary window missing")?;
        (entity, window.scale_factor())
    };
    match check.phase {
        0 => world
            .get_mut::<Window>(window)
            .ok_or("native window disappeared")?
            .set_cursor_position(Some(point / factor)),
        1 | 2 => {
            world.write_message(MouseButtonInput {
                button: MouseButton::Left,
                state: if check.phase == 1 {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
                window,
            });
        }
        _ => unreachable!("three emitted pointer phases before completion"),
    }
    check.phase += 1;
    Ok(())
}

fn visible_point(size: Vec2, centre: Vec2, clip: Option<Rect>) -> Option<Vec2> {
    let bounds = Rect::from_center_size(centre, size);
    let bounds = clip.map_or(bounds, |clip| bounds.intersect(clip));
    bounds
        .size()
        .cmpgt(Vec2::ZERO)
        .all()
        .then_some(bounds.center())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_row_pointer_uses_visible_clip_and_rejects_fully_clipped_row() {
        let clip = Rect::new(10.0, 20.0, 100.0, 80.0);
        assert_eq!(
            visible_point(Vec2::new(90.0, 16.0), Vec2::new(55.0, 20.0), Some(clip)),
            Some(Vec2::new(55.0, 24.0))
        );
        assert_eq!(
            visible_point(Vec2::new(90.0, 16.0), Vec2::new(55.0, 100.0), Some(clip)),
            None
        );
    }

    #[test]
    fn released_gadget_can_disappear_before_native_click_completes() {
        let mut app = App::new();
        enable_capacity(&mut app, 2);
        let mut check = app.world_mut().remove_resource::<Check>().unwrap();
        // Skirmish release opens setup and hides the old Skirmish gadget.
        // No action entity or window survives in this fixture's next layout.
        check.stage = 1;
        check.phase = 3;
        pointer(app.world_mut(), &mut check, 0).unwrap();
        assert_eq!(check.stage, 2);
        assert_eq!(check.phase, 0);
    }

    #[test]
    fn completed_scroll_never_counts_as_clicking_newly_visible_target_row() {
        let mut app = App::new();
        enable_capacity(&mut app, 2);
        let mut check = app.world_mut().remove_resource::<Check>().unwrap();
        check.stage = 3;
        check.phase = 3;
        check.scrolling = true;
        // The released scroll can change the layout to expose the target row.
        // Completion still belongs to that scroll, not a new selection click.
        pointer(app.world_mut(), &mut check, 0).unwrap();
        assert_eq!(check.stage, 3);
        assert_eq!(check.phase, 0);
        check.scrolling = false;
        check.phase = 3;
        pointer(app.world_mut(), &mut check, 0).unwrap();
        assert_eq!(check.stage, 4);
    }
}
