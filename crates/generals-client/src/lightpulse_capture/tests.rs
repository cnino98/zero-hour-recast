use super::setup;

#[ignore = "requires original game installation"]
#[test]
fn paid_tank_damage_creates_original_fifteen_display_frame_light() {
    let (assets, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let setup = setup::prepare(&mut battle.simulation).unwrap();
    let mut particles = crate::particles::load(&assets, &battle.simulation.catalog).unwrap();
    let root = &generals_assets::game_root().unwrap();
    let mut audio = crate::source_audio::load(
        &assets,
        &battle.simulation.catalog,
        &root.join("UserData/Options.ini"),
    )
    .unwrap();
    battle
        .simulation
        .force_attack(setup.attacker, setup.target)
        .unwrap();
    let mut observed = None;
    for _ in 0..1800 {
        battle.simulation.step();
        if let Some((index, event)) = battle
            .simulation
            .presentation
            .object_fx_events
            .iter()
            .enumerate()
            .find(|(_, event)| {
                event.object == setup.target && event.effect == "FX_DamageTankStruck"
            })
        {
            observed = Some(event.position);
            particles.actor_event(&battle, index, None, &mut audio, &mut animations);
            break;
        }
    }
    let position = observed.expect("actual paid tank damage callback");
    let game = &mut battle.simulation;
    assert!(game.entities[setup.target].alive());
    assert!(game.entities[setup.target].health < setup.health);
    assert_eq!(
        game.entities[setup.target].last_attacker(),
        Some(setup.attacker)
    );
    game.stop(setup.attacker).unwrap();
    game.stop(setup.target).unwrap();
    let frozen_tick = game.tick;
    assert_eq!(particles.pulses.lights().len(), 1);
    let light = &particles.pulses.lights()[0];
    assert_eq!(light.position.map(f32::to_bits), position.map(f32::to_bits));
    assert_eq!(
        light.color.map(f32::to_bits),
        [1.0_f32, 1.0, 128.0 / 255.0].map(f32::to_bits)
    );
    assert_eq!(light.outer_radius.to_bits(), 31.0_f32.to_bits());
    for frame in 1..=15_u16 {
        particles.pulses.advance_frame();
        let light = &particles.pulses.lights()[0];
        assert_eq!(light.enabled, frame < 15);
        if frame < 15 {
            let factor = f32::from(15 - frame) / 15.0;
            assert_eq!(light.color[0].to_bits(), factor.to_bits());
            assert_eq!(
                light.outer_radius.to_bits(),
                (31.0 * factor).max(1.0).to_bits()
            );
        }
        assert_eq!(
            battle.simulation.tick, frozen_tick,
            "display-frame light fade must not depend on a simulation step"
        );
    }
}
