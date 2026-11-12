//! Camera framing and real mesh bounds for native aircraft capture fixtures.
use crate::{
    playtest::{CameraRig, Match},
    playtest_ui::Picture,
    playtest_view::{UnitVisual, WorldCamera},
    source_material::SourceMaterial,
    terrain::Terrain,
};
use bevy::{
    ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*, window::PrimaryWindow,
};
use std::error::Error;

#[derive(SystemParam)]
pub struct AircraftFrame<'w, 's> {
    terrain: Res<'w, Terrain>,
    animated: Query<'w, 's, &'static crate::model_animation::AnimatedPart>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    hud: Query<
        'w,
        's,
        (
            &'static Picture,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static InheritedVisibility,
        ),
    >,
    roots: Query<
        'w,
        's,
        (
            &'static UnitVisual,
            &'static Children,
            &'static GlobalTransform,
        ),
    >,
    parts: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static GlobalTransform,
            &'static InheritedVisibility,
            &'static ViewVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
    >,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
}

impl AircraftFrame<'_, '_> {
    pub fn physical_pose(&self, battle: &Match, id: usize) -> Result<bool, Box<dyn Error>> {
        let Some((_, _, transform)) = self
            .roots
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        let actor = &battle.simulation.entities[id];
        let basis = actor
            .physical_basis()
            .ok_or("captured hulk lost its physical basis")?;
        Ok(matches_physical_pose(
            transform,
            battle.simulation.object_transform(id).0,
            basis,
            actor.aircraft_draw_height(),
        ))
    }

    pub fn focus(&self, rig: &mut CameraRig, position: [f32; 3]) {
        focus_on(rig, &self.terrain, position);
    }

    pub fn visible(&self, battle: &Match, id: usize) -> Result<bool, Box<dyn Error>> {
        let Some((_, children, _)) = self
            .roots
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        let Some(viewport) = self.uncovered_viewport() else {
            return Ok(false);
        };
        Ok(self
            .child_bounds(children)?
            .is_some_and(|bounds| contained(viewport, bounds)))
    }

