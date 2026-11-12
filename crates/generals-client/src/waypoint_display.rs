//! `W3DWaypointBuffer` (EA, GPL-3.0-or-later), pinned `SuperHackers` 2bfaea60.
//! Original `SCMNode`/`EXLaser` assets and `SegLineRenderer` camera-space edge planes.
//! Intersection merging remains unported.
use crate::{
    input::Mouse, model_render, playtest::Match, playtest_view::WorldCamera,
    source_material::SourceMaterial, source_texture,
};
use bevy::{
    asset::RenderAssetUsages, camera::visibility::RenderLayers, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use generals_assets::{virtual_assets::VirtualAssets, w3d::Model};
use std::error::Error;

mod inspection;
mod revealed;
pub use inspection::RouteInspection;

#[derive(Resource)]
pub struct Waypoints {
    node: Vec<model_render::Part>,
    images: Vec<Option<Image>>,
    line_texture: Option<Image>,
}

#[derive(Resource)]
pub struct Display {
    node_parts: Vec<(Handle<Mesh>, Handle<SourceMaterial>)>,
    line_material: Handle<SourceMaterial>,
    revealed_material: Handle<SourceMaterial>,
    nodes: Vec<Entity>,
    lines: Vec<(Entity, Handle<Mesh>)>,
}

pub fn load(assets: &VirtualAssets) -> Result<Waypoints, Box<dyn Error>> {
    let model = Model::parse(&assets.read("art/w3d/scmnode.w3d")?.bytes)?;
    let mut node = model_render::parts(&model)?;
    let images = node
        .iter_mut()
        .map(|part| {
            part.material.parameters.scene_ambient = Vec4::ONE;
            part.texture
                .as_ref()
                .map(|(name, _)| texture(assets, name))
                .transpose()
        })
        .collect::<Result<_, _>>()?;
    Ok(Waypoints {
        node,
        images,
        line_texture: Some(texture(assets, "EXLaser.tga")?),
    })
}

fn texture(assets: &VirtualAssets, name: &str) -> Result<Image, Box<dyn Error>> {
    let mut image = source_texture::load(assets, name)?;
    image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
    Ok(image)
}

pub fn setup(
    mut commands: Commands,
    mut pending: ResMut<Waypoints>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let parts = std::mem::take(&mut pending.node);
    let textures = std::mem::take(&mut pending.images);
    let node_parts = parts
        .into_iter()
        .zip(textures)
        .map(|(part, texture)| {
            let mut material = part.material;
            material.texture = texture.map(|image| images.add(image));
            (meshes.add(part.mesh), materials.add(material))
        })
        .collect();
    let mut line = SourceMaterial::new(
        [0.0; 3],
        [0.0; 3],
        [0.25, 0.5, 1.0],
        1.0,
        AlphaMode::Add,
        true,
    );
    line.depth_compare = Some(bevy::render::render_resource::CompareFunction::Always);
    line.texture = pending.line_texture.take().map(|image| images.add(image));
    let mut revealed = SourceMaterial::new(
        [0.0; 3],
        [0.0; 3],
        [0.95, 0.5, 0.0],
        1.0,
        AlphaMode::Add,
        true,
    );
    revealed.depth_compare = line.depth_compare;
    revealed.texture.clone_from(&line.texture);
    commands.insert_resource(Display {
        node_parts,
        line_material: materials.add(line),
        revealed_material: materials.add(revealed),
        nodes: Vec::new(),
        lines: Vec::new(),
    });
    commands.remove_resource::<Waypoints>();
}

pub fn update(
    mut commands: Commands,
    mut display: ResMut<Display>,
    mut meshes: ResMut<Assets<Mesh>>,
    battle: Res<Match>,
    mouse: Res<Mouse>,
    camera: Single<&GlobalTransform, With<WorldCamera>>,
) {
    let battle = battle.into_inner();
    let mouse = mouse.into_inner();
    let display = display.as_mut();
    let camera = camera.into_inner();
    for &entity in &display.nodes {
        commands.entity(entity).insert(Visibility::Hidden);
    }
    for &(entity, _) in &display.lines {
        commands.entity(entity).insert(Visibility::Hidden);
    }
    if !battle.started {
        return;
    }
    let mut counts = [0, 0];
    for &index in &battle.selected {
        if !mouse.waypoint_mode && revealed::selected_outpost(battle, index) {
            if let Some(route) = revealed::points(battle, index, mouse.hovered) {
                show_route(
                    &mut commands,
                    display,
                    &mut meshes,
                    camera,
                    route,
                    &mut counts,
                );
            }
            // Native breaks on the first selected Outpost, including no enemy
            // under the pointer or an enemy without a displayable main goal.
            break;
        }
        let Some((points, marker_points)) = display_points(battle, index, mouse.waypoint_mode)
        else {
            continue;
        };
        show_route(
            &mut commands,
            display,
            &mut meshes,
            camera,
            Route {
                points,
                marker_points,
                revealed: false,
            },
            &mut counts,
        );
    }
}

struct Route {
    points: Vec<Vec3>,
    marker_points: Vec<Vec3>,
    revealed: bool,
}

fn show_route(
    commands: &mut Commands,
    display: &mut Display,
    meshes: &mut Assets<Mesh>,
    camera: &GlobalTransform,
    route: Route,
    counts: &mut [usize; 2],
) {
    for point in route.marker_points {
        show_node(commands, display, counts[0], point);
        counts[0] += 1;
    }
    let material = if route.revealed {
        &display.revealed_material
    } else {
        &display.line_material
    };
    let half_width = if route.revealed { 0.15 } else { 0.075 };
    for chunk in route
        .points
        .windows(2)
        .enumerate()
        .step_by(128)
        .map(|(start, _)| &route.points[start..route.points.len().min(start + 129)])
    {
        let Some(mesh) = line_mesh(chunk, camera, half_width) else {
            continue;
        };
        if let Some((entity, handle)) = display.lines.get(counts[1]) {
            if let Some(mut existing) = meshes.get_mut(handle) {
                *existing = mesh;
            }
            commands
                .entity(*entity)
                .insert((Visibility::Visible, MeshMaterial3d(material.clone())));
        } else {
            let mesh = meshes.add(mesh);
            let entity = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    RenderLayers::layer(1),
                ))
                .id();
            display.lines.push((entity, mesh));
        }
        counts[1] += 1;
    }
}

