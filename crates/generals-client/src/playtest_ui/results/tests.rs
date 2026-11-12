use super::{ResultAction, ResultsScreen};

fn screen() -> ResultsScreen {
    ResultsScreen {
        generation: 1,
        elapsed: None,
        message: None,
        local_defeat_shown: false,
        close_at: None,
        rows: vec![],
    }
}

#[test]
fn local_defeat_closes_without_scores_and_later_outcome_uses_observer_art() {
    let mut screen = screen();
    screen.show(ResultAction::LocalDefeat, 200);
    screen.close_expired(319);
    assert!(screen.local_defeat());
    assert!(!screen.scores());
    screen.close_expired(320);
    assert!(!screen.local_defeat());
    assert!(!screen.scores());
    screen.show(ResultAction::Victory, 700);
    screen.elapsed = Some(0);
    assert!(screen.observer_quit());
    assert!(!screen.banner(true));
    screen.close_expired(800);
    assert!(screen.observer_quit());
    screen.elapsed = Some(120);
    assert!(screen.scores());
    assert!(!screen.observer_quit());
}

#[test]
fn native_close_window_timer_can_close_replacement_message_before_end_timer() {
    let mut screen = screen();
    screen.show(ResultAction::LocalDefeat, 200);
    screen.close_expired(240);
    screen.show(ResultAction::Defeat, 240);
    screen.elapsed = Some(0);
    assert!(screen.observer_quit());
    screen.close_expired(319);
    assert!(screen.observer_quit());
    screen.close_expired(320);
    assert!(!screen.observer_quit());
    assert!(!screen.scores());
    // ScriptEngine expires the close timer before evaluating a new action.
    let mut simultaneous = super::tests::screen();
    simultaneous.show(ResultAction::LocalDefeat, 200);
    simultaneous.close_expired(320);
    simultaneous.show(ResultAction::Defeat, 320);
    simultaneous.elapsed = Some(0);
    simultaneous.close_expired(321);
    assert!(simultaneous.observer_quit());
}

#[test]
fn direct_terminal_outcome_uses_original_side_banner() {
    for action in [ResultAction::Victory, ResultAction::Defeat] {
        let mut screen = screen();
        screen.show(action, 400);
        screen.elapsed = Some(0);
        let victory = matches!(action, ResultAction::Victory);
        assert!(screen.banner(victory));
        assert!(!screen.banner(!victory));
        assert!(!screen.observer_quit());
        screen.elapsed = Some(119);
        assert!(screen.banner(victory));
        screen.elapsed = Some(120);
        assert!(!screen.banner(victory));
    }
}
