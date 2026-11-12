//! Native `W3DDebrisDraw` models and native physical-object matrices, SH 2bfaea60.
use crate::{
    model_animation::{Animations, Clock, Frame},
    playtest::Match,
    playtest_view::{Geometry, UnitVisual},
};
use bevy::prelude::*;
use generals_assets::{
    virtual_assets::VirtualAssets,
    w3d::{Model, animation::Clip},
};
use generals_sim::skirmish::{UnitType, physical_ocl::lists, visibility::Status};
use std::{collections::BTreeMap, error::Error};

#[derive(Resource)]
pub struct Models(BTreeMap<String, usize>, BTreeMap<String, Clip>);

#[derive(Component)]
pub struct Draw(Option<GlobalTransform>, animation::Playback);

pub mod animation;

pub fn load(
    assets: &VirtualAssets,
    types: &[UnitType],
    models: &mut Vec<Model>,
) -> Result<Models, Box<dyn Error>> {
    let mut result = BTreeMap::new();
    for name in types
        .iter()
        .flat_map(lists)
        .flat_map(|list| &list.nuggets)
        .filter(|nugget| nugget.debris_mass.is_some())
        .flat_map(|nugget| &nugget.names)
    {
        let key = name.to_ascii_lowercase();
        if result.contains_key(&key) {
            continue;
        }
        let model = Model::load(assets, name)?;
        result.insert(key, models.len());
        models.push(model);
    }
    let mut clips = BTreeMap::new();
    for name in types
        .iter()
        .flat_map(lists)
        .flat_map(|list| &list.nuggets)
        .filter(|nugget| nugget.debris_mass.is_some())
        .flat_map(|nugget| &nugget.draw.animations)
        .flatten()
    {
        if name.is_empty() || name.eq_ignore_ascii_case("STOP") || clips.contains_key(name) {
            continue;
        }
        let leaf = name
            .rsplit('.')
            .next()
            .ok_or("debris animation name missing")?;
        let path = format!("art/w3d/{leaf}.w3d");
        let mut clip = Clip::parse(&assets.read(&path)?.bytes)?;
        if !clip.name.eq_ignore_ascii_case(name) {
            return Err(format!("debris animation name mismatch: {} vs {name}", clip.name).into());
        }
        clip.bind_hierarchy(
            &assets
                .read(&format!("art/w3d/{}.w3d", clip.hierarchy))?
                .bytes,
        )?;
        clips.insert(name.clone(), clip);
    }
    Ok(Models(result, clips))
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    models: Res<Models>,
    geometry: Res<Geometry>,
    animations: Res<Animations>,
    mut meshes: ResMut<Assets<Mesh>>,
    visuals: Query<(Entity, &UnitVisual), Without<Draw>>,
) {
    let battle = battle.into_inner();
    let models = models.into_inner();
    let geometry = geometry.into_inner();
    let animations = animations.into_inner();
    for (entity, visual) in &visuals {
        if visual.generation != battle.generation {
            continue;
        }
        let actor = &battle.simulation.entities[visual.index];
        if actor.physical_basis().is_none()
            && battle.simulation.catalog[actor.type_index]
                .structure_death
                .is_none()
        {
            continue;
        }
        commands
            .entity(entity)
            .insert(Draw(None, animation::Playback::new()));
        let Some(name) = actor.physical_model() else {
            continue;
        };
        let Some(&model) = models.0.get(&name.to_ascii_lowercase()) else {
            error!("Physical object has no prepared original debris model {name}");
            continue;
        };
        let color = actor.debris_draw().and_then(|draw| draw.color);
        let owner = color.and_then(|color| geometry.packed_color_index(color));
        if color.is_some() && owner.is_none() {
            error!("Debris PlayerColor has no prepared original material: {color:?}");
            continue;
        }
        let pose = if actor
            .debris_draw()
            .is_some_and(|draw| draw.animations.is_some())
        {
            match animations.debris_pose(model, None, 0.0) {
                Ok(pose) => Some(pose),
                Err(error) => {
                    error!("Original debris base pose: {error}");
                    continue;
                }
            }
        } else {
            None
        };
        geometry.spawn_unit_parts(
            &mut commands,
            entity,
            model,
            owner,
            &mut meshes,
            &Frame {
                supply: None,
                meshes: pose,
                height_offset: 0.0,
            },
        );
    }
}

type PhysicalRoot = (
    &'static UnitVisual,
    Option<&'static Children>,
    &'static mut Draw,
    &'static mut GlobalTransform,
    Option<&'static mut crate::light_environment::DrawPose>,
);

/// Keep the full native basis: quaternion decomposition would discard matrix drift.
pub fn transform(
    battle: Res<Match>,
    structures: Res<crate::structure_view::Structures>,
    mut roots: Query<PhysicalRoot>,
    mut parts: Query<(&Transform, &mut GlobalTransform), Without<UnitVisual>>,
) {
    let battle = battle.into_inner();
    let structures = structures.into_inner();
    for (visual, children, mut draw, mut world, native_pose) in &mut roots {
        if visual.generation != battle.generation {
            continue;
        }
        let game = &battle.simulation;
        let actor = &game.entities[visual.index];
        let next = if matches!(game.visibility_status(0, visual.index), Status::Fogged) {
            // Transform propagation may run again after the real drawable was
            // deleted. Keep the independent last-seen ghost's complete matrix.
            let Some(pose) = draw.0 else { continue };
            pose
        } else {
            let (position, basis) = game.object_transform(visual.index);
            let offset = structures.offset(battle.generation, visual.index);
            if let Some(mut native) = native_pose {
                *native = crate::light_environment::DrawPose::from_object(game, visual.index);
                let local = [
                    offset[0],
                    offset[1],
                    offset[2] + actor.aircraft_draw_height(),
                ];
                native.position = native.point(local);
            }
            // Drawable::getTransformMatrix multiplies the instance translation
            // on the right, so the building's native basis rotates the shudder.
            let translation = Vec3::new(
                offset[0] * 0.1,
                (offset[2] + actor.aircraft_draw_height()) * 0.1,
                -offset[1] * 0.1,
            );
            let mut pose = source_transform(position, basis);
            if let Some(instance) = actor.drawable_instance_matrix() {
                let position = instance.map(|row| row[3]);
                let basis = instance.map(|row| [row[0], row[1], row[2]]);
                pose = pose * source_transform(position, basis);
            }
            let pose = pose.mul_transform(Transform::from_translation(translation));
            draw.0 = Some(pose);
            pose
        };
        world.set_if_neq(next);
        for child in children.into_iter().flatten() {
            if let Ok((local, mut transform)) = parts.get_mut(*child) {
                transform.set_if_neq(next.mul_transform(*local));
            }
        }
    }
}

fn source_transform(position: [f32; 3], basis: [[f32; 3]; 3]) -> GlobalTransform {
    // B * M * B^-1, where B maps source XYZ to Bevy XZ(-Y). Matrix3D is row-major.
    GlobalTransform::from(Mat4::from_cols(
        Vec4::new(basis[0][0], basis[2][0], -basis[1][0], 0.0),
        Vec4::new(basis[0][2], basis[2][2], -basis[1][2], 0.0),
        Vec4::new(-basis[0][1], -basis[2][1], basis[1][1], 0.0),
        Vec4::new(
            position[0] * 0.1,
            position[2] * 0.1,
            -position[1] * 0.1,
            1.0,
        ),
    ))
}

#[cfg(test)]
mod tests;
