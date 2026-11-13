//! Share identical prepared W3D passes, retaining every original render value.
use crate::source_material::{Parameters, SourceMaterial};
use bevy::prelude::*;
use std::collections::BTreeMap;

type Key = (Vec<u32>, Option<AssetId<Image>>, Option<AssetId<Image>>);

pub(super) struct Cache(BTreeMap<Key, Handle<SourceMaterial>>);

impl Cache {
    pub(super) const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub(super) fn add(
        &mut self,
        material: SourceMaterial,
        materials: &mut Assets<SourceMaterial>,
    ) -> Handle<SourceMaterial> {
        self.0
            .entry(key(&material))
            .or_insert_with(|| materials.add(material))
            .clone()
    }
}

fn key(material: &SourceMaterial) -> Key {
    // Destructure all fields so additions cannot silently alias distinct passes.
    // Float bits also preserve signed zero and exact alpha-mask thresholds.
    let SourceMaterial {
        parameters,
        texture,
        shroud,
        alpha_mode,
        double_sided,
        depth_bias,
        depth_write,
        depth_compare,
        force_sorted_phase,
    } = material;
    let Parameters {
        ambient,
        diffuse,
        emissive,
        scene_ambient,
        light_color,
        light_direction,
        point_position,
        point_ambient,
        terrain_lights,
        alpha_test,
        vertex_diffuse,
        opacity,
        primary_gradient,
        shroud_extent,
        uv_transform,
        shroud_mode,
    } = parameters;
    let mut bits = Vec::with_capacity(256);
    bits.push(*terrain_lights);
    bits.push(*shroud_mode);
    for value in [
        ambient,
        diffuse,
        emissive,
        scene_ambient,
        shroud_extent,
        uv_transform,
    ]
    .into_iter()
    .chain(light_color)
    .chain(light_direction)
    .chain(point_position)
    .chain(point_ambient)
    {
        bits.extend(value.to_array().map(f32::to_bits));
    }
    bits.extend([
        alpha_test.to_bits(),
        *vertex_diffuse,
        opacity.to_bits(),
        *primary_gradient,
        u32::from(*double_sided),
        depth_bias.to_bits(),
        u32::from(*depth_write),
        u32::from(*force_sorted_phase),
        depth_compare.map_or(0, |compare| compare as u32 + 1),
    ]);
    bits.extend(match *alpha_mode {
        AlphaMode::Opaque => [0, 0],
        AlphaMode::Mask(threshold) => [1, threshold.to_bits()],
        AlphaMode::Blend => [2, 0],
        AlphaMode::Premultiplied => [3, 0],
        AlphaMode::AlphaToCoverage => [4, 0],
        AlphaMode::Add => [5, 0],
        AlphaMode::Multiply => [6, 0],
    });
    (
        bits,
        texture.as_ref().map(Handle::id),
        shroud.as_ref().map(Handle::id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::CompareFunction;

    #[test]
    fn original_passes_share_only_exact_render_state() {
        let mut cache = Cache::new();
        let mut materials = Assets::default();
        let mut images = Assets::default();
        let texture = images.add(Image::default());
        let mut source =
            SourceMaterial::new([0.1; 3], [0.7; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false);
        source.texture = Some(texture);
        let shared = cache.add(source.clone(), &mut materials);
        for _ in 0..4_000 {
            assert_eq!(cache.add(source.clone(), &mut materials), shared);
        }
        assert_eq!(materials.len(), 1);

        let mut variants = Vec::new();
        let mut changed = source.clone();
        changed.parameters.diffuse.x = 0.9;
        variants.push(changed);
        let mut changed = source.clone();
        changed.parameters.light_color[2].z = 0.2;
        variants.push(changed);
        let mut changed = source.clone();
        changed.parameters.emissive.x = -0.0;
        variants.push(changed);
        let mut changed = source.clone();
        changed.texture = Some(images.add(Image::default()));
        variants.push(changed);
        let mut changed = source.clone();
        changed.depth_compare = Some(CompareFunction::Equal);
        variants.push(changed);
        for alpha in [AlphaMode::Mask(0.3), AlphaMode::Mask(0.4), AlphaMode::Add] {
            let mut changed = source.clone();
            changed.alpha_mode = alpha;
            variants.push(changed);
        }
        let mut handles = std::collections::BTreeSet::new();
        for variant in variants {
            let handle = cache.add(variant.clone(), &mut materials);
            assert_ne!(handle, shared);
            assert!(handles.insert(handle.id()));
            assert_eq!(cache.add(variant, &mut materials), handle);
        }
        // Per-object cloak/disabled/preview passes clone the base before edits.
        let mut preview = materials.get(&shared).unwrap().clone();
        preview.parameters.opacity = 0.5;
        let preview = materials.add(preview);
        assert_eq!(
            materials.get(&shared).unwrap().parameters.opacity.to_bits(),
            1.0_f32.to_bits()
        );
        assert_eq!(
            materials
                .get(&preview)
                .unwrap()
                .parameters
                .opacity
                .to_bits(),
            0.5_f32.to_bits()
        );
    }
}
