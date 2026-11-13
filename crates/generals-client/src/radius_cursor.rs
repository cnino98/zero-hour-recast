//! Original `InGameUI` guard/cleanup radii, `RadiusDecal` pulse and terrain artwork.
use crate::{
    control_bar::UiData,
    input::{CommandMode, Mouse},
    playtest::Match,
    playtest_ui::PromotionTransition,
    playtest_view::WorldCamera,
    radar::Radar,
    skirmish_data::{definition, finite_number, scalar},
    source_material::SourceMaterial,
    source_texture,
    terrain::Terrain,
};
use bevy::{
    camera::visibility::{NoFrustumCulling, RenderLayers},
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    window::PrimaryWindow,
};
use generals_assets::{
    content::{self, ContentLine},
    virtual_assets::VirtualAssets,
};
use std::error::Error;

pub mod cleanup_verification;
pub mod verification;

#[derive(Resource)]
pub struct RadiusCursor {
    styles: [Style; 6],
    current: usize,
    cleanup_radius: f32,
    helix_radius: f32,
    spectre_radii: Vec<(String, f32)>,
    mesh: Handle<Mesh>,
    radius: Option<f32>,
    position: Vec2,
    generation: u32,
}

struct Style {
    template: Template,
    image: Option<Image>,
    material: Handle<SourceMaterial>,
}

impl Style {
    fn update_opacity(&self, materials: &mut Assets<SourceMaterial>, opacity: f32) {
        if materials
            .get(&self.material)
            .is_some_and(|material| material.parameters.opacity.to_bits() != opacity.to_bits())
        {
            materials
                .get_mut(&self.material)
                .expect("radius material")
                .parameters
                .opacity = opacity;
        }
    }
}

impl RadiusCursor {
    pub fn mine_clearing_radius(&self, radius: f32) -> bool {
        self.current == 3 && self.radius.map(f32::to_bits) == Some(radius.to_bits())
    }
}

struct Template {
    texture: String,
    minimum: f32,
    maximum: f32,
    period: u16,
    color: Option<[f32; 3]>,
    only_owner: bool,
}

#[derive(Component)]
pub struct RadiusVisual;

pub fn load(assets: &VirtualAssets) -> Result<RadiusCursor, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/ingameui.ini")?)?];
    let lines = &definition(&files, "InGameUI", "")?.lines;
    let powers = [content::scan(assets.read("data/ini/specialpower.ini")?)?];
    let power = definition(
        &powers,
        "SpecialPower",
        "SpecialAbilityAmbulanceCleanupArea",
    )?;
    let radius = power
        .lines
        .iter()
        .rfind(|line| {
            line.depth == Some(0)
                && line
                    .tokens
                    .first()
                    .is_some_and(|key| key == "RadiusCursorRadius")
        })
        .ok_or("Ambulance special power has no RadiusCursorRadius")?;
    Ok(RadiusCursor {
        styles: [
            load_style(assets, lines, "GuardAreaRadiusCursor")?,
            load_style(assets, lines, "AmbulanceRadiusCursor")?,
            load_style(assets, lines, "AttackScatterAreaRadiusCursor")?,
            load_style(assets, lines, "ClearMinesRadiusCursor")?,
            load_style(assets, lines, "HelixNapalmBombRadiusCursor")?,
            load_style(assets, lines, "SpectreGunshipRadiusCursor")?,
        ],
        current: 0,
        cleanup_radius: finite_number(scalar(radius)?)?,
        helix_radius: finite_number(scalar(
            crate::skirmish_data::field(
                &definition(&powers, "SpecialPower", "SpecialAbilityHelixNapalmBomb")?.lines,
                0,
                "RadiusCursorRadius",
            )
            .ok_or("Helix radius missing")?,
        )?)?,
        spectre_radii: powers[0]
            .declarations
            .iter()
            .filter(|power| power.kind == "SpecialPower")
            .filter(|power| {
                crate::skirmish_data::field(&power.lines, 0, "Enum")
                    .and_then(|line| line.tokens.get(1))
                    .is_some_and(|name| {
                        matches!(
                            name.as_str(),
                            "SPECIAL_SPECTRE_GUNSHIP" | "AIRF_SPECIAL_SPECTRE_GUNSHIP"
                        )
                    })
            })
            .map(|power| {
                let radius = crate::skirmish_data::field(&power.lines, 0, "RadiusCursorRadius")
                    .map_or(Ok(0.0), |line| finite_number(scalar(line)?))?;
                Ok((power.name.clone(), radius))
            })
            .collect::<Result<_, Box<dyn Error>>>()?,
        mesh: Handle::default(),
        radius: None,
        position: Vec2::ZERO,
        generation: u32::MAX,
    })
}

