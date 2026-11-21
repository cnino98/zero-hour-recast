use crate::{model_animation::Frame, playtest::Match, playtest_view::Geometry};
use bevy::prelude::*;

#[derive(Resource)]
pub struct Projectiles {
    pub partition_cell_size: f32,
    pub models: Vec<Vec<Option<usize>>>,
}

#[derive(Component)]
pub struct ProjectileVisual {
    pub id: u64,
    pub generation: u32,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct RenderResources<'w> {
    geometry: Res<'w, Geometry>,
    meshes: ResMut<'w, Assets<Mesh>>,
    timings: Option<Res<'w, crate::render_performance::Timings>>,
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    data: Res<Projectiles>,
    resources: RenderResources,
    mut visuals: Query<(Entity, &ProjectileVisual, &mut Transform)>,
) {
    let RenderResources {
        geometry,
        mut meshes,
        timings,
    } = resources;
    let _span = timings
        .as_deref()
        .map(|timings| timings.span("projectiles"));
    let battle = battle.into_inner();
    let data = data.into_inner();
    let geometry = geometry.into_inner();
    let projectiles: std::collections::BTreeMap<_, _> = battle
        .simulation
        .projectiles
        .iter()
        .map(|projectile| (projectile.id, projectile))
        .collect();
    let mut present = std::collections::BTreeSet::new();
    for (entity, visual, mut transform) in &mut visuals {
        let projectile = projectiles.get(&visual.id).copied();
        if visual.generation != battle.generation || projectile.is_none() {
            commands.entity(entity).despawn();
            continue;
        }
        let Some((position, axes)) = projectile
            .and_then(|projectile| projectile.render_transform(&battle.simulation.entities))
        else {
            commands.entity(entity).despawn();
            continue;
        };
        let projectile = projectile.expect("retained projectile visual");
        let instance = projectile
            .object
            .and_then(|id| battle.simulation.entities[id].drawable_instance_matrix());
        transform.set_if_neq(projectile_transform(position, axes, instance));
        present.insert(visual.id);
    }
    for projectile in &battle.simulation.projectiles {
        if present.contains(&projectile.id) {
            continue;
        }
        let Some((position, axes)) = projectile.render_transform(&battle.simulation.entities)
        else {
            continue;
        };
        // A10 and Artillery OCLs create a VisiblePayloadTemplateName. Their
        // separate detonation Weapon has no ProjectileObject.
        let Some(model) =
            data.models[projectile.weapon_type][projectile.weapon_index].or_else(|| {
                projectile
                    .object
                    .map(|id| battle.simulation.entities[id].type_index)
            })
        else {
            continue;
        };
        let instance = projectile
            .object
            .and_then(|id| battle.simulation.entities[id].drawable_instance_matrix());
        let entity = commands
            .spawn((
                ProjectileVisual {
                    id: projectile.id,
                    generation: battle.generation,
                },
                projectile_transform(position, axes, instance),
                Visibility::Inherited,
            ))
            .id();
        geometry.spawn_unit_parts(
            &mut commands,
            entity,
            model,
            battle
                .simulation
                .player_colors
                .get(projectile.owner)
                .copied()
                .flatten(),
            &mut meshes,
            &Frame {
                supply: None,
                meshes: None,
                height_offset: 0.0,
            },
        );
    }
}

fn projectile_transform(
    position: [f32; 3],
    axes: [[f32; 3]; 3],
    instance: Option<[[f32; 4]; 3]>,
) -> Transform {
    let convert = |value: [f32; 3]| Vec3::new(value[0], value[2], -value[1]);
    let rotation = Quat::from_mat3(&Mat3::from_cols(
        convert(axes[0]),
        convert(axes[2]),
        -convert(axes[1]),
    ));
    let base = Transform {
        translation: convert(position) * 0.1,
        rotation,
        ..default()
    };
    instance.map_or(base, |matrix| {
        // Drawable postmultiplies the instance translation. The missile has
        // already rotated its native jitter through the current launch basis.
        Transform {
            translation: base.translation + rotation * convert(matrix.map(|row| row[3])) * 0.1,
            ..base
        }
    })
}

#[cfg(test)]
mod tests {
    use super::projectile_transform;
    use bevy::prelude::*;

    #[test]
    fn missile_drawable_applies_instance_translation_without_moving_logic_pose() {
        let position = [20.0, 30.0, 40.0];
        let axes = [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let instance = [
            [1.0, 0.0, 0.0, 0.25],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        let base = projectile_transform(position, axes, None);
        let jitter = projectile_transform(position, axes, Some(instance));
        assert!(
            base.translation
                .abs_diff_eq(Vec3::new(2.0, 4.0, -3.0), 0.000_001)
        );
        assert!(
            jitter
                .translation
                .abs_diff_eq(Vec3::new(2.0, 4.0, -3.025), 0.000_001)
        );
        assert_eq!(jitter.rotation, base.rotation);
    }
}
