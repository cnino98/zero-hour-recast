//! `JetAIUpdate`'s independent original `LockonCursor` drawable, SH 2bfaea60.
use crate::{
    model_animation::Frame,
    playtest::Match,
    playtest_view::Geometry,
    skirmish_data::{blocks, definition, field, finite_number, scalar},
    sorted_view::{SourceScale, source_scale},
};
use bevy::prelude::*;
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::{UnitType, aircraft::lockon::Drawable};
use std::{collections::BTreeSet, error::Error};

#[derive(Resource)]
pub struct LockonCursors {
    scales: Vec<Option<f32>>,
}

#[derive(Component)]
pub struct LockonVisual {
    object: usize,
    template: usize,
    generation: u32,
}

pub fn load(assets: &VirtualAssets, catalog: &[UnitType]) -> Result<LockonCursors, Box<dyn Error>> {
    let selected: Vec<_> = catalog
        .iter()
        .filter_map(|unit| unit.aircraft.as_ref())
        .map(|rules| rules.lockon.cursor.as_str())
        .filter(|name| !name.is_empty())
        .collect();
    let (files, _) = crate::skirmish_data::source_files::effective(assets, &selected)?;
    let mut scales = vec![None; catalog.len()];
    for name in selected {
        let Some(kind) = catalog.iter().position(|unit| unit.name == name) else {
            continue;
        };
        let object = definition(&files, "Object", name)?;
        let draws = blocks(&object.lines, 0, "Draw")?;
        if draws.len() != 1
            || draws[0][0]
                .tokens
                .get(1)
                .is_none_or(|family| family != "W3DModelDraw")
        {
            return Err(
                format!("{name}: lockon cursor requires its native static W3DModelDraw").into(),
            );
        }
        if draws[0].iter().any(|line| {
            line.tokens.first().is_some_and(|key| {
                matches!(
                    key.as_str(),
                    "Animation"
                        | "IdleAnimation"
                        | "AnimationMode"
                        | "ParticleSysBone"
                        | "TransitionState"
                )
            })
        }) {
            return Err(format!(
                "{name}: animated lockon drawable requires unbound drawable animation state"
            )
            .into());
        }
        let scale = field(&object.lines, 0, "Scale")
            .map_or(Ok(1.0), |line| finite_number(scalar(line)?))?;
        scales[kind] = Some(source_scale(scale));
    }
    Ok(LockonCursors { scales })
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    cursors: Res<LockonCursors>,
    geometry: Res<Geometry>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut visuals: Query<(Entity, &LockonVisual, &mut Transform, &mut Visibility)>,
) {
    let battle = battle.into_inner();
    let cursors = cursors.into_inner();
    let geometry = geometry.into_inner();
    let mut present = BTreeSet::new();
    for (entity, visual, mut transform, mut visibility) in &mut visuals {
        let drawable = (visual.generation == battle.generation)
            .then(|| battle.simulation.entities.get(visual.object))
            .flatten()
            .filter(|actor| actor.exists())
            .and_then(generals_sim::skirmish::Entity::lockon_drawable)
            .filter(|drawable| drawable.template == visual.template);
        let Some(drawable) = drawable else {
            commands.entity(entity).despawn();
            continue;
        };
        let scale = cursors.scales[drawable.template].expect("validated cursor drawable");
        transform.set_if_neq(pose(drawable, scale));
        visibility.set_if_neq(if drawable.hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        present.insert(visual.object);
    }
    for (id, actor) in battle.simulation.entities.iter().enumerate() {
        if !actor.exists() || present.contains(&id) {
            continue;
        }
        let Some(drawable) = actor.lockon_drawable() else {
            continue;
        };
        let scale = cursors.scales[drawable.template].expect("validated cursor drawable");
        let entity = commands
            .spawn((
                LockonVisual {
                    object: id,
                    template: drawable.template,
                    generation: battle.generation,
                },
                pose(drawable, scale),
                SourceScale(scale),
                if drawable.hidden {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                },
            ))
            .id();
        // Catalog base-model indices precede all auxiliary animation models.
        // A newDrawable has no Object/house-color binding or selection identity.
        geometry.spawn_unit_parts(
            &mut commands,
            entity,
            drawable.template,
            None,
            &mut meshes,
            &Frame {
                supply: None,
                meshes: None,
                height_offset: 0.0,
            },
        );
    }
}

fn pose(drawable: &Drawable, scale: f32) -> Transform {
    Transform::from_translation(Vec3::new(
        drawable.position[0] * 0.1,
        drawable.position[2] * 0.1,
        -drawable.position[1] * 0.1,
    ))
    .with_rotation(Quat::from_rotation_y(drawable.facing))
    .with_scale(Vec3::splat(scale))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_cursor_retains_native_altitude_orientation_and_authored_scale() {
        let drawable = Drawable {
            template: 0,
            position: [100.0, 200.0, 300.0],
            facing: std::f32::consts::FRAC_PI_2,
            hidden: false,
        };
        let pose = pose(&drawable, 0.5);
        assert_eq!(pose.translation, Vec3::new(10.0, 30.0, -20.0));
        assert_eq!(pose.scale, Vec3::splat(0.5));
        assert_eq!(
            pose.rotation,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
        );
    }
}
