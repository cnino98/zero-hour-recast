use super::*;

#[test]
fn player_area_filter_rejects_allies_and_neutral_demo_traps_but_admits_neutral_mines() {
    let (mut game, builder) = fixture();
    let neutral = game.neutral_owner();
    let allied = game.spawn(5, 0, [301.0, 300.0], true);
    let neutral_trap = game.spawn(5, neutral, [302.0, 300.0], true);
    game.catalog[6].kind_of = vec!["MINE".into()];
    game.catalog[6].weapon = WeaponRules::unarmed();
    game.catalog[6].ai.acquisition = 0;
    let neutral_mine = game.spawn(6, neutral, [320.0, 300.0], true);
    let mut forbidden_type = tests::fixture_type(
        crate::skirmish::Faction::Gla,
        crate::skirmish::Kind::Infantry,
    );
    forbidden_type.kind_of = vec!["MINE".into(), "UNATTACKABLE".into()];
    forbidden_type.weapon = WeaponRules::unarmed();
    let type_index = game.catalog.len();
    game.catalog.push(forbidden_type);
    let forbidden = game.spawn(type_index, 1, [303.0, 300.0], true);
    game.update_ground_contacts();
    game.clear_mines_at(builder, 0, [302.0, 300.0]).unwrap();
    assert!(
        matches!(game.entities[builder].ai.order, Order::Attack(target) if target == neutral_mine)
    );
    for _ in 0..120 {
        game.step();
        if game.entities[neutral_mine].masked() {
            break;
        }
    }
    assert!(game.entities[neutral_mine].masked());
    for id in [allied, neutral_trap, forbidden] {
        assert!(game.entities[id].alive() && !game.entities[id].masked());
    }
}

#[test]
fn empty_position_digs_once_and_reloads_without_a_victim_effect_or_projectile() {
    let (mut game, builder) = fixture();
    let weapon = &mut game.catalog[1].weapon_set_rules.templates[0].rules;
    weapon.clip_size = 1;
    weapon.reload_seconds = 4.0;
    game.entities[builder].weapon_set.weapons[0].clip_remaining = 1;
    game.tick = 2;
    let point = game.entities[builder].position;
    game.select_mine_clearing(builder, 0).unwrap();
    game.clear_mines_at(builder, 0, point).unwrap();
    let preattack = game.entities[builder].weapon_set.weapons[0]
        .pre_attack
        .as_ref()
        .unwrap();
    assert_eq!((preattack.start, preattack.frames), (2, 3));
    let mut began = None;
    let mut fired = None;
    for _ in 0..150 {
        game.step();
        let actor = &game.entities[builder];
        if let Some((start, duration, _)) = actor.animation_duration() {
            // The command entered prefire at frame 2. Object's FINAL helper
            // first observes it at frame 3 and uses the remaining duration.
            assert_eq!((start, duration), (3, 2));
            began.get_or_insert_with(|| start + u64::from(duration));
        }
        if fired.is_none() && actor.weapon_set.weapons[0].max_shots == Some(0) {
            fired = Some(game.tick);
            assert_eq!(game.tick, 5);
            assert_eq!(game.tick, began.unwrap());
            assert_eq!(actor.weapon_set.weapons[0].max_shots, Some(0));
        }
        if let Some(frame) = fired {
            assert_eq!(
                actor.weapon_rounds_remaining(0),
                u32::from(game.tick >= frame + 120)
            );
            assert_eq!(
                game.mine_clearing_ready(builder, 0),
                game.tick >= frame + 122
            );
        }
        assert_eq!(game.entities[builder].weapon_set.weapons[0].last_shot, None);
        assert!(
            game.presentation
                .shots
                .iter()
                .all(|shot| shot.source != builder)
        );
        assert!(
            game.projectiles
                .iter()
                .all(|projectile| projectile.source != builder)
        );
    }
    assert!(fired.is_some());
    assert!(matches!(game.entities[builder].ai.order, Order::Idle));
    assert!(game.mine_clearing_ready(builder, 0));
}

#[test]
fn button_installs_missing_work_weapon_then_ground_click_cancels_the_task() {
    let (mut game, builder) = fixture();
    let building = game.spawn(0, 0, [400.0, 300.0], true);
    game.set_order(builder, Order::Repair(building));
    game.set_mine_clearing(builder, false);
    assert!(!game.weapon_available(builder, 0));
    assert!(game.mine_clearing_ready(builder, 0));
    game.select_mine_clearing(builder, 0).unwrap();
    assert!(matches!(game.entities[builder].ai.order, Order::Repair(id) if id == building));
    let weapon = game.entities[builder].weapon_index(0);
    game.clear_mines_at(builder, 0, [300.0, 300.0]).unwrap();
    game.step();
    assert!(
        matches!(game.entities[builder].ai.order, Order::FireAt { weapon: entry, .. } if entry == weapon)
    );
    assert_eq!(game.entities[builder].weapon_index(0), weapon);
}

#[test]
fn inaccessible_contact_click_uses_native_bounded_probe_but_in_range_point_stays_exact() {
    let (mut game, builder) = fixture();
    let point = [450.0, 300.0];
    game.spawn(0, 0, point, true);
    game.rebuild_navigation();
    game.clear_mines_at(builder, 0, point).unwrap();
    let Order::FireAt { position, .. } = game.entities[builder].ai.order else {
        panic!("ground attack")
    };
    assert_ne!(position.map(f32::to_bits), point.map(f32::to_bits));
    assert!(crate::skirmish::movement::distance_squared(position, point) <= 100.0 * 100.0);
    let from = game.cell_at(game.entities[builder].position).unwrap();
    let to = game.cell_at(position).unwrap();
    assert!(game.navigation_for(builder).quick_path_exists(1, from, to));
    game.entities[builder].position = [442.0, 300.0];
    let actor = &game.entities[builder];
    let weapon = actor.active_weapon(&game.catalog[actor.type_index]);
    assert!(weapon.contact());
    assert!(
        (crate::skirmish::movement::distance_squared(actor.position, point).sqrt()
            - game.object_radius(builder))
        .max(0.0)
            <= actor.weapon_range(&game.catalog[actor.type_index], 0)
    );
    assert!(
        !game.navigation_for(builder).quick_path_exists(
            1,
            game.cell_at(actor.position).unwrap(),
            game.cell_at(point).unwrap()
        ),
        "the contact target remains inaccessible to the ordinary quick path"
    );
    game.clear_mines_at(builder, 0, point).unwrap();
    let Order::FireAt { position, .. } = game.entities[builder].ai.order else {
        panic!("ground attack")
    };
    assert_eq!(position.map(f32::to_bits), point.map(f32::to_bits));
}