fn load_style(
    assets: &VirtualAssets,
    lines: &[ContentLine],
    name: &str,
) -> Result<Style, Box<dyn Error>> {
    let template = named_template(lines, name)?;
    let mut image = source_texture::load(assets, &template.texture)?;
    image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
    // addDecal clamps both axes and disables mip mapping. Source textures remain
    // encoded RGB for the existing fixed-function SourceMaterial modulation.
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        lod_min_clamp: 0.0,
        lod_max_clamp: 0.0,
        ..default()
    });
    Ok(Style {
        template,
        image: Some(image),
        material: Handle::default(),
    })
}

fn named_template(lines: &[ContentLine], name: &str) -> Result<Template, Box<dyn Error>> {
    let start = lines
        .iter()
        .rposition(|line| {
            line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == name)
        })
        .ok_or_else(|| format!("InGameUI has no {name}"))?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.depth == Some(0))
        .ok_or_else(|| format!("unterminated {name}"))?
        + start
        + 1;
    template(&lines[start + 1..end])
}

fn template(lines: &[ContentLine]) -> Result<Template, Box<dyn Error>> {
    let mut result = Template {
        texture: String::new(),
        minimum: 1.0,
        maximum: 1.0,
        period: 30,
        color: None,
        only_owner: true,
    };
    for line in lines {
        let Some(key) = line.tokens.first() else {
            continue;
        };
        match key.as_str() {
            "Texture" => scalar(line)?.clone_into(&mut result.texture),
            "Style" if scalar(line)? == "SHADOW_ALPHA_DECAL" => {}
            "OpacityMin" => result.minimum = percent_value(scalar(line)?)?,
            "OpacityMax" => result.maximum = percent_value(scalar(line)?)?,
            "OpacityThrobTime" => {
                let frames = (finite_number(scalar(line)?)? * (30.0 / 1000.0)).ceil();
                result.period = format!("{frames:.0}").parse()?;
                if result.period == 0 {
                    return Err("zero radius cursor throb period".into());
                }
            }
            "Color" => {
                let mut rgba = [0_u8, 0, 0, 255];
                let tokens: Vec<_> = line.tokens[1..]
                    .iter()
                    .flat_map(|token| token.split(':'))
                    .filter(|token| !token.is_empty())
                    .collect();
                if !matches!(tokens.len(), 6 | 8) {
                    return Err("invalid radius cursor color".into());
                }
                for (index, pair) in tokens.chunks_exact(2).enumerate() {
                    if !pair[0].eq_ignore_ascii_case(["R", "G", "B", "A"][index]) {
                        return Err("invalid radius cursor color order".into());
                    }
                    rgba[index] = pair[1].parse()?;
                }
                result.color = (rgba != [0; 4])
                    .then(|| [rgba[0], rgba[1], rgba[2]].map(|value| f32::from(value) / 255.0));
            }
            "OnlyVisibleToOwningPlayer" => {
                result.only_owner = match scalar(line)? {
                    value if value.eq_ignore_ascii_case("yes") => true,
                    value if value.eq_ignore_ascii_case("no") => false,
                    _ => return Err("invalid radius cursor ownership flag".into()),
                };
            }
            _ => return Err(format!("unsupported radius cursor field {key}").into()),
        }
    }
    if result.texture.is_empty() {
        return Err("radius cursor texture is empty".into());
    }
    Ok(result)
}

fn percent_value(text: &str) -> Result<f32, Box<dyn Error>> {
    let value = finite_number(
        text.strip_suffix('%')
            .ok_or("radius cursor opacity lacks percent")?,
    )? / 100.0;
    if value > 1.0 {
        return Err("radius cursor opacity exceeds 100 percent".into());
    }
    Ok(value)
}

