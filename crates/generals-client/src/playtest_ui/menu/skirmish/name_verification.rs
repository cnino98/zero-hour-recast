//! Drive the installed gadget through Bevy mouse and keyboard input messages.
use super::{
    Menu, UiData,
    name_entry::{EntryPart, NameText},
    options::OptionText,
};
use crate::playtest_view::Capture;
use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    math::Affine2,
    prelude::*,
    text::TextLayoutInfo,
    ui::CalculatedClip,
    window::{Ime, PrimaryWindow, WindowEvent},
};

#[derive(SystemParam)]
pub struct Scene<'w, 's> {
    menu: ResMut<'w, Menu>,
    data: Res<'w, UiData>,
    window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
    mouse: MessageWriter<'w, MouseButtonInput>,
    keyboard: MessageWriter<'w, WindowEvent>,
    label: Single<'w, 's, NameGeometry, With<NameText>>,
    cash: Query<
        'w,
        's,
        (
            &'static OptionText,
            &'static Text,
            &'static InheritedVisibility,
        ),
    >,
    parts: Query<
        'w,
        's,
        (
            &'static EntryPart,
            &'static Node,
            &'static InheritedVisibility,
        ),
    >,
}

#[derive(QueryData)]
struct NameGeometry {
    text: &'static Text,
    visible: &'static InheritedVisibility,
    layout: &'static TextLayoutInfo,
    font: &'static TextFont,
    node: &'static ComputedNode,
    transform: &'static UiGlobalTransform,
    clip: &'static CalculatedClip,
}

pub fn verify(
    mut frame: Local<u8>,
    mut scene: Scene,
    mut original_options: Local<Option<(u32, bool, u8)>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if *frame >= 28 {
        return;
    }
    let result = if *frame == 20
        && !scene.cash.iter().any(|(field, text, visible)| {
            matches!(field, OptionText::Cash) && text.0 == "$20000" && visible.get()
        }) {
        Err("selected cash did not reach the visible native money label")
    } else if *frame < 14 {
        step(*frame, &mut scene)
    } else {
        let (window, view) = &mut *scene.window;
        super::options_verification::step(
            *frame - 14,
            &mut scene.menu,
            &scene.data,
            (*window, view),
            &mut scene.mouse,
            &mut original_options,
        )
    };
    if let Err(error) = result {
        error!("Original player-name verification failed: {error}");
        exit.write(AppExit::error());
        *frame = 28;
        return;
    }
    *frame += 1;
    if *frame == 28 {
        scene.menu.open = Some(super::Dropdown::Faction(0));
        eprintln!(
            "Original player-name gadget: real mouse focus, Unicode keyboard append, source UTF-16 limit, backspace, visible glyph layout, IME preedit/commit/cancel and native empty-tab-list focus; native cash dropdown, checkbox release and speed-thumb drag passed"
        );
        capture.finish_scenario();
    }
}

fn step(frame: u8, scene: &mut Scene) -> Result<(), &'static str> {
    let (window, view) = &mut *scene.window;
    match frame {
        0 => {
            scene.menu.open = None;
            // The menu layout and visible hierarchy settle before the click.
            let point = scene.data.name_entry.window.area.center();
            let scale = Vec2::new(view.width() / 800.0, view.height() / 600.0);
            view.set_cursor_position(Some(point * scale));
        }
        1 => {
            scene.mouse.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                window: *window,
            });
        }
        2 => {
            scene.mouse.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Released,
                window: *window,
            });
            for _ in scene.data.preferences.name.chars() {
                key(
                    &mut scene.keyboard,
                    *window,
                    KeyCode::Backspace,
                    Key::Backspace,
                    None,
                );
            }
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::KeyP,
                Key::Character("PortTest🚀XYZ".into()),
                Some("PortTest🚀XYZ"),
            );
        }
        3 => {
            if scene.data.preferences.name != "PortTest🚀X" {
                return Err("keyboard append or native maximum not applied");
            }
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::Backspace,
                Key::Backspace,
                None,
            );
        }
        4 => {
            let label = &*scene.label;
            if scene.data.preferences.name != "PortTest🚀"
                || label.text.0 != "PortTest🚀"
                || !label.visible.get()
                || label.layout.glyphs.is_empty()
            {
                return Err("backspace did not reach the visible original name gadget");
            }
            let pixels = (scene.data.name_entry.window.font_size.min(512.0) * 96.0 / 72.0).round();
            if label.font.font_size != FontSize::Px(pixels) {
                return Err("original WND font point size changed");
            }
            let transform = Affine2::from(*label.transform)
                * Affine2::from_translation(label.node.content_box().min);
            for glyph in &label.layout.glyphs {
                let bounds = Rect::from_center_size(
                    transform.transform_point2(glyph.position),
                    glyph.atlas_info.rect.size(),
                );
                let painted = bounds.intersect(label.clip.clip);
                if bounds.height() > 0.0 && painted.height() < bounds.height() - 0.5 {
                    return Err("native name text is vertically clipped");
                }
            }
            check_tiles(scene)?;
        }
        _ => return ime_step(frame, scene),
    }
    Ok(())
}

