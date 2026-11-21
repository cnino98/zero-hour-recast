use crate::{
    cleanup_verification, control_bar, disarm_verification, input, playtest_ui::selected_command,
    source_audio::Voice,
};
use generals_sim::skirmish::Order;

#[ignore = "requires original game installation"]
#[test]
fn paid_dozer_original_controls_context_voice_and_full_disarm_timing() {
    let (assets, mut battle) = cleanup_verification::tests::installed();
    let setup = disarm_verification::prepare(&mut battle.simulation).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    battle.select_objects(&[setup.builder], true);
    assert_eq!(
        selected_command(&battle, &data, 1).unwrap().name,
        "Command_ConstructAmericaPowerPlant"
    );
    let kind = battle.simulation.entities[setup.builder].type_index;
    assert!(!data.units[kind].portrait.is_empty());
    mouse.hovered = Some(setup.trap);
    assert_eq!(crate::attack_cursor::context(&mut battle, &mouse), Some(0));
    assert!(input::context_order(
        &mut battle,
        Some([setup.position[0], setup.position[1]].into()),
        Some(input::PickTarget::Unit(setup.trap)),
        &mut mouse
    ));
    assert!(
        mouse
            .voices
            .last()
            .is_some_and(|voice| matches!(voice.voice(), Voice::Disarm))
    );
    // PostUpdate playback may consume the client response before the next logic frame.
    mouse.voices.clear();
    input::commands::step(&mut battle, &mut mouse);
    assert!(
        matches!(battle.simulation.entities[setup.builder].ai.order, Order::Attack(id) if id == setup.trap)
    );
    assert!(
        mouse.voices.is_empty(),
        "logic dispatch must not repeat the client voice"
    );
    let mut trace = disarm_verification::Trace::new();
    trace.record(&battle.simulation, &setup).unwrap();
    for _ in 0..1800 {
        battle.simulation.step();
        trace.record(&battle.simulation, &setup).unwrap();
        if trace.reloaded {
            break;
        }
    }
    assert!(
        trace.reloaded,
        "original conditional disarm did not finish its exact reload"
    );
    disarm_verification::verify_removed(&battle.simulation, &setup).unwrap();
    area_voice_requires_retirement_and_a_committed_click(setup, trace);
}

fn area_voice_requires_retirement_and_a_committed_click(
    setup: disarm_verification::Setup,
    trace: disarm_verification::Trace,
) {
    let mut check = super::Check {
        setup,
        trace,
        stage: super::Stage::AreaMode,
        began: None,
        pointer: None,
        animation: None,
        animated: false,
        audio: [true, true],
        prior_voice_retired: false,
        failure: None,
        area: super::area::Progress::new(),
    };
    // A previously advanced sink still counts as present even when empty and
    // waiting for the audio system's normal deferred despawn.
    check.observe_voice(true, true);
    assert!(!check.prior_voice_retired);
    check.observe_voice(false, false);
    assert!(check.prior_voice_retired);
    check.audio[0] = false;
    for stage in [super::Stage::AreaPress, super::Stage::AreaRelease] {
        check.stage = stage;
        check.observe_voice(true, true);
        assert!(!check.audio[0], "no voice before the new click commits");
    }
    check.stage = super::Stage::AreaOrder;
    check.observe_voice(true, false);
    assert!(!check.audio[0], "the fresh stream must advance");
    check.stage = super::Stage::AreaRun;
    check.observe_voice(true, true);
    assert!(check.audio[0]);
}
