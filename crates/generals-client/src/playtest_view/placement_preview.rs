//! `InGameUI::placeBuildAvailable`/`destroyPlacementIcons`: only the active temporary draw.
use super::{Geometry, Match, SourceMaterial, Terrain};
use bevy::{camera::visibility::RenderLayers, prelude::*};

#[cfg(test)]
mod tests;

#[derive(Component)]
pub struct Placement {
    pub model: usize,
    pub legal: bool,
}

struct Active {
    generation: u32,
    model: usize,
    roots: [Entity; 2],
    materials: Vec<Handle<SourceMaterial>>,
}

#[derive(Resource)]
pub struct Previews {
    opacity: f32,
    active: Option<Active>,
}

type PreviewQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Placement,
        &'static mut Transform,
        &'static mut Visibility,
    ),
>;

impl Previews {
    pub(super) const fn new(opacity: f32) -> Self {
        Self {
            opacity,
            active: None,
        }
    }

    fn reconcile(
        &mut self,
        requested: Option<(u32, usize)>,
        pose: Option<(Transform, bool)>,
        commands: &mut Commands,
        geometry: &Geometry,
        materials: &mut Assets<SourceMaterial>,
    ) {
        if self
            .active
            .as_ref()
            .map(|active| (active.generation, active.model))
            == requested
        {
            return;
        }
        if let Some(active) = self.active.take() {
            for root in active.roots {
                commands.entity(root).despawn();
            }
            for material in active.materials {
                materials.remove(material.id());
            }
        }
        let Some((generation, model)) = requested else {
            return;
        };
        let mut owned_materials = Vec::new();
        let roots = [true, false].map(|legal| {
            let (transform, visibility) = presentation(pose, legal);
            commands
                .spawn((Placement { model, legal }, transform, visibility))
                .with_children(|parent| {
                    for part in &geometry.models[model] {
                        let Some(source) = materials.get(&part.materials[1]) else {
                            continue;
                        };
                        let material = materials.add(preview_material(source, self.opacity, legal));
                        owned_materials.push(material.clone());
                        parent.spawn((
                            Mesh3d(part.mesh.clone()),
                            MeshMaterial3d(material),
                            RenderLayers::layer(2),
                        ));
                    }
                })
                .id()
        });
        self.active = Some(Active {
            generation,
            model,
            roots,
            materials: owned_materials,
        });
    }

    fn display(&self, pose: Option<(Transform, bool)>, previews: &mut PreviewQuery) {
        let Some(active) = &self.active else { return };
        for root in active.roots {
            let Ok((preview, mut transform, mut visibility)) = previews.get_mut(root) else {
                continue;
            };
            let (current, visible) = presentation(pose, preview.legal);
            if pose.is_some() {
                transform.set_if_neq(current);
            }
            visibility.set_if_neq(visible);
        }
    }
}

fn presentation(pose: Option<(Transform, bool)>, legal: bool) -> (Transform, Visibility) {
    pose.map_or(
        (Transform::IDENTITY, Visibility::Hidden),
        |(transform, allowed)| {
            (
                transform,
                if allowed == legal {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
            )
        },
    )
}

fn preview_material(source: &SourceMaterial, opacity: f32, legal: bool) -> SourceMaterial {
    let mut material = source.clone();
    material.alpha_mode = AlphaMode::Blend;
    material.depth_write = false;
    material.parameters.opacity = opacity;
    if !legal {
        // W3DScene applies IllegalBuildColor to every light and ambient.
        let red = Vec4::new(1.0, 0.0, 0.0, 0.0);
        material.parameters.scene_ambient += red;
        for light in &mut material.parameters.light_color {
            *light += red;
        }
    }
    material
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    geometry: Res<Geometry>,
    mut state: ResMut<Previews>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    mut previews: PreviewQuery,
) {
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    let geometry = geometry.into_inner();
    let pending = battle.placing.as_ref().filter(|_| battle.started);
    let requested = pending.map(|pending| (battle.generation, pending.type_index));
    let pose = pending
        .filter(|_| !battle.paused)
        .and_then(crate::input::placement::Placement::pose)
        .map(|(position, angle, legal)| {
            let point = position * 0.1;
            (
                Transform::from_xyz(point.x, terrain.sample(point), -point.y)
                    .with_rotation(Quat::from_rotation_y(angle)),
                legal,
            )
        });
    state.reconcile(requested, pose, &mut commands, geometry, &mut materials);
    state.display(pose, &mut previews);
}
