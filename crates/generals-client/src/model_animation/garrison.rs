//! `GarrisonContain`'s objectless original barrel and muzzle-flash drawables.
mod verification;
use super::{Frame, Model, data};
use crate::{
    playtest::Match,
    playtest_view::Geometry,
    skirmish_data::{blocks, definition},
};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};
pub use verification::prepare;

#[derive(Resource)]
pub struct Guns {
    models: [Vec<usize>; 2],
    recorded: Option<(u32, u64)>,
}

#[derive(Component)]
pub struct Gun {
    carrier: usize,
    point: usize,
    placed: u64,
    generation: u32,
    firing: bool,
}

pub fn load(assets: &VirtualAssets, models: &mut Vec<Model>) -> Result<Guns, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/object/system.ini")?)?];
    let object = definition(&files, "Object", "GarrisonGun")?;
    let mut selected = [Vec::new(), Vec::new()];
    let mut clips = BTreeMap::new();
    for module in blocks(&object.lines, 0, "Draw")? {
        let mut draw = data::parse(module)?;
        for (firing, selected) in selected.iter_mut().enumerate() {
            let flags = if firing == 1 {
                BTreeSet::from(["FIRING_A".into()])
            } else {
                BTreeSet::new()
            };
            let state = draw.select(&flags);
            let state = &mut draw.states[state];
            if !state.motions.is_empty() {
                return Err("animated GarrisonGun requires original playback integration".into());
            }
            if state.model.is_none() {
                data::load_state(assets, state, models, &mut clips)?;
            }
            if let Some(model) = state.model {
                selected.push(model);
            }
        }
    }
    if selected[0].is_empty() || selected[1].is_empty() {
        return Err("original GarrisonGun draw states missing".into());
    }
    Ok(Guns {
        models: selected,
        recorded: None,
    })
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    mut guns: ResMut<Guns>,
    geometry: Res<Geometry>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut visuals: Query<(Entity, &mut Gun, &mut Transform, Option<&Children>)>,
) {
    let battle = battle.into_inner();
    let geometry = geometry.into_inner();
    let game = &battle.simulation;
    let stamp = (battle.generation, game.tick);
    if guns.recorded == Some(stamp) {
        return;
    }
    guns.recorded = Some(stamp);
    let mut wanted = BTreeMap::new();
    if battle.started {
        for (carrier, actor) in game.entities.iter().enumerate() {
            if !actor.alive() {
                continue;
            }
            let Some(points) = game.garrison_points(carrier) else {
                continue;
            };
            for (point, rider) in points.iter().enumerate() {
                if let Some(rider) = rider {
                    wanted.insert((carrier, point), *rider);
                }
            }
        }
    }
    for (entity, mut gun, mut transform, children) in &mut visuals {
        let current = wanted.get(&(gun.carrier, gun.point)).copied();
        let effect = game.garrison_effect(gun.carrier, gun.point);
        if gun.generation != battle.generation
            || effect.is_none_or(|(placed, _)| placed != gun.placed)
            || current.is_none()
        {
            commands.entity(entity).despawn();
            continue;
        }
        let rider = current.expect("live point checked");
        transform.set_if_neq(gun_transform(game, gun.carrier, gun.point, rider));
        let firing = effect.expect("live effect checked").1;
        if firing != gun.firing {
            if let Some(children) = children {
                for &child in children {
                    commands.entity(child).despawn();
                }
            }
            gun.firing = firing;
            spawn_parts(&mut commands, geometry, &mut meshes, &guns, entity, firing);
        }
        wanted.remove(&(gun.carrier, gun.point));
    }
    for ((carrier, point), rider) in wanted {
        let (placed, firing) = game
            .garrison_effect(carrier, point)
            .expect("occupied gun point");
        // Objectless native drawables copy the host's obscuration at creation;
        // GameClient's later object-shroud update does not visit them.
        let clear = matches!(
            game.visibility_status(0, carrier),
            generals_sim::skirmish::visibility::Status::Clear
                | generals_sim::skirmish::visibility::Status::Partial
        );
        let entity = commands
            .spawn((
                Gun {
                    carrier,
                    point,
                    placed,
                    generation: battle.generation,
                    firing,
                },
                gun_transform(game, carrier, point, rider),
                if clear {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ))
            .id();
        spawn_parts(&mut commands, geometry, &mut meshes, &guns, entity, firing);
    }
}

fn spawn_parts(
    commands: &mut Commands,
    geometry: &Geometry,
    meshes: &mut Assets<Mesh>,
    guns: &Guns,
    entity: Entity,
    firing: bool,
) {
    for &model in &guns.models[usize::from(firing)] {
        geometry.spawn_unit_parts(
            commands,
            entity,
            model,
            None,
            meshes,
            &Frame {
                supply: None,
                meshes: None,
                height_offset: 0.0,
            },
        );
    }
}

fn gun_transform(
    game: &generals_sim::skirmish::Skirmish,
    carrier: usize,
    point: usize,
    rider: usize,
) -> Transform {
    let [x, y, z] = game.object_position(rider);
    Transform::from_xyz(x * 0.1, z * 0.1, -y * 0.1).with_rotation(Quat::from_rotation_y(
        game.garrison_effect_facing(carrier, point)
            .expect("occupied gun facing"),
    ))
}
