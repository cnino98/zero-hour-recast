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
        transform.set_if_neq(projectile_transform(position, axes));
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
        let unit = &battle.simulation.entities[projectile.source];
        let Some(model) = data.models[unit.type_index][projectile.weapon_index] else {
            continue;
        };
        let entity = commands
            .spawn((
                ProjectileVisual {
                    id: projectile.id,
                    generation: battle.generation,
                },
                projectile_transform(position, axes),
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

fn projectile_transform(position: [f32; 3], axes: [[f32; 3]; 3]) -> Transform {
    let convert = |value: [f32; 3]| Vec3::new(value[0], value[2], -value[1]);
    let rotation = Quat::from_mat3(&Mat3::from_cols(
        convert(axes[0]),
        convert(axes[2]),
        -convert(axes[1]),
    ));
    Transform {
        translation: convert(position) * 0.1,
        rotation,
        ..default()
    }
}
