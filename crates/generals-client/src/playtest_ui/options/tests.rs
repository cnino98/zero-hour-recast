use super::*;
use bevy::ecs::system::RunSystemOnce;
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

fn installed_editor(original: &Path) -> World {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let preferences = Preferences::load(&assets, original).unwrap();
    let mut data = crate::control_bar::load(&assets, &battle.simulation.catalog, original).unwrap();
    let mut images = Assets::<Image>::default();
    for (name, image) in std::mem::take(&mut data.textures) {
        data.handles.insert(name, images.add(image));
    }
    let mut world = World::new();
    world.insert_resource(data);
    world.insert_resource(battle);
    world.insert_resource(preferences);
    world.spawn((Window::default(), PrimaryWindow));
    world
        .run_system_once(
            |mut commands: Commands, data: Res<UiData>, battle: Res<crate::playtest::Match>| {
                super::super::menu::setup(&mut commands, data.into_inner(), battle.into_inner());
            },
        )
        .unwrap();
    assert!(world.query::<&BoxPart>().iter(&world).count() > 0);
    assert_eq!(world.query::<&Thumb>().iter(&world).count(), 5);
    assert_eq!(world.query::<&Checkbox>().iter(&world).count(), 3);
    world
}

#[ignore = "requires original game installation"]
#[test]
fn installed_editor_cancel_discards_and_accept_saves_only_rewrite_overlay() {
    let fixture = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!(
            "options-editor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(fixture.join("UserData")).unwrap();
    let original = fixture.join("UserData/Options.ini");
    let source = "MusicVolume = 41\nSFXVolume = 61\nSFX3DVolume = 61\nVoiceVolume = 31\nScrollFactor = 50\nUseAlternateMouse = no\n";
    fs::write(&original, source).unwrap();
    let mut world = installed_editor(&original);
    let dynamic = world
        .query_filtered::<Entity, Or<(With<BoxPart>, With<Thumb>)>>()
        .iter(&world)
        .collect::<Vec<_>>();
    assert!(
        dynamic
            .iter()
            .all(|&entity| world.get::<Layer>(entity).is_none())
    );
    let update_system = world.register_system(update);
    activate(&Action::Options, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    assert!(world.resource::<Menu>().options_open);
    world.resource_mut::<Editor>().draft.music = 0;
    world.resource_mut::<Editor>().draft.alternate_mouse = true;
    activate(
        &Action::OptionsRetaliation,
        &mut world.resource_mut::<Editor>(),
    );
    world.run_system(update_system).unwrap();
    activate(
        &Action::OptionsDoubleClickAttackMove,
        &mut world.resource_mut::<Editor>(),
    );
    world.run_system(update_system).unwrap();
    assert!(!world.resource::<Editor>().draft.retaliation);
    assert!(world.resource::<Editor>().draft.double_click_attack_move);
    assert!(world.resource::<Preferences>().applied().retaliation);
    assert!(
        !world
            .resource::<Preferences>()
            .applied()
            .double_click_attack_move
    );
    activate(&Action::OptionsBack, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    assert!(!world.resource::<Menu>().options_open);
    assert!(
        dynamic
            .iter()
            .all(|&entity| matches!(world.get::<Visibility>(entity), Some(Visibility::Hidden)))
    );
    assert_eq!(
        world.resource::<Preferences>().applied().music.to_bits(),
        (41.0_f32 / 100.0).to_bits()
    );
    assert!(!fixture.join("output/Options.ini").exists());
    activate(&Action::Options, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    assert_eq!(world.resource::<Editor>().draft.music, 41);
    activate(
        &Action::OptionsDefaults,
        &mut world.resource_mut::<Editor>(),
    );
    world.run_system(update_system).unwrap();
    assert_eq!(
        world.resource::<Preferences>().applied().music.to_bits(),
        (41.0_f32 / 100.0).to_bits()
    );
    assert!(!fixture.join("output/Options.ini").exists());
    world.resource_mut::<Editor>().draft.music = 0;
    world.resource_mut::<Editor>().draft.voice = 17;
    world.resource_mut::<Editor>().draft.scroll = 145;
    world.resource_mut::<Editor>().draft.alternate_mouse = true;
    world.resource_mut::<Editor>().draft.retaliation = false;
    world
        .resource_mut::<Editor>()
        .draft
        .double_click_attack_move = true;
    activate(&Action::OptionsAccept, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    assert!(!world.resource::<Menu>().options_open);
    assert_eq!(
        world.resource::<Preferences>().applied().music.to_bits(),
        0.0_f32.to_bits()
    );
    assert!(world.resource::<Preferences>().applied().alternate_mouse);
    let saved = fs::read_to_string(fixture.join("output/Options.ini")).unwrap();
    assert!(saved.contains("VoiceVolume = 17"));
    assert!(saved.contains("ScrollFactor = 145"));
    assert!(saved.contains("Retaliation = no"));
    assert!(saved.contains("UseDoubleClickAttackMove = yes"));
    assert_eq!(fs::read_to_string(&original).unwrap(), source);
    fs::remove_dir_all(fixture).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
fn installed_options_keeps_parent_menus_visible_under_original_backdrop() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    battle.started = false;
    let root = &generals_assets::game_root().unwrap();
    let mut data = crate::control_bar::load(
        &assets,
        &battle.simulation.catalog,
        &root.join("UserData/Options.ini"),
    )
    .unwrap();
    let mut images = Assets::<Image>::default();
    for (name, image) in std::mem::take(&mut data.textures) {
        data.handles.insert(name, images.add(image));
    }
    assert_eq!(
        data.options.background,
        Rect::from_corners(Vec2::ZERO, Vec2::new(799.0, 599.0))
    );
    let mut world = World::new();
    world.insert_resource(data);
    world.insert_resource(battle);
    world
        .run_system_once(
            |mut commands: Commands, data: Res<UiData>, battle: Res<crate::playtest::Match>| {
                super::super::menu::setup(&mut commands, &data, &battle);
                super::super::results::setup(&mut commands, &data);
            },
        )
        .unwrap();
    let backdrop = world
        .query::<(&Layer, &Node, &BackgroundColor, &ZIndex)>()
        .iter(&world)
        .find(|(layer, _, _, order)| matches!(layer, Layer::Options) && order.0 == 59)
        .map(|(_, node, color, _)| (node.clone(), color.0))
        .unwrap();
    assert_eq!(backdrop.0.width, Val::Percent(799.0 / 8.0));
    assert_eq!(backdrop.0.height, Val::Percent(599.0 / 6.0));
    assert_eq!(backdrop.1, Color::srgba_u8(2, 2, 2, 126));
    let layers = world.register_system(
        |battle: Res<crate::playtest::Match>,
         menu: Res<Menu>,
         screen: Res<super::super::results::ResultsScreen>,
         mut layers: Query<(&Layer, &mut Visibility)>| {
            super::super::update_layers(&battle, &menu, &screen, None, &mut layers);
        },
    );
    world.resource_mut::<Menu>().options_open = true;
    world.run_system(layers).unwrap();
    let parent_controls = world
        .query::<(Entity, &Action, &Layer)>()
        .iter(&world)
        .filter(|(_, _, layer)| matches!(layer, Layer::Menu))
        .map(|(entity, _, _)| entity)
        .collect::<Vec<_>>();
    assert!(!parent_controls.is_empty());
    assert!(
        parent_controls
            .iter()
            .all(|&entity| matches!(world.get::<Visibility>(entity), Some(Visibility::Inherited)))
    );
    assert!(
        world
            .query::<(&Layer, &Visibility)>()
            .iter(&world)
            .filter(|(layer, _)| matches!(layer, Layer::Shell | Layer::Options))
            .all(|(_, visibility)| matches!(visibility, Visibility::Inherited))
    );
    world.resource_mut::<Menu>().options_open = false;
    world.run_system(layers).unwrap();
    assert!(
        parent_controls
            .iter()
            .all(|&entity| matches!(world.get::<Visibility>(entity), Some(Visibility::Inherited)))
    );
    world.resource_mut::<crate::playtest::Match>().started = true;
    world.resource_mut::<crate::playtest::Match>().paused = true;
    world.resource_mut::<Menu>().options_open = true;
    world.run_system(layers).unwrap();
    assert!(
        world
            .query::<(&Layer, &Visibility)>()
            .iter(&world)
            .filter(|(layer, _)| matches!(layer, Layer::Pause))
            .all(|(_, visibility)| matches!(visibility, Visibility::Inherited))
    );
    world.resource_mut::<Menu>().options_open = false;
    world.run_system(layers).unwrap();
    assert!(
        world
            .query::<(&Layer, &Visibility)>()
            .iter(&world)
            .filter(|(layer, _)| matches!(layer, Layer::Pause))
            .all(|(_, visibility)| matches!(visibility, Visibility::Inherited))
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_resolution_accept_then_decline_reverts_only_display_settings() {
    let fixture = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!(
            "options-resolution-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(fixture.join("UserData")).unwrap();
    let original = fixture.join("UserData/Options.ini");
    fs::write(
        &original,
        "Resolution = 1217 763\nGamma = 73\nMusicVolume = 41\n",
    )
    .unwrap();
    let mut world = installed_editor(&original);
    world.resource_mut::<crate::playtest::Match>().started = false;
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(&world)
        .unwrap();
    world
        .get_mut::<Window>(window)
        .unwrap()
        .resolution
        .set_physical_resolution(1217, 763);
    let update_system = world.register_system(update);
    activate(&Action::Options, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    assert_eq!(world.resource::<Editor>().draft.resolution, [1217, 763]);
    world.resource_mut::<Editor>().draft.resolution = [1920, 1080];
    activate(
        &Action::OptionsDefaults,
        &mut world.resource_mut::<Editor>(),
    );
    world.run_system(update_system).unwrap();
    assert_eq!(world.resource::<Editor>().draft.resolution, [1920, 1080]);
    assert_eq!(world.resource::<Editor>().draft.gamma, 50);
    world.resource_mut::<Editor>().draft.music = 17;
    activate(&Action::OptionsAccept, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    assert_eq!(
        world.get::<Window>(window).unwrap().physical_size(),
        UVec2::new(1920, 1080)
    );
    assert_eq!(
        world.resource::<Editor>().display.previous,
        Some([1217, 763])
    );
    assert!(world.resource::<Menu>().options_open);
    // Lower editor buttons cannot accept or decline a resolution dialog.
    activate(&Action::OptionsBack, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    assert!(world.resource::<Menu>().options_open);
    let point =
        world.resource::<UiData>().options.confirmation["MessageBox.wnd:ButtonCancel"].center();
    let request = display::confirmation_request(world.resource::<UiData>(), point);
    world.resource_mut::<Editor>().request = request;
    world.run_system(update_system).unwrap();
    assert!(!world.resource::<Menu>().options_open);
    assert_eq!(
        world.get::<Window>(window).unwrap().physical_size(),
        UVec2::new(1217, 763)
    );
    assert_eq!(
        world.resource::<Preferences>().applied().resolution,
        [1217, 763]
    );
    assert_eq!(world.resource::<Preferences>().applied().gamma, 50);
    assert_eq!(
        world.resource::<Preferences>().applied().music.to_bits(),
        (17.0_f32 / 100.0).to_bits()
    );
    let saved = fs::read_to_string(fixture.join("output/Options.ini")).unwrap();
    assert!(saved.contains("Resolution = 1217 763"));
    assert!(saved.contains("MusicVolume = 17"));
    activate(&Action::Options, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    world.resource_mut::<Editor>().draft.resolution = [1600, 900];
    activate(&Action::OptionsAccept, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    activate(&Action::OptionsAccept, &mut world.resource_mut::<Editor>());
    world.run_system(update_system).unwrap();
    assert!(world.resource::<Menu>().options_open);
    let point = world.resource::<UiData>().options.confirmation["MessageBox.wnd:ButtonOk"].center();
    let request = display::confirmation_request(world.resource::<UiData>(), point);
    world.resource_mut::<Editor>().request = request;
    world.run_system(update_system).unwrap();
    assert!(!world.resource::<Menu>().options_open);
    assert_eq!(
        world.resource::<Preferences>().applied().resolution,
        [1600, 900]
    );
    assert_eq!(
        fs::read_to_string(&original).unwrap(),
        "Resolution = 1217 763\nGamma = 73\nMusicVolume = 41\n"
    );
    fs::remove_dir_all(fixture).unwrap();
}

#[ignore = "requires original game installation"]
#[test]
fn installed_display_controls_begin_hidden_and_confirmation_requires_resolution_change() {
    let fixture = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!(
            "options-display-lifecycle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
    fs::create_dir_all(fixture.join("UserData")).unwrap();
    let original = fixture.join("UserData/Options.ini");
    fs::write(&original, "Resolution = 1280 720\n").unwrap();
    let mut world = installed_editor(&original);
    let update = world.register_system(super::update);
    // Closed editors skip presentation work, both on the shell and in a match.
    // Their standalone display controls must therefore be hidden at creation.
    for started in [false, true] {
        world.resource_mut::<crate::playtest::Match>().started = started;
        assert!(!world.resource::<Menu>().options_open);
        assert!(
            world
                .query::<(&display::Part, &Visibility)>()
                .iter(&world)
                .all(|(_, visibility)| *visibility == Visibility::Hidden)
        );
        world.run_system(update).unwrap();
        assert!(
            world
                .query::<(&display::Part, &Visibility)>()
                .iter(&world)
                .all(|(_, visibility)| *visibility == Visibility::Hidden)
        );
    }
    world.resource_mut::<crate::playtest::Match>().started = false;
    activate(&Action::Options, &mut world.resource_mut::<Editor>());
    world.run_system(update).unwrap();
    assert!(world.resource::<Menu>().options_open);
    assert_resolution_confirmation(&mut world, Visibility::Hidden);
    assert!(
        world
            .query::<(&display::Part, &Visibility)>()
            .iter(&world)
            .filter(|(part, _)| matches!(part, display::Part::Combo | display::Part::Arrow))
            .all(|(_, visibility)| *visibility == Visibility::Visible)
    );
    let current = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(&world)
        .unwrap()
        .physical_size()
        .to_array();
    world.resource_mut::<Editor>().draft.resolution = [current[0] + 200, current[1] + 100];
    activate(&Action::OptionsAccept, &mut world.resource_mut::<Editor>());
    world.run_system(update).unwrap();
    assert!(world.resource::<Editor>().display.previous.is_some());
    assert_resolution_confirmation(&mut world, Visibility::Visible);
    world.resource_mut::<Editor>().request = Some(Request::KeepResolution);
    world.run_system(update).unwrap();
    assert!(!world.resource::<Menu>().options_open);
    assert!(
        world
            .query::<(&display::Part, &Visibility)>()
            .iter(&world)
            .all(|(_, visibility)| *visibility == Visibility::Hidden)
    );
    activate(&Action::Options, &mut world.resource_mut::<Editor>());
    world.run_system(update).unwrap();
    assert_resolution_confirmation(&mut world, Visibility::Hidden);
    activate(&Action::OptionsBack, &mut world.resource_mut::<Editor>());
    world.run_system(update).unwrap();
    assert!(
        world
            .query::<(&display::Part, &Visibility)>()
            .iter(&world)
            .all(|(_, visibility)| *visibility == Visibility::Hidden)
    );
    assert_eq!(
        fs::read_to_string(&original).unwrap(),
        "Resolution = 1280 720\n"
    );
    fs::remove_dir_all(fixture).unwrap();
}

fn assert_resolution_confirmation(world: &mut World, expected: Visibility) {
    let parts: Vec<_> = world
        .query::<(&display::Part, &Visibility)>()
        .iter(world)
        .filter(|(part, _)| {
            matches!(
                part,
                display::Part::Confirmation | display::Part::ConfirmationMessage
            )
        })
        .map(|(_, visibility)| *visibility)
        .collect();
    assert!(!parts.is_empty());
    assert!(parts.iter().all(|visibility| *visibility == expected));
}