    pub fn visible_mesh(
        &self,
        battle: &Match,
        id: usize,
        animations: &crate::model_animation::Animations,
        name: &str,
    ) -> Result<bool, Box<dyn Error>> {
        let Some((_, children, _)) = self
            .roots
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        let matches = |child| {
            self.animated.get(child).is_ok_and(|part| {
                animations
                    .mesh_name(part.model, part.binding.mesh)
                    .is_some_and(|mesh| mesh.eq_ignore_ascii_case(name))
            })
        };
        let mut found = false;
        for child in children.iter().filter(|&child| matches(child)) {
            found = true;
            let (mesh, _, inherited, visible, material) = self.parts.get(child)?;
            let mesh = self.meshes.get(&mesh.0).ok_or("body mesh missing")?;
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return Err("body mesh lacks original positions".into());
            };
            let material = self
                .materials
                .get(&material.0)
                .ok_or("body material missing")?;
            if !inherited.get()
                || !visible.get()
                || !solid(mesh, points)?
                || material.parameters.opacity <= 0.0
            {
                return Ok(false);
            }
        }
        let Some(viewport) = self.uncovered_viewport() else {
            return Ok(false);
        };
        Ok(found
            && self
                .child_bounds_matching(children, matches, true)?
                .is_some_and(|bounds| contained(viewport, bounds)))
    }

    /// Project only the named original mesh for diagnostic camera fitting.
    /// Culled geometry is measurable here; `visible_mesh` still requires its
    /// actual render visibility and unchanged body/payload pixel gates.
    pub fn mesh_projection(
        &self,
        battle: &Match,
        id: usize,
        animations: &crate::model_animation::Animations,
        name: &str,
    ) -> Result<Option<Rect>, Box<dyn Error>> {
        let Some((_, children, _)) = self
            .roots
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(None);
        };
        self.child_bounds_matching(
            children,
            |child| {
                self.animated.get(child).is_ok_and(|part| {
                    animations
                        .mesh_name(part.model, part.binding.mesh)
                        .is_some_and(|mesh| mesh.eq_ignore_ascii_case(name))
                })
            },
            false,
        )
    }

    pub fn visibility_diagnostic(
        &self,
        battle: &Match,
        id: usize,
    ) -> Result<String, Box<dyn Error>> {
        let Some((visual, children, transform)) = self
            .roots
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(format!(
                "actor={id} generation={} render_root=missing",
                battle.generation
            ));
        };
        let bounds = self.child_bounds(children)?;
        let viewport = self.uncovered_viewport();
        let mut parts = Vec::new();
        for child in children.iter().take(8) {
            let Ok((mesh, world, inherited, visible, material)) = self.parts.get(child) else {
                parts.push(format!("part={child:?} mesh_query=missing"));
                continue;
            };
            let material = self
                .materials
                .get(&material.0)
                .ok_or("diagnostic material missing")?;
            let mesh = self.meshes.get(&mesh.0).ok_or("diagnostic mesh missing")?;
            let geometry = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(points)) => {
                    format!("vertices={} solid={}", points.len(), solid(mesh, points)?)
                }
                _ => "positions=missing".into(),
            };
            let origin = self
                .camera
                .0
                .world_to_viewport(self.camera.1, world.translation());
            parts.push(format!("part={child:?} inherited={} view_visible={} opacity={} {geometry} origin={origin:?}",
                inherited.get(), visible.get(), material.parameters.opacity));
        }
        Ok(format!(
            "actor={id} generation={} root_world={:?} children={} viewport={viewport:?} bounds={bounds:?} size={:?} required_size=(16,12) contained={} parts={parts:?}",
            visual.generation,
            transform.translation(),
            children.len(),
            bounds.map(|bounds| bounds.size()),
            bounds
                .zip(viewport)
                .is_some_and(|(bounds, viewport)| contained(viewport, bounds))
        ))
    }

    /// A projected actor must clear the original geometry of its opaque carrier.
    /// Measure the carrier even after it leaves the viewport; absence of a
    /// projectable mesh is not evidence of separation.
    pub fn separated_actor(
        &self,
        battle: &Match,
        carrier: usize,
        actor: usize,
    ) -> Result<bool, Box<dyn Error>> {
        let children = |id| {
            self.roots.iter().find_map(|(visual, children, _)| {
                (visual.index == id && visual.generation == battle.generation).then_some(children)
            })
        };
        let (Some(carrier), Some(actor), Some(viewport)) = (
            children(carrier),
            children(actor),
            self.uncovered_viewport(),
        ) else {
            return Ok(false);
        };
        let Some(carrier) = self.child_bounds_matching(carrier, |_| true, false)? else {
            return Ok(false);
        };
        let Some(actor) = self.child_bounds(actor)? else {
            return Ok(false);
        };
        Ok(contained(viewport, actor) && separated_bomb(viewport, carrier, actor))
    }

    pub fn separated_projectile(
        &self,
        battle: &Match,
        id: usize,
        projectile: &Children,
    ) -> Result<bool, Box<dyn Error>> {
        let Some((_, children, _)) = self
            .roots
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        let Some(viewport) = self.uncovered_viewport() else {
            return Ok(false);
        };
        let Some(aircraft) = self.child_bounds(children)? else {
            return Ok(false);
        };
        let Some(bomb) = self.child_bounds(projectile)? else {
            return Ok(false);
        };
        Ok(separated_bomb(viewport, aircraft, bomb))
    }

    /// A live projectile must clear every actor silhouette, including its target.
    /// `ViewVisibility` alone cannot detect a missile hidden behind a bunker.
    pub fn isolated_projectile(
        &self,
        battle: &Match,
        projectile: &Children,
    ) -> Result<Option<Rect>, Box<dyn Error>> {
        let Some(viewport) = self.uncovered_viewport() else {
            return Ok(None);
        };
        let Some(bounds) = self.child_bounds(projectile)? else {
            return Ok(None);
        };
        let margin = Vec2::splat(12.0);
        if bounds.min.cmplt(viewport.min + margin).any()
            || bounds.max.cmpgt(viewport.max - margin).any()
            || bounds.size().cmplt(Vec2::splat(4.0)).any()
        {
            return Ok(None);
        }
        for (visual, children, _) in &self.roots {
            if visual.generation != battle.generation {
                continue;
            }
            // An offscreen child cannot erase its actor's other visible parts
            // from the occluder union. Complete-body acceptance above remains
            // stricter and still requires every selected body part to render.
            if let Some(actor) = self.child_bounds_matching(
                children,
                |child| {
                    self.parts
                        .get(child)
                        .is_ok_and(|(_, _, _, visible, _)| visible.get())
                },
                true,
            )? && !separated_bomb(viewport, actor, bounds)
            {
                return Ok(None);
            }
        }
        Ok(Some(bounds))
    }

    fn child_bounds(&self, children: &Children) -> Result<Option<Rect>, Box<dyn Error>> {
        self.child_bounds_matching(children, |_| true, true)
    }

    fn child_bounds_matching(
        &self,
        children: &Children,
        matches: impl Fn(Entity) -> bool,
        require_view: bool,
    ) -> Result<Option<Rect>, Box<dyn Error>> {
        let mut bounds: Option<Rect> = None;
        for child in children {
            if !matches(*child) {
                continue;
            }
            let Ok((mesh, world, inherited, visible, material)) = self.parts.get(*child) else {
                continue;
            };
            if !inherited.get() {
                continue;
            }
            let material = self
                .materials
                .get(&material.0)
                .ok_or("aircraft material missing")?;
            if !material.parameters.opacity.is_finite() {
                return Err("aircraft body has nonfinite opacity".into());
            }
            if material.parameters.opacity <= 0.0 {
                continue;
            }
            let mesh = self.meshes.get(&mesh.0).ok_or("aircraft mesh missing")?;
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return Err("aircraft mesh has no original positions".into());
            };
            if !solid(mesh, points)? {
                continue;
            }
            if require_view && !visible.get() {
                return Ok(None);
            }
            for point in points {
                let point = world.transform_point(Vec3::from_array(*point));
                if !point.is_finite() {
                    return Err("aircraft body has nonfinite world geometry".into());
                }
                let Ok(point) = self.camera.0.world_to_viewport(self.camera.1, point) else {
                    return Ok(None);
                };
                let bounds = bounds.get_or_insert_with(|| Rect::from_corners(point, point));
                bounds.min = bounds.min.min(point);
                bounds.max = bounds.max.max(point);
            }
        }
        Ok(bounds)
    }

    pub fn uncovered_viewport(&self) -> Option<Rect> {
        let viewport = self.camera.0.logical_viewport_rect()?;
        let (_, node, transform, _) = self
            .hud
            .iter()
            .find(|(picture, _, _, visible)| matches!(picture, Picture::Bar) && visible.get())?;
        if node.size().min_element() <= 0.0 {
            return None;
        }
        let top = node.size().y.mul_add(-0.5, transform.translation.y);
        Some(above_hud(viewport, top, self.window.scale_factor()))
    }
}