fn display_points(
    battle: &Match,
    index: usize,
    waypoint_mode: bool,
) -> Option<(Vec<Vec3>, Vec<Vec3>)> {
    let unit = &battle.simulation.entities[index];
    if !unit.alive() {
        return None;
    }
    let stats = &battle.simulation.catalog[unit.type_index];
    if waypoint_mode {
        let (goals, current) = unit.waypoint_path()?;
        let nodes: Vec<_> = goals[current..]
            .iter()
            .map(|point| Vec3::new(point[0] * 0.1, point[2] * 0.1, -point[1] * 0.1))
            .collect();
        let origin = crate::playtest_view::unit_position(&battle.simulation, index);
        let points = std::iter::once(origin)
            .chain(nodes.iter().copied())
            .take(513)
            .collect();
        Some((points, nodes))
    } else {
        if unit.owner != battle.resource_owner().unwrap_or(0) || unit.rally_point.is_none() {
            return None;
        }
        let path = battle.simulation.producer_rally_path(index)?;
        let [create, natural, rally] = path.as_slice() else {
            return None;
        };
        let base_height = battle.simulation.object_position(index)[2] * 0.1;
        let source_point = |point: Vec2| Vec3::new(point.x * 0.1, base_height, -point.y * 0.1);
        let corners = rally_corners(
            Vec2::new(create[0], create[1]),
            Vec2::new(natural[0], natural[1]),
            Vec2::new(rally[0], rally[1]),
            unit,
            stats,
        );
        let world_point =
            |point: &[f32; 3]| Vec3::new(point[0] * 0.1, point[2] * 0.1, -point[1] * 0.1);
        let mut points = vec![world_point(create)];
        if !create
            .iter()
            .zip(natural)
            .all(|(left, right)| left.partial_cmp(right) == Some(std::cmp::Ordering::Equal))
        {
            points.push(world_point(natural));
        }
        let mut nodes: Vec<_> = corners.into_iter().map(source_point).collect();
        points.extend(nodes.iter().copied());
        points.push(Vec3::new(rally[0] * 0.1, rally[2] * 0.1, -rally[1] * 0.1));
        nodes.push(world_point(natural));
        Some((points, nodes))
    }
}

