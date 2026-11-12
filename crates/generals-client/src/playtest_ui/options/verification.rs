//! Traverse original options gadgets without writing the player's preferences.
use super::Editor;
use crate::{
    options_preferences::{Preferences, Values},
    playtest_ui::{Action, Menu},
    playtest_view::Capture,
};
use bevy::{
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    window::PrimaryWindow,
};

#[derive(Resource)]
struct Check {
    stage: u8,
    phase: u8,
    began: Option<f64>,
    original: Option<Values>,
}

pub fn register(app: &mut App) {
    app.insert_resource(Check {
        stage: 0,
        phase: 0,
        began: None,
        original: None,
    });
    app.add_systems(First, step);
}

fn step(world: &mut World) {
    world.resource_scope(|world, mut check: Mut<Check>| {
        if check.stage == 10 {
            return;
        }
        if let Err(reason) = advance(world, &mut check) {
            error!("Original options verification: {reason}");
            check.stage = 10;
            world.write_message(AppExit::error());
        }
    });
}

fn advance(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    if now - *check.began.get_or_insert(now) > 90.0 {
        error!(
            "Options traversal timeout stage={} phase={} options_open={}",
            check.stage,
            check.phase,
            world.resource::<Menu>().options_open,
        );
        for window in world
            .query_filtered::<&Window, With<PrimaryWindow>>()
            .iter(world)
        {
            error!(
                "Options traversal window physical_size={:?} scale={} cursor={:?} focused={}",
                window.resolution.physical_size(),
                window.scale_factor(),
                window.cursor_position(),
                window.focused,
            );
        }
        return Err("options pointer traversal timed out");
    }
    if check.original.is_none() {
        check.original = Some(world.resource::<Preferences>().applied().clone());
    }
    if check.stage == 1 || check.stage == 9 {
        if !world.resource::<Menu>().options_open
            || !world.resource::<Capture>().ready_for_scenario()
        {
            return Ok(());
        }
        if check.stage == 1 {
            world
                .resource_mut::<Capture>()
                .intermediate("options-before.png");
            check.stage = 2;
        } else {
            println!(
                "OPTIONS_RENDERED original_gadgets=true actual_pointer=true defaults_and_game_controls_cancelled=true preferences_retained=true"
            );
            world.resource_mut::<Capture>().finish_scenario();
            check.stage = 10;
        }
        return Ok(());
    }
    if check.stage == 2 && !world.resource::<Capture>().ready_for_scenario() {
        return Ok(());
    }
    if check.stage == 7 {
        if world.resource::<Menu>().options_open {
            return Err("Cancel retained modal options");
        }
        let original = check.original.as_ref().expect("original options retained");
        let current = world.resource::<Preferences>().applied();
        if original.alternate_mouse != current.alternate_mouse
            || original.retaliation != current.retaliation
            || original.double_click_attack_move != current.double_click_attack_move
            || [
                original.music,
                original.sound,
                original.sound3d,
                original.voice,
                original.scroll_factor,
                original.money_transaction_volume,
            ]
            .map(f32::to_bits)
                != [
                    current.music,
                    current.sound,
                    current.sound3d,
                    current.voice,
                    current.scroll_factor,
                    current.money_transaction_volume,
                ]
                .map(f32::to_bits)
        {
            return Err("cancelled edits changed applied preferences");
        }
        check.stage = 8;
    }
    if check.phase == 3 {
        check.phase = 0;
        check.stage += 1;
        return Ok(());
    }
    if check.phase == 0 && (3..=6).contains(&check.stage) {
        pending_controls(world, check.stage)?;
    }

    pointer(world, check)
}

fn pending_controls(world: &World, stage: u8) -> Result<(), &'static str> {
    let defaults = world.resource::<Preferences>().defaults();
    let draft = &world.resource::<Editor>().draft;
    if [draft.music, draft.sfx, draft.voice, draft.scroll]
        != [
            defaults.music,
            defaults.sfx,
            defaults.voice,
            defaults.scroll,
        ]
        || draft.alternate_mouse
            != if stage == 3 {
                defaults.alternate_mouse
            } else {
                !defaults.alternate_mouse
            }
        || draft.retaliation
            != if stage <= 4 {
                defaults.retaliation
            } else {
                !defaults.retaliation
            }
        || draft.double_click_attack_move
            != if stage <= 5 {
                defaults.double_click_attack_move
            } else {
                !defaults.double_click_attack_move
            }
    {
        return Err("Original game-control gadgets failed to change pending edits");
    }
    Ok(())
}

