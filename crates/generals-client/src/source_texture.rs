//! Installed texture lookup following `TextureLoadTaskClass::Load` (DDS, then TGA).
use bevy::{
    asset::RenderAssetUsages,
    image::{
        CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
        ImageSamplerDescriptor, ImageType, TextureFormatPixelInfo,
    },
    prelude::Image,
};
use generals_assets::{AssetError, virtual_assets::VirtualAssets};
use std::error::Error;

pub fn load(assets: &VirtualAssets, name: &str) -> Result<Image, Box<dyn Error>> {
    read(assets, name)?.ok_or_else(|| {
        format!("cannot load original texture {name}: DDS and TGA are absent").into()
    })
}

/// WW3D mesh materials bind the native missing surface when both files are absent.
/// # Errors
/// Preserves I/O and texture decoding errors; only absent files use the surface.
pub fn load_mesh(assets: &VirtualAssets, name: &str) -> Result<Image, Box<dyn Error>> {
    if let Some(image) = read(assets, name)? {
        return Ok(image);
    }
    eprintln!("Original W3D mesh texture absent: {name}; binding native MissingTexture");
    Ok(missing_mesh_texture())
}

fn read(assets: &VirtualAssets, name: &str) -> Result<Option<Image>, Box<dyn Error>> {
    let path = if name.contains('/') || name.contains('\\') {
        name.to_owned()
    } else {
        format!("art/textures/{name}")
    };
    let stem = path
        .rsplit_once('.')
        .map_or(path.as_str(), |(stem, _)| stem);
    let mut failures = Vec::new();
    for extension in ["dds", "tga"] {
        let path = format!("{stem}.{extension}");
        let localized = (!name.contains('/') && !name.contains('\\')).then(|| {
            format!(
                "data/english/art/textures/{stem}.{extension}",
                stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem)
            )
        });
        let asset = localized.as_ref().map_or_else(
            || assets.read(&path),
            |localized| match assets.read(localized) {
                Err(AssetError::Missing(_)) => assets.read(&path),
                result => result,
            },
        );
        let bytes = match asset {
            Ok(asset) => asset.bytes,
            Err(AssetError::Missing(_)) => continue,
            Err(error) => return Err(error.into()),
        };
        match Image::from_buffer(
            &bytes,
            ImageType::Extension(extension),
            CompressedImageFormats::BC,
            true,
            sampler(),
            RenderAssetUsages::default(),
        ) {
            Ok(image) => return Ok(Some(image)),
            Err(error) => failures.push(format!("{path}: {error}")),
        }
    }
    if failures.is_empty() {
        Ok(None)
    } else {
        Err(format!(
            "cannot load original texture {name}: {}",
            failures.join("; ")
        )
        .into())
    }
}

fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..Default::default()
    })
}

fn missing_mesh_texture() -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    // MissingTexture::_Init declares A8R8G8B8 0x7FFF00FF and MIP_LEVELS_ALL.
    // Its last row is uninitialized due to the native row-pointer reset. Fill
    // every row with the declared color instead of reproducing undefined memory.
    let mut image = Image::new_uninit(
        Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    let mut bytes = Vec::with_capacity(87_380);
    for level in 0..8 {
        let extent = 128 >> level;
        for _ in 0..extent * extent {
            bytes.extend_from_slice(&[255, 0, 255, 127]);
        }
    }
    image.texture_descriptor.mip_level_count = 8;
    image.sampler = sampler();
    image.data = Some(bytes);
    image
}

pub fn load_terrain(
    assets: &VirtualAssets,
    name: &str,
    extent: u32,
) -> Result<Image, Box<dyn Error>> {
    let bytes = assets.read(name)?.bytes;
    // WorldHeightMap::readTexClass/readTiles read bottom-origin TGA directly.
    if bytes
        .get(17)
        .is_none_or(|descriptor| descriptor & 0x30 != 0)
    {
        return Err(format!("unsupported terrain TGA origin: {name}").into());
    }
    let mut image = Image::from_buffer(
        &bytes,
        ImageType::Extension("tga"),
        CompressedImageFormats::NONE,
        false,
        ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            ..Default::default()
        }),
        RenderAssetUsages::default(),
    )?;
    if image.width() < extent || image.height() < extent {
        return Err(
            format!("terrain texture {name} is smaller than source class extent {extent}").into(),
        );
    }
    let pixel_size = image.texture_descriptor.format.pixel_size()?;
    let row_bytes = usize::try_from(extent)?
        .checked_mul(pixel_size)
        .ok_or("terrain row size overflow")?;
    let original_row = usize::try_from(image.width())?
        .checked_mul(pixel_size)
        .ok_or("terrain image size overflow")?;
    let start_row = usize::try_from(image.height() - extent)?;
    let data = image.data.take().ok_or("terrain image lacks pixels")?;
    let mut cropped = Vec::with_capacity(row_bytes * usize::try_from(extent)?);
    for row in start_row..start_row + usize::try_from(extent)? {
        cropped.extend_from_slice(
            data.get(row * original_row..row * original_row + row_bytes)
                .ok_or("truncated terrain pixels")?,
        );
    }
    image.texture_descriptor.size.width = extent;
    image.texture_descriptor.size.height = extent;
    image.data = Some(cropped);
    Ok(image)
}