fn above_hud(mut viewport: Rect, physical_top: f32, scale_factor: f32) -> Rect {
    // WorldCamera's native 80% viewport extends underneath the control-bar art.
    // UI layout bounds are physical pixels; world_to_viewport returns logical pixels.
    viewport.max.y = viewport.max.y.min(physical_top / scale_factor);
    viewport
}

fn matches_physical_pose(
    transform: &GlobalTransform,
    position: [f32; 3],
    basis: [[f32; 3]; 3],
    draw_height: f32,
) -> bool {
    let map = |point: [f32; 3]| Vec3::new(point[0], point[2], -point[1]);
    let expected_position = std::array::from_fn(|axis| {
        let offset = basis[axis][2] * draw_height;
        position[axis] + offset
    });
    let axes = [
        [basis[0][0], basis[1][0], basis[2][0]],
        [basis[0][2], basis[1][2], basis[2][2]],
        [-basis[0][1], -basis[1][1], -basis[2][1]],
    ];
    let affine = transform.affine();
    transform
        .translation()
        .abs_diff_eq(map(expected_position) * 0.1, 0.0001)
        && [Vec3::X, Vec3::Y, Vec3::Z]
            .into_iter()
            .zip(axes)
            .all(|(axis, expected)| {
                affine
                    .transform_vector3(axis)
                    .abs_diff_eq(map(expected), 0.0001)
            })
}

