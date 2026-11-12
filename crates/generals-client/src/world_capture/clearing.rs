//! Original optimized tree: real scouting, remembered fog and paid construction.
use super::{Check, Control, View};
use crate::{
    playtest::Match,
    playtest_view::WorldObjects,
    skirmish_verification::{friendly_entity, require},
    tree_buffer::{Trees, verification},
};
use bevy::prelude::*;
use generals_sim::skirmish::{Kind, visibility::Status};
use std::error::Error;

type Pixels = [[u8; 4]; 4];
pub(super) struct Site {
    index: usize,
    tree: usize,
    builder: usize,
    building_type: usize,
    position: [f32; 2],
    scout: [f32; 3],
    homes: [[f32; 3]; 2],
    building: Option<usize>,
    travel: Option<([[f32; 3]; 2], u64)>,
    initial_fog: Option<Pixels>,
    clear: Option<Pixels>,
    camera: Option<[u32; 16]>,
}

pub(super) fn advance(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    let setup = check.setup.as_mut().ok_or("tree capture not prepared")?;
    if check.phase == 9 {
        let site = select(
            &control.battle,
            &control.objects,
            &control.trees,
            setup.tank,
        )?;
        setup.focus = control.objects.0[site.index].position;
        check.site = Some(site);
        check.phase = 10;
    } else if matches!(check.phase, 11 | 13 | 15) {
        let site = check.site.as_mut().ok_or("original tree site missing")?;
        if travel(&mut control.battle, site, setup.tank, check.phase == 13)? {
            check.phase += 1;
        }
        control.time.unpause();
    } else if check.phase == 17 {
        let site = check
            .site
            .as_mut()
            .ok_or("original tree construction site missing")?;
        let game = &mut control.battle.simulation;
        let cash = game.players[0].cash;
        let cost = game.catalog[site.building_type].cost;
        let building = game.build(site.builder, site.building_type, site.position)?;
        require(
            cash.checked_sub(cost) == Some(game.players[0].cash),
            "original tree clearing did not pay building cost",
        )?;
        require(
            !game.entities[building].complete(),
            "original tree clearing foundation completed instantly",
        )?;
        site.building = Some(building);
        control.battle.select_objects(&[building], true);
        check.phase = 18;
        control.time.unpause();
    }
    Ok(())
}

fn select(
    battle: &Match,
    objects: &WorldObjects,
    trees: &Trees,
    tank: usize,
) -> Result<Site, Box<dyn Error>> {
    let game = &battle.simulation;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let building_type = game
        .type_for(0, Kind::PowerPlant)
        .ok_or("USA power plant missing")?;
    let home = Vec2::from_array(game.entities[builder].position);
    let (index, object) = objects
        .0
        .iter()
        .enumerate()
        .filter(|(_, object)| {
            object
                .tree_data
                .as_ref()
                .is_some_and(|(tree, _)| verification::intact(trees, *tree))
                && game.can_place(building_type, [object.position[0], object.position[1]])
                // Original MultiplayerSettings UseShroud=No clears passive shroud
                // at skirmish startup (GameLogic.cpp); unseen trees begin fogged.
                && verification::all_status(game, 0, object.position, &Status::Fogged)
        })
        .min_by(|(_, a), (_, b)| {
            Vec2::new(a.position[0], a.position[1])
                .distance_squared(home)
                .total_cmp(&Vec2::new(b.position[0], b.position[1]).distance_squared(home))
        })
        .ok_or_else(|| site_failure(battle, objects, trees, building_type))?;
    let position = [object.position[0], object.position[1]];
    // Scout from the home side, outside the selected tree's contact footprint.
    let direction = (home - Vec2::from_array(position))
        .try_normalize()
        .ok_or("initially fogged tree coincides with builder home")?;
    let scout = Vec2::from_array(position) + direction * 90.0;
    let scout = [scout.x, scout.y, game.ground_height(scout.to_array())];
    let (tree, data) = object
        .tree_data
        .as_ref()
        .ok_or("selected record lost original W3DTreeDraw data")?;
    println!(
        "Original tree vision site: world_record={index} native_record={tree} model={} texture={} position={position:?} scout={scout:?}",
        data.model, data.texture
    );
    Ok(Site {
        index,
        tree: *tree,
        builder,
        building_type,
        position,
        scout,
        homes: [game.object_position(tank), game.object_position(builder)],
        building: None,
        travel: None,
        initial_fog: None,
        clear: None,
        camera: None,
    })
}

fn site_failure(
    battle: &Match,
    objects: &WorldObjects,
    trees: &Trees,
    building_type: usize,
) -> String {
    let game = &battle.simulation;
    let mut counts = [0; 5];
    let mut examples = Vec::new();
    for object in &objects.0 {
        let Some((tree, data)) = &object.tree_data else {
            continue;
        };
        let intact = verification::intact(trees, *tree);
        let legal = game.can_place(building_type, [object.position[0], object.position[1]]);
        let fogged = verification::all_status(game, 0, object.position, &Status::Fogged);
        for (count, accepted) in
            counts
                .iter_mut()
                .zip([true, intact, legal, fogged, intact && legal && fogged])
        {
            *count += usize::from(accepted);
        }
        if examples.len() < 6 {
            examples.push((tree, &data.model, object.position, intact, legal, fogged));
        }
    }
    let additions = game
        .presentation
        .terrain_movement_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                generals_sim::skirmish::terrain_movement::Event::TreeAdded(_)
            )
        })
        .count();
    format!(
        "original rainy map has no initially fogged legal paid tree site: generation={} tick={} counts[original,intact,legal,fogged,intersection]={counts:?} pending_tree_additions={additions} first_original[tree,model,position,intact,legal,fogged]={examples:?}",
        battle.generation, game.tick,
    )
}