/// `W3DAssetManager::Remap_Palette` / `remapAlphaTexture32Bit` for original ZHC art.
pub fn recolor(
    image: &Image,
    color: [f32; 3],
    palette_only: bool,
) -> Result<Image, Box<dyn Error>> {
    use bevy::{
        color::{ColorToPacked, Hsva, Srgba},
        render::render_resource::TextureFormat,
    };
    if !matches!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
    ) {
        return Err("house texture needs uncompressed RGBA pixels".into());
    }
    // The source surface is already RGBA8; keep its encoded bytes and sampler.
    // Bevy's Image::convert does not accept Rgba8Unorm as an output format.
    let mut image = image.clone();
    image.texture_descriptor.format = TextureFormat::Rgba8Unorm;
    let bytes = image
        .data
        .as_mut()
        .ok_or("house texture has no CPU pixels")?;
    let rgba = |rgb: [f32; 3]| {
        let values = rgb.map(|value| (value.clamp(0.0, 1.0) * 255.0).trunc() / 255.0);
        Srgba::new(values[0], values[1], values[2], 1.0).to_u8_array()
    };
    if palette_only {
        const SCALE: [u8; 16] = [
            255, 239, 223, 211, 195, 174, 167, 151, 135, 123, 107, 91, 79, 63, 47, 35,
        ];
        let palette = bytes.get_mut(..64).ok_or("short house color palette")?;
        for (pixel, scale) in palette.chunks_exact_mut(4).zip(SCALE) {
            pixel.copy_from_slice(&rgba(
                color.map(|channel| channel * f32::from(scale) / 255.0),
            ));
        }
    } else {
        let target = Hsva::from(Srgba::new(color[0], color[1], color[2], 1.0));
        for pixel in bytes.chunks_exact_mut(4) {
            if pixel[3] != 255 {
                let mut hsv = Hsva::from(Srgba::new(
                    f32::from(pixel[0]) / 255.0,
                    f32::from(pixel[1]) / 255.0,
                    f32::from(pixel[2]) / 255.0,
                    1.0,
                ));
                hsv.hue = target.hue;
                hsv.saturation *= target.saturation;
                let rgb = Srgba::from(hsv);
                pixel.copy_from_slice(&rgba([rgb.red, rgb.green, rgb.blue]));
            }
            pixel[3] = 255;
        }
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::{load_mesh, read, recolor};
    use bevy::{
        asset::RenderAssetUsages,
        prelude::Image,
        render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    };

    #[test]
    fn mesh_texture_fallback_is_only_for_absent_dds_and_tga() {
        use generals_assets::virtual_assets::{Layer, VirtualAssets};
        use std::{
            fs,
            time::{SystemTime, UNIX_EPOCH},
        };

        let root = std::env::temp_dir().join("generals-client-tests");
        fs::create_dir_all(&root).unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = root.join(format!("{}-{unique}", std::process::id()));
        fs::create_dir(&path).unwrap();
        fs::create_dir_all(path.join("art/textures")).unwrap();
        let assets = VirtualAssets::from_layers(vec![Layer::Loose(path.clone())]);

        assert!(read(&assets, "missing").unwrap().is_none());
        for extension in ["dds", "tga"] {
            let file = path.join(format!("art/textures/corrupt.{extension}"));
            fs::write(&file, [0_u8; 4]).unwrap();
            assert!(read(&assets, "corrupt").is_err());
            assert!(load_mesh(&assets, "corrupt").is_err());
            fs::remove_file(file).unwrap();
        }
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn house_alpha_is_a_hue_mask_not_a_blend_amount() {
        let image = Image::new(
            Extent3d {
                width: 3,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![0, 0, 255, 0, 0, 255, 0, 255, 0, 0, 128, 254],
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        );
        let colored = recolor(&image, [1.0, 0.0, 0.0], false).expect("RGBA house texture");
        assert_eq!(
            colored.data,
            Some(vec![255, 0, 0, 255, 0, 255, 0, 255, 128, 0, 0, 255])
        );
    }
}
