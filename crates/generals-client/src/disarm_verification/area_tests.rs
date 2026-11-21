use super::*;
use crate::{cleanup_verification, control_bar, input, playtest_ui, source_audio::Voice};

fn controls(
    assets: &generals_assets::virtual_assets::VirtualAssets,
    battle: &crate::playtest::Match,
) -> (control_bar::UiData, input::Mouse) {
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    (
        control_bar::load(assets, &battle.simulation.catalog, &options).unwrap(),
        input::load(assets, &options, &battle.simulation.catalog).unwrap(),
    )
}

fn click(
    battle: &mut crate::playtest::Match,
    data: &control_bar::UiData,
    mouse: &mut input::Mouse,
    point: [f32; 2],
) {
    let command = playtest_ui::selected_command(battle, data, 14).unwrap();
    assert_eq!(command.name, "Command_DisarmMinesAtPosition");
    assert!(matches!(
        command.action,
        control_bar::CommandAction::ClearMines(0)
    ));
    let voices = mouse.voices.len();
    playtest_ui::dispatch_command(battle, mouse, command.action);
    assert_eq!(
        mouse.voices.len(),
        voices,
        "authored mode has no UnitSpecificSound"
    );
    assert!(matches!(
        mouse.pending_command,
        input::CommandMode::ClearMines(0)
    ));
    assert_eq!(crate::attack_cursor::context(battle, mouse), Some(5));
    input::targeted_order(battle, mouse, point.into(), None);
    assert!(matches!(mouse.pending_command, input::CommandMode::Normal));
    input::commands::step(battle, mouse);
    assert!(matches!(
        mouse.voices.last().unwrap().voice(),
        Voice::Disarm
    ));
    assert!(mouse.move_hints.is_empty());
}

#[ignore = "requires original game installation"]
#[test]
fn original_slot14_adjacent_empty_ground_digs_and_reloads_without_firefx_or_missile() {
    let (assets, mut battle) = cleanup_verification::tests::installed();
    battle.simulation.reset(Faction::Usa, Faction::Gla).unwrap();
    let builder = friendly_entity(&battle.simulation, Kind::Builder, 0).unwrap();
    battle.simulation.step();
    battle.simulation.step();
    let (data, mut mouse) = controls(&assets, &battle);
    battle.select_objects(&[builder], true);
    let point = battle.simulation.entities[builder].position;
    let order_frame = battle.simulation.tick;
    click(&mut battle, &data, &mut mouse, [point[0] + 2.0, point[1]]);
    let mut start = None;
    let mut spent = None;
    for _ in 0..240 {
        let game = &mut battle.simulation;
        let actor = &game.entities[builder];
        if let Some((frame, duration, _)) = actor.animation_duration() {
            // Command admission and the FINAL helper share this logic frame.
            assert_eq!((frame, duration), (order_frame + 1, 36));
            start.get_or_insert(frame);
        }
        if spent.is_none() && actor.weapon_rounds_remaining(0) == 0 {
            spent = Some(game.tick);
            assert_eq!(game.tick, start.unwrap() + 36);
            assert_eq!(game.tick, order_frame + 37);
            assert_eq!(actor.weapon_condition(), Some("RELOADING_A"));
        }
        if let Some(frame) = spent {
            assert_eq!(
                actor.weapon_rounds_remaining(0),
                u32::from(game.tick >= frame + 120)
            );
            assert_eq!(
                game.mine_clearing_ready(builder, 0),
                game.tick >= frame + 122
            );
            if game.tick == frame + 60 {
                assert_eq!(
                    game.weapon_reload_progress(builder, 0).unwrap().to_bits(),
                    0.5_f32.to_bits()
                );
            }
        }
        assert!(
            game.presentation
                .shots
                .iter()
                .all(|shot| shot.source != builder)
        );
        assert!(
            game.projectiles
                .iter()
                .all(|projectile| projectile.source != builder)
        );
        assert!(game.hazards.is_empty());
        battle.simulation.step();
    }
    assert!(spent.is_some());
    assert!(matches!(
        battle.simulation.entities[builder].ai.order,
        Order::Idle
    ));
}

#[ignore = "requires original game installation"]
#[test]
fn paid_hidden_demo_trap_is_found_by_original_area_click_without_global_revelation() {
    let (assets, mut battle) = cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    game.reset(Faction::Usa, Faction::Gla).unwrap();
    let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
    let trap = build_trap(game).unwrap();
    let center = game.entities[trap].position;
    let origin = game.entities[builder].position;
    let distance = (origin[0] - center[0]).hypot(origin[1] - center[1]);
    let direction = [
        (origin[0] - center[0]) / distance,
        (origin[1] - center[1]) / distance,
    ];
    move_near(game, builder, point(center, direction, 60.0)).unwrap();
    assert!(game.entities[trap].stealthed() && !game.entities[trap].detected());
    assert!(!game.weapon_can_target(builder, trap));
    let losses = game.players[1].statistics.units_lost;
    let kills = game.players[0].statistics.units_destroyed;
    let experience = game.entities[builder].experience;
    let (data, mut mouse) = controls(&assets, &battle);
    battle.select_objects(&[builder], true);
    click(&mut battle, &data, &mut mouse, [center[0] + 5.0, center[1]]);
    assert!(
        matches!(battle.simulation.entities[builder].ai.order, Order::Attack(id) if id == trap)
    );
    assert!(!battle.simulation.entities[trap].detected());
    let mut shot = None;
    for _ in 0..1800 {
        let game = &battle.simulation;
        if let Some(event) = game
            .presentation
            .shots
            .iter()
            .find(|shot| shot.source == builder)
        {
            assert!(shot.replace(game.tick).is_none());
            assert_eq!(event.target, Some(trap));
            assert_eq!(
                event.source_position.map(f32::to_bits),
                game.object_position(trap).map(f32::to_bits)
            );
            // The destroyed target ends AIAttackState before the FINAL drawable
            // update. The reload clock keeps running after its attack pose clears.
            assert!(game.entities[trap].masked());
            assert_eq!(game.entities[builder].weapon_condition(), None);
            assert_eq!(game.entities[builder].weapon_rounds_remaining(0), 0);
        }
        if let Some(frame) = shot {
            assert_eq!(
                game.entities[builder].weapon_rounds_remaining(0),
                u32::from(game.tick >= frame + 120)
            );
            if game.tick >= frame + 122 {
                break;
            }
        }
        battle.simulation.step();
    }
    let game = &mut battle.simulation;
    assert!(shot.is_some() && game.entities[trap].masked());
    assert!(game.mine_clearing_ready(builder, 0));
    assert!(!game.entities[trap].corpse_present());
    assert_eq!(game.players[1].statistics.units_lost, losses);
    assert_eq!(game.players[0].statistics.units_destroyed, kills);
    assert_eq!(game.entities[builder].experience, experience);
}
