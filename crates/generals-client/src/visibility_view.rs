//! Native W3D shroud intensity texture, applied before encoded-color conversion.

use crate::{playtest::Match, source_material::SourceMaterial, terrain::Terrain};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::visibility::Status;
use num_traits::ToPrimitive;
use std::error::Error;

#[derive(Resource)]
pub struct Fog {
    levels: [u8; 3],
    color: [u8; 3],
    image: Option<Handle<Image>>,
    layout: Option<Layout>,
    enabled: bool,
}

struct Layout {
    cells: [u32; 2],
    texture: [u32; 2],
    cell: f32,
}

impl Layout {
    fn new(tree_bounds: [f32; 2], cell: f32) -> Self {
        let cells = tree_bounds.map(|extent| {
            u32::try_from(generals_sim::native_real::ceil((extent - 10.0) / cell))
                .expect("native positive shroud extent")
        });
        Self {
            cells,
            texture: cells.map(|extent| (extent + 2).next_power_of_two()),
            cell,
        }
    }

    fn image(&self, pixels: Vec<u8>) -> Image {
        let mut image = Image::new(
            Extent3d {
                width: self.texture[0],
                height: self.texture[1],
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        );
        // W3DShroud requests DEFAULT min/mag (linear on stage1), CLAMP and no mips.
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            min_filter: ImageFilterMode::Linear,
            mag_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Nearest,
            ..Default::default()
        });
        image
    }

    fn mapping(&self) -> ([f32; 2], [f32; 2]) {
        (
            [self.cell; 2],
            self.texture.map(|extent| {
                1.0 / (self.cell * extent.to_f32().expect("native shroud texture extent"))
            }),
        )
    }

    fn source_extent(&self) -> Vec4 {
        Vec4::new(
            self.texture[0].to_f32().expect("native shroud texture") * self.cell * 0.1,
            self.texture[1].to_f32().expect("native shroud texture") * self.cell * 0.1,
            self.cell * 0.1,
            self.cell * 0.1,
        )
    }
}

impl Fog {
    pub(crate) const fn texture(&self) -> Option<&Handle<Image>> {
        self.image.as_ref()
    }

    pub(crate) fn tree_mapping(&self) -> ([f32; 2], [f32; 2]) {
        self.layout
            .as_ref()
            .filter(|_| self.enabled)
            .map_or(([0.0; 2], [0.0; 2]), Layout::mapping)
    }

    pub(crate) fn level(&self, status: &Status) -> u8 {
        match status {
            Status::Clear | Status::Partial => self.levels[0],
            Status::Fogged => self.levels[1],
            Status::Shrouded => self.levels[2],
        }
        .max(self.levels[2])
    }

    pub fn load(assets: &VirtualAssets) -> Result<Self, Box<dyn Error>> {
        let file = content::scan(assets.read("data/ini/gamedata.ini")?)?;
        let global = crate::skirmish_data::definition(std::slice::from_ref(&file), "GameData", "")?;
        let mut levels = [0; 3];
        for (slot, key) in levels
            .iter_mut()
            .zip(["ClearAlpha", "FogAlpha", "ShroudAlpha"])
        {
            *slot = crate::skirmish_data::text(&global.lines, 0, key)?.parse()?;
        }
        let line = crate::skirmish_data::field(&global.lines, 0, "ShroudColor")
            .ok_or("missing ShroudColor")?;
        let mut color = [0; 3];
        for (slot, prefix) in color.iter_mut().zip(["R:", "G:", "B:"]) {
            *slot = line
                .tokens
                .iter()
                .find_map(|token| token.strip_prefix(prefix))
                .ok_or("invalid ShroudColor")?
                .parse()?;
        }
        Ok(Self {
            levels,
            color,
            image: None,
            layout: None,
            enabled: false,
        })
    }
}

pub fn update(
    mut fog: ResMut<Fog>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    timings: Option<Res<crate::render_performance::Timings>>,
) {
    let timings = timings.map(Res::into_inner);
    let _span = timings.map(|timings| timings.span("fog"));
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    let game = &battle.simulation;
    let layout = Layout::new(terrain.tree_bounds(), game.visibility_cell_size());
    let changed_layout = fog.layout.as_ref().is_none_or(|old| {
        old.cells != layout.cells
            || old.texture != layout.texture
            || old.cell.to_bits() != layout.cell.to_bits()
    });
    let pixels = pixels(&layout, &fog, |[x, y]| {
        game.visibility_cell(
            battle.observer_player.unwrap_or(0),
            [
                (x.to_f32().expect("native shroud cell") + 0.5) * layout.cell,
                (y.to_f32().expect("native shroud cell") + 0.5) * layout.cell,
            ],
        )
    });
    if changed_layout || fog.image.is_none() {
        let image = layout.image(pixels.clone());
        if let Some(handle) = &fog.image {
            *images.get_mut(handle).expect("initialized fog texture") = image;
        } else {
            fog.image = Some(images.add(image));
        }
    }
    let handle = fog.image.as_ref().expect("initialized fog texture");
    update_pixels(&mut images, handle, &pixels);
    update_materials(
        &mut materials,
        handle,
        if battle.started {
            layout.source_extent()
        } else {
            Vec4::ZERO
        },
    );
    fog.enabled = battle.started;
    fog.layout = Some(layout);
}

