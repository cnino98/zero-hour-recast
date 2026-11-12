use crate::{
    input::PickTarget,
    model_render::Pickable,
    playtest::Match,
    playtest_view::{self, SupplyVisual, UnitVisual, WorldCamera},
    terrain::Terrain,
};
use bevy::{
    ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*, window::PrimaryWindow,
};

#[derive(SystemParam)]
pub struct View<'w, 's> {
    pub window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    pub camera: Single<
        'w,
        's,
        (
            &'static Camera,
            &'static GlobalTransform,
            &'static Projection,
        ),
        With<WorldCamera>,
    >,
    pub terrain: Res<'w, Terrain>,
    pub ui: Res<'w, crate::control_bar::UiData>,
    pub promotions: Res<'w, crate::playtest_ui::PromotionTransition>,
    meshes: Res<'w, Assets<Mesh>>,
    parts: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static GlobalTransform,
            &'static ChildOf,
            &'static InheritedVisibility,
        ),
        With<Pickable>,
    >,
    units: Query<'w, 's, &'static UnitVisual>,
    supplies: Query<'w, 's, &'static SupplyVisual>,
}

impl View<'_, '_> {
    pub(super) fn project(&self, battle: &Match, id: usize) -> Option<Vec2> {
        battle.simulation.entities.get(id)?;
        self.camera
            .0
            .world_to_viewport(
                self.camera.1,
                playtest_view::unit_position(&battle.simulation, id),
            )
            .ok()
    }

    pub(crate) fn terrain_point(&self, cursor: Vec2) -> Option<Vec2> {
        let ray = self
            .camera
            .0
            .viewport_to_world(self.camera.1, cursor)
            .ok()?;
        // W3DView::screenToTerrain passes the contact coordinates unchanged;
        // converting to a navigation cell here incorrectly snapped every click.
        self.terrain.ray_point(ray).map(|point| point * 10.0)
    }

    pub(crate) fn world_point(&self, battle: &Match, cursor: Vec2) -> Option<Vec3> {
        let (camera, transform, projection) = *self.camera;
        let ray = camera.viewport_to_world(transform, cursor).ok()?;
        let Projection::Perspective(projection) = projection else {
            return None;
        };
        let ground = self
            .terrain
            .ray_point(ray)
            .map(|point| Vec3::new(point.x, point.y, self.terrain.sample(point)) * 10.0);
        let source = |point: Vec3| [point.x * 10.0, -point.z * 10.0, point.y * 10.0];
        let from = transform.translation();
        let to = from + *ray.direction * (2.0 * projection.far);
        let bridge = battle.simulation.pick_bridge(source(from), source(to));
        // W3DView::screenToTerrain replaces a terrain contact only with a
        // higher bridge contact, using the original XYZ command coordinates.
        if let Some(point) = bridge
            && point[2] > ground.map_or(0.0, |point| point.z)
        {
            return Some(Vec3::from_array(point));
        }
        ground
    }

    pub(crate) fn pick(&self, battle: &Match, cursor: Vec2) -> Option<PickTarget> {
        self.pick_context(battle, cursor, false)
    }

