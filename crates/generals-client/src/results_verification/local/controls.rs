//! Exercise observer selection and Back through the actual mouse event path.
use super::{Flow, Target};
use crate::{
    control_bar::UiData,
    playtest::Match,
    playtest_ui::{Field, observer::Label},
};
use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    text::TextLayoutInfo,
    window::PrimaryWindow,
};

#[derive(SystemParam)]
pub struct Scene<'w, 's> {
    battle: Res<'w, Match>,
    data: Res<'w, UiData>,
    window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
    mouse: MessageWriter<'w, MouseButtonInput>,
    labels: Query<
        'w,
        's,
        (
            &'static Label,
            &'static Text,
            &'static InheritedVisibility,
            &'static TextLayoutInfo,
        ),
    >,
    money: Query<'w, 's, (&'static Field, &'static Text, &'static Node)>,
}

pub fn update(
    mut phase: Local<u8>,
    mut flow: ResMut<Flow>,
    mut scene: Scene,
    mut exit: MessageWriter<AppExit>,
) {
    if matches!(flow.target, Target::Defeat) || flow.viewed_at.is_none() || flow.controls_done {
        return;
    }
    if let Err(error) = step(*phase, &mut scene) {
        error!("Original observer controls failed: {error}");
        exit.write(AppExit::error());
        return;
    }
    *phase += 1;
    if *phase == 4 && matches!(flow.target, Target::PlayerInfo) {
        flow.controls_done = true;
        println!(
            "Observer information: original player label, four counters and chosen-player money passed; capturing the selected panel"
        );
    } else if *phase == 8 {
        flow.controls_done = true;
        println!(
            "Observer controls: real left-click player selection, original painted information labels, chosen-player money and right-click Back passed"
        );
    }
}

fn step(phase: u8, scene: &mut Scene) -> Result<(), &'static str> {
    match phase {
        0 | 4 => {
            let name = if phase == 0 {
                "ButtonPlayer0"
            } else {
                "ButtonCancel"
            };
            let point = scene.data.observer.windows[name].area.center();
            let view = &mut scene.window.1;
            let scale = Vec2::new(view.width() / 800.0, view.height() / 600.0);
            view.set_cursor_position(Some(point * scale));
        }
        1 | 2 | 5 | 6 => {
            scene.mouse.write(MouseButtonInput {
                button: if phase < 4 {
                    MouseButton::Left
                } else {
                    MouseButton::Right
                },
                state: if matches!(phase, 1 | 5) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
                window: scene.window.0,
            });
        }
        3 => {
            let owner = *scene
                .battle
                .observer_players
                .first()
                .ok_or("observer has no surviving players")?;
            if scene.battle.observer_player != Some(owner) {
                return Err("original player button did not choose the displayed player");
            }
            let expected_money = format!("${}", scene.battle.simulation.players[owner].cash);
            if !scene.money.iter().any(|(field, text, node)| {
                matches!(field, Field::Money)
                    && node.display != Display::None
                    && text.0 == expected_money
            }) {
                return Err("observer money did not show the chosen player");
            }
            let mut visible = 0;
            for (label, text, shown, layout) in &scene.labels {
                if !shown.get() {
                    continue;
                }
                if text.0.is_empty() || text.0.contains("%ls") || layout.glyphs.is_empty() {
                    return Err("observer information has no original laid-out text");
                }
                if label.0 == "StaticTextPlayerName"
                    && text.0
                        != crate::playtest_ui::result_player_name(&scene.battle, &scene.data, owner)
                {
                    return Err("observer information shows the wrong player name");
                }
                visible += 1;
            }
            if visible != 9 {
                return Err("observer information lost its name, four labels or four counters");
            }
        }
        7 => {
            if scene.battle.observer_player.is_some() || !scene.battle.selected.is_empty() {
                return Err("observer Back did not restore the player list");
            }
            if scene.money.iter().any(|(field, _, node)| {
                matches!(field, Field::Money) && node.display != Display::None
            }) {
                return Err("observer Back retained a chosen player's money");
            }
        }
        _ => unreachable!("bounded observer input scenario"),
    }
    Ok(())
}
