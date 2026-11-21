use super::*;
use crate::{
    control_bar::{CommandAction, PowerAim, PowerCommand},
    input,
    playtest_ui::dispatch_command,
    skirmish_verification::friendly_entity,
};
use generals_sim::skirmish::{Faction, Kind as UnitKind, Order};

const FRENZY: &str = "SuperweaponFrenzy";

fn installed() -> (Match, Mouse, usize, usize) {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    game.set_match_options(20_000, 1);
    game.reset(Faction::China, Faction::China).unwrap();
    let (army, _) = crate::progression_verification::earn_combat_rank(game, 3).unwrap();
    game.purchase_science(0, "SCIENCE_Frenzy1").unwrap();
    let provider = friendly_entity(game, UnitKind::CommandCenter, 0).unwrap();
    let mouse = input::load(
        &assets,
        &generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini"),
        &game.catalog,
    )
    .unwrap();
    (battle, mouse, provider, army[0])
}

fn marker_ids(battle: &Match) -> Vec<usize> {
    battle
        .simulation
        .entities
        .iter()
        .enumerate()
        .filter_map(|(id, actor)| {
            (battle.simulation.catalog[actor.type_index].name == "Frenzy_InvisibleMarker_Level1")
                .then_some(id)
        })
        .collect()
}

fn cast(battle: &mut Match, mouse: &mut Mouse, provider: usize, target: usize) {
    battle.select_objects(&[provider], true);
    dispatch_command(
        battle,
        mouse,
        CommandAction::NativePower(PowerCommand {
            name: FRENZY,
            aim: PowerAim::Location,
        }),
    );
    let point = Vec2::from_array(battle.simulation.entities[target].position);
    input::targeted_order(battle, mouse, point, None);
    assert!(matches!(mouse.pending_command, CommandMode::Normal));
}

#[ignore = "requires original game installation"]
#[test]
fn original_one_millisecond_marker_survives_its_completed_birth_frame() {
    let (mut battle, mut mouse, provider, target) = installed();
    let before = battle.simulation.tick;
    cast(&mut battle, &mut mouse, provider, target);
    assert!(marker_ids(&battle).is_empty());
    assert_eq!(
        battle.simulation.power_cooldown_remaining(provider, FRENZY),
        0
    );
    step(&mut battle, &mut mouse);
    let markers = marker_ids(&battle);
    assert_eq!(markers.len(), 1);
    assert_eq!(battle.simulation.tick, before + 1);
    assert!(battle.simulation.entities[markers[0]].exists());
    assert!(battle.simulation.power_cooldown_remaining(provider, FRENZY) > 0);
    assert!(!battle.simulation.entities[target].frenzy_tint());
    step(&mut battle, &mut mouse);
    assert!(!battle.simulation.entities[markers[0]].exists());
    assert!(battle.simulation.entities[target].frenzy_tint());
}

