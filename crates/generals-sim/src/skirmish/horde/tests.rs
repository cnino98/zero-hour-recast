use super::*;
use crate::skirmish::{
    Faction, Kind,
    tests::{fixture_type, game},
};

fn rules(infantry: bool) -> Rules {
    Rules {
        update_frames: 30,
        count: 5,
        radius: if infantry { 30.0 } else { 75.0 },
        rub_off_radius: if infantry { 60.0 } else { 150.0 },
        kind_of: vec![if infantry { "INFANTRY" } else { "VEHICLE" }.into()],
        allies_only: true,
        exact_match: !infantry,
        allowed_nationalism: true,
        action: Action::Classic,
    }
}

fn formation(infantry: bool) -> (Skirmish, Vec<usize>) {
    let mut game = game();
    let kind = if infantry { Kind::Infantry } else { Kind::Tank };
    let mut unit = fixture_type(Faction::China, kind);
    unit.behavior.horde = Some(rules(infantry));
    crate::skirmish::tests::register_update(&mut unit, "HordeUpdate");
    for (flag, rate) in [(1, 1.5), (4, 1.25), (23, 1.25)] {
        unit.weapon.bonuses.global[flag][3] = rate;
    }
    let index = game.catalog.len();
    game.catalog.push(unit);
    let ids = [
        [300.0, 300.0],
        [310.0, 300.0],
        [300.0, 310.0],
        [310.0, 310.0],
        [320.0, 300.0],
    ]
    .map(|position| game.spawn(index, 0, position, true))
    .to_vec();
    game.update_ground_contacts();
    (game, ids)
}

#[test]
fn only_living_partitioned_horde_capable_allies_count_and_self_is_excluded() {
    let (mut game, ids) = formation(true);
    game.refresh_horde(ids[0]);
    assert!(game.entities[ids[0]].true_horde_member());
    game.entities[ids[4]].health = 0.0;
    game.refresh_horde(ids[0]);
    assert!(!game.entities[ids[0]].in_horde());
    let ordinary = game.spawn(5, 0, [300.0, 320.0], true);
    game.update_ground_contacts();
    game.refresh_horde(ids[0]);
    assert!(!game.entities[ids[0]].in_horde());
    game.entities[ordinary].type_index = game.entities[ids[0]].type_index;
    game.entities[ordinary].owner = 1;
    game.refresh_horde(ids[0]);
    assert!(!game.entities[ids[0]].in_horde());
    game.entities[ordinary].owner = 0;
    game.refresh_horde(ids[0]);
    assert!(game.entities[ids[0]].true_horde_member());
    // OpenContain removes the rider from the source partition as it boards.
    game.entities[ordinary].containment.carrier = Some(0);
    game.observe_ground_partition(ordinary);
    game.refresh_horde(ids[0]);
    assert!(!game.entities[ids[0]].in_horde());
}

#[test]
fn mixed_infantry_counts_but_exact_vehicle_templates_do_not_use_reskin_equivalence() {
    for infantry in [true, false] {
        let (mut game, ids) = formation(infantry);
        let mut other = fixture_type(
            Faction::China,
            if infantry { Kind::Infantry } else { Kind::Tank },
        );
        other.behavior.horde = Some(rules(infantry));
        other.reskin = Some(game.catalog[game.entities[ids[0]].type_index].name.clone());
        let index = game.catalog.len();
        game.catalog.push(other);
        game.entities[ids[4]].type_index = index;
        game.refresh_horde(ids[0]);
        assert_eq!(game.entities[ids[0]].true_horde_member(), infantry);
    }
}

