//! Isolated ECS regression run before the paid Technical scene is rendered.
use super::{Geometry, Match, UnitVisual, render};
use bevy::{ecs::system::RunSystemOnce, prelude::*};
use generals_sim::skirmish::Kind;

pub fn verify(world: &mut World) {
    let result = verify_lifecycle(world);
    // No simulation step runs in this fixture. Recreate only the presentation
    // roots so the paid scene starts with its original gameplay state.
    let mut visuals = world.query_filtered::<Entity, With<UnitVisual>>();
    let entities = visuals.iter(world).collect::<Vec<_>>();
    for entity in entities {
        world.despawn(entity);
    }
    let mut geometry = world.resource_mut::<Geometry>();
    geometry.generation = u32::MAX;
    geometry.spawned = 0;
    match result {
        Ok(()) => println!(
            "Crate ECS regression: initial no-science visibility, retired initial IDs, root/mesh-child retirement and no resurrection passed; gameplay state restored"
        ),
        Err(error) => {
            error!("Crate ECS lifecycle: {error}");
            world.write_message(AppExit::error());
        }
    }
}

fn verify_lifecycle(world: &mut World) -> Result<(), String> {
    let (live, retired) = {
        let game = &world.resource::<Match>().simulation;
        let live = game
            .entities
            .iter()
            .position(generals_sim::skirmish::Entity::crate_present)
            .ok_or("paid setup needs a live crate")?;
        let retired = game
            .entities
            .iter()
            .position(|unit| {
                game.catalog[unit.type_index].kind == Kind::Crate && !unit.crate_present()
            })
            .ok_or("paid setup needs a previously collected crate")?;
        if !game.crate_visible_to(0, live) || game.defeated[0] {
            return Err("paid setup needs a visible crate and surviving local player".into());
        }
        let science = &game.catalog[game.entities[live].type_index]
            .salvage
            .pickup
            .as_ref()
            .ok_or("live crate lacks original visibility rules")?
            .visible_science;
        if game.players[1].has_science(science) {
            return Err("paid opponent must lack the crate visibility science".into());
        }
        (live, retired)
    };
    // Present the real USA player's unchanged science set to the renderer only.
    world.resource_mut::<Match>().simulation.players.swap(0, 1);
    let result = world
        .run_system_once(render)
        .map_err(|error| error.to_string());
    world.resource_mut::<Match>().simulation.players.swap(0, 1);
    result?;
    let drawable = root(world, live).ok_or("live crate did not receive a drawable")?;
    if world.get::<Visibility>(drawable) != Some(&Visibility::Hidden) {
        return Err("a newly created no-science crate appeared for one frame".into());
    }
    if root(world, retired).is_some() {
        return Err("a collected initial crate retained a drawable".into());
    }
    world
        .run_system_once(render)
        .map_err(|error| error.to_string())?;
    if world.get::<Visibility>(drawable) != Some(&Visibility::Visible) {
        return Err("restored source science did not reveal the live crate".into());
    }
    let mesh = world
        .spawn((
            ChildOf(drawable),
            Mesh3d(Handle::default()),
            Transform::IDENTITY,
        ))
        .id();
    let child = world.spawn((ChildOf(mesh), Transform::IDENTITY)).id();
    // Use a real collected crate state at this stable drawable ID; no pickup,
    // RNG, visibility or other simulation update is run with the swapped IDs.
    world
        .resource_mut::<Match>()
        .simulation
        .entities
        .swap(live, retired);
    let result = world
        .run_system_once(render)
        .map_err(|error| error.to_string());
    world
        .resource_mut::<Match>()
        .simulation
        .entities
        .swap(live, retired);
    result?;
    if [drawable, mesh, child]
        .into_iter()
        .any(|entity| world.get_entity(entity).is_ok())
    {
        return Err("crate retirement left its root or mesh hierarchy alive".into());
    }
    world
        .run_system_once(render)
        .map_err(|error| error.to_string())?;
    if root(world, live).is_some() || root(world, retired).is_some() {
        return Err("processed crate IDs were recreated after retirement".into());
    }
    Ok(())
}

fn root(world: &mut World, id: usize) -> Option<Entity> {
    let mut visuals = world.query::<(Entity, &UnitVisual)>();
    visuals
        .iter(world)
        .find_map(|(entity, visual)| (visual.index == id).then_some(entity))
}
