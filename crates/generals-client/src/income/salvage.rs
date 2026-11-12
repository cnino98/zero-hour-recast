//! The crate's `GUI:AddCash` popup, with original owner color, alpha and height.
use super::{Floating, Match, SourceAudio, UiData};
use bevy::{prelude::*, ui::FocusPolicy};
use generals_sim::skirmish::salvage::Reward;

pub(super) fn record(
    commands: &mut Commands,
    battle: &mut Match,
    audio: &mut SourceAudio,
    ui: &UiData,
) {
    for event in std::mem::take(&mut battle.simulation.presentation.salvage_events) {
        let actor = &battle.simulation.entities[event.collector];
        audio.salvage(
            &event.reward,
            event.collector,
            battle.simulation.object_position(event.collector),
        );
        let Reward::Money(amount) = event.reward else {
            continue;
        };
        if amount == 0 {
            continue;
        }
        let color = super::owner_color(&battle.simulation, actor.owner);
        let position = Vec3::new(
            event.position[0],
            event.position[2] + 10.0,
            -event.position[1],
        ) * 0.1;
        commands.spawn((
            Floating {
                generation: battle.generation,
                started: event.frame,
                last_update: event.frame.saturating_sub(1),
                frames: 0,
                alpha: 230,
                color,
                position,
                map_position: [event.position[0], event.position[1]],
            },
            Text::new(
                ui.label("GUI:AddCash")
                    .replacen("%d", &amount.to_string(), 1),
            ),
            TextFont {
                font: FontSource::Handle(ui.serif_handle.clone()),
                ..default()
            },
            TextColor(Color::srgba(color[0], color[1], color[2], 230.0 / 255.0)),
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
}