fn pixels(layout: &Layout, fog: &Fog, mut sample: impl FnMut([u32; 2]) -> Status) -> Vec<u8> {
    let border = shroud_pixel(fog.levels[2], fog.color);
    let mut pixels = border.repeat(
        usize::try_from(layout.texture[0] * layout.texture[1]).expect("native shroud size"),
    );
    for y in 0..layout.cells[1] {
        for x in 0..layout.cells[0] {
            let offset = usize::try_from(((y + 1) * layout.texture[0] + x + 1) * 4)
                .expect("native shroud size");
            pixels[offset..offset + 4]
                .copy_from_slice(&shroud_pixel(fog.level(&sample([x, y])), fog.color));
        }
    }
    pixels
}

fn update_pixels(images: &mut Assets<Image>, handle: &Handle<Image>, pixels: &[u8]) {
    let image = images.get(handle).expect("initialized fog image");
    if image.data.as_deref().expect("CPU shroud pixels") != pixels {
        images
            .get_mut(handle)
            .expect("initialized fog image")
            .data
            .as_mut()
            .expect("CPU shroud pixels")
            .copy_from_slice(pixels);
    }
}

fn update_materials(materials: &mut Assets<SourceMaterial>, handle: &Handle<Image>, extent: Vec4) {
    // Assets::iter_mut emits Modified for every yielded asset, even when its
    // fields are not written. Inspect first so unchanged GPU bindings survive.
    let changed: Vec<_> = materials
        .iter()
        .filter_map(|(id, material)| {
            (material.shroud.as_ref() != Some(handle)
                || material.parameters.shroud_extent != extent)
                .then_some(id)
        })
        .collect();
    for id in changed {
        let mut material = materials.get_mut(id).expect("existing source material");
        material.shroud = Some(handle.clone());
        material.parameters.shroud_extent = extent;
    }
}

