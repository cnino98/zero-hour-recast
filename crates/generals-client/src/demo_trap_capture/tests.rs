use super::*;
use crate::{
    control_bar,
    playtest_ui::{dispatch_command, selected_command},
};

#[ignore = "requires original game installation"]
#[test]
fn paid_demo_trap_native_commands_preserve_the_warning_and_final_weapon_callbacks() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut mouse = crate::input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    for proximity in [false, true] {
        let setup = demo_trap_verification::prepare(&mut battle.simulation).unwrap();
        battle.select_objects(&[setup.trap], true);
        let mut check = Check::new(&battle.simulation, setup, proximity);
        for (button, slot) in [(1, 1), (2, 2), (5, 0)] {
            assert!(
                matches!(selected_command(&battle, &data, button).unwrap().action,
                crate::control_bar::CommandAction::SwitchWeapon(index) if index == slot)
            );
        }
        let manual = selected_command(&battle, &data, 2).unwrap().action;
        dispatch_command(&mut battle, &mut mouse, manual);
        for _ in 0..5 {
            crate::input::commands::step(&mut battle, &mut mouse);
            assert!(!check.observe(&battle.simulation).unwrap());
            assert!(battle.simulation.entities[check.setup.trap].alive());
        }
        let command = selected_command(&battle, &data, if proximity { 1 } else { 5 })
            .unwrap()
            .action;
        dispatch_command(&mut battle, &mut mouse, command);
        for _ in 0..60 {
            crate::input::commands::step(&mut battle, &mut mouse);
            check.observe(&battle.simulation).unwrap();
            if check.explosion.is_some() {
                break;
            }
        }
        let warning = check.warning.expect("real source initial FX callback");
        assert_eq!(check.explosion.unwrap().0 - warning, 30);
        assert!(!battle.simulation.entities[check.setup.trap].corpse_present());
    }
}
