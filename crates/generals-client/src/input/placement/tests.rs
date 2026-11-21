use super::*;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

pub fn verify_sneak_release(
    battle: &mut Match,
    assets: &generals_assets::virtual_assets::VirtualAssets,
    caster: usize,
    kind: usize,
    point: [f32; 2],
    angle: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    let power = crate::p03_gla_capture::Kind::Sneak.power();
    let root = generals_assets::game_root()?;
    let mut mouse = crate::input::load(
        assets,
        &root.join("UserData/Options.ini"),
        &battle.simulation.catalog,
    )?;
    let mut placement = Placement::special_power(kind, caster, angle, power);
    let anchor = Some([IVec2::ZERO, IVec2::ONE]);
    placement.anchor = anchor;
    placement.legal = true;
    let blocked = Vec2::from_array(battle.simulation.entities[caster].position);
    let count = battle.simulation.entities.len();
    assert!(!commit(battle, &mut mouse, &mut placement, blocked));
    assert!(placement.anchor.is_none());
    assert_eq!(battle.simulation.entities.len(), count);
    assert!(battle.simulation.can_use_power(caster, power));
    assert_eq!(mouse.sneak_placement_rejections.len(), 1);
    assert_eq!(
        mouse.sneak_placement_rejections[0].0,
        battle.simulation.tick
    );
    assert!(matches!(
        mouse.sneak_placement_rejections[0].1,
        generals_sim::skirmish::PlacementFailure::ObjectsInTheWay
    ));
    assert!(matches!(
        mouse.voices.as_slice(),
        [request] if matches!(request.voice(), Voice::NoBuild)
    ));
    placement.anchor = anchor;
    assert!(commit(
        battle,
        &mut mouse,
        &mut placement,
        Vec2::from_array(point)
    ));
    crate::input::commands::step(battle, &mut mouse);
    assert!(!battle.simulation.can_use_power(caster, power));
    assert_eq!(mouse.sneak_placement_rejections.len(), 1);
    Ok(())
}

#[test]
fn force_attack_remembers_ctrl_first_but_does_not_start_for_shift_first() {
    let mut mode = ForceAttack::new();
    mode.event(KeyCode::ControlLeft, true, false);
    mode.event(KeyCode::ShiftLeft, true, false);
    assert!(mode.active);
    mode.event(KeyCode::ControlLeft, false, false);
    assert!(!mode.active);
    mode.event(KeyCode::ControlLeft, true, false);
    assert!(
        !mode.active,
        "Shift was still held when Ctrl was pressed again"
    );
    mode.event(KeyCode::ShiftLeft, false, false);
    assert!(!mode.active, "releasing Shift is not a Ctrl-down event");
    mode.event(KeyCode::ControlLeft, true, true);
    assert!(
        !mode.active,
        "autorepeat must not activate a skipped mapping"
    );
    mode.event(KeyCode::ControlLeft, false, false);
    mode.event(KeyCode::ControlLeft, true, false);
    assert!(mode.active);
}

#[test]
fn force_attack_tracks_both_ctrl_keys_alt_release_and_clears_on_match_reset_or_focus_loss() {
    let mut mode = ForceAttack::new();
    mode.event(KeyCode::ControlLeft, true, false);
    mode.event(KeyCode::ControlRight, true, false);
    mode.event(KeyCode::AltRight, true, false);
    mode.event(KeyCode::ControlLeft, false, false);
    assert!(mode.active);
    mode.event(KeyCode::AltRight, false, false);
    assert!(mode.active);
    mode.event(KeyCode::ControlRight, false, false);
    assert!(!mode.active);
    mode.event(KeyCode::AltLeft, true, false);
    mode.event(KeyCode::ControlRight, true, false);
    assert!(!mode.active);
    mode.reset();
    assert!(!mode.active);
    mode.event(KeyCode::ControlRight, true, false);
    assert!(mode.active, "reset must clear the remembered Alt key too");
    mode.reset();
    assert!(!mode.active);
}

fn project(pixel: IVec2) -> Option<Vec2> {
    // A simple test camera: screen down maps to native -Y.
    (pixel.x >= 0 && pixel.y >= 0).then(|| pixel.as_vec2() * Vec2::new(1.0, -1.0))
}

