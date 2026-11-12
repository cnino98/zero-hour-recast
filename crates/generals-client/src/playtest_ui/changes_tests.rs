use super::{PromotionIcon, promotion_icon, set_font, set_image};
use bevy::{asset::AssetPlugin, ecs::system::RunSystemOnce, prelude::*};

#[test]
fn repeated_ui_images_do_not_dirty_layout_and_each_visual_field_still_updates() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let entity = app.world_mut().spawn(ImageNode::default()).id();
    app.update();
    for index in 0..8 {
        let mut next = app.world().get::<ImageNode>(entity).unwrap().clone();
        match index {
            0 => {
                next.image = Handle::Uuid(
                    bevy::asset::uuid::Uuid::from_u128(1),
                    std::marker::PhantomData,
                );
            }
            1 => next.color = Color::BLACK,
            2 => next.rect = Some(Rect::new(1.0, 2.0, 3.0, 4.0)),
            3 => {
                next.texture_atlas = Some(TextureAtlas {
                    layout: Handle::default(),
                    index: 1,
                });
            }
            4 => next.flip_x = true,
            5 => next.flip_y = true,
            6 => next.image_mode = NodeImageMode::Stretch,
            _ => next.visual_box = bevy::ui::VisualBox::BorderBox,
        }
        let repeated = next.clone();
        app.world_mut()
            .run_system_once(move |mut images: Query<&mut ImageNode>| {
                set_image(&mut images.single_mut().unwrap(), next.clone());
            })
            .unwrap();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, Changed<ImageNode>>()
                .iter(app.world())
                .count(),
            1
        );
        app.update();
        app.world_mut()
            .run_system_once(move |mut images: Query<&mut ImageNode>| {
                set_image(&mut images.single_mut().unwrap(), repeated.clone());
            })
            .unwrap();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, Changed<ImageNode>>()
                .iter(app.world())
                .count(),
            0
        );
    }
}

#[test]
fn repeated_font_appearance_is_clean_but_resize_and_font_changes_propagate() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let entity = app.world_mut().spawn(TextFont::default()).id();
    for (size, handle) in [
        (12.0, Handle::default()),
        (24.0, Handle::default()),
        (
            24.0,
            Handle::Uuid(
                bevy::asset::uuid::Uuid::from_u128(2),
                std::marker::PhantomData,
            ),
        ),
    ] {
        let next = handle.clone();
        app.world_mut()
            .run_system_once(move |mut fonts: Query<&mut TextFont>| {
                set_font(&mut fonts.single_mut().unwrap(), next.clone(), size);
            })
            .unwrap();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, Changed<TextFont>>()
                .iter(app.world())
                .count(),
            1
        );
        app.update();
        app.world_mut()
            .run_system_once(move |mut fonts: Query<&mut TextFont>| {
                set_font(&mut fonts.single_mut().unwrap(), handle.clone(), size);
            })
            .unwrap();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, Changed<TextFont>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world().get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(size)
        );
    }
}

#[test]
fn unchanged_command_material_is_not_uploaded_but_all_render_fields_propagate() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<PromotionIcon>();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<PromotionIcon>>()
        .add(PromotionIcon {
            uv_rect: Vec4::ONE,
            state: Vec4::new(0.0, 1.0, -1.0, 0.0),
            clock_color: Vec4::ZERO,
            image: Handle::default(),
        });
    app.update();
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<PromotionIcon>>>()
        .clear();
    for field in 0..4 {
        let mut next = app
            .world()
            .resource::<Assets<PromotionIcon>>()
            .get(&handle)
            .unwrap()
            .clone();
        match field {
            0 => next.uv_rect = Vec4::ZERO,
            1 => next.state.z = 42.0,
            2 => next.clock_color = Vec4::ONE,
            _ => {
                next.image = Handle::Uuid(
                    bevy::asset::uuid::Uuid::from_u128(3),
                    std::marker::PhantomData,
                );
            }
        }
        promotion_icon::update(&mut app.world_mut().resource_mut(), &handle, next.clone());
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<AssetEvent<PromotionIcon>>>()
                .drain()
                .any(|event| matches!(event, AssetEvent::Modified { id } if id == handle.id()))
        );
        promotion_icon::update(&mut app.world_mut().resource_mut(), &handle, next);
        app.update();
        assert!(
            !app.world_mut()
                .resource_mut::<Messages<AssetEvent<PromotionIcon>>>()
                .drain()
                .any(|event| matches!(event, AssetEvent::Modified { .. }))
        );
    }
}

#[test]
fn separate_hud_target_clears_transparently_and_loads_the_world_output() {
    use bevy::{camera::CameraOutputMode, render::render_resource::BlendState};
    let mut app = App::new();
    app.world_mut()
        .run_system_once(|mut commands: Commands| super::register_camera(&mut commands))
        .unwrap();
    let (camera, msaa) = app
        .world_mut()
        .query_filtered::<(&Camera, Option<&Msaa>), With<IsDefaultUiCamera>>()
        .single(app.world())
        .unwrap();
    assert_eq!(msaa, Some(&Msaa::Off));
    // The world camera stores encoded RGB while Camera2d's HUD remains linear.
    // Bevy's main texture cache separates these formats: a Load clear operation
    // on the HUD cannot preserve world pixels and would read undefined storage.
    let ClearColorConfig::Custom(clear) = camera.clear_color else {
        panic!("separate HUD texture must be initialized");
    };
    assert_eq!(clear.to_linear().to_f32_array().map(f32::to_bits), [0; 4]);
    assert!(camera.order > 0);
    let CameraOutputMode::Write {
        blend_state,
        clear_color,
    } = camera.output_mode
    else {
        panic!("HUD must composite into the world output");
    };
    assert_eq!(blend_state, Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING));
    assert!(matches!(clear_color, ClearColorConfig::None));
}
