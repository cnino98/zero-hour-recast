use super::{Line, Streams, Template, logic_milliseconds, runs};
use crate::{playtest::Match, playtest_view::WorldCamera, source_material::SourceMaterial};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use generals_sim::skirmish::visibility::Status;
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

#[derive(Component)]
pub struct Visual {
    pub(super) source: usize,
    pub(super) weapon: usize,
    line: usize,
    pub(super) generation: u32,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Resources<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    time: Res<'w, Time<Fixed>>,
}

type Visuals<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Visual,
        &'static Mesh3d,
        &'static mut Visibility,
    ),
>;

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    mut streams: ResMut<Streams>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut resources: Resources,
    mut visuals: Visuals,
) {
    let battle = battle.into_inner();
    let camera = camera.into_inner();
    streams.upload(&mut resources);
    if streams.generation != battle.generation {
        streams.active.clear();
        streams.generation = battle.generation;
    }
    if !battle.paused {
        streams.now_ms = logic_milliseconds(battle.simulation.tick, resources.time.overstep());
    }
    streams.active.retain(|key, _| {
        battle.started
            && battle
                .simulation
                .projectile_streams
                .iter()
                .any(|stream| (stream.source, stream.weapon_index) == *key)
    });
    let mut existing = retire(&mut commands, &mut resources, &mut visuals, &streams);
    if !battle.started {
        return;
    }
    let now = streams.now_ms;
    for stream in &battle.simulation.projectile_streams {
        let key = (stream.source, stream.weapon_index);
        let visible = matches!(
            battle.simulation.visibility_cell(
                battle.observer_player.unwrap_or(0),
                [stream.position[0], stream.position[1]]
            ),
            Status::Clear | Status::Partial
        );
        let name = battle.simulation.catalog[battle.simulation.entities[stream.source].type_index]
            .weapon_template(stream.weapon_index)
            .and_then(|weapon| weapon.projectile.stream.as_deref())
            .expect("stream template");
        let points = battle.simulation.projectile_stream_points(stream);
        let Streams {
            templates, active, ..
        } = &mut *streams;
        let template = &templates[name];
        let lines = active.entry(key).or_default();
        if !visible {
            for (index, _) in lines.iter().enumerate() {
                if let Some((entity, _)) = existing.get(&(key.0, key.1, index))
                    && let Ok((_, _, _, mut visibility)) = visuals.get_mut(*entity)
                {
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
            continue;
        }
        let runs = runs(&points, template.maximum);
        for index in runs.len()..lines.len() {
            if let Some((entity, handle)) = existing.remove(&(key.0, key.1, index)) {
                commands.entity(entity).despawn();
                resources.meshes.remove(&handle);
            }
        }
        lines.truncate(runs.len());
        for (index, run) in runs.into_iter().enumerate() {
            if index == lines.len() {
                lines.push(Line::new(now));
            }
            let offset = lines[index].advance(now, template.scroll);
            let mesh = match geometry(template, run, offset, camera) {
                Ok(mesh) => mesh,
                Err(error) => {
                    error!("W3DProjectileStreamDraw: {error}");
                    continue;
                }
            };
            draw_line(
                &mut commands,
                &mut resources,
                &mut visuals,
                existing.get(&(key.0, key.1, index)),
                Visual {
                    source: key.0,
                    weapon: key.1,
                    line: index,
                    generation: battle.generation,
                },
                mesh,
                template.material.as_ref().expect("stream texture loaded"),
            );
        }
    }
}

type Existing = BTreeMap<(usize, usize, usize), (Entity, Handle<Mesh>)>;

fn retire(
    commands: &mut Commands,
    resources: &mut Resources,
    visuals: &mut Visuals,
    streams: &Streams,
) -> Existing {
    let mut existing = BTreeMap::new();
    for (entity, visual, mesh, _) in visuals.iter_mut() {
        if visual.generation != streams.generation
            || !streams.active.contains_key(&(visual.source, visual.weapon))
        {
            commands.entity(entity).despawn();
            resources.meshes.remove(&mesh.0);
        } else {
            existing.insert(
                (visual.source, visual.weapon, visual.line),
                (entity, mesh.0.clone()),
            );
        }
    }
    existing
}

fn draw_line(
    commands: &mut Commands,
    resources: &mut Resources,
    visuals: &mut Visuals,
    existing: Option<&(Entity, Handle<Mesh>)>,
    visual: Visual,
    mesh: Mesh,
    material: &Handle<SourceMaterial>,
) {
    if let Some((entity, handle)) = existing {
        if let Some(mut destination) = resources.meshes.get_mut(handle) {
            *destination = mesh;
        }
        if let Ok((_, _, _, mut visibility)) = visuals.get_mut(*entity) {
            visibility.set_if_neq(Visibility::Visible);
        }
    } else {
        commands.spawn((
            visual,
            Mesh3d(resources.meshes.add(mesh)),
            MeshMaterial3d(material.clone()),
            Transform::IDENTITY,
            RenderLayers::layer(2),
            NoFrustumCulling,
        ));
    }
}

impl Streams {
    fn upload(&mut self, resources: &mut Resources) {
        for template in self.templates.values_mut() {
            if let Some(mut image) = template.image.take() {
                image.texture_descriptor.format =
                    image.texture_descriptor.format.remove_srgb_suffix();
                let mut material =
                    SourceMaterial::new([0.0; 3], [0.0; 3], [0.0; 3], 1.0, AlphaMode::Add, true);
                material.parameters.vertex_diffuse = 3;
                material.texture = Some(resources.images.add(image));
                template.material = Some(resources.materials.add(material));
            }
        }
    }
}

fn geometry(
    template: &Template,
    points: &[[f32; 3]],
    offset: f32,
    camera: &Transform,
) -> Result<Mesh, &'static str> {
    let world = camera.compute_affine();
    let inverse = world.inverse();
    let mut points: Vec<_> = points
        .iter()
        .map(|point| inverse.transform_point3(Vec3::new(point[0], point[2], -point[1]) / 10.0))
        .collect();
    let strip = crate::particles::streak::segmented_geometry(
        &mut points,
        template.width / 20.0,
        template.tile,
    )?;
    let positions: Vec<_> = strip
        .vertices
        .into_iter()
        .map(|point| world.transform_point3(point).to_array())
        .collect();
    let uv: Vec<_> = strip.uv.into_iter().map(|[u, v]| [u, v + offset]).collect();
    let colors = vec![[1.0; 4]; positions.len()];
    // The original prelit XYZDUV strip has no normals. Bevy's shared vertex
    // interface requires them; vertex_diffuse=3 ignores this adapter value.
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(strip.indices));
    Ok(mesh)
}
