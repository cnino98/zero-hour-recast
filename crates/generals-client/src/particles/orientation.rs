//! `W3DInGameUI::drawPlaceAngle`: original `Locater01`/`Locater02` models.
use crate::{playtest::Match, playtest_view::Geometry, terrain::Terrain};
use bevy::prelude::*;
use generals_assets::{virtual_assets::VirtualAssets, w3d::Model};

#[derive(Resource)]
pub struct Orientation {
    models: [usize; 2],
}
#[derive(Component)]
pub struct Indicator {
    model: usize,
}

impl Orientation {
    pub fn matches(&self, indicator: &Indicator, arrow: bool) -> bool {
        indicator.model == self.models[usize::from(arrow)]
    }
}

pub fn load(
    assets: &VirtualAssets,
    models: &mut Vec<Model>,
) -> Result<Orientation, Box<dyn std::error::Error>> {
    let first = models.len();
    for name in ["locater01", "locater02"] {
        models.push(crate::skirmish_data::state_model(assets, name, &[])?);
    }
    Ok(Orientation {
        models: [first, first + 1],
    })
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    data: Res<Orientation>,
    geometry: Res<Geometry>,
    terrain: Res<Terrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut visuals: Query<(Entity, &Indicator, &mut Transform)>,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let geometry = geometry.into_inner();
    let terrain = terrain.into_inner();
    let pose = battle
        .placing
        .as_ref()
        .and_then(crate::input::placement::Placement::orientation_indicator)
        .filter(|_| !battle.paused);
    let mut present = false;
    for (entity, indicator, mut transform) in &mut visuals {
        if let Some((point, angle, arrow)) = pose.filter(|_| battle.started)
            && indicator.model == data.models[usize::from(arrow)]
        {
            *transform = placement_transform(point, angle, terrain);
            present = true;
            continue;
        }
        commands.entity(entity).despawn();
    }
    if !present
        && battle.started
        && let Some((point, angle, arrow)) = pose
    {
        let model = data.models[usize::from(arrow)];
        let entity = commands
            .spawn((
                Indicator { model },
                placement_transform(point, angle, terrain),
                Visibility::Inherited,
            ))
            .id();
        geometry.spawn_unit_parts(
            &mut commands,
            entity,
            model,
            None,
            &mut meshes,
            &crate::model_animation::Frame {
                supply: None,
                meshes: None,
                height_offset: 0.0,
            },
        );
    }
}

fn placement_transform(point: Vec2, angle: f32, terrain: &Terrain) -> Transform {
    let p = point * 0.1;
    Transform::from_translation(Vec3::new(p.x, terrain.sample(p), -p.y))
        .with_rotation(Quat::from_rotation_y(angle))
}
