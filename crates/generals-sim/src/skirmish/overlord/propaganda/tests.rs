use super::Propaganda;
use crate::skirmish::{
    Faction, Kind, Skirmish,
    tests::{fixture_type, game},
};

fn setup() -> (Skirmish, usize, usize) {
    let mut game = game();
    let mut unit = fixture_type(Faction::China, Kind::Defense);
    unit.name = "ChinaSpeakerTower".into();
    unit.kind_of = vec!["STRUCTURE".into(), "POWERED".into()];
    unit.behavior.propaganda = Some(Propaganda {
        radius: 150.0,
        delay: 60,
        heal: [0.02, 0.04],
        upgrade: "Subliminal".into(),
        pulse: ["Pulse".into(), "SubliminalPulse".into()],
        affects_self: false,
    });
    crate::skirmish::tests::register_update(&mut unit, "PropagandaTowerBehavior");
    game.catalog.push(unit);
    let tower = game.spawn(game.catalog.len() - 1, 0, [355.0, 255.0], true);
    let tank = game.spawn(
        game.type_for(0, Kind::Tank).unwrap(),
        0,
        [395.0, 255.0],
        true,
    );
    game.entities[tank].health *= 0.5;
    game.update_ground_contacts();
    game.tick = 60;
    game.players[0].power = 1;
    game.run_object_updates();
    assert_ne!(game.entities[tank].attachment.bonuses, 0);
    (game, tower, tank)
}

#[test]
fn disabling_propaganda_releases_latched_benefits_and_restoration_keeps_native_scan_deadline() {
    for disabled in 0..3 {
        let (mut game, tower, tank) = setup();
        let before = game.entities[tank].health;
        game.tick = 61;
        match disabled {
            0 => {
                game.players[0].energy.sabotaged_until = 100;
                game.sabotage_power(0);
            }
            1 => game.set_hacked_until(tower, 100),
            _ => game.entities[tower].battle_bonuses.paralyzed_until = 100,
        }
        game.run_object_updates();
        assert_eq!(game.entities[tank].health.to_bits(), before.to_bits());
        assert_eq!(game.entities[tank].attachment.bonuses, 0);
        assert!(game.entities[tower].attachment.influence.is_empty());

        game.clear_hacked_disable(tower);
        game.entities[tower].battle_bonuses.paralyzed_until = 0;
        game.tick = 119;
        game.update_power();
        game.run_object_updates();
        assert_eq!(game.entities[tank].health.to_bits(), before.to_bits());
        game.tick = 120;
        game.run_object_updates();
        let healed = 0.02 / 30.0 * game.entities[tank].max_health;
        assert_eq!(
            game.entities[tank].health.to_bits(),
            (before + healed).to_bits()
        );
        assert_ne!(game.entities[tank].attachment.bonuses, 0);
    }
}

#[test]
fn selling_a_tower_releases_influence_before_its_destruction_animation_finishes() {
    let (mut game, tower, tank) = setup();
    let before = game.entities[tank].health;
    game.sell(0, tower).unwrap();
    assert!(game.entities[tower].alive());
    // Selling sets SOLD; the already-run update resumes on the next frame.
    game.tick += 1;
    game.run_object_updates();
    assert!(game.entities[tower].attachment.influence.is_empty());
    assert_eq!(game.entities[tank].attachment.bonuses, 0);
    assert_eq!(game.entities[tank].health.to_bits(), before.to_bits());
}

#[test]
fn sole_benefactor_changes_only_after_its_expiration_frame() {
    let (mut game, tower, tank) = setup();
    let kind = game.entities[tower].type_index;
    let second = game.spawn(kind, 0, game.entities[tower].position, true);
    let before = game.entities[tank].health;
    let expires = game.entities[tank].maintenance.healing_benefactor_until;
    assert_eq!(
        game.entities[tank].maintenance.healing_benefactor,
        Some(tower)
    );
    assert_eq!(expires, 120);
    game.set_hacked_until(tower, 200);
    game.tick = 61;
    game.run_object_updates();
    assert_eq!(game.entities[tank].health.to_bits(), before.to_bits());
    game.tick = expires;
    game.run_object_updates();
    assert_eq!(game.entities[tank].health.to_bits(), before.to_bits());
    assert_eq!(
        game.entities[tank].maintenance.healing_benefactor,
        Some(tower)
    );
    game.tick += 1;
    game.run_object_updates();
    let healed = 0.02 / 30.0 * game.entities[tank].max_health;
    assert_eq!(
        game.entities[tank].health.to_bits(),
        (before + healed).to_bits()
    );
    assert_eq!(
        game.entities[tank].maintenance.healing_benefactor,
        Some(second)
    );
}