#[ignore = "requires original game installation"]
#[test]
fn duplicate_queued_power_casts_once_with_the_single_in_logic_cast_rng() {
    let mut observed = Vec::new();
    for queued in [false, true] {
        let (mut battle, mut mouse, provider, target) = installed();
        battle.select_objects(&[provider], true);
        let point = battle.simulation.entities[target].position;
        let point = [point[0], point[1], battle.simulation.ground_height(point)];
        let before = generals_sim::skirmish::diagnostics::record(
            &battle.simulation,
            &mut std::collections::hash_map::DefaultHasher::new(),
        );
        if queued {
            cast(&mut battle, &mut mouse, provider, target);
            cast(&mut battle, &mut mouse, provider, target);
        }
        assert!(marker_ids(&battle).is_empty());
        assert_eq!(
            battle.simulation.power_cooldown_remaining(provider, FRENZY),
            0
        );
        assert_eq!(
            generals_sim::skirmish::diagnostics::record(
                &battle.simulation,
                &mut std::collections::hash_map::DefaultHasher::new(),
            ),
            before,
        );
        // GameLogic dispatches after scripts and before object updates. The
        // control uses one cast at that same boundary, never between frames.
        assert!(battle.simulation.begin_step());
        drain(&mut battle, &mut mouse);
        if !queued {
            battle
                .simulation
                .cast_power(provider, FRENZY, PowerTarget::Position3D(point))
                .unwrap();
        }
        let markers = marker_ids(&battle);
        assert_eq!(markers.len(), 1);
        assert_eq!(
            battle
                .simulation
                .special_power_triggers
                .iter()
                .filter(|trigger| trigger.object == provider && trigger.power == FRENZY)
                .count(),
            1,
        );
        let dispatched = generals_sim::skirmish::diagnostics::record(
            &battle.simulation,
            &mut std::collections::hash_map::DefaultHasher::new(),
        );
        battle.simulation.finish_step();
        assert!(battle.simulation.entities[markers[0]].exists());
        let remaining = battle.simulation.power_cooldown_remaining(provider, FRENZY);
        assert!(remaining > 0);
        let completed = generals_sim::skirmish::diagnostics::record(
            &battle.simulation,
            &mut std::collections::hash_map::DefaultHasher::new(),
        );
        step(&mut battle, &mut mouse);
        assert!(!battle.simulation.entities[markers[0]].exists());
        assert!(battle.simulation.entities[target].frenzy_tint());
        observed.push((before, dispatched, completed, remaining));
    }
    assert_eq!(observed[0], observed[1]);
}

#[ignore = "requires original game installation"]
#[test]
fn power_and_sell_execute_in_submission_order() {
    for sell_first in [false, true] {
        let (mut battle, mut mouse, provider, target) = installed();
        battle.select_objects(&[provider], true);
        if sell_first {
            dispatch_command(&mut battle, &mut mouse, CommandAction::Sell);
        }
        cast(&mut battle, &mut mouse, provider, target);
        if !sell_first {
            dispatch_command(&mut battle, &mut mouse, CommandAction::Sell);
        }
        assert!(!battle.simulation.entities[provider].selling());
        assert!(marker_ids(&battle).is_empty());
        step(&mut battle, &mut mouse);
        assert!(battle.simulation.entities[provider].selling());
        assert_eq!(marker_ids(&battle).len(), usize::from(!sell_first));
    }
}

