use super::*;

fn rules() -> Rules {
    Rules {
        frames: 90,
        count: 3,
        radius: 20.0,
        weapon: "FirestormSmallCreationWeapon".into(),
    }
}

#[test]
fn fixed_firestorm_consumes_only_matching_history_and_not_the_triggering_hit() {
    let rules = rules();
    let mut history = History::new();
    assert!(!history.impact(&rules, 100, [0.0, 0.0]));
    assert!(!history.impact(&rules, 101, [200.0, 0.0]));
    assert!(!history.impact(&rules, 102, [0.0, 0.0]));
    assert!(history.impact(&rules, 103, [20.0, 0.0]));
    assert_eq!(history.hits.len(), 1);
    assert!(!history.impact(&rules, 104, [200.0, 0.0]));
    assert!(history.impact(&rules, 105, [200.0, 0.0]));
    assert!(history.hits.is_empty());
}

#[test]
fn expiration_is_inclusive_and_failed_partial_matches_remain_available() {
    let rules = rules();
    let mut history = History::new();
    assert!(!history.impact(&rules, 100, [0.0, 0.0]));
    assert!(!history.impact(&rules, 101, [100.0, 0.0]));
    assert!(!history.impact(&rules, 102, [0.0, 0.0]));
    assert!(!history.impact(&rules, 190, [0.0, 0.0]));
    assert!(history.impact(&rules, 191, [0.0, 0.0]));
    assert!(history.hits.is_empty());
}

#[test]
fn source_unsigned_expiration_underflow_and_radius_boundary_are_preserved() {
    let rules = rules();
    let mut history = History::new();
    for frame in 0..3 {
        assert!(!history.impact(&rules, frame, [0.0, 0.0]));
    }
    assert_eq!(history.hits.len(), 1);
    let mut history = History::new();
    assert!(!history.impact(&rules, 100, [0.0, 0.0]));
    assert!(!history.impact(&rules, 101, [0.0, 0.0]));
    assert!(!history.impact(&rules, 102, [f32::from_bits(20.0_f32.to_bits() + 1), 0.0]));
    assert!(history.impact(&rules, 103, [20.0, 0.0]));
    assert_eq!(history.hits.len(), 1);
}