#[test]
fn five_physical_pixels_start_rotation_and_returning_inside_keeps_the_last_endpoint() {
    let mut placement = Placement::new(2, 7, -FRAC_PI_4);
    let start = IVec2::new(100, 100);
    assert!(placement.press(start, false, project(start)));
    placement.motion(start + IVec2::new(2, 4), false);
    placement.update(start, false, 0, project, |_, _| true);
    assert_eq!(placement.angle.to_bits(), (-FRAC_PI_4).to_bits());
    assert!(
        placement
            .orientation_indicator()
            .is_some_and(|(_, _, arrow)| !arrow)
    );
    placement.motion(start + IVec2::new(3, 4), false);
    placement.update(start, false, 1, project, |_, _| true);
    assert_eq!(
        placement.angle.to_bits(),
        facing(Vec2::new(3.0, -4.0)).to_bits()
    );
    assert!(
        placement
            .orientation_indicator()
            .is_some_and(|(_, _, arrow)| arrow)
    );
    placement.motion(start + IVec2::new(1, 1), false);
    placement.update(start, false, 2, project, |_, _| true);
    assert_eq!(
        placement.angle.to_bits(),
        facing(Vec2::new(3.0, -4.0)).to_bits()
    );
    assert_eq!(placement.release(project), project(start));
}

#[test]
fn gui_consumes_unanchored_press_and_motion_but_forwards_anchored_release() {
    let mut placement = Placement::new(2, 7, 0.0);
    let start = IVec2::new(100, 100);
    assert!(placement.press(start, true, project(start)));
    assert!(placement.anchor.is_none());
    assert!(placement.press(start, false, project(start)));
    placement.motion(IVec2::new(110, 120), false);
    placement.motion(IVec2::new(900, 900), true);
    placement.update(IVec2::new(900, 900), false, 3, project, |_, _| true);
    assert_eq!(placement.release(project), project(start));
    assert_eq!(
        placement.angle.to_bits(),
        facing(Vec2::new(10.0, -20.0)).to_bits()
    );
}

#[test]
fn invalid_ground_cancels_press_but_does_not_erase_an_existing_preview_or_anchor() {
    let mut placement = Placement::new(2, 7, FRAC_PI_4);
    assert!(!placement.press(IVec2::new(-1, 10), false, None));
    assert!(placement.anchor.is_none());
    let start = IVec2::new(100, 100);
    placement.update(start, false, 1, project, |_, _| true);
    assert!(placement.press(start, false, project(start)));
    placement.motion(IVec2::new(-10, 100), false);
    placement.update(start, false, 3, project, |_, _| true);
    assert_eq!(placement.angle.to_bits(), FRAC_PI_4.to_bits());
    assert!(placement.release(|_| None).is_none());
    placement.update(start, false, 5, |_| None, |_, _| false);
    assert_eq!(placement.position, project(start));
    assert!(placement.legal);
    assert!(placement.anchor.is_some());
}

#[test]
fn release_uses_displayed_angle_and_legality_uses_odd_logic_frames_without_hold_timeout() {
    let mut placement = Placement::new(2, 7, FRAC_PI_4);
    let start = IVec2::new(100, 100);
    assert!(placement.press(start, false, project(start)));
    placement.update(start, false, 0, project, |_, _| false);
    placement.motion(start + IVec2::new(0, 20), false);
    assert_eq!(placement.release(project), project(start));
    assert_eq!(placement.angle.to_bits(), FRAC_PI_4.to_bits());
    assert!(placement.legal);
    placement.update(start, false, 2, project, |_, _| false);
    assert_eq!(placement.angle.to_bits(), (-FRAC_PI_2).to_bits());
    assert!(placement.legal);
    placement.update(start, false, u64::MAX, project, |_, _| false);
    assert!(!placement.legal);
    assert_eq!(placement.release(project), project(start));
}

#[test]
fn force_attack_snaps_with_original_negative_half_ties() {
    assert_eq!(snap_facing(FRAC_PI_4 * 0.5).to_bits(), FRAC_PI_4.to_bits());
    assert_eq!(snap_facing(-FRAC_PI_4 * 0.5).to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        snap_facing(-FRAC_PI_4 * 1.5).to_bits(),
        (-FRAC_PI_4).to_bits()
    );
    let mut placement = Placement::new(2, 7, 0.0);
    let start = IVec2::new(100, 100);
    assert!(placement.press(start, false, project(start)));
    placement.motion(start + IVec2::new(20, 11), false);
    placement.update(start, true, 1, project, |_, _| true);
    assert_eq!(placement.angle.to_bits(), (-FRAC_PI_4).to_bits());
}

#[test]
fn native_facing_and_bevy_preview_preserve_world_y_without_mirroring() {
    for direction in [
        Vec2::X,
        Vec2::Y,
        Vec2::NEG_X,
        Vec2::NEG_Y,
        Vec2::new(3.0, -4.0),
    ] {
        let angle = facing(direction);
        let visible_forward = Quat::from_rotation_y(angle) * Vec3::X;
        let expected = direction.normalize();
        assert!(visible_forward.distance(Vec3::new(expected.x, 0.0, -expected.y)) < 0.000_001);
    }
}

