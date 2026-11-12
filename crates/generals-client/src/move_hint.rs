//! Original `HintSpy`/`InGameUI`/`W3DInGameUI` feedback, `SuperHackers` 2bfaea60.
use crate::{
    input::Mouse,
    model_render::{self, Part, PartBinding},
    playtest::Match,
    source_material::SourceMaterial,
    source_texture,
    terrain::Terrain,
};
use bevy::{camera::visibility::RenderLayers, prelude::*};
use generals_assets::{
    content,
    map::MapLighting,
    virtual_assets::VirtualAssets,
    w3d::{Model, animation::Clip},
};
use num_traits::ToPrimitive;
use std::error::Error;

#[derive(Resource)]
pub struct MoveHints {
    model: Model,
    clip: Clip,
    pending: Vec<(Part, Option<Image>)>,
    parts: Vec<HintPart>,
    hints: Vec<Hint>,
    next: usize,
    generation: u32,
}

struct HintPart {
    mesh: Handle<Mesh>,
    material: Handle<SourceMaterial>,
    binding: PartBinding,
}

struct Hint {
    entity: Entity,
    parts: Vec<(Entity, Handle<Mesh>)>,
    tick: Option<u64>,
    last_sync_ms: u32,
    frame: f32,
}

pub fn load(
    assets: &VirtualAssets,
    lighting: Option<&MapLighting>,
) -> Result<MoveHints, Box<dyn Error>> {
    let data = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let name = data
        .declarations
        .iter()
        .filter(|item| item.kind == "GameData")
        .flat_map(|item| &item.lines)
        .rev()
        .find(|line| line.tokens.first().is_some_and(|key| key == "MoveHintName"))
        .and_then(|line| line.tokens.get(1))
        .ok_or("GameData lacks MoveHintName")?;
    let bytes = assets.read(&format!("art/w3d/{name}.w3d"))?.bytes;
    let hierarchy = Model::hierarchy_name(&bytes)?
        .map(|name| assets.read(&format!("art/w3d/{name}.w3d")))
        .transpose()?;
    let model =
        Model::parse_with_hierarchy(&bytes, hierarchy.as_ref().map(|file| file.bytes.as_slice()))?;
    let clip = Clip::parse(&bytes)?;
    let mut parts = model_render::parts(&model)?;
    if let Some(lighting) = lighting {
        let lights = &lighting
            .active()
            .ok_or("invalid move hint map lighting")?
            .objects;
        for part in &mut parts {
            let params = &mut part.material.parameters;
            params.scene_ambient = Vec3::from_array(lights[0].ambient).extend(0.0);
            for (index, light) in lights.iter().enumerate() {
                params.light_color[index] = Vec3::from_array(light.diffuse).extend(0.0);
                let [x, y, z] = light.position;
                params.light_direction[index] = Vec3::new(x, z, -y).normalize_or_zero().extend(0.0);
            }
        }
    }
    let pending = parts
        .into_iter()
        .map(|part| {
            let texture = part
                .texture
                .as_ref()
                .map(|(name, _)| {
                    let mut image = source_texture::load(assets, name)?;
                    image.texture_descriptor.format =
                        image.texture_descriptor.format.remove_srgb_suffix();
                    Ok::<_, Box<dyn Error>>(image)
                })
                .transpose()?;
            Ok((part, texture))
        })
        .collect::<Result<_, Box<dyn Error>>>()?;
    // Evaluate once before opening the window: malformed rig bindings cannot
    // become a delayed failure on the user's first movement command.
    model.pose(&clip, 0.0)?;
    Ok(MoveHints {
        model,
        clip,
        pending,
        parts: Vec::new(),
        hints: Vec::new(),
        next: 0,
        generation: 0,
    })
}

pub fn setup(
    mut hints: ResMut<MoveHints>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for (part, image) in std::mem::take(&mut hints.pending) {
        let mut material = part.material;
        material.texture = image.map(|image| images.add(image));
        hints.parts.push(HintPart {
            mesh: meshes.add(part.mesh),
            material: materials.add(material),
            binding: part
                .binding
                .expect("move hint model part has a source binding"),
        });
    }
}

