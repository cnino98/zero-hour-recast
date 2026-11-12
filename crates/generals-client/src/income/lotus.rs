//! `SpecialAbilityUpdate` cash theft uses two original world-space floating texts.
use super::{Floating, Match, UiData};
use bevy::{prelude::*, ui::FocusPolicy};

pub(super) fn record(commands: &mut Commands, battle: &Match, ui: &UiData) {
    for event in &battle.simulation.presentation.ability_feedback {
        if let generals_sim::skirmish::special_ability::FeedbackEffect::CashStolen(amount) =
            event.effect
            && event.frame == battle.simulation.tick
        {
            cash_theft_positions(
                commands,
                battle,
                ui,
                event.frame,
                [event.source_position, event.target_position],
                amount,
            );
        }
    }
}

pub fn cash_theft(
    commands: &mut Commands,
    battle: &Match,
    ui: &UiData,
    frame: u64,
    source: usize,
    target: usize,
    amount: u32,
) {
    cash_theft_positions(
        commands,
        battle,
        ui,
        frame,
        [
            battle.simulation.object_position(source),
            battle.simulation.object_position(target),
        ],
        amount,
    );
}

fn cash_theft_positions(
    commands: &mut Commands,
    battle: &Match,
    ui: &UiData,
    frame: u64,
    positions: [[f32; 3]; 2],
    amount: u32,
) {
    for (point, label, height, color) in [
        (positions[0], "GUI:AddCash", 20.0, [0.0, 1.0, 0.0]),
        (positions[1], "GUI:LoseCash", 30.0, [1.0, 0.0, 0.0]),
    ] {
        commands.spawn((
            Floating {
                color,
                generation: battle.generation,
                started: frame,
                last_update: frame.saturating_sub(1),
                frames: 0,
                alpha: 255,
                position: Vec3::new(point[0], point[2] + height, -point[1]) * 0.1,
                map_position: [point[0], point[1]],
            },
            Text::new(ui.label(label).replacen("%d", &amount.to_string(), 1)),
            TextFont {
                font: FontSource::Handle(ui.serif_handle.clone()),
                ..default()
            },
            TextColor(Color::srgb(color[0], color[1], color[2])),
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[ignore = "requires original game installation"]
    #[test]
    fn theft_text_reuses_original_green_source_red_victim_heights_and_labels() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let ui = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let source = 0;
        let target = 1;
        let from = battle.simulation.object_position(source);
        let to = battle.simulation.object_position(target);
        let source_label = ui.label("GUI:AddCash").replacen("%d", "700", 1);
        let target_label = ui.label("GUI:LoseCash").replacen("%d", "700", 1);
        let mut world = World::new();
        world
            .run_system_once(move |mut commands: Commands| {
                cash_theft(&mut commands, &battle, &ui, 20, source, target, 700);
            })
            .unwrap();
        let mut query = world.query::<(&Floating, &Text)>();
        let records: Vec<_> = query.iter(&world).collect();
        assert_eq!(records.len(), 2);
        for (text, label) in records {
            if label.0 == source_label {
                assert_eq!(
                    text.color.map(f32::to_bits),
                    [0.0_f32, 1.0, 0.0].map(f32::to_bits)
                );
                assert_eq!(
                    text.position,
                    Vec3::new(from[0], from[2] + 20.0, -from[1]) * 0.1
                );
            } else {
                assert_eq!(label.0, target_label);
                assert_eq!(
                    text.color.map(f32::to_bits),
                    [1.0_f32, 0.0, 0.0].map(f32::to_bits)
                );
                assert_eq!(text.position, Vec3::new(to[0], to[2] + 30.0, -to[1]) * 0.1);
            }
            assert_eq!(text.started, 20);
            assert_eq!(text.alpha, 255);
        }
    }
}