// Preserve W3DWaypointBuffer's original absolute wayOutPoint normalization,
// corner order, and two elbow tests; this is a visual path, not a navigation path.
fn rally_corners(
    create: Vec2,
    natural: Vec2,
    rally: Vec2,
    unit: &generals_sim::skirmish::Entity,
    stats: &generals_sim::skirmish::UnitType,
) -> Vec<Vec2> {
    if create == natural {
        return Vec::new();
    }
    let out = (natural - create).normalize_or_zero() * 99_999.9;
    let length = out.length();
    let out = out + natural;
    if 100.0 + (out - rally).length() <= length {
        return Vec::new();
    }
    let out = out.normalize_or_zero();
    if (natural - rally).normalize_or_zero().dot(out) <= 0.0 {
        return Vec::new();
    }
    let (s, c) = unit.facing.sin_cos();
    let ex = Vec2::new(stats.geometry.major * c, stats.geometry.major * s);
    let ey = Vec2::new(-stats.geometry.minor * s, stats.geometry.minor * c);
    let center = Vec2::from_array(unit.position);
    let corners = [
        center - ex + ey,
        center + ex + ey,
        center + ex - ey,
        center - ex - ey,
    ];
    let mut near = None;
    let mut far = None;
    let mut near_distance = 99_999.9;
    let mut far_distance = 99_999.9;
    for corner in corners {
        let distance = corner.distance(rally);
        if (create - corner).normalize_or_zero().dot(out) < 0.0 {
            if distance < near_distance {
                near = Some(corner);
                near_distance = distance;
            }
        } else if distance < far_distance {
            far = Some(corner);
            far_distance = distance;
        }
    }
    let Some(near) = near else {
        return Vec::new();
    };
    let mut result = vec![near];
    if (near - rally)
        .normalize_or_zero()
        .dot((natural - near).normalize_or_zero())
        < 0.0
    {
        result.extend(far);
    }
    result
}

fn show_node(commands: &mut Commands, display: &mut Display, index: usize, point: Vec3) {
    let transform = Transform::from_translation(point);
    if let Some(&entity) = display.nodes.get(index) {
        commands
            .entity(entity)
            .insert((transform, Visibility::Visible));
    } else {
        let entity = commands
            .spawn((transform, Visibility::Visible))
            .with_children(|parent| {
                for (mesh, material) in &display.node_parts {
                    parent.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        RenderLayers::layer(1),
                    ));
                }
            })
            .id();
        display.nodes.push(entity);
    }
}

fn line_mesh(points: &[Vec3], camera: &GlobalTransform, half_width: f32) -> Option<Mesh> {
    let world_from_view = camera.affine();
    let view_from_world = world_from_view.inverse();
    let mut points: Vec<_> = points
        .iter()
        .map(|&point| view_from_world.transform_point3(point))
        .collect();
    let planes = edge_planes(&mut points, half_width)?;
    let mut vertices = Vec::with_capacity(points.len() * 2);
    for (index, &point) in points.iter().enumerate() {
        for (edge, &first_plane) in planes[0].iter().enumerate() {
            let direction = if index == 0 {
                projected_direction(point, first_plane)
            } else if index == points.len() - 1 {
                projected_direction(point, planes[index - 1][edge])
            } else {
                intersection_direction(point, planes[index - 1][edge], planes[index][edge])
            };
            if direction == Vec3::ZERO {
                return None;
            }
            vertices.push(
                world_from_view
                    .transform_point3(direction * point.dot(direction))
                    .to_array(),
            );
        }
    }
    let mut positions = Vec::new();
    let mut coordinates = Vec::new();
    for index in 0..points.len() - 1 {
        let current = index * 2;
        for vertex in [
            current,
            current + 1,
            current + 2,
            current + 1,
            current + 3,
            current + 2,
        ] {
            positions.push(vertices[vertex]);
            let u = if vertex % 2 == 0 { 0.0 } else { 1.0 };
            let v = f32::from(u16::try_from(vertex / 2).ok()?);
            coordinates.push([u, 1.0 - v]);
        }
    }
    let count = positions.len();
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, coordinates)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count]),
    )
}

