use super::*;

#[test]
fn native_delay_and_both_transition_endpoints_precede_the_reverse_leg() {
    let rules = Rules {
        delay: 2,
        initial: [10.0, 1.0, 0.0],
        final_size: [10.0, 90.0, 0.0],
        transition: 2,
        reverse: true,
    };
    let mut state = State::new(&rules);
    assert!(state.update(&rules).is_none());
    assert!(!state.started());
    for expected in [1.0_f32, 45.5, 90.0, 90.0, 45.5, 1.0] {
        let actual = state.update(&rules).unwrap();
        assert_eq!(actual[1].to_bits(), expected.to_bits());
    }
    assert!(state.started());
    assert!(state.reversed());
    assert!(state.update(&rules).is_none());
}

#[test]
fn zero_initial_delay_still_updates_once_before_changing_the_authored_size() {
    let rules = Rules {
        delay: 0,
        initial: [1.0, 2.0, 3.0],
        final_size: [7.0, 8.0, 9.0],
        transition: 1,
        reverse: false,
    };
    let mut state = State::new(&rules);
    assert_eq!(
        state.update(&rules).unwrap().map(f32::to_bits),
        rules.initial.map(f32::to_bits)
    );
    assert_eq!(
        state.update(&rules).unwrap().map(f32::to_bits),
        rules.final_size.map(f32::to_bits)
    );
    assert!(state.update(&rules).is_none());
    assert!(!state.reversed());
}
