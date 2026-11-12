//! `BaseHeightMap::updateShorelineTile` and initDestAlphaLUT.
use super::{data::Settings, geometry::Vertices};
use crate::terrain::Terrain;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use generals_assets::map::PolygonTrigger;
use generals_sim::terrain::{WaterPolygon, water_height};
use num_traits::ToPrimitive;
use std::error::Error;

pub(super) fn mesh(
    triggers: &[PolygonTrigger],
    terrain: &Terrain,
    settings: &Settings,
) -> Result<Mesh, Box<dyn Error>> {
    let polygons = triggers
        .iter()
        .filter(|trigger| trigger.water)
        .map(|trigger| WaterPolygon::new(trigger.points.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut vertices = Vertices::new();
    if !settings.soft || settings.depth == 0.0 || settings.opacity == 0.0 {
        return Ok(vertices.mesh());
    }
    let depth = settings.depth * settings.opacity;
    for (points, flip) in terrain.shoreline_cells() {
        let mut water = [0.0; 4];
        for (height, point) in water.iter_mut().zip(points) {
            *height = water_height(&polygons, [point[0], point[1]])?.unwrap_or(-10_000.0);
        }
        let depths = std::array::from_fn::<_, 4, _>(|index| water[index] - points[index][2]);
        if !depths.iter().any(|value| *value > 0.0)
            || water.iter().product::<f32>() <= 0.0
            || depths.iter().all(|value| *value >= depth)
        {
            continue;
        }
        let base = u32::try_from(vertices.positions.len()).expect("shoreline vertices");
        for (point, amount) in points.into_iter().zip(depths) {
            vertices.vertex(
                Vec3::from_array(point),
                [amount / depth, 0.0],
                [0.0; 2],
                [0.0; 4],
            );
        }
        vertices.indices.extend(
            if flip {
                [1, 3, 0, 1, 2, 3]
            } else {
                [0, 2, 3, 0, 1, 2]
            }
            .map(|index| base + index),
        );
    }
    Ok(vertices.mesh())
}

pub(super) fn lookup(opacity: f32) -> Image {
    let maximum = (opacity * 255.0)
        .trunc()
        .to_u8()
        .expect("validated water opacity");
    let pixels: Vec<u8> = (0..=255)
        .flat_map(|alpha: u8| [255, 255, 255, alpha.min(maximum)])
        .collect();
    let mut image = Image::new(
        Extent3d {
            width: 256,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    image
}

pub(super) fn screen() -> Mesh {
    let mut vertices = Vertices::new();
    vertices.positions.extend([
        [-1.0, -1.0, 0.0],
        [1.0, -1.0, 0.0],
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
    ]);
    vertices.uv.extend([[0.0; 2]; 4]);
    vertices.uv_b.extend([[0.0; 2]; 4]);
    vertices.colors.extend([[0.0; 4]; 4]);
    vertices.indices.extend([0, 1, 2, 0, 2, 3]);
    vertices.mesh()
}