impl Template {
    fn opacity(&self, frame: u64) -> f32 {
        let cycle = u16::try_from(frame % u64::from(self.period)).expect("bounded pulse phase");
        let theta = std::f32::consts::TAU * f32::from(cycle) / f32::from(self.period);
        let percent = 0.5 * (theta.sin() + 1.0);
        ((self.minimum + percent * (self.maximum - self.minimum)) * 255.0).trunc() / 255.0
    }
}

pub fn setup(
    mut commands: Commands,
    mut cursor: ResMut<RadiusCursor>,
    terrain: Res<Terrain>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SourceMaterial>>,
) {
    let terrain = terrain.into_inner();
    for style in &mut cursor.styles {
        let mut material =
            SourceMaterial::new([0.0; 3], [0.0; 3], [0.0; 3], 1.0, AlphaMode::Blend, false);
        material.parameters.vertex_diffuse = 3;
        material.texture =
            Some(images.add(style.image.take().expect("radius texture loaded once")));
        style.material = materials.add(material);
    }
    cursor.mesh = meshes.add(terrain.radius_decal(Vec2::ZERO, 1.0, [1.0; 4]));
    commands.spawn((
        RadiusVisual,
        Mesh3d(cursor.mesh.clone()),
        MeshMaterial3d(cursor.styles[0].material.clone()),
        Transform::IDENTITY,
        Visibility::Hidden,
        NoFrustumCulling,
        RenderLayers::layer(2),
    ));
}

