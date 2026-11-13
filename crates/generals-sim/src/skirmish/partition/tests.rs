use super::*;

#[test]
fn closest_healing_scan_keeps_dead_enemy_pad_and_excludes_the_range_boundary() {
    use crate::skirmish::{
        DeathType, Faction, Kind,
        tests::{fixture_type, game},
    };
    let mut game = game();
    let mut pad = fixture_type(Faction::Usa, Kind::TechBuilding);
    pad.kind_of.push("HEAL_PAD".into());
    let kind = game.catalog.len();
    game.catalog.push(pad);
    let seeker = game.spawn(5, 0, [300.0, 300.0], true);
    let near = game.spawn(kind, 1, [320.0, 300.0], true);
    let far = game.spawn(kind, 0, [350.0, 300.0], true);
    game.kill_object(near, DeathType::Normal);
    game.update_ground_contacts();
    assert_eq!(game.closest_heal_pad(seeker, 20.0), None);
    assert_eq!(game.closest_heal_pad(seeker, 100.0), Some(near));
    game.destroy_maintenance_object(near);
    game.process_update_destruction();
    assert_eq!(game.closest_heal_pad(seeker, 100.0), Some(far));
}

#[test]
fn firestorm_range_uses_victim_footprint_and_does_not_filter_its_source() {
    use crate::skirmish::{
        Faction, Kind,
        tests::{fixture_type, game},
    };
    let mut game = game();
    let mut unit = fixture_type(Faction::China, Kind::Effect);
    unit.geometry.major = 10.0;
    unit.geometry.minor = 10.0;
    unit.geometry.shape = crate::collision_geometry::Shape::Cylinder;
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let source = game.spawn(kind, 0, [300.0, 300.0], true);
    let boundary = game.spawn(kind, 0, [330.0, 300.0], true);
    let overlap = game.spawn(kind, 1, [329.0, 300.0], true);
    game.update_ground_contacts();
    let targets = game.firestorm_damage_targets(source, 20.0);
    assert!(targets.contains(&source));
    assert!(targets.contains(&overlap));
    assert!(!targets.contains(&boundary));
}

#[test]
fn map_status_uses_strict_bounds_and_consecutive_transform_changes() {
    let mut game = crate::skirmish::tests::game();
    let id = game.spawn(5, 0, [0.015, 100.0], true);
    assert!(!game.entities[id].is_off_map());
    game.entities[id].position[0] = 0.006;
    game.observe_ground_partition(id);
    game.entities[id].position[0] = -0.003;
    game.observe_ground_partition(id);
    assert!(!game.entities[id].is_off_map());
    game.entities[id].position[0] = -0.014;
    game.observe_ground_partition(id);
    assert!(game.entities[id].is_off_map());
    game.entities[id].position[0] = 0.0;
    game.observe_ground_partition(id);
    assert!(game.entities[id].is_off_map());
}

fn collect(partition: &mut Partition, cells: &[[u16; 2]]) -> Vec<[usize; 2]> {
    let mut contacts = ContactList::new();
    while let Some((identity, update)) = partition.take_dirty() {
        let Identity::Actor(id) = identity else {
            panic!("Actor contact fixture contains map objects")
        };
        if update {
            partition.replace_cells(id, cells.to_vec());
        }
        partition.add_candidates(id, &mut contacts);
    }
    contacts
        .contacts
        .into_iter()
        .rev()
        .map(|contact| {
            let Identity::Actor(other) = contact.other else {
                panic!("Actor contact fixture contains map objects")
            };
            [contact.actor, other]
        })
        .collect()
}

#[test]
fn contact_hash_collisions_preserve_first_orientation_and_callback_order() {
    let mut contacts = ContactList::new();
    let mut expected = Vec::new();
    for id in 0..128 {
        for other in [1, 2, 3, 5382, 5383, 65537, 65538] {
            for pair in [[id, other], [other, id]] {
                if pair[0] != pair[1]
                    && !expected
                        .iter()
                        .any(|item| *item == pair || *item == [pair[1], pair[0]])
                {
                    expected.push(pair);
                }
                contacts.add(pair[0], Identity::Actor(pair[1]));
            }
        }
    }
    let actual: Vec<_> = contacts
        .contacts
        .into_iter()
        .rev()
        .map(|contact| {
            let Identity::Actor(other) = contact.other else {
                panic!("Actor contact fixture contains map objects")
            };
            [contact.actor, other]
        })
        .collect();
    expected.reverse();
    assert_eq!(actual, expected);
}

#[test]
fn dirty_and_cell_prepend_order_selects_first_callback_and_deduplicates_pairs() {
    let mut partition = Partition::new(10.0, [40.0; 2]);
    for id in 0..3 {
        partition.observe(id, [15.0, 15.0, 0.0], 0.0);
    }
    // Two shared cells must not create duplicate callbacks or reverse the
    // pair orientation chosen by its first native cell-list encounter.
    assert_eq!(
        collect(&mut partition, &[[1, 1], [1, 2]]),
        [[0, 2], [0, 1], [1, 2]]
    );
    assert!(collect(&mut partition, &[]).is_empty());

    // processContactList redirties first then second for each accepted pair.
    for [id, other] in [[0, 2], [0, 1], [1, 2]] {
        partition.make_dirty(id, false);
        partition.make_dirty(other, false);
    }
    assert_eq!(collect(&mut partition, &[]), [[2, 0], [1, 2], [1, 0]]);
}