fn solid(mesh: &Mesh, points: &[[f32; 3]]) -> Result<bool, Box<dyn Error>> {
    let indices: Vec<_> = mesh.indices().map_or_else(
        || (0..points.len()).collect(),
        |indices| indices.iter().collect(),
    );
    for triangle in indices.chunks_exact(3) {
        let mut vertices = [Vec3::ZERO; 3];
        for (vertex, &index) in vertices.iter_mut().zip(triangle) {
            *vertex = Vec3::from_array(*points.get(index).ok_or("invalid aircraft mesh index")?);
            if !vertex.is_finite() {
                return Err("nonfinite aircraft mesh vertex".into());
            }
        }
        if (vertices[1] - vertices[0])
            .cross(vertices[2] - vertices[0])
            .length_squared()
            > 0.0
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn separated_bomb(viewport: Rect, aircraft: Rect, bomb: Rect) -> bool {
    let margin = Vec2::splat(12.0);
    let inside = bomb.min.cmpge(viewport.min + margin).all()
        && bomb.max.cmple(viewport.max - margin).all()
        && bomb.size().cmpge(Vec2::splat(4.0)).all();
    inside
        && (bomb.max.x + 12.0 <= aircraft.min.x
            || bomb.min.x >= aircraft.max.x + 12.0
            || bomb.max.y + 12.0 <= aircraft.min.y
            || bomb.min.y >= aircraft.max.y + 12.0)
}

fn contained(viewport: Rect, bounds: Rect) -> bool {
    let margin = Vec2::splat(12.0);
    bounds.min.cmpge(viewport.min + margin).all()
        && bounds.max.cmple(viewport.max - margin).all()
        && bounds.size().cmpge(Vec2::new(16.0, 12.0)).all()
}

pub fn focus_on(rig: &mut CameraRig, terrain: &Terrain, position: [f32; 3]) {
    let point = Vec2::new(position[0], position[1]) * 0.1;
    let height = position[2] * 0.1;
    let mut focus = point;
    // Keep the original camera pitch/yaw/distance. Its ground focus must account
    // for elevation to put the actual aircraft pose on the centre ray.
    for _ in 0..4 {
        focus = elevated_focus(
            point,
            height - terrain.sample(focus),
            rig.settings.pitch,
            rig.settings.yaw,
        );
    }
    rig.focus = focus;
}

fn elevated_focus(point: Vec2, height: f32, pitch: f32, yaw: f32) -> Vec2 {
    let (sine, cosine) = yaw.sin_cos();
    let horizontal = height / pitch.tan();
    point + Vec2::new(-sine, cosine) * horizontal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missile_clear_of_aircraft_but_hidden_by_target_is_not_visual_evidence() {
        let viewport = Rect::new(0.0, 0.0, 1200.0, 700.0);
        let aircraft = Rect::new(300.0, 200.0, 400.0, 280.0);
        let target = Rect::new(550.0, 350.0, 700.0, 480.0);
        let hidden = Rect::new(600.0, 400.0, 615.0, 415.0);
        assert!(separated_bomb(viewport, aircraft, hidden));
        assert!(
            ![aircraft, target]
                .into_iter()
                .all(|actor| separated_bomb(viewport, actor, hidden))
        );
        let clear = Rect::new(470.0, 310.0, 485.0, 325.0);
        assert!(
            [aircraft, target]
                .into_iter()
                .all(|actor| separated_bomb(viewport, actor, clear))
        );
    }

    #[test]
    fn falling_bomb_proof_rejects_fuselage_overlap_and_hud_occlusion() {
        let viewport = Rect::new(0.0, 0.0, 1200.0, 700.0);
        let aircraft = Rect::new(500.0, 200.0, 640.0, 300.0);
        assert!(!separated_bomb(
            viewport,
            aircraft,
            Rect::new(550.0, 270.0, 565.0, 290.0)
        ));
        assert!(!separated_bomb(
            viewport,
            aircraft,
            Rect::new(550.0, 301.0, 565.0, 321.0)
        ));
        assert!(separated_bomb(
            viewport,
            aircraft,
            Rect::new(550.0, 320.0, 565.0, 340.0)
        ));
        assert!(!separated_bomb(
            viewport,
            aircraft,
            Rect::new(550.0, 680.0, 565.0, 720.0)
        ));
        assert!(!separated_bomb(
            viewport,
            aircraft,
            Rect::new(550.0, 320.0, 552.0, 322.0)
        ));
    }
    use bevy::{asset::RenderAssetUsages, render::render_resource::PrimitiveTopology};

    #[test]
    fn physical_hulk_proof_rejects_flat_yaw_and_stale_position() {
        let basis = [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let world = GlobalTransform::from(Mat4::from_cols(
            Vec4::new(0.0, 0.0, -1.0, 0.0),
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, -1.0, 0.0, 0.0),
            Vec4::new(8.5, 4.5, -3.0, 1.0),
        ));
        assert!(matches_physical_pose(
            &world,
            [80.0, 30.0, 45.0],
            basis,
            5.0
        ));
        assert!(!matches_physical_pose(
            &world,
            [81.0, 30.0, 45.0],
            basis,
            5.0
        ));
        assert!(!matches_physical_pose(
            &GlobalTransform::from_translation(world.translation()),
            [80.0, 30.0, 45.0],
            basis,
            5.0,
        ));
    }

    #[test]
    fn elevated_focus_centres_actual_aircraft_height_at_original_camera_angles() {
        let point = Vec2::new(101.25, 187.5);
        let pitch = 37.5_f32.to_radians();
        for yaw in [0.0, 0.6, -1.2] {
            for height in [0.0, 14.0, 28.0] {
                let focus = elevated_focus(point, height, pitch, yaw);
                let focus = Vec3::new(focus.x, 4.0, -focus.y);
                let (sine, cosine) = yaw.sin_cos();
                let horizontal = 90.0 / pitch.tan();
                let offset = Vec3::new(horizontal * sine, 90.0, horizontal * cosine);
                let camera = Transform::from_translation(focus + offset).looking_at(focus, Vec3::Y);
                let actual = Vec3::new(point.x, 4.0 + height, -point.y);
                let view = camera.compute_affine().inverse().transform_point3(actual);
                assert!(view.x.abs() < 0.0001 && view.y.abs() < 0.0001 && view.z < 0.0);
            }
        }
    }

    #[test]
    fn body_proof_rejects_cropped_degenerate_and_tiny_visible_parts() {
        let viewport = Rect::new(0.0, 0.0, 1200.0, 800.0);
        assert!(contained(viewport, Rect::new(400.0, 250.0, 480.0, 290.0)));
        assert!(!contained(viewport, Rect::new(400.0, -10.0, 480.0, 20.0)));
        assert!(!contained(viewport, Rect::new(400.0, 790.0, 480.0, 830.0)));
        assert!(!contained(viewport, Rect::new(400.0, 250.0, 405.0, 255.0)));
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        assert!(!solid(&mesh, &[[0.0; 3]; 3]).unwrap());
        assert!(solid(&mesh, &[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]).unwrap());
        assert!(solid(&mesh, &[[f32::NAN; 3]; 3]).is_err());
    }

    #[test]
    fn body_inside_camera_viewport_but_behind_control_bar_is_rejected() {
        let viewport = Rect::new(0.0, 0.0, 1440.0, 720.0);
        let body = Rect::new(670.0, 605.0, 780.0, 680.0);
        assert!(contained(viewport, body));
        // A 2880x1800 Retina frame has an 80% camera viewport, while the
        // actual control-bar layout starts at physical y=1250.
        let uncovered = above_hud(viewport, 1250.0, 2.0);
        assert!(!contained(uncovered, body));
        assert!(contained(uncovered, Rect::new(670.0, 460.0, 780.0, 535.0)));
        assert_eq!(uncovered, above_hud(viewport, 625.0, 1.0));
    }
}
