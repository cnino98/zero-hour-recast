//! Render diagnostics issue ordinary commands through the shared input dispatcher.
use crate::{
    input::{self, Mouse},
    playtest::Match,
    playtest_view::Capture,
};
use bevy::{app::AppExit, prelude::*};
use generals_sim::skirmish::Order;

#[derive(Resource)]
pub struct FeedbackCapture {
    pub waypoint: bool,
    pub issued: bool,
}

pub fn update(
    mut scenario: ResMut<FeedbackCapture>,
    capture: Res<Capture>,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut exit: MessageWriter<AppExit>,
) {
    let capture = capture.into_inner();
    // Exercise the source waypoint display mode independently from OS input.
    // This diagnostic does not assert a physically held modifier was delivered.
    mouse.waypoint_mode = scenario.waypoint;
    if scenario.issued || !capture.ready_for_feedback() {
        return;
    }
    scenario.issued = true;
    let Some(&id) = battle.selected.first() else {
        error!("Feedback capture has no selected source builder");
        exit.write(AppExit::error());
        return;
    };
    let origin = Vec2::from_array(battle.simulation.entities[id].position);
    for offset in [
        Vec2::new(-140.0, 80.0),
        Vec2::new(140.0, 80.0),
        Vec2::new(0.0, 140.0),
    ] {
        input::context_order(&mut battle, Some(origin + offset), None, &mut mouse);
        if matches!(
            battle.simulation.entities[id].ai.order,
            Order::Move(_) | Order::FollowPath
        ) {
            eprintln!(
                "Feedback capture: source {} command and move hint issued",
                if scenario.waypoint {
                    "waypoint"
                } else {
                    "move"
                }
            );
            return;
        }
    }
    error!("Feedback capture could not issue a reachable movement order");
    exit.write(AppExit::error());
}