#[test]
fn movement_upgrades_pending_contact_check_without_reordering_the_dirty_list() {
    let mut partition = Partition::new(10.0, [40.0; 2]);
    for id in 0..2 {
        partition.observe(id, [15.0, 15.0, 0.0], 0.0);
    }
    collect(&mut partition, &[[1, 1]]);
    partition.make_dirty(0, false);
    partition.make_dirty(1, false);
    partition.observe(0, [25.0, 15.0, 0.0], 0.0);
    assert_eq!(partition.take_dirty(), Some((Identity::Actor(1), false)));
    assert_eq!(partition.take_dirty(), Some((Identity::Actor(0), true)));
    partition.replace_cells(0, vec![[2, 1]]);
    assert_eq!(partition.cells[6], [Identity::Actor(1)]);
    assert_eq!(partition.cells[7], [Identity::Actor(0)]);
}

#[test]
fn transform_noise_is_compared_with_each_previous_change_not_last_cell_update() {
    let mut partition = Partition::new(10.0, [40.0; 2]);
    partition.observe(0, [0.0; 3], 0.0);
    collect(&mut partition, &[[0, 0]]);
    partition.observe(0, [0.009, 0.0, 0.0], 0.009);
    partition.observe(0, [0.018, 0.0, 0.0], 0.018);
    assert!(partition.take_dirty().is_none());
    partition.observe(0, [0.029, 0.0, 0.0], 0.018);
    assert_eq!(partition.take_dirty(), Some((Identity::Actor(0), true)));
}

#[test]
fn removal_releases_cells_and_pending_dirty_work_before_reusing_storage() {
    let mut partition = Partition::new(10.0, [40.0; 2]);
    partition.observe(0, [15.0, 15.0, 0.0], 0.0);
    partition.observe(1, [15.0, 15.0, 0.0], 0.0);
    collect(&mut partition, &[[1, 1]]);
    partition.make_dirty(0, false);
    partition.make_dirty(1, false);
    partition.remove(0);
    assert!(collect(&mut partition, &[]).is_empty());
    assert_eq!(partition.cells[6], [Identity::Actor(1)]);
    partition.clear();
    assert!(partition.records.is_empty());
    assert!(partition.cells.iter().all(Vec::is_empty));
    // Zero Hour fast ceil(40 / 10) allocates five cells per axis.
    assert_eq!(partition.dimensions, [5, 5]);
}

#[test]
fn source_no_collisions_suppresses_registered_dead_objects_until_destruction() {
    let mut game = super::super::tests::game();
    let kind = game.type_for(0, super::super::Kind::Tank).unwrap();
    let first = game.spawn(kind, 0, [400.0, 400.0], true);
    let second = game.spawn(kind, 1, [401.0, 400.0], true);
    assert!(game.ground_contact_intersects(first, second));
    game.catalog[kind].kind_of.push("NO_COLLIDE".into());
    assert!(!game.ground_contact_intersects(first, second));
    game.catalog[kind]
        .kind_of
        .retain(|kind| kind != "NO_COLLIDE");
    game.kill_object(second, super::super::DeathType::Normal);
    // PartitionManager tests geometry and NO_COLLISIONS, not current health.
    assert!(game.ground_contact_intersects(first, second));
    game.entities[second].object_status |= 1 << 5;
    assert!(!game.ground_contact_intersects(first, second));
    assert!(game.partition.records[second].is_some());
    game.destroy_maintenance_object(second);
    assert!(game.partition.records[second].is_some());
    game.process_update_destruction();
    assert!(game.partition.records[second].is_none());
}

#[test]
fn retained_rubble_status_suppresses_ground_contacts_without_unregistering() {
    let mut game = super::super::tests::game();
    let mut unit = super::super::tests::fixture_type(
        super::super::Faction::Civilian,
        super::super::Kind::TechBuilding,
    );
    unit.maintenance.rubble_height = Some(5.0);
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let building = game.spawn(kind, game.neutral_owner(), [355.0, 205.0], true);
    let infantry = game.spawn(5, 0, [355.0, 205.0], true);
    assert!(game.ground_contact_intersects(building, infantry));
    game.apply_hit(building, infantry, 2000.0);
    game.update_ground_contacts();
    assert!(game.partition.records[building].is_some());
    assert!(!game.ground_contact_intersects(building, infantry));
}

#[test]
fn masked_object_retains_partition_and_transform_cannot_register_an_explicitly_removed_object() {
    let mut game = crate::skirmish::tests::game();
    let id = game.spawn(3, 0, [350.0, 350.0], true);
    game.mask_object(id, true);
    game.set_object_position(id, [450.0, 350.0, 0.0]);
    game.update_ground_contacts();
    assert!(game.partition.records[id].is_some());
    assert!(game.partition.outward_objects([450.0, 350.0]).contains(&id));
    game.partition.remove(id);
    game.set_object_position(id, [550.0, 350.0, 0.0]);
    game.set_dynamic_geometry(id, [30.0, 15.0, 10.0]);
    game.update_ground_contacts();
    assert!(game.partition.records[id].is_none());
    game.register_object_partition(id);
    game.update_ground_contacts();
    assert!(game.partition.outward_objects([550.0, 350.0]).contains(&id));
}