#[test]
fn radius_uses_strict_3d_bounding_sphere_distance_and_ruboff_is_not_an_extra_scan() {
    let (mut game, ids) = formation(true);
    let index = game.entities[ids[0]].type_index;
    let base = game.entities[ids[0]].position;
    let candidate = ids[4];
    game.entities[candidate].position = [base[0] + 40.0, base[1]];
    assert!(!game.horde_candidate(
        ids[0],
        candidate,
        game.catalog[index].behavior.horde.as_ref().unwrap()
    ));
    game.entities[candidate].position[0] -= 0.001;
    assert!(game.horde_candidate(
        ids[0],
        candidate,
        game.catalog[index].behavior.horde.as_ref().unwrap()
    ));
    let source_position = game.object_position(ids[0]);
    game.set_object_position(
        ids[0],
        [
            source_position[0],
            source_position[1],
            source_position[2] + 100.0,
        ],
    );
    let other_type = 5;
    game.catalog[other_type].behavior.horde = Some(rules(true));
    game.entities[candidate].type_index = other_type;
    assert!(!game.horde_candidate(
        ids[0],
        candidate,
        game.catalog[index].behavior.horde.as_ref().unwrap()
    ));
    game.set_object_position(ids[0], source_position);
    game.entities[candidate].position = [base[0] + 50.0, base[1]];
    game.entities[candidate].horde.in_horde = true;
    game.entities[candidate].horde.true_member = true;
    game.refresh_horde(ids[0]);
    assert!(!game.entities[ids[0]].in_horde());
    game.entities[candidate].position[0] = base[0] + 39.0;
    game.entities[ids[3]].health = 0.0;
    game.refresh_horde(ids[0]);
    assert!(game.entities[ids[0]].in_horde());
    assert!(!game.entities[ids[0]].true_horde_member());
}

#[test]
fn native_initial_wake_and_infantry_sleep_differ_from_vehicle_refresh_strictness() {
    for infantry in [true, false] {
        let (mut game, ids) = formation(infantry);
        let initial = game.tick
            + u64::from(
                game.update_remaining_sleep(
                    ids[0],
                    game.update_objects[game.entities[ids[0]].update_owner]
                        .handles
                        .len()
                        - 1,
                ),
            );
        assert!((1..=30).contains(&initial));
        game.tick = initial;
        game.run_object_updates();
        assert_eq!(game.entities[ids[0]].in_horde(), infantry);
        if infantry {
            assert_eq!(
                (game.tick
                    + u64::from(
                        game.update_remaining_sleep(
                            ids[0],
                            game.update_objects[game.entities[ids[0]].update_owner]
                                .handles
                                .len()
                                - 1
                        )
                    )),
                initial + 30
            );
        } else {
            game.tick = 30;
            game.run_object_updates();
            assert!(!game.entities[ids[0]].in_horde());
            game.tick = 31;
            game.run_object_updates();
            assert!(game.entities[ids[0]].in_horde());
        }
    }
}

#[test]
fn authored_classic_and_fixed_actions_preserve_their_distinct_nationalism_rules() {
    let mut rules = rules(true);
    assert_eq!(morale(&rules, 0, false, true, false), 2);
    assert_eq!(morale(&rules, 7, false, false, false), 4);
    rules.action = Action::Fixed;
    assert_eq!(morale(&rules, 7, false, true, true), 0);
    assert_eq!(morale(&rules, 0, true, false, true), 5);
    assert_eq!(morale(&rules, 7, true, false, false), 7);
}

#[test]
fn horde_nationalism_and_veterancy_rates_add_in_native_condition_order() {
    let (mut game, ids) = formation(true);
    let id = ids[0];
    let index = game.entities[id].type_index;
    game.catalog[index].weapon.bonuses.global[9][3] = 1.2;
    game.set_veterancy(id, 1);
    game.entities[id].horde.bonuses = 3;
    let rate =
        game.entities[id].weapon_rate_for(game.entities[id].active_weapon(&game.catalog[index]), 0);
    assert!((rate - 1.95).abs() < f32::EPSILON);
    game.entities[id].horde.bonuses = 2;
    let rate =
        game.entities[id].weapon_rate_for(game.entities[id].active_weapon(&game.catalog[index]), 0);
    assert!((rate - 1.45).abs() < f32::EPSILON);
}

