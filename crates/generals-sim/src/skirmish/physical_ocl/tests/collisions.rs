use super::*;

#[test]
fn zero_health_physical_contacts_deliver_both_callbacks_and_clear_only_immobile_velocity() {
    let (mut game, source) = fixture();
    let kind = game.catalog.len() - 1;
    game.catalog[kind].crush.physics_module = Some(0);
    game.catalog[kind].kind_of.clear();
    crate::skirmish::tests::register_update(&mut game.catalog[kind], "PhysicsBehavior");
    let mut list = list();
    list.nuggets[0].disposition = LIKE_EXISTING;
    list.nuggets[0].count = 2;
    let ids = game.create_physical_ocl(source, &list).unwrap();
    game.entities[ids[0]]
        .physical
        .as_mut()
        .unwrap()
        .motion
        .velocity = [1.0, 2.0, 3.0];
    game.entities[ids[1]]
        .physical
        .as_mut()
        .unwrap()
        .motion
        .velocity = [-1.0, -2.0, -3.0];
    game.entities[ids[1]].position[0] += 2.0;
    game.entities[ids[1]]
        .physical
        .as_mut()
        .unwrap()
        .motion
        .position[0] += 2.0;
    game.observe_ground_partition(ids[1]);
    game.update_ground_contacts();
    for (id, other) in [(ids[0], ids[1]), (ids[1], ids[0])] {
        let actor = &game.entities[id];
        assert!(!actor.alive() && actor.physical_present());
        let state = actor.physical.as_ref().unwrap();
        assert_eq!(state.last_collidee, Some(other));
        let expected = if id == ids[0] {
            [1.0_f32, 2.0, 3.0]
        } else {
            [-1.0_f32, -2.0, -3.0]
        };
        assert_eq!(
            state.motion.velocity.map(f32::to_bits),
            expected.map(f32::to_bits)
        );
    }
    assert!(
        game.entities[ids[0]]
            .physical
            .as_ref()
            .unwrap()
            .motion
            .acceleration[0]
            < 0.0
    );
    assert!(
        game.entities[ids[1]]
            .physical
            .as_ref()
            .unwrap()
            .motion
            .acceleration[0]
            > 0.0
    );
}

#[test]
fn static_side_contact_zeroes_old_velocity_and_applies_native_stiffness_force() {
    let (mut game, source) = fixture();
    let kind = game.type_for(1, Kind::Barracks).unwrap();
    let target = game.spawn(kind, 1, [310.0, 300.0], true);
    let mut list = list();
    list.nuggets[0].disposition = LIKE_EXISTING;
    list.nuggets[0].offset = [0.0; 3];
    let id = game.create_physical_ocl(source, &list).unwrap()[0];
    let state = game.entities[id].physical.as_mut().unwrap();
    state.motion.velocity = [2.0, 0.0, 0.0];
    game.physical_contact(id, target);
    let state = game.entities[id].physical.as_ref().unwrap();
    assert_eq!(
        state.motion.velocity.map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    assert_eq!(state.motion.acceleration[0].to_bits(), (-1.0_f32).to_bits());
    assert_eq!(state.last_collidee, Some(target));
    assert!(game.entities[id].physical_present());
}

#[test]
fn ocl_primary_ignore_survives_ai_goal_changes_and_suppresses_reverse_physics() {
    let (mut game, _) = fixture();
    let kind = game.catalog.len() - 1;
    game.catalog[kind].crush.physics_module = Some(0);
    game.catalog[kind].kind_of.clear();
    let source = game.new_object(kind, 0, true);
    game.set_object_transform(source, [300.0, 300.0, 10.0], transform::yaw(0.0));
    let mut list = list();
    list.nuggets[0].disposition = LIKE_EXISTING;
    list.nuggets[0].offset = [2.0, 0.0, 0.0];
    list.nuggets[0].ignore_primary_obstacle = true;
    let child = game.create_physical_ocl(source, &list).unwrap()[0];
    game.entities[child].ai.ignored_producer = None;
    game.physical_contact(child, source);
    game.physical_contact(source, child);
    assert_eq!(game.entities[child].physics_last_collidee(), None);
    assert_eq!(game.entities[source].physics_last_collidee(), None);
    game.entities[child].set_physics_ignored_object(None);
    game.physical_contact(child, source);
    game.physical_contact(source, child);
    assert_eq!(game.entities[child].physics_last_collidee(), Some(source));
    assert_eq!(game.entities[source].physics_last_collidee(), Some(child));
}