#[ignore = "requires original game installation"]
#[test]
fn power_shortcuts_preserve_movement_stop_order_and_the_selected_group() {
    for stop_last in [false, true] {
        let (mut battle, mut mouse, provider, target) = installed();
        let mover = friendly_entity(&battle.simulation, UnitKind::Builder, 0).unwrap();
        battle.select_objects(&[mover], true);
        let destination =
            Vec2::from_array(battle.simulation.entities[mover].position) + Vec2::new(100.0, 0.0);
        if stop_last {
            assert!(input::context_order(
                &mut battle,
                Some(destination),
                None,
                &mut mouse
            ));
        } else {
            dispatch_command(&mut battle, &mut mouse, CommandAction::Stop);
        }
        mouse.pending_command = CommandMode::Power {
            caster: provider,
            name: FRENZY,
            target_object: false,
            specific_source: true,
        };
        let point = Vec2::from_array(battle.simulation.entities[target].position);
        input::targeted_order(&mut battle, &mut mouse, point, None);
        if stop_last {
            dispatch_command(&mut battle, &mut mouse, CommandAction::Stop);
        } else {
            assert!(input::context_order(
                &mut battle,
                Some(destination),
                None,
                &mut mouse
            ));
        }
        step(&mut battle, &mut mouse);
        assert_eq!(marker_ids(&battle).len(), 1);
        assert_eq!(battle.selected, [mover]);
        if stop_last {
            assert!(matches!(
                battle.simulation.entities[mover].ai.order,
                Order::Idle
            ));
        } else {
            assert!(
                matches!(battle.simulation.entities[mover].ai.order, Order::Move(point)
                if point.map(f32::to_bits) == destination.to_array().map(f32::to_bits))
            );
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn paused_requests_wait_and_captured_providers_or_restarted_matches_reject_them() {
    let (mut battle, mut mouse, provider, target) = installed();
    cast(&mut battle, &mut mouse, provider, target);
    let frame = battle.simulation.tick;
    battle.paused = true;
    step(&mut battle, &mut mouse);
    assert_eq!(battle.simulation.tick, frame);
    assert!(marker_ids(&battle).is_empty());
    battle.paused = false;
    step(&mut battle, &mut mouse);
    assert_eq!(marker_ids(&battle).len(), 1);

    let (mut battle, mut mouse, provider, target) = installed();
    cast(&mut battle, &mut mouse, provider, target);
    battle.simulation.entities[provider].owner = 1;
    step(&mut battle, &mut mouse);
    assert!(marker_ids(&battle).is_empty());

    let (mut battle, mut mouse, provider, target) = installed();
    cast(&mut battle, &mut mouse, provider, target);
    battle.restart();
    step(&mut battle, &mut mouse);
    assert!(marker_ids(&battle).is_empty());
    assert!(battle.simulation.special_power_triggers.is_empty());
}

#[ignore = "requires original game installation"]
#[test]
fn spectre_selection_changes_the_next_stop_and_survives_later_additive_selection() {
    for add_builder in [false, true] {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        game.set_match_options(50_000, 1);
        game.reset(Faction::Usa, Faction::China).unwrap();
        crate::progression_verification::earn_combat_rank(game, 5).unwrap();
        game.purchase_science(0, "SCIENCE_SpectreGunshipSolo")
            .unwrap();
        let center = friendly_entity(game, UnitKind::CommandCenter, 0).unwrap();
        let builder = friendly_entity(game, UnitKind::Builder, 0).unwrap();
        let point = Vec2::from_array(game.entities[builder].position);
        let mut mouse = input::load(
            &assets,
            &generals_assets::game_root()
                .unwrap()
                .join("UserData/Options.ini"),
            &game.catalog,
        )
        .unwrap();
        battle.select_objects(&[center], true);
        dispatch_command(&mut battle, &mut mouse, CommandAction::SpectreGunship);
        input::targeted_order(&mut battle, &mut mouse, point, None);
        if add_builder {
            battle.select_objects(&[builder], false);
        } else {
            dispatch_command(&mut battle, &mut mouse, CommandAction::Stop);
        }
        assert!(battle.simulation.begin_step());
        drain(&mut battle, &mut mouse);
        let ship = battle
            .simulation
            .entities
            .iter()
            .position(|actor| {
                actor.owner == 0
                    && battle.simulation.catalog[actor.type_index].name
                        == "AmericaJetSpectreGunship"
            })
            .unwrap();
        if add_builder {
            assert_eq!(battle.simulation.selected_objects(0), [ship, builder]);
        } else {
            assert_eq!(battle.simulation.selected_objects(0), [ship]);
            assert!(matches!(
                battle.simulation.entities[ship].ai.order,
                Order::Idle
            ));
        }
        battle.simulation.finish_step();
        input::apply_spectre_selections(&mut battle);
        assert_eq!(
            battle.logic_selection,
            battle.simulation.selected_objects(0)
        );
        assert_eq!(battle.selected.first(), Some(&ship));
    }
}

#[ignore = "requires original game installation"]
#[test]
fn startup_selects_the_command_center_before_the_first_world_command() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    battle.restart();
    let center = friendly_entity(&battle.simulation, UnitKind::CommandCenter, 0).unwrap();
    assert_eq!(battle.selected, [center]);
    assert_eq!(battle.simulation.selected_objects(0), [center]);
    let mut mouse = input::load(
        &assets,
        &generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini"),
        &battle.simulation.catalog,
    )
    .unwrap();
    dispatch_command(&mut battle, &mut mouse, CommandAction::Sell);
    assert!(!battle.simulation.entities[center].selling());
    step(&mut battle, &mut mouse);
    assert!(battle.simulation.entities[center].selling());
}