#[test]
fn disabled_horde_module_retries_next_frame_until_paralysis_and_held_clear() {
    let (mut game, ids) = formation(true);
    let id = ids[0];
    let registration = game.update_objects[game.entities[id].update_owner]
        .handles
        .len()
        - 1;
    game.tick += u64::from(game.update_remaining_sleep(id, registration));
    game.entities[id].battle_bonuses.paralyzed_until = game.tick + 150;
    game.run_object_updates();
    assert_eq!(game.update_remaining_sleep(id, registration), 1);
    assert!(!game.entities[id].in_horde());
    game.entities[id].battle_bonuses.paralyzed_until = 0;
    game.set_disabled_held(id);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.update_remaining_sleep(id, registration), 1);
    assert!(!game.entities[id].in_horde());
    game.clear_disabled_held(id);
    game.tick += 1;
    game.run_object_updates();
    assert!(game.entities[id].in_horde());
    assert_eq!(game.update_remaining_sleep(id, registration), 30);
}

#[test]
fn membership_edge_restarts_active_weapon_delay_but_unchanged_refresh_does_not() {
    let (mut game, ids) = formation(true);
    let id = ids[0];
    let index = game.entities[id].type_index;
    game.catalog[index].weapon.delay = 2.0;
    let victim = game.spawn(5, 1, [340.0, 300.0], true);
    game.tick = 10;
    game.fire_weapon(id, victim);
    assert!(matches!(
        game.entities[id].weapon_status(0, &game.catalog[index], 10),
        Some(crate::skirmish::ai_dispatch::WeaponStatus::BetweenShots)
    ));
    game.refresh_horde(id);
    assert_eq!(
        game.entities[id].weapon_set.weapons[0].cooldown.to_bits(),
        40.0_f32.to_bits()
    );
    game.entities[id].weapon_set.weapons[0].cooldown = 7.0;
    game.refresh_horde(id);
    assert_eq!(
        game.entities[id].weapon_set.weapons[0].cooldown.to_bits(),
        7.0_f32.to_bits()
    );
    game.entities[ids[4]].health = 0.0;
    game.refresh_horde(id);
    assert_eq!(
        game.entities[id].weapon_set.weapons[0].cooldown.to_bits(),
        60.0_f32.to_bits()
    );
}

#[test]
fn original_decal_type_retains_last_horde_texture_after_membership_ends() {
    for infantry in [true, false] {
        let (mut game, ids) = formation(infantry);
        let id = ids[0];
        assert!(game.entities[id].terrain_decal_none());
        game.tick = 31;
        game.update_horde_module(id);
        assert!(matches!(
            (infantry, game.entities[id].terrain_decal()),
            (true, TerrainDecal::Horde) | (false, TerrainDecal::HordeVehicle)
        ));
        game.players[0].upgrades.push("Upgrade_Nationalism".into());
        game.tick = 62;
        game.update_horde_module(id);
        assert!(matches!(
            game.entities[id].terrain_decal(),
            TerrainDecal::HordeNationalism
        ));
        game.players[0].upgrades.push("Upgrade_Fanaticism".into());
        game.tick = 93;
        game.update_horde_module(id);
        assert!(matches!(
            game.entities[id].terrain_decal(),
            TerrainDecal::HordeFanaticism
        ));
        game.entities[ids[4]].health = 0.0;
        game.tick = 124;
        game.update_horde_module(id);
        assert!(!game.entities[id].in_horde());
        assert!(matches!(
            game.entities[id].terrain_decal(),
            TerrainDecal::HordeFanaticism
        ));
    }
}

#[test]
fn native_draw_icon_ui_clears_decal_and_only_actual_nonportable_horde_restores_it() {
    let (mut game, ids) = formation(true);
    let id = ids[0];
    game.update_horde_module(id);
    assert!(game.entities[id].in_horde());
    assert!(!game.entities[id].terrain_decal_none());
    game.draw_icon_ui = false;
    game.update_horde_module(id);
    assert!(game.entities[id].in_horde());
    assert!(game.entities[id].terrain_decal_none());
    game.draw_icon_ui = true;
    game.update_horde_module(id);
    assert!(!game.entities[id].terrain_decal_none());
    let kind = game.entities[id].type_index;
    game.catalog[kind].kind_of.push("PORTABLE_STRUCTURE".into());
    game.draw_icon_ui = false;
    game.update_horde_module(id);
    game.draw_icon_ui = true;
    game.update_horde_module(id);
    assert!(game.entities[id].in_horde());
    // Native portable horde riders do not issue setTerrainDecal(HORDE).
    assert!(game.entities[id].terrain_decal_none());
}