fn shroud_pixel(level: u8, color: [u8; 3]) -> [u8; 4] {
    if level == 255 {
        return [255; 4];
    }
    let values = color.map(|channel| u16::from(level) * u16::from(channel) / 255);
    // W3DShroud uploads RGB565. Expand the stored bits as the sampler does.
    let red = values[0] >> 3;
    let green = values[1] >> 2;
    let blue = values[2] >> 3;
    [
        u8::try_from((red << 3) | (red >> 2)).expect("five bits"),
        u8::try_from((green << 2) | (green >> 4)).expect("six bits"),
        u8::try_from((blue << 3) | (blue >> 2)).expect("five bits"),
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::{AssetApp, AssetPlugin};

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<SourceMaterial>();
        app
    }

    fn material() -> SourceMaterial {
        SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false)
    }

    fn modified<A: Asset>(app: &mut App) -> Vec<AssetId<A>> {
        app.update();
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<A>>>()
            .drain()
            .filter_map(|event| match event {
                AssetEvent::Modified { id } => Some(id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn unchanged_fog_does_not_rebuild_materials_but_new_bindings_still_update() {
        let mut app = app();
        let image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let first = app
            .world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .add(material());
        let second = app
            .world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .add(material());
        let extent = Vec4::new(280.0, 250.0, 0.0, 0.0);
        update_materials(&mut app.world_mut().resource_mut(), &image, extent);
        let changed = modified::<SourceMaterial>(&mut app);
        assert_eq!(changed.len(), 2);
        assert!(changed.contains(&first.id()) && changed.contains(&second.id()));
        for _ in 0..3 {
            update_materials(&mut app.world_mut().resource_mut(), &image, extent);
            assert!(modified::<SourceMaterial>(&mut app).is_empty());
        }
        // Returning to the shell changes the shader extent without replacing
        // textures; entering the game again must also notify the renderer.
        for next in [Vec4::ZERO, extent] {
            update_materials(&mut app.world_mut().resource_mut(), &image, next);
            assert_eq!(modified::<SourceMaterial>(&mut app).len(), 2);
        }
        let third = app
            .world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .add(material());
        update_materials(&mut app.world_mut().resource_mut(), &image, extent);
        assert_eq!(modified::<SourceMaterial>(&mut app), vec![third.id()]);
        let replacement = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        update_materials(&mut app.world_mut().resource_mut(), &replacement, extent);
        assert_eq!(modified::<SourceMaterial>(&mut app).len(), 3);
        let assets = app.world().resource::<Assets<SourceMaterial>>();
        for handle in [&first, &second, &third] {
            let material = assets.get(handle).unwrap();
            assert_eq!(material.shroud.as_ref(), Some(&replacement));
            assert_eq!(material.parameters.shroud_extent, extent);
        }
        update_materials(&mut app.world_mut().resource_mut(), &replacement, extent);
        assert!(modified::<SourceMaterial>(&mut app).is_empty());
    }

    #[test]
    fn fog_pixels_only_emit_image_change_when_encoded_visibility_changes() {
        let mut app = app();
        let pixels = [255; 8];
        let image = Image::new_fill(
            Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &pixels,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        );
        let handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        assert!(modified::<Image>(&mut app).is_empty());
        for _ in 0..3 {
            update_pixels(&mut app.world_mut().resource_mut(), &handle, &pixels);
            assert!(modified::<Image>(&mut app).is_empty());
        }
        let mut fogged = pixels;
        fogged[4..].copy_from_slice(&shroud_pixel(127, [255; 3]));
        update_pixels(&mut app.world_mut().resource_mut(), &handle, &fogged);
        assert_eq!(modified::<Image>(&mut app), vec![handle.id()]);
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&handle)
                .unwrap()
                .data
                .as_deref(),
            Some(fogged.as_slice())
        );
        update_pixels(&mut app.world_mut().resource_mut(), &handle, &fogged);
        assert!(modified::<Image>(&mut app).is_empty());
        update_pixels(&mut app.world_mut().resource_mut(), &handle, &pixels);
        assert_eq!(modified::<Image>(&mut app), vec![handle.id()]);
    }

    #[test]
    fn native_rgb565_fog_and_clear_override_are_preserved() {
        assert_eq!(shroud_pixel(0, [255; 3]), [0, 0, 0, 255]);
        assert_eq!(shroud_pixel(127, [255; 3]), [123, 125, 123, 255]);
        assert_eq!(shroud_pixel(255, [20, 40, 80]), [255; 4]);
    }
    fn native_fog() -> Fog {
        Fog {
            levels: [255, 127, 127],
            color: [105, 67, 29],
            image: None,
            layout: None,
            enabled: true,
        }
    }

    #[test]
    fn raw_heightmap_shroud_keeps_fastceil_border_padding_and_original_cell_centers() {
        let layout = Layout::new([1290.0, 650.0], 40.0);
        // Source raw extents minus one sample give exact ratios32/16;
        // fast_float_ceil advances each exact positive integer here.
        assert_eq!(layout.cells, [33, 17]);
        assert_eq!(layout.texture, [64, 32]);
        let (offset, scale) = layout.mapping();
        assert_eq!(offset.map(f32::to_bits), [40.0_f32; 2].map(f32::to_bits));
        let point = [60.0, 100.0];
        let uv = std::array::from_fn::<_, 2, _>(|axis| (point[axis] + offset[axis]) * scale[axis]);
        assert_eq!(
            uv.map(f32::to_bits),
            [2.5_f32 / 64.0, 3.5 / 32.0].map(f32::to_bits)
        );
        let extent = layout.source_extent();
        let ordinary = [(6.0 + extent.z) / extent.x, (10.0 + extent.w) / extent.y];
        assert_eq!(ordinary.map(f32::to_bits), uv.map(f32::to_bits));
    }

    #[test]
    fn padded_source_surface_preserves_colored_border_and_source_cell_offset() {
        let layout = Layout::new([1290.0, 650.0], 40.0);
        let fog = native_fog();
        let mut visited = Vec::new();
        let pixels = pixels(&layout, &fog, |point| {
            visited.push(point);
            if point == [0, 0] {
                Status::Clear
            } else {
                Status::Fogged
            }
        });
        assert_eq!(visited.len(), 33 * 17);
        assert_eq!(visited.first(), Some(&[0, 0]));
        assert_eq!(visited.last(), Some(&[32, 16]));
        let at = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4];
        assert_eq!(at(1, 1), [255; 4]);
        for point in [[0, 0], [0, 1], [34, 1], [63, 31], [1, 18]] {
            assert_eq!(at(point[0], point[1]), [49, 32, 8, 255]);
        }
        assert_eq!(at(2, 1), [49, 32, 8, 255]);
        let image = layout.image(pixels);
        assert_eq!([image.width(), image.height()], [64, 32]);
        let ImageSampler::Descriptor(sampler) = image.sampler else {
            panic!("native explicit shroud sampler")
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.min_filter, ImageFilterMode::Linear);
        assert_eq!(sampler.mag_filter, ImageFilterMode::Linear);
    }

    #[test]
    fn tree_mapping_disables_shroud_in_shell_and_uses_prepared_native_surface_in_game() {
        let mut fog = native_fog();
        fog.layout = Some(Layout::new([1290.0, 650.0], 40.0));
        let active = fog.tree_mapping();
        assert_eq!(active.0.map(f32::to_bits), [40.0_f32; 2].map(f32::to_bits));
        fog.enabled = false;
        let inactive = fog.tree_mapping();
        assert_eq!(inactive.0.map(f32::to_bits), [0.0_f32; 2].map(f32::to_bits));
        assert_eq!(inactive.1.map(f32::to_bits), [0.0_f32; 2].map(f32::to_bits));
    }
}