    pub(crate) fn pick_context(
        &self,
        battle: &Match,
        cursor: Vec2,
        force_attack: bool,
    ) -> Option<PickTarget> {
        let (camera, camera_transform, projection) = *self.camera;
        let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
        let Projection::Perspective(projection) = projection else {
            return None;
        };
        // W3DView::getPickRay uses the camera position and twice its far depth.
        let start = camera_transform.translation();
        let delta = *ray.direction * (2.0 * projection.far);
        let mut closest = 1.0;
        let mut selected = None;
        for (mesh, transform, parent, visible) in &self.parts {
            if !visible.get() {
                continue;
            }
            let Some(target) = self.target(battle, parent.parent(), force_attack) else {
                continue;
            };
            let Some(mesh) = self.meshes.get(&mesh.0) else {
                continue;
            };
            let Some(VertexAttributeValues::Float32x3(vertices)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            // Model parts are imported triangle lists with no index buffer.
            let inverse = transform.affine().inverse();
            let origin = inverse.transform_point3(start);
            let direction = inverse.transform_vector3(delta);
            for triangle in vertices.chunks_exact(3) {
                if let Some(fraction) = intersect_triangle(
                    origin,
                    direction,
                    [triangle[0], triangle[1], triangle[2]].map(Vec3::from_array),
                ) && fraction < closest
                {
                    closest = fraction;
                    selected = Some(target);
                }
            }
        }
        selected
    }

    fn target(&self, battle: &Match, parent: Entity, force_attack: bool) -> Option<PickTarget> {
        if !force_attack && let Ok(visual) = self.supplies.get(parent) {
            let object = battle.simulation.supplies[visual.0].object;
            return (battle.simulation.entities[object].exists()
                && !matches!(
                    battle.simulation.visibility_status(0, object),
                    generals_sim::skirmish::visibility::Status::Shrouded
                ))
            .then_some(PickTarget::Supply(visual.0));
        }
        if let Ok(visual) = self.units.get(parent) {
            let unit = battle.simulation.entities.get(visual.index)?;
            if visual.generation == battle.generation && unit.crate_present() {
                return battle
                    .simulation
                    .crate_visible_to(0, visual.index)
                    .then_some(PickTarget::Unit(visual.index));
            }
            let stats = &battle.simulation.catalog[unit.type_index];
            let force_pick = force_attack
                && stats.kind_of.iter().any(|kind| kind == "FORCEATTACKABLE")
                && !stats.kind_of.iter().any(|kind| kind == "CLICK_THROUGH")
                && unit.alive()
                && !unit.masked();
            return (visual.generation == battle.generation
                && (force_pick
                    || unit.selectable(stats)
                        && !unit.masked()
                        && !unit.unselectable()
                        && !unit.drawable_effectively_hidden()
                        && (stats.selectable
                            || !stats.kind_of.iter().any(|kind| kind == "FORCEATTACKABLE")))
                && battle.simulation.logically_visible(0, visual.index)
                && matches!(
                    battle.simulation.visibility_status(0, visual.index),
                    generals_sim::skirmish::visibility::Status::Clear
                        | generals_sim::skirmish::visibility::Status::Partial
                ))
            .then_some(PickTarget::Unit(visual.index));
        }
        self.supplies
            .get(parent)
            .ok()
            .filter(|visual| {
                !matches!(
                    battle
                        .simulation
                        .visibility_cell(0, battle.simulation.supply_position(visual.0)),
                    generals_sim::skirmish::visibility::Status::Shrouded
                )
            })
            .map(|visual| PickTarget::Supply(visual.0))
    }

    pub(crate) fn supply_screen_point(&self, battle: &Match, source: usize) -> Option<Vec2> {
        // Diagnostic chooses a visible triangle, then uses the ordinary picking
        // ray. This does not assume that the terrain under a depot is its target.
        for (mesh, transform, parent, visible) in &self.parts {
            if !visible.get()
                || !self
                    .supplies
                    .get(parent.parent())
                    .is_ok_and(|supply| supply.0 == source)
            {
                continue;
            }
            let mesh = self.meshes.get(&mesh.0)?;
            let Some(VertexAttributeValues::Float32x3(vertices)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            for triangle in vertices.chunks_exact(3) {
                let center = triangle.iter().copied().map(Vec3::from_array).sum::<Vec3>() / 3.0;
                let center = transform.transform_point(center);
                let Ok(screen) = self.camera.0.world_to_viewport(self.camera.1, center) else {
                    continue;
                };
                if self
                    .camera
                    .0
                    .logical_viewport_rect()
                    .is_some_and(|area| area.contains(screen))
                    && matches!(self.pick(battle, screen), Some(PickTarget::Supply(id)) if id == source)
                {
                    return Some(screen);
                }
            }
        }
        None
    }
}

fn intersect_triangle(origin: Vec3, delta: Vec3, vertices: [Vec3; 3]) -> Option<f32> {
    // CollisionMath::Collide(LineSeg,Tri) and TriClass::Contains_Point: plane
    // intersection followed by the dominant-plane edge tests, with no backface cull.
    let normal = (vertices[1] - vertices[0])
        .cross(vertices[2] - vertices[0])
        .try_normalize()?;
    let denominator = normal.dot(delta);
    if denominator == 0.0 {
        return None;
    }
    let fraction = -(normal.dot(origin) - normal.dot(vertices[0])) / denominator;
    if !(0.0..=1.0).contains(&fraction) {
        return None;
    }
    let point = origin + delta * fraction;
    let magnitude = normal.abs();
    let axes = if magnitude.y > magnitude.x {
        if magnitude.z > magnitude.y {
            [0, 1]
        } else {
            [0, 2]
        }
    } else if magnitude.z > magnitude.x {
        [0, 1]
    } else {
        [1, 2]
    };
    let mut sides = [false; 3];
    for index in 0..3 {
        let edge = vertices[(index + 1) % 3] - vertices[index];
        let relative = point - vertices[index];
        let products = [
            edge[axes[0]] * relative[axes[1]],
            edge[axes[1]] * relative[axes[0]],
        ];
        sides[index] = products[0] - products[1] >= 0.0;
    }
    (sides[0] == sides[1] && sides[1] == sides[2]).then_some(fraction)
}

#[cfg(test)]
mod tests {
    use super::intersect_triangle;
    use bevy::prelude::*;

    #[test]
    fn ray_rejects_empty_corner_of_model_bounds_and_accepts_both_faces() {
        let triangle = [Vec3::ZERO, Vec3::X * 10.0, Vec3::Y * 10.0];
        assert_eq!(
            intersect_triangle(Vec3::new(8.0, 8.0, 5.0), Vec3::NEG_Z * 10.0, triangle),
            None
        );
        assert_eq!(
            intersect_triangle(Vec3::new(2.0, 2.0, 5.0), Vec3::NEG_Z * 10.0, triangle),
            Some(0.5)
        );
        assert_eq!(
            intersect_triangle(Vec3::new(2.0, 2.0, -5.0), Vec3::Z * 10.0, triangle),
            Some(0.5)
        );
    }

    #[test]
    fn ray_rejects_parallel_degenerate_and_behind_camera_triangles() {
        let triangle = [Vec3::ZERO, Vec3::X, Vec3::Y];
        assert_eq!(intersect_triangle(Vec3::Z, Vec3::X, triangle), None);
        assert_eq!(intersect_triangle(Vec3::Z, Vec3::Z, triangle), None);
        assert_eq!(
            intersect_triangle(Vec3::Z, Vec3::NEG_Z, [Vec3::ZERO; 3]),
            None
        );
    }
}
