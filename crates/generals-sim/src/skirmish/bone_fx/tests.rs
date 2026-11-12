use super::*;
use crate::skirmish::{
    DamageType, DeathType, Faction, Kind, model_bones,
    tests::{fixture_type, game, register_behavior, register_update},
};
use std::collections::{BTreeMap, BTreeSet};

fn rules() -> Rules {
    Rules {
        slots: std::array::from_fn(|_| {
            std::array::from_fn(|_| Slot {
                fx: None,
                particle: None,
            })
        }),
        damage_fx: u64::MAX,
        damage_particle: u64::MAX,
    }
}
fn at(name: &str, bone: &str, delay: i32) -> AtBone {
    AtBone {
        name: name.into(),
        bone: bone.into(),
        delay,
    }
}
fn fixture(mut modules: Vec<Rules>) -> (Skirmish, usize) {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::Effect);
    // Stationary BoneFX object: no AI module may move or orient its transform.
    unit.update_modules.clear();
    unit.behavior_modules.clear();
    unit.max_health = 100.0;
    unit.initial_health = 100.0;
    unit.behavior.bone_fx.append(&mut modules);
    for _ in 0..unit.behavior.bone_fx.len() {
        register_update(&mut unit, "BoneFXUpdate");
    }
    register_behavior(&mut unit, "BoneFXDamage", 4);
    let mut conditions = crate::skirmish::tests::death_conditions();
    conditions.death_mask = 0;
    unit.behavior.destroy_die.push(conditions);
    register_behavior(&mut unit, "DestroyDie", 2);
    unit.pristine_draws = vec![model_bones::Draw {
        ignored: BTreeSet::new(),
        states: vec![model_bones::State {
            conditions: vec![BTreeSet::new()],
            transforms: BTreeMap::from([(
                "nozzle".into(),
                [
                    [1.0, 0.0, 0.0, 2.0],
                    [0.0, 1.0, 0.0, 3.0],
                    [0.0, 0.0, 1.0, 4.0],
                ],
            )]),
        }],
    }];
    let index = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(index, 0, [300.0, 200.0], true);
    (game, id)
}

#[test]
fn real_first_update_fires_numeric_slots_fx_before_particle_and_preserves_null_zero() {
    let mut rules = rules();
    rules.slots[0][0].fx = Some(at("FirstFx", "Nozzle", 0));
    rules.slots[0][0].particle = Some(at("FirstParticle", "NULL", 0));
    rules.slots[0][5].particle = Some(at("SixthParticle", "Nozzle", 0));
    let (mut game, id) = fixture(vec![rules]);
    assert!(game.presentation.bone_fx.is_empty());
    let basis = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    game.set_object_transform(id, [300.0, 200.0, 50.0], basis);
    game.step();
    let events: Vec<_> = game
        .presentation
        .weapon_fx
        .iter()
        .filter_map(|event| match event {
            WeaponFxEvent::ObjectFx(index)
                if game.presentation.object_fx_events[*index].object == id =>
            {
                Some("fx")
            }
            WeaponFxEvent::BoneFx(index) if game.presentation.bone_fx[*index].object == id => {
                Some("particle")
            }
            _ => None,
        })
        .collect();
    assert_eq!(events, ["fx", "particle", "particle"]);
    let fx = game
        .presentation
        .object_fx_events
        .iter()
        .find(|event| event.object == id && event.effect == "FirstFx")
        .unwrap();
    assert_eq!(
        fx.position.map(f32::to_bits),
        [297.0_f32, 202.0, 54.0].map(f32::to_bits)
    );
    assert!(!fx.oriented_object);
    assert!(fx.secondary_position.is_none());
    assert_eq!(fx.override_radius.to_bits(), 0.0_f32.to_bits());

    let events: Vec<_> = game
        .presentation
        .bone_fx
        .iter()
        .filter(|event| event.object == id)
        .collect();
    assert_eq!(events.len(), 2);
    for (event, expected_slot, expected_local) in [
        (events[0], 0, [0.0_f32; 3]),
        (events[1], 5, [2.0, 3.0, 4.0]),
    ] {
        let Action::Particle {
            slot,
            local,
            position,
            basis: snapshot,
            ..
        } = &event.action
        else {
            panic!("native particle callback")
        };
        assert_eq!(*slot, expected_slot);
        assert_eq!(local.map(f32::to_bits), expected_local.map(f32::to_bits));
        assert_eq!(
            position.map(f32::to_bits),
            [300.0_f32, 200.0, 50.0].map(f32::to_bits)
        );
        assert_eq!(
            snapshot.map(|row| row.map(f32::to_bits)),
            basis.map(|row| row.map(f32::to_bits))
        );
    }
    game.step();
    assert!(
        !game
            .presentation
            .bone_fx
            .iter()
            .any(|event| event.object == id)
    );
}

