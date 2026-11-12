use super::*;

struct PlayerIdentity {
    playable_side: bool,
    color: u32,
    side: Option<String>,
}

fn rules() -> Rules {
    Rules {
        min_delay: 30,
        max_delay: 30,
        ocl: None,
        faction_ocl: vec![
            FactionOcl {
                faction: "America".into(),
                ocl: "first".into(),
            },
            FactionOcl {
                faction: "America".into(),
                ocl: "duplicate".into(),
            },
            FactionOcl {
                faction: "GLADemolitionGeneral".into(),
                ocl: "two_bikes".into(),
            },
        ],
        create_at_edge: true,
        faction_triggered: true,
    }
}
fn player(color: u32, side: &str) -> PlayerIdentity {
    PlayerIdentity {
        playable_side: true,
        color,
        side: Some(side.into()),
    }
}
fn facts(frame: u32, player: Option<&PlayerIdentity>) -> Facts<'_> {
    Facts {
        frame,
        playable_color: player
            .filter(|player| player.playable_side)
            .map(|player| player.color),
        faction: player.and_then(|player| player.side.as_deref()),
        disabled: false,
        under_construction: false,
    }
}

#[test]
fn capture_uses_source_color_and_exact_side_and_fixed_delay_preserves_rng() {
    let rules = rules();
    let mut state = State::new();
    let mut random = [1, 2, 3, 4, 5, 6];
    let original = random;
    let usa = player(0xff12_3456, "America");
    let demo = player(usa.color, "GLADemolitionGeneral");
    assert_eq!(state.update(&rules, &facts(1, None), &mut random), None);
    assert_eq!(
        state.update(&rules, &facts(10, Some(&usa)), &mut random),
        None
    );
    // A different owner with the same actual PlayerColor does not reset the timer.
    assert_eq!(
        state.update(&rules, &facts(39, Some(&demo)), &mut random),
        None
    );
    assert_eq!(
        state.update(&rules, &facts(40, Some(&demo)), &mut random),
        Some("two_bikes")
    );
    assert_eq!(
        state.update(&rules, &facts(70, Some(&usa)), &mut random),
        Some("first")
    );
    assert_eq!(random, original);
}

#[test]
fn recapture_and_neutral_return_reset_but_disabled_shift_precedes_owner_test() {
    let rules = rules();
    let mut state = State::new();
    let mut random = [1, 2, 3, 4, 5, 6];
    let first = player(0xff00_ffff, "America");
    let second = player(0xffff_0000, "America");
    assert_eq!(
        state.update(&rules, &facts(10, Some(&first)), &mut random),
        None
    );
    assert_eq!(
        state.update(
            &rules,
            &Facts {
                disabled: true,
                ..facts(20, Some(&second))
            },
            &mut random
        ),
        None
    );
    assert_eq!(state.player_color, first.color);
    assert_eq!(state.next_creation, 41);
    assert_eq!(state.timer_started, 11);
    assert_eq!(
        state.update(&rules, &facts(21, Some(&second)), &mut random),
        None
    );
    assert_eq!(state.next_creation, 51);
    assert_eq!(state.update(&rules, &facts(50, None), &mut random), None);
    assert!(state.faction_neutral);
    assert_eq!(
        state.update(&rules, &facts(51, Some(&second)), &mut random),
        None
    );
    assert_eq!(
        state.update(&rules, &facts(80, Some(&second)), &mut random),
        None
    );
    assert_eq!(
        state.update(&rules, &facts(81, Some(&second)), &mut random),
        Some("first")
    );
}

#[test]
fn subdued_unbuilt_nonfaction_timer_is_initialized_only_after_construction() {
    let mut rules = rules();
    rules.faction_triggered = false;
    rules.ocl = Some("supply".into());
    let mut state = State::new();
    let mut random = [1, 2, 3, 4, 5, 6];
    assert_eq!(
        state.update(
            &rules,
            &Facts {
                disabled: true,
                under_construction: true,
                ..facts(10, None)
            },
            &mut random
        ),
        None
    );
    assert_eq!(state.timer_started, 0);
    assert_eq!(state.next_creation, 0);
    assert_eq!(
        state.update(
            &rules,
            &Facts {
                disabled: true,
                ..facts(20, None)
            },
            &mut random
        ),
        None
    );
    assert_eq!(state.timer_started, 20);
    assert_eq!(state.next_creation, 50);
    assert_eq!(
        state.update(
            &rules,
            &Facts {
                disabled: true,
                ..facts(21, None)
            },
            &mut random
        ),
        None
    );
    assert_eq!(state.timer_started, 21);
    assert_eq!(state.next_creation, 51);
    assert_eq!(state.update(&rules, &facts(50, None), &mut random), None);
    assert_eq!(
        state.update(&rules, &facts(51, None), &mut random),
        Some("supply")
    );
}

#[test]
fn missing_side_and_case_mismatch_still_reset_before_recipient_lookup() {
    let rules = rules();
    let mut state = State::new();
    let mut random = [1, 2, 3, 4, 5, 6];
    let mut owner = player(7, "America");
    assert_eq!(
        state.update(&rules, &facts(1, Some(&owner)), &mut random),
        None
    );
    owner.side = None;
    assert_eq!(
        state.update(&rules, &facts(31, Some(&owner)), &mut random),
        None
    );
    assert_eq!(state.next_creation, 61);
    owner.side = Some("america".into());
    assert_eq!(
        state.update(&rules, &facts(61, Some(&owner)), &mut random),
        None
    );
    assert_eq!(state.next_creation, 91);
    owner.side = Some("America".into());
    assert_eq!(
        state.update(&rules, &facts(90, Some(&owner)), &mut random),
        None
    );
    assert_eq!(
        state.update(&rules, &facts(91, Some(&owner)), &mut random),
        Some("first")
    );
}

#[test]
fn native_unsigned_timer_wrap_and_inverted_rng_bounds_are_retained() {
    let mut rules = rules();
    rules.min_delay = 9;
    rules.max_delay = 3;
    rules.faction_triggered = false;
    rules.ocl = Some("list".into());
    let mut state = State::new();
    let mut random = [1, 2, 3, 4, 5, 6];
    let original = random;
    state.reset(&rules, u32::MAX - 1, &mut random);
    assert_eq!(state.next_creation, 1);
    assert_eq!(random, original);
    // Source uses plain now<deadline, not an invented wrapping-time comparison.
    assert_eq!(
        state.update(&rules, &facts(u32::MAX, None), &mut random),
        Some("list")
    );
    assert_eq!(state.next_creation, 2);
    assert_eq!(
        state.update(
            &rules,
            &Facts {
                disabled: true,
                ..facts(0, None)
            },
            &mut random
        ),
        None
    );
    assert_eq!(state.timer_started, 0);
    assert_eq!(state.next_creation, 3);
}