fn ime_step(frame: u8, scene: &mut Scene) -> Result<(), &'static str> {
    let (window, view) = &mut *scene.window;
    match frame {
        5 => {
            // Leave spare capacity so a missing, repeated, or reordered commit
            // cannot be hidden by the original maximum-length limit.
            for _ in scene.data.preferences.name.chars() {
                key(
                    &mut scene.keyboard,
                    *window,
                    KeyCode::Backspace,
                    Key::Backspace,
                    None,
                );
            }
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::KeyP,
                Key::Character("P".into()),
                Some("P"),
            );
            scene.keyboard.write(WindowEvent::Ime(Ime::Preedit {
                window: *window,
                value: "あ".into(),
                cursor: Some((0, 3)),
            }));
        }
        6 => {
            if scene.data.preferences.name != "P"
                || scene.label.text.0 != "Pあ"
                || !view.ime_enabled
            {
                return Err(
                    "ordered keyboard and IME preedit did not remain separate in the focused name gadget",
                );
            }
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::Backspace,
                Key::Backspace,
                None,
            );
        }
        7 => {
            scene.keyboard.write(WindowEvent::Ime(Ime::Preedit {
                window: *window,
                value: "中".into(),
                cursor: Some((0, 3)),
            }));
        }
        8 => {
            if scene.data.preferences.name != "P" || scene.label.text.0 != "P中" {
                return Err("composition replacement or composing-key suppression failed");
            }
            // Native Winit emits the empty preedit followed by the commit and
            // suppresses its committing key. Subsequent ordinary keys are real.
            scene.keyboard.write(WindowEvent::Ime(Ime::Preedit {
                window: *window,
                value: String::new(),
                cursor: None,
            }));
            scene.keyboard.write(WindowEvent::Ime(Ime::Commit {
                window: *window,
                value: "文".into(),
            }));
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::KeyA,
                Key::Character("文".into()),
                Some("文"),
            );
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::KeyZ,
                Key::Character("Z".into()),
                Some("Z"),
            );
        }
        _ => return ime_cancel_and_focus(frame, scene),
    }
    Ok(())
}

fn ime_cancel_and_focus(frame: u8, scene: &mut Scene) -> Result<(), &'static str> {
    let (window, view) = &mut *scene.window;
    match frame {
        9 => {
            if scene.data.preferences.name != "P文文Z" || scene.label.text.0 != "P文文Z" {
                return Err("IME commit or later same-frame ordinary keyboard input changed");
            }
            scene.keyboard.write(WindowEvent::Ime(Ime::Preedit {
                window: *window,
                value: "あ".into(),
                cursor: Some((0, 3)),
            }));
        }
        10 => {
            if scene.data.preferences.name != "P文文Z" || scene.label.text.0 != "P文文Zあ" {
                return Err("second preedit changed committed player name");
            }
            scene.keyboard.write(WindowEvent::Ime(Ime::Preedit {
                window: *window,
                value: String::new(),
                cursor: None,
            }));
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::Backspace,
                Key::Backspace,
                None,
            );
        }
        11 => {
            if scene.data.preferences.name != "P文文" || scene.label.text.0 != "P文文" {
                return Err("IME cancellation swallowed later same-frame Backspace");
            }
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::Backspace,
                Key::Backspace,
                None,
            );
        }
        12 => {
            for (code, logical) in [
                (KeyCode::Tab, Key::Tab),
                (KeyCode::ArrowLeft, Key::ArrowLeft),
                (KeyCode::ArrowRight, Key::ArrowRight),
                (KeyCode::ArrowUp, Key::ArrowUp),
                (KeyCode::ArrowDown, Key::ArrowDown),
                (KeyCode::Enter, Key::Enter),
            ] {
                key(&mut scene.keyboard, *window, code, logical, None);
            }
            key(
                &mut scene.keyboard,
                *window,
                KeyCode::KeyZ,
                Key::Character("Z".into()),
                Some("Z"),
            );
        }
        13 => {
            if scene.data.preferences.name != "P文Z"
                || scene.label.text.0 != "P文Z"
                || !view.ime_enabled
            {
                return Err(
                    "native empty-tab-list navigation or Enter incorrectly relinquished name focus",
                );
            }
        }
        _ => unreachable!("IME verification frames"),
    }
    Ok(())
}

fn check_tiles(scene: &Scene) -> Result<(), &'static str> {
    let window = &scene.window.1;
    let width = (scene.data.name_entry.window.area.width() * window.width() / 800.0).floor();
    let size = |part| scene.data.image_size(&format!("TextEntryEnabled{part}")).x;
    let left = size("LeftEnd");
    let right = size("RightEnd");
    let center = size("RepeatingCenter");
    let small = size("SmallRepeatingCenter");
    let full = ((width - left - right) / center).floor();
    let remaining = ((width - left - right - full * center) / small).floor() + 1.0;
    let mut count = 0_u16;
    for (part, node, visible) in &scene.parts {
        if !visible.get() || node.width != px(scene.data.image_size(&part.0[0]).x) {
            return Err("native name-entry tiles stretched or became invisible");
        }
        count += 1;
    }
    if f32::from(count).to_bits() != (full + remaining + 2.0).to_bits() {
        return Err("native repeating/small/end tile coverage changed with resolution");
    }
    Ok(())
}

fn key(
    events: &mut MessageWriter<WindowEvent>,
    window: Entity,
    code: KeyCode,
    key: Key,
    text: Option<&str>,
) {
    events.write(WindowEvent::KeyboardInput(KeyboardInput {
        key_code: code,
        logical_key: key.clone(),
        state: ButtonState::Pressed,
        text: text.map(Into::into),
        repeat: false,
        window,
    }));
    events.write(WindowEvent::KeyboardInput(KeyboardInput {
        key_code: code,
        logical_key: key,
        state: ButtonState::Released,
        text: None,
        repeat: false,
        window,
    }));
}