#[ignore = "requires original game installation"]
#[test]
fn admission_retries_insufficient_cash_but_exits_unavailable_placement() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    battle
        .simulation
        .reset(
            generals_sim::skirmish::Faction::Usa,
            generals_sim::skirmish::Faction::Gla,
        )
        .unwrap();
    let builder = crate::skirmish_verification::friendly_entity(
        &battle.simulation,
        generals_sim::skirmish::Kind::Builder,
        0,
    )
    .unwrap();
    let product = battle
        .simulation
        .type_for(0, generals_sim::skirmish::Kind::PowerPlant)
        .unwrap();
    battle.select_objects(&[builder], true);
    let root = &generals_assets::game_root().unwrap();
    let mut mouse = crate::input::load(
        &assets,
        &root.join("UserData/Options.ini"),
        &battle.simulation.catalog,
    )
    .unwrap();
    let mut placement = Placement::new(product, builder, 0.0);
    placement.anchor = Some([IVec2::ZERO, IVec2::ONE]);
    let selected = battle.logic_selection.clone();
    battle.simulation.players[0].cash = 0;
    assert_eq!(
        battle.simulation.construction_admission(builder, product),
        Err("Not enough money")
    );
    assert!(!commit(&mut battle, &mut mouse, &mut placement, Vec2::ZERO));
    assert!(
        placement.anchor.is_some(),
        "insufficient money retains the anchored placement"
    );
    assert_eq!(battle.logic_selection, selected);
    placement.type_index = battle.simulation.catalog.len();
    mouse.pending_command = crate::input::CommandMode::Guard;
    assert!(
        commit(&mut battle, &mut mouse, &mut placement, Vec2::ZERO),
        "unavailable construction exits placement"
    );
    assert!(matches!(
        mouse.pending_command,
        crate::input::CommandMode::Normal
    ));
    assert_eq!(battle.logic_selection, selected);
}

#[ignore = "requires original game installation"]
#[test]
fn stale_legal_preview_rechecks_location_without_simulation_side_effects() {
    use std::hash::Hasher;
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    battle
        .simulation
        .reset(
            generals_sim::skirmish::Faction::Usa,
            generals_sim::skirmish::Faction::Gla,
        )
        .unwrap();
    let builder = crate::skirmish_verification::friendly_entity(
        &battle.simulation,
        generals_sim::skirmish::Kind::Builder,
        0,
    )
    .unwrap();
    let center = crate::skirmish_verification::friendly_entity(
        &battle.simulation,
        generals_sim::skirmish::Kind::CommandCenter,
        0,
    )
    .unwrap();
    let product = battle
        .simulation
        .type_for(0, generals_sim::skirmish::Kind::PowerPlant)
        .unwrap();
    battle.select_objects(&[builder], true);
    drop(battle.command_message_members());
    let root = &generals_assets::game_root().unwrap();
    let mut mouse = crate::input::load(
        &assets,
        &root.join("UserData/Options.ini"),
        &battle.simulation.catalog,
    )
    .unwrap();
    let point = Vec2::from_array(battle.simulation.entities[center].position);
    let mut placement = Placement::new(product, builder, 0.0);
    placement.anchor = Some([IVec2::ZERO, IVec2::ONE]);
    placement.legal = true; // The earlier preview is stale; the live center blocks this location.
    assert!(
        battle
            .simulation
            .construction_admission(builder, product)
            .is_ok()
    );
    assert!(
        !battle
            .simulation
            .can_build_facing(builder, product, point.to_array(), 0.0)
    );
    let members = battle.logic_selection.clone();
    let entities = battle.simulation.entities.len();
    let cash = battle.simulation.players[0].cash;
    let mut before = std::collections::hash_map::DefaultHasher::new();
    let random = generals_sim::skirmish::diagnostics::record(&battle.simulation, &mut before);
    assert!(!commit(&mut battle, &mut mouse, &mut placement, point));
    let mut after = std::collections::hash_map::DefaultHasher::new();
    assert_eq!(
        generals_sim::skirmish::diagnostics::record(&battle.simulation, &mut after),
        random
    );
    assert_eq!(
        before.finish(),
        after.finish(),
        "no entity displacement/removal or commands"
    );
    assert_eq!(battle.simulation.entities.len(), entities);
    assert_eq!(battle.simulation.players[0].cash, cash);
    assert_eq!(battle.logic_selection, members);
    assert!(placement.anchor.is_none());
}

#[test]
fn zero_hour_facing_divides_before_acos_and_keeps_source_near_axis_rounding() {
    // Binary32 x/length is 0x3f7fffe9 here; x*(1/length) is 0x3f7fffea.
    let expected = f32::from_bits(0x3f7f_ffe9).acos();
    assert_eq!(facing(Vec2::new(3.0, 0.005)).to_bits(), expected.to_bits());
    assert_eq!(
        facing(Vec2::new(3.0, -0.005)).to_bits(),
        (-expected).to_bits()
    );
    // Native acos rounds the near-axis cosine to one. A normalized drag
    // retains a nonzero Y component and is not the source facing contract.
    assert_eq!(facing(Vec2::new(80.0, 0.005)).to_bits(), 0.0_f32.to_bits());
}