pub fn update(
    mut cursor: ResMut<RadiusCursor>,
    game: (Res<Match>, ResMut<Mouse>),
    view: (
        Res<Terrain>,
        Res<UiData>,
        Res<PromotionTransition>,
        Res<Radar>,
    ),
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    visual: Single<(&mut Visibility, &mut MeshMaterial3d<SourceMaterial>), With<RadiusVisual>>,
    assets: (ResMut<Assets<Mesh>>, ResMut<Assets<SourceMaterial>>),
) {
    let window = window.into_inner();
    let camera = camera.into_inner();
    let (battle, mut mouse) = game;
    mouse.advance_guard_hint();
    let hint = mouse.guard_hint();
    let (terrain, data, promotions, radar) = view;
    let (mut meshes, mut materials) = assets;
    let (mut visibility, mut visual_material) = visual.into_inner();
    let (current, spectre_radius) = cursor.command_style(&mouse.pending_command, hint.is_some());
    if cursor.current != current {
        cursor.current = current;
        cursor.radius = None;
        visual_material.0 = cursor.styles[current].material.clone();
    }
    if cursor.generation != battle.generation {
        cursor.generation = battle.generation;
        cursor.radius = None;
    }
    if !battle.started
        || battle.paused
        || battle.simulation.end_game_at.is_some()
        || battle.placing.is_some()
        || (hint.is_none()
            && spectre_radius.is_none()
            && !matches!(
                mouse.pending_command,
                CommandMode::Guard
                    | CommandMode::GuardFlying
                    | CommandMode::HelixBomb
                    | CommandMode::Cleanup
                    | CommandMode::FireWeapon(2)
                    | CommandMode::ClearMines(_)
            ))
    {
        cursor.radius = None;
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    let pointer = window.cursor_position().filter(|_| window.focused);
    let Some(pointer) = pointer.or_else(|| hint.map(|_| Vec2::ZERO)) else {
        cursor.radius = None;
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let radar_point = battle
        .simulation
        .has_radar(0)
        .then(|| radar.mouse_world(&data, window, pointer))
        .flatten();
    // createCommandHint clears the old radius before its opaque-window early
    // return. The original radar input window is explicitly exempt from it.
    if hint.is_none()
        && radar_point.is_none()
        && crate::playtest_ui::captures_pointer(&battle, window, pointer, &data, &promotions)
    {
        cursor.radius = None;
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    let radius = selected_radius(&battle, &cursor, spectre_radius);
    let Some(radius) = radius else {
        cursor.radius = None;
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let mut rebuild = cursor.radius.map(f32::to_bits) != Some(radius.to_bits());
    if cursor.radius.is_none() {
        cursor.position = Vec2::ZERO;
    }
    cursor.radius = Some(radius);
    let point = hint.map(|(point, _)| point).or(radar_point).or_else(|| {
        let ray = camera.0.viewport_to_world(camera.1, pointer).ok()?;
        terrain.ray_point(ray).map(|point| point * 10.0)
    });
    if let Some(point) = point {
        rebuild |= cursor.position != point;
        cursor.position = point;
        let opacity = hint.map_or_else(
            || {
                cursor.styles[current]
                    .template
                    .opacity(battle.simulation.tick)
            },
            |(_, frames)| f32::from(frames) * 0.1,
        );
        cursor.styles[current].update_opacity(&mut materials, opacity);
    }
    // handleRadiusCursor retains its position and pulse without an intersection.
    if rebuild {
        rebuild_mesh(&cursor, &battle, &data, &terrain, &mut meshes);
    }
    visibility.set_if_neq(Visibility::Inherited);
}

fn rebuild_mesh(
    cursor: &RadiusCursor,
    battle: &Match,
    data: &UiData,
    terrain: &Terrain,
    meshes: &mut Assets<Mesh>,
) {
    let color = cursor.styles[cursor.current]
        .template
        .color
        .unwrap_or_else(|| {
            let id = battle.selected[0];
            let owner = battle.simulation.entities[id].owner;
            data.results.colors
                [battle.simulation.player_colors[owner].expect("selected player color")]
        });
    *meshes.get_mut(&cursor.mesh).expect("radius mesh") = terrain.radius_decal(
        cursor.position,
        cursor.radius.expect("active radius"),
        [color[0], color[1], color[2], 1.0],
    );
}

fn selected_radius(
    battle: &Match,
    cursor: &RadiusCursor,
    spectre_radius: Option<f32>,
) -> Option<f32> {
    let only_owner = cursor.styles[cursor.current].template.only_owner;
    battle.selected.first().and_then(|&id| {
        let actor = battle.simulation.entities.get(id)?;
        (actor.alive() && (!only_owner || actor.owner == 0))
            .then(|| match cursor.current {
                5 => spectre_radius.unwrap_or(0.0),
                4 => cursor.helix_radius,
                1 => cursor.cleanup_radius,
                2 => battle.simulation.catalog[actor.type_index]
                    .weapon_template(actor.weapon_index(2))
                    .map_or(0.0, |weapon| weapon.scatter + weapon.scatter_target_scalar),
                3 => {
                    actor
                        .active_weapon(&battle.simulation.catalog[actor.type_index])
                        .continue_attack_range
                }
                _ => battle.simulation.guard_radius(id),
            })
            .filter(|radius| *radius > 0.0)
    })
}

impl RadiusCursor {
    fn command_style(&self, command: &CommandMode, hint: bool) -> (usize, Option<f32>) {
        let spectre_radius = if let CommandMode::Power { name, .. } = command {
            self.spectre_radii
                .iter()
                .find(|(power, _)| power == name)
                .map(|(_, radius)| *radius)
        } else {
            None
        };
        let current = if hint {
            0
        } else if spectre_radius.is_some() {
            5
        } else {
            match command {
                CommandMode::HelixBomb => 4,
                CommandMode::Cleanup => 1,
                CommandMode::FireWeapon(2) => 2,
                CommandMode::ClearMines(_) => 3,
                _ => 0,
            }
        };
        (current, spectre_radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_opacity_uses_absolute_logic_phase_and_integer_alpha() {
        let template = Template {
            texture: String::new(),
            minimum: 0.5,
            maximum: 1.0,
            period: 30,
            color: None,
            only_owner: true,
        };
        // Extracted RadiusDecal::update expression, clang++/sinf, frames 0..29.
        let expected: [u8; 30] = [
            191, 204, 217, 228, 238, 246, 251, 254, 254, 251, 246, 238, 228, 217, 204, 191, 177,
            165, 153, 143, 136, 130, 127, 127, 130, 136, 143, 153, 165, 177,
        ];
        for (frame, alpha) in (0_u64..).zip(expected) {
            assert_eq!(
                template.opacity(frame).to_bits(),
                (f32::from(alpha) / 255.0).to_bits()
            );
            assert_eq!(
                template.opacity(frame).to_bits(),
                template.opacity(frame + 30).to_bits()
            );
        }
    }
}