fn pointer(world: &mut World, check: &mut Check) -> Result<(), &'static str> {
    let point = world
        .query::<(
            &Action,
            &Interaction,
            &ComputedNode,
            &UiGlobalTransform,
            &InheritedVisibility,
        )>()
        .iter(world)
        .find_map(|(action, interaction, node, transform, visible)| {
            let target = matches!(
                (check.stage, action),
                (0 | 8, Action::Options)
                    | (2, Action::OptionsDefaults)
                    | (3, Action::OptionsAlternate)
                    | (4, Action::OptionsRetaliation)
                    | (5, Action::OptionsDoubleClickAttackMove)
                    | (6, Action::OptionsBack)
            );
            (target && visible.get() && !node.is_empty())
                .then_some((transform.translation, *interaction))
        });
    let Some((point, interaction)) = point else {
        return Ok(());
    };
    let (window, factor) = world
        .query_filtered::<(Entity, &Window), With<PrimaryWindow>>()
        .single(world)
        .map(|(entity, window)| (entity, window.scale_factor()))
        .map_err(|_| "primary window missing")?;
    // Native CursorMoved events can replace the preceding frame's cursor before
    // this phase. Keep the pointer on the gadget until UI focus acknowledges it.
    world
        .get_mut::<Window>(window)
        .ok_or("window disappeared")?
        .set_cursor_position(Some(point / factor));
    let state = match check.phase {
        0 => {
            check.phase = 1;
            return Ok(());
        }
        1 if interaction == Interaction::Hovered => ButtonState::Pressed,
        2 if interaction == Interaction::Pressed => ButtonState::Released,
        _ => return Ok(()),
    };
    world.write_message(MouseButtonInput {
        button: MouseButton::Left,
        state,
        window,
    });
    check.phase += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_cursor_overwrite_cannot_send_unacknowledged_gadget_click() {
        let mut world = World::new();
        world.init_resource::<Messages<MouseButtonInput>>();
        let window = world
            .spawn((
                Window {
                    resolution: bevy::window::WindowResolution::new(1280, 800)
                        .with_scale_factor_override(2.0),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        // The retained pause menu carries a second, hidden Options action.
        world.spawn((
            Action::Options,
            Interaction::None,
            ComputedNode {
                size: Vec2::new(180.0, 40.0),
                ..default()
            },
            UiGlobalTransform::from_xy(10.0, 10.0),
            InheritedVisibility::HIDDEN,
        ));
        let target = world
            .spawn((
                Action::Options,
                Interaction::None,
                ComputedNode {
                    size: Vec2::new(180.0, 40.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(1030.0, 338.0),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        let mut check = Check {
            stage: 0,
            phase: 0,
            began: None,
            original: None,
        };
        pointer(&mut world, &mut check).unwrap();
        // A native event moves off the gadget after the initial cursor request.
        world
            .get_mut::<Window>(window)
            .unwrap()
            .set_cursor_position(Some(Vec2::ZERO));
        pointer(&mut world, &mut check).unwrap();
        assert!(world.resource::<Messages<MouseButtonInput>>().is_empty());
        assert_eq!(
            world
                .get::<Window>(window)
                .unwrap()
                .physical_cursor_position(),
            Some(Vec2::new(1030.0, 338.0)),
        );
        *world.get_mut::<Interaction>(target).unwrap() = Interaction::Hovered;
        pointer(&mut world, &mut check).unwrap();
        let events: Vec<_> = world
            .resource_mut::<Messages<MouseButtonInput>>()
            .drain()
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].state, ButtonState::Pressed);
        assert_eq!(events[0].window, window);
        pointer(&mut world, &mut check).unwrap();
        assert!(world.resource::<Messages<MouseButtonInput>>().is_empty());
        *world.get_mut::<Interaction>(target).unwrap() = Interaction::Pressed;
        world
            .get_mut::<Window>(window)
            .unwrap()
            .set_cursor_position(Some(Vec2::ZERO));
        pointer(&mut world, &mut check).unwrap();
        let events: Vec<_> = world
            .resource_mut::<Messages<MouseButtonInput>>()
            .drain()
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].state, ButtonState::Released);
        assert_eq!(
            world
                .get::<Window>(window)
                .unwrap()
                .physical_cursor_position(),
            Some(Vec2::new(1030.0, 338.0)),
        );
    }
}
