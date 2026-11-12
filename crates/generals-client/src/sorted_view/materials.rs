//! The native draw state follows its source material; Bevy sorts the pooled entity separately.
use super::{SourceMaterial, interchangeable_materials};
use bevy::prelude::*;
use std::collections::HashMap;

pub(super) struct Variant {
    pub(super) handle: Handle<SourceMaterial>,
    checked: bool,
}

pub(super) fn begin_frame(
    variants: &mut HashMap<AssetId<SourceMaterial>, Variant>,
    assets: &Assets<SourceMaterial>,
) {
    // Keys are source IDs, not strong references. Hidden runs keep their own
    // variant handles alive until rebound or retired, even if the source goes away.
    variants.retain(|source, variant| {
        variant.checked = false;
        assets.contains(*source)
    });
}

pub(super) fn bind(
    variants: &mut HashMap<AssetId<SourceMaterial>, Variant>,
    source: &Handle<SourceMaterial>,
    assets: &mut Assets<SourceMaterial>,
) -> Handle<SourceMaterial> {
    let variant = variants.get_mut(&source.id());
    if let Some(variant) = variant.as_ref()
        && variant.checked
    {
        return variant.handle.clone();
    }
    let original = assets.get(source).expect("sorted source material exists");
    if let Some(variant) = variant
        && let Some(current) = assets.get(&variant.handle)
    {
        if !interchangeable_materials(original, current) {
            let changed = forced(original);
            *assets
                .get_mut(&variant.handle)
                .expect("live sorted variant") = changed;
        }
        variant.checked = true;
        return variant.handle.clone();
    }
    let handle = assets.add(forced(original));
    variants.insert(
        source.id(),
        Variant {
            handle: handle.clone(),
            checked: true,
        },
    );
    handle
}

fn forced(original: &SourceMaterial) -> SourceMaterial {
    let mut copy = original.clone();
    copy.force_sorted_phase = true;
    copy.depth_bias = 0.0;
    copy
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::{AssetApp, AssetPlugin};

    #[test]
    fn removed_source_releases_cache_ownership_while_a_hidden_binding_survives() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<SourceMaterial>();
        let mut variants = HashMap::new();
        let (source, variant) = {
            let mut assets = app.world_mut().resource_mut::<Assets<SourceMaterial>>();
            let source = assets.add(SourceMaterial::new(
                [0.0; 3],
                [1.0; 3],
                [0.0; 3],
                1.0,
                AlphaMode::Opaque,
                false,
            ));
            let variant = bind(&mut variants, &source, &mut assets);
            assert!(assets.get(&variant).unwrap().force_sorted_phase);
            assert!(!assets.get(&source).unwrap().force_sorted_phase);
            (source, variant)
        };
        let id = variant.id();
        let hidden = app
            .world_mut()
            .spawn((MeshMaterial3d(variant), Visibility::Hidden))
            .id();
        app.world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .remove(&source);
        begin_frame(
            &mut variants,
            app.world().resource::<Assets<SourceMaterial>>(),
        );
        assert!(variants.is_empty());
        app.update();
        assert!(
            app.world()
                .resource::<Assets<SourceMaterial>>()
                .contains(id)
        );
        app.world_mut().despawn(hidden);
        for _ in 0..3 {
            app.update();
        }
        assert!(
            !app.world()
                .resource::<Assets<SourceMaterial>>()
                .contains(id)
        );
    }

    #[test]
    fn one_source_change_emits_one_variant_modification_for_repeated_draws() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<SourceMaterial>();
        let mut variants = HashMap::new();
        let source = app
            .world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .add(SourceMaterial::new(
                [0.0; 3],
                [1.0; 3],
                [0.0; 3],
                1.0,
                AlphaMode::Opaque,
                false,
            ));
        let variant = bind(
            &mut variants,
            &source,
            &mut app.world_mut().resource_mut::<Assets<SourceMaterial>>(),
        );
        app.update();
        let mut cursor = app
            .world()
            .resource::<Messages<AssetEvent<SourceMaterial>>>()
            .get_cursor_current();
        for (opacity, expected) in [(1.0, 0), (0.75, 1), (0.75, 0)] {
            let mut assets = app.world_mut().resource_mut::<Assets<SourceMaterial>>();
            assets.get_mut(&source).unwrap().parameters.opacity = opacity;
            begin_frame(&mut variants, &assets);
            for _ in 0..80 {
                assert_eq!(bind(&mut variants, &source, &mut assets), variant);
            }
            app.update();
            let events = app
                .world()
                .resource::<Messages<AssetEvent<SourceMaterial>>>();
            let changed = cursor
                .read(events)
                .filter(|event| matches!(event, AssetEvent::Modified { id } if *id == variant.id()))
                .count();
            assert_eq!(changed, expected);
        }
    }
}