fn travel(
    battle: &mut Match,
    site: &mut Site,
    tank: usize,
    retreat: bool,
) -> Result<bool, Box<dyn Error>> {
    let game = &mut battle.simulation;
    if site.travel.is_none() {
        let mut goals = if retreat { site.homes } else { [site.scout; 2] };
        // Separate arrivals avoid two real units contending for one destination cell.
        if !retreat {
            goals[1][0] += 25.0;
        }
        // Native adjustDestination can replace an obstructed click. Arrival is
        // measured against the actual accepted command, never an invented point.
        for (id, goal) in [tank, site.builder].into_iter().zip(&mut goals) {
            let requested = *goal;
            game.move_to_world(id, requested)?;
            *goal = game.entities[id].ai_goal_position();
            println!(
                "Original tree travel order: tick={} retreat={retreat} actor={id} requested={requested:?} accepted={goal:?} position={:?}",
                game.tick,
                game.object_position(id),
            );
        }
        site.travel = Some((goals, game.tick));
    }
    let (goals, began) = site.travel.ok_or("tree scouting goals missing")?;
    require(
        game.tick - began <= 18000,
        "real original-tree scouting/retreat exceeded18000 ordinary ticks",
    )?;
    let arrived = [tank, site.builder]
        .into_iter()
        .zip(goals)
        .all(|(id, goal)| {
            Vec2::from_array(game.entities[id].position)
                .distance_squared(Vec2::new(goal[0], goal[1]))
                < 20.0 * 20.0
        });
    let status = if retreat {
        Status::Fogged
    } else {
        Status::Clear
    };
    if !arrived
        || !verification::all_status(
            game,
            0,
            [site.position[0], site.position[1], site.scout[2]],
            &status,
        )
    {
        return Ok(false);
    }
    game.stop(tank)?;
    game.stop(site.builder)?;
    battle.select_objects(&[tank], true);
    site.travel = None;
    Ok(true)
}

pub(super) fn inspect(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    let site = check.site.as_mut().ok_or("original tree site missing")?;
    if check.phase == 18 {
        let building = site
            .building
            .ok_or("paid tree clearing foundation missing")?;
        if !view.battle.simulation.entities[building].complete()
            || !view.framing.visible(&view.battle, building)?
            || !view.trees.cleared(&view.battle, site.tree)?
        {
            return Ok(());
        }
        require(
            site.camera == Some(view.trees.camera_pose()),
            "paid erasure capture changed the original tree camera",
        )?;
        view.time.pause();
        view.capture.finish_scenario();
        check.phase = 255;
        println!(
            "Original tree initial-fogged/clear/fogged/revealed/paid erasure verified; inspect all saved pixels."
        );
        return Ok(());
    }
    let expected = match check.phase {
        10 | 14 => Status::Fogged,
        12 | 16 => Status::Clear,
        _ => return Ok(()),
    };
    require(
        verification::all_status(
            &view.battle.simulation,
            0,
            [site.position[0], site.position[1], site.scout[2]],
            &expected,
        ),
        "tree capture lost required four-cell source shroud state",
    )?;
    let Some(viewport) = view.framing.uncovered_viewport() else {
        return Ok(());
    };
    let Some(pixels) = view.trees.sample(&view.battle, site.tree, viewport)? else {
        return Ok(());
    };
    let camera = view.trees.camera_pose();
    require(
        site.camera.is_none_or(|original| original == camera),
        "tree vision capture changed the original tree camera",
    )?;
    site.camera = Some(camera);
    let name = match check.phase {
        10 => {
            site.initial_fog = Some(pixels);
            "world-tree-initial-fogged.png"
        }
        12 => {
            require(
                energy(pixels)
                    > energy(
                        site.initial_fog
                            .ok_or("tree initial fog pixel sample missing")?,
                    ),
                "scouting did not brighten actual tree shader shroud texels",
            )?;
            site.clear = Some(pixels);
            "world-tree-before.png"
        }
        14 => {
            require(
                pixels
                    == site
                        .initial_fog
                        .ok_or("tree initial fog pixel sample missing")?
                    && energy(pixels)
                        < energy(site.clear.ok_or("tree clear pixel sample missing")?),
                "retreat did not restore original fog texels below clear",
            )?;
            "world-tree-fogged.png"
        }
        16 => {
            require(
                pixels == site.clear.ok_or("tree clear pixel sample missing")?,
                "return scouting did not restore actual source clear texels",
            )?;
            "world-tree-revealed.png"
        }
        _ => return Ok(()),
    };
    println!(
        "Original fixed-camera tree capture: phase={} tick={} world_record={} native_record={} pixels={pixels:?}",
        check.phase, view.battle.simulation.tick, site.index, site.tree
    );
    view.time.pause();
    view.capture.intermediate(name);
    check.phase += 1;
    Ok(())
}
fn energy(pixels: Pixels) -> u32 {
    pixels
        .into_iter()
        .flat_map(|pixel| pixel.into_iter().take(3))
        .map(u32::from)
        .sum()
}

#[cfg(test)]
pub fn test_tree_travel(world: &mut World) -> Result<bool, Box<dyn Error>> {
    world.resource_scope(|world, mut battle: Mut<Match>| {
        world.resource_scope(|world, mut check: Mut<Check>| {
            let tank = check.setup.as_ref().ok_or("tree preparation missing")?.tank;
            if check.site.is_none() {
                check.site = Some(select(
                    &battle,
                    world.resource::<WorldObjects>(),
                    world.resource::<Trees>(),
                    tank,
                )?);
            }
            travel(
                &mut battle,
                check.site.as_mut().ok_or("tree site missing")?,
                tank,
                false,
            )
        })
    })
}