pub fn update(
    mut commands: Commands,
    mut hints: ResMut<MoveHints>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mouse: ResMut<Mouse>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    clock: Res<crate::model_animation::Clock>,
) {
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    let milliseconds = clock.into_inner().milliseconds();
    let hints = hints.as_mut();
    if hints.generation != battle.generation || !battle.started {
        for hint in &mut hints.hints {
            hint.tick = None;
        }
        hints.next = 0;
        hints.generation = battle.generation;
    }
    if !battle.started {
        mouse.move_hints.clear();
    }
    for point in std::mem::take(&mut mouse.move_hints) {
        if hints.next == hints.hints.len() {
            hints
                .hints
                .push(spawn(&mut commands, &mut meshes, &hints.parts));
        }
        let hint = &mut hints.hints[hints.next];
        hint.tick = Some(battle.simulation.tick);
        hint.last_sync_ms = milliseconds;
        hint.frame = 0.0;
        commands.entity(hint.entity).insert((
            terrain.aligned_transform(Vec2::from_array(point) * 0.1),
            Visibility::Visible,
        ));
        hints.next = (hints.next + 1) % 256;
    }
    for hint in &mut hints.hints {
        if !hint
            .tick
            .is_some_and(|tick| alive(tick, battle.simulation.tick))
        {
            commands.entity(hint.entity).insert(Visibility::Hidden);
            continue;
        }
        // W3DInGameUI::drawMoveHints uses Set_Animation(ANIM_MODE_ONCE),
        // sharing HAnim's WW3D clock while retaining its separate tick lifetime.
        hint.advance(
            milliseconds,
            hints.clip.frame_rate,
            hints.clip.frame_count - 1,
        );
        let pose = hints
            .model
            .pose(&hints.clip, hint.frame)
            .expect("validated move hint animation");
        for ((entity, mesh), part) in hint.parts.iter().zip(&hints.parts) {
            let mut mesh = meshes.get_mut(mesh).expect("live move hint mesh");
            let visible = model_render::deform(&mut mesh, &part.binding, &pose)
                .expect("validated move hint geometry");
            commands.entity(*entity).insert(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

impl Hint {
    fn advance(&mut self, milliseconds: u32, rate: u16, last: u16) {
        let elapsed = milliseconds.wrapping_sub(self.last_sync_ms);
        let animation_ms =
            f32::from(rate) * elapsed.to_f32().expect("native u32 animation interval");
        let delta = animation_ms * 0.001;
        self.frame = (self.frame + delta).min(f32::from(last));
        self.last_sync_ms = milliseconds;
    }
}

fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, templates: &[HintPart]) -> Hint {
    let mut parts = Vec::new();
    let entity = commands
        .spawn((Transform::default(), Visibility::Hidden))
        .with_children(|parent| {
            for part in templates {
                let mesh = meshes.add(
                    meshes
                        .get(&part.mesh)
                        .expect("move hint template mesh")
                        .clone(),
                );
                let entity = parent
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(part.material.clone()),
                        RenderLayers::layer(2),
                    ))
                    .id();
                parts.push((entity, mesh));
            }
        })
        .id();
    Hint {
        entity,
        parts,
        tick: None,
        last_sync_ms: 0,
        frame: 0.0,
    }
}

const fn alive(created: u64, frame: u64) -> bool {
    frame.saturating_sub(created) <= 40
}

#[cfg(test)]
mod tests {
    use super::alive;
    #[test]
    fn original_move_hint_lasts_through_frame_forty() {
        assert!(alive(50, 50));
        assert!(alive(50, 90));
        assert!(!alive(50, 91));
    }
}

#[cfg(test)]
mod clock_tests {
    use super::Hint;
    use bevy::prelude::Entity;

    #[test]
    fn move_hint_uses_fractional_logic_time_holds_paused_and_clamps_once_endpoint() {
        let mut hint = Hint {
            entity: Entity::PLACEHOLDER,
            parts: Vec::new(),
            tick: Some(40),
            last_sync_ms: 1000,
            frame: 0.0,
        };
        hint.advance(1008, 30, 30);
        assert!((hint.frame - 0.24).abs() < 0.000_01);
        assert_eq!(hint.tick, Some(40));
        let held = hint.frame.to_bits();
        hint.advance(1008, 30, 30);
        assert_eq!(hint.frame.to_bits(), held);
        hint.advance(1041, 30, 30);
        assert!((hint.frame - 1.23).abs() < 0.000_01);
        hint.advance(2400, 30, 30);
        assert_eq!(hint.frame.to_bits(), 30.0_f32.to_bits());
    }

    #[test]
    fn move_hint_elapsed_time_wraps_like_hanim_unsigned_clock() {
        let mut hint = Hint {
            entity: Entity::PLACEHOLDER,
            parts: Vec::new(),
            tick: Some(40),
            last_sync_ms: u32::MAX - 5,
            frame: 0.0,
        };
        hint.advance(3, 30, 30);
        assert!((hint.frame - 0.27).abs() < 0.000_01);
    }
}