#[test]
fn first_damage_instance_routes_to_first_update_and_healing_reuses_lazy_pristine_cache() {
    let mut first = rules();
    first.slots[0][0].particle = Some(at("First", "Nozzle", 0));
    let mut second = rules();
    second.slots[0][0].particle = Some(at("Second", "Nozzle", 0));
    let (mut game, id) = fixture(vec![first, second]);
    game.step();
    game.presentation.bone_fx.clear();
    game.attempt_health_damage(id, None, 60.0, DamageType::Unresistable, DeathType::Normal);
    assert_eq!(game.body_damage_state(id), 1);
    assert_eq!(game.presentation.bone_fx.len(), 1);
    assert_eq!(game.presentation.bone_fx[0].module, 0);
    assert!(matches!(game.presentation.bone_fx[0].action, Action::Clear));
    let kind = game.entities[id].type_index;
    game.catalog[kind].pristine_draws[0].states[0]
        .transforms
        .get_mut("nozzle")
        .unwrap()[0][3] = 99.0;
    game.apply_healing(id, None, 60.0);
    let random = game.random;
    game.update_bone_fx(id, 0);
    game.update_bone_fx(id, 1);
    assert_eq!(game.random, random);
    let event = game.presentation.bone_fx.last().unwrap();
    assert_eq!(event.module, 0);
    let Action::Particle { local, .. } = event.action else {
        panic!("pristine particle restarted")
    };
    assert_eq!(
        local.map(f32::to_bits),
        [2.0_f32, 3.0, 4.0].map(f32::to_bits)
    );
    assert_eq!(
        game.presentation
            .bone_fx
            .iter()
            .filter(|event| matches!(event.action, Action::Particle { .. }))
            .count(),
        1
    );
}

#[test]
fn independent_bone_deadline_survives_filtered_die_and_real_destructor_clears_every_instance() {
    let mut first = rules();
    first.slots[3][0].fx = Some(at("RubbleFx", "Nozzle", 2));
    first.slots[3][0].particle = Some(at("RubbleParticle", "Nozzle", 2));
    let (mut game, id) = fixture(vec![first, rules()]);
    game.step();
    game.kill_object(id, DeathType::Normal);
    assert!(game.entities[id].is_effectively_dead());
    assert!(!game.update_objects[game.entities[id].update_owner].destroy_requested);
    game.step();
    assert!(
        !game
            .presentation
            .bone_fx
            .iter()
            .any(|event| matches!(event.action, Action::Particle { .. }))
    );
    game.step();
    assert_eq!(
        game.presentation
            .object_fx_events
            .iter()
            .filter(|event| event.object == id && event.effect == "RubbleFx")
            .count(),
        1
    );
    assert_eq!(
        game.presentation
            .bone_fx
            .iter()
            .filter(|event| event.object == id && matches!(event.action, Action::Particle { .. }))
            .count(),
        1
    );
    game.presentation.weapon_fx.clear();
    game.destroy_maintenance_object(id);
    game.process_update_destruction();
    let events: Vec<_> = game
        .presentation
        .weapon_fx
        .iter()
        .filter_map(|event| match event {
            WeaponFxEvent::DrawableDestroyed(object) if *object == id => Some(usize::MAX),
            WeaponFxEvent::BoneFx(index)
                if matches!(game.presentation.bone_fx[*index].action, Action::Clear) =>
            {
                Some(game.presentation.bone_fx[*index].module)
            }
            _ => None,
        })
        .collect();
    assert_eq!(events, [usize::MAX, 0, 1]);
}

#[test]
fn structure_completion_stops_first_active_bone_update_without_reinitializing_it() {
    let mut first = rules();
    first.slots[0][0].particle = Some(at("First", "Nozzle", 100));
    let mut second = rules();
    second.slots[0][0].particle = Some(at("Second", "Nozzle", 100));
    let (mut game, id) = fixture(vec![first, second]);
    game.step();
    let second_timer = game.entities[id].behaviors.bone_fx[1].next_particle;
    assert!(game.entities[id].behaviors.bone_fx[0].active);
    assert!(game.entities[id].behaviors.bone_fx[0].next_particle[0][0] >= 0);
    game.stop_first_bone_fx(id);
    let first = &game.entities[id].behaviors.bone_fx[0];
    assert!(first.active);
    assert!(first.next_fx.iter().flatten().all(|timer| *timer == -1));
    assert!(
        first
            .next_particle
            .iter()
            .flatten()
            .all(|timer| *timer == -1)
    );
    assert_eq!(
        game.entities[id].behaviors.bone_fx[1].next_particle,
        second_timer
    );
    assert!(matches!(
        game.presentation.bone_fx.last().unwrap().action,
        Action::Clear
    ));
    assert_eq!(game.presentation.bone_fx.last().unwrap().module, 0);
    game.step();
    assert!(
        game.entities[id].behaviors.bone_fx[0]
            .next_particle
            .iter()
            .flatten()
            .all(|timer| *timer == -1)
    );
}