fn edge_planes(points: &mut [Vec3], half_width: f32) -> Option<Vec<[Vec3; 2]>> {
    if points.len() < 2 {
        return None;
    }
    let mut planes = Vec::with_capacity(points.len() - 1);
    let mut switched = false;
    for index in 0..points.len() - 1 {
        let point = points[index];
        if (points[index + 1] - point).abs().max_element() < 0.000_01 {
            points[index + 1].x += 0.000_1;
        }
        let next = points[index + 1];
        let direction = (next - point).normalize_or_zero();
        let nearest = point - direction * direction.dot(point);
        let offset = direction.cross(nearest).normalize_or_zero() * half_width;
        let top = (point + offset).cross(direction).normalize_or_zero();
        let bottom = direction.cross(point - offset).normalize_or_zero();
        if top == Vec3::ZERO || bottom == Vec3::ZERO {
            return None;
        }
        if index > 0
            && points[index - 1]
                .cross(point)
                .normalize_or_zero()
                .dot(point.cross(next).normalize_or_zero())
                < 0.0
        {
            switched = !switched;
        }
        planes.push(if switched {
            [-bottom, -top]
        } else {
            [top, bottom]
        });
    }
    Some(planes)
}

fn projected_direction(point: Vec3, plane: Vec3) -> Vec3 {
    (point - plane * plane.dot(point)).normalize_or_zero()
}

fn intersection_direction(point: Vec3, previous: Vec3, next: Vec3) -> Vec3 {
    let product = previous.dot(next);
    if product.abs() < 0.9 {
        let direction = previous.cross(next).normalize_or_zero();
        if direction.dot(point) < 0.0 {
            -direction
        } else {
            direction
        }
    } else {
        let plane = if product > 0.0 {
            previous + next
        } else {
            previous - next
        };
        projected_direction(point, plane.normalize_or_zero())
    }
}

#[cfg(test)]
mod tests {
    use super::{edge_planes, line_mesh};
    use bevy::{mesh::VertexAttributeValues, prelude::*};

    #[test]
    fn straight_segment_has_source_width_and_original_texture_coordinates() {
        let mesh = line_mesh(
            &[Vec3::new(-1.0, 0.0, -10.0), Vec3::new(1.0, 0.0, -10.0)],
            &GlobalTransform::IDENTITY,
            0.075,
        )
        .expect("segment");
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let width = Vec3::from_array(vertices[0]).distance(Vec3::from_array(vertices[1]));
        assert!((width - 0.15).abs() < 0.0001);
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("UVs");
        };
        assert_eq!(
            uv,
            &[
                [0.0, 1.0],
                [1.0, 1.0],
                [0.0, 0.0],
                [1.0, 1.0],
                [1.0, 0.0],
                [0.0, 0.0]
            ]
        );
    }

    #[test]
    fn revealed_enemy_line_uses_native_three_unit_width() {
        let mesh = line_mesh(
            &[Vec3::new(-1.0, 0.0, -10.0), Vec3::new(1.0, 0.0, -10.0)],
            &GlobalTransform::IDENTITY,
            0.15,
        )
        .unwrap();
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let width = Vec3::from_array(vertices[0]).distance(Vec3::from_array(vertices[1]));
        assert!((width - 0.3).abs() < 0.0001);
    }

    #[test]
    fn coincident_nodes_use_original_perturbation_without_nonfinite_vertices() {
        let mut points = [Vec3::new(0.0, 0.0, -10.0); 2];
        assert!(edge_planes(&mut points, 0.075).is_some());
        assert!(points[1].x > points[0].x);
        assert!(line_mesh(&points, &GlobalTransform::IDENTITY, 0.075).is_some());
        assert!(line_mesh(&[Vec3::ZERO], &GlobalTransform::IDENTITY, 0.075).is_none());
    }
}
