//! AutoDepositUpdate.cpp130–196: player-colored text and client-only position RNG.
use super::Floating;
use crate::{
    control_bar::UiData, model_animation::Animations, playtest::Match, source_audio::SourceAudio,
};
use bevy::{prelude::*, ui::FocusPolicy};

pub(super) fn record(
    commands: &mut Commands,
    battle: &mut Match,
    audio: &mut SourceAudio,
    ui: &UiData,
    animations: &mut Animations,
) {
    let viewer = battle.observer_player.unwrap_or(0);
    for event in std::mem::take(&mut battle.simulation.presentation.money_deposit_events) {
        audio.money_deposit(event.owner == viewer);
    }
    for event in std::mem::take(&mut battle.simulation.presentation.cash_bounties) {
        show_cash(
            commands,
            ui,
            (battle.generation, event.frame),
            event.position,
            i64::from(event.amount),
            ([1.0, 1.0, 0.0], 255),
        );
    }
    for event in std::mem::take(&mut battle.simulation.presentation.deposit_events) {
        if event.visible_players & (1 << viewer) == 0 {
            continue;
        }
        let mut position = event.position;
        if let Some(radii) = event.radii {
            for axis in 0..2 {
                let extent = radii[axis] * 0.3;
                position[axis] += animations.effect_real(battle.generation, -extent, extent);
            }
        }
        let color = super::owner_color(&battle.simulation, event.owner);
        show_cash(
            commands,
            ui,
            (battle.generation, event.frame),
            position,
            i64::from(event.amount),
            (color, 230),
        );
    }
}

fn show_cash(
    commands: &mut Commands,
    ui: &UiData,
    (generation, frame): (u32, u64),
    position: [f32; 3],
    amount: i64,
    (color, alpha): ([f32; 3], u8),
) {
    commands.spawn((
        Floating {
            color,
            generation,
            started: frame,
            last_update: frame.saturating_sub(1),
            frames: 0,
            alpha,
            position: Vec3::new(position[0], position[2], -position[1]) * 0.1,
            map_position: [position[0], position[1]],
        },
        Text::new(
            ui.label("GUI:AddCash")
                .replacen("%d", &amount.to_string(), 1),
        ),
        TextFont {
            font: FontSource::Handle(ui.serif_handle.clone()),
            ..default()
        },
        TextColor(Color::srgba(
            color[0],
            color[1],
            color[2],
            f32::from(alpha) / 255.0,
        )),
        TextShadow {
            offset: Vec2::ONE,
            color: Color::BLACK,
        },
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        FocusPolicy::Pass,
        ZIndex(3),
    ));
}
