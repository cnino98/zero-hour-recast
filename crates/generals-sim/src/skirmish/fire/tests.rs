use super::*;
use crate::skirmish::{
    AreaDamage, DamageType, DeathType, Faction, Kind, Skirmish, WeaponRules, startup,
};

fn field(game: &mut Skirmish, name: &str) -> usize {
    let mut unit = crate::skirmish::tests::fixture_type(Faction::China, Kind::Effect);
    name.clone_into(&mut unit.name);
    unit.body_kind = crate::skirmish::BodyKind::Inactive;
    unit.kind_of = vec!["IMMOBILE".into(), "NO_COLLIDE".into()];
    unit.initial_health = 0.0;
    unit.max_health = 0.0;
    unit.speed = 0.0;
    unit.locomotor.wander_length = None;
    unit.ai = crate::skirmish::UnitAi::default();
    unit.fire = Some(Rules {
        body: Body::Inactive,
        ambient: String::new(),
        lifetime: [75, 75],
        initial_delay: 0,
        storm: None,
    });
    let mut weapon = WeaponRules::unarmed();
    weapon.range = 15.0;
    weapon.min_range = 10.0;
    weapon.name = "SmallFireFieldWeapon".into();
    weapon.damage = 5.0;
    weapon.damage_type = DamageType::Flame;
    weapon.death_type = DeathType::Burned;
    weapon.delay = 8.0 / 30.0;
    weapon.area = Some(AreaDamage {
        at_source: false,
        primary_radius: 30.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 7,
    });
    unit.kind_of.push("NO_COLLIDE".into());
    unit.geometry.major = 1.0;
    unit.geometry.minor = 1.0;
    unit.geometry.height = 1.0;
    unit.geometry_bounds.sphere_radius = 1.0;
    unit.weapon = WeaponRules::unarmed();
    unit.behavior.fire_weapons = vec![weapon_update::Rules {
        weapon: Some(weapon),
        initial_delay: 0,
        exclusive_delay: 0,
    }];
    unit.behavior.deletions = vec![[75; 2]];
    unit.update_modules = vec![
        crate::skirmish::module_updates::ModuleRegistration::Helper(
            crate::skirmish::module_updates::Helper::StateMachine,
        ),
        crate::skirmish::module_updates::ModuleRegistration::Helper(
            crate::skirmish::module_updates::Helper::Defection,
        ),
    ];
    unit.behavior_modules.clear();
    crate::skirmish::tests::register_update(&mut unit, "FireWeaponUpdate");
    crate::skirmish::tests::register_update(&mut unit, "DeletionUpdate");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    kind
}

fn storm_modules(game: &mut Skirmish, kind: usize) {
    let unit = &mut game.catalog[kind];
    let storm = unit.fire.as_mut().unwrap().storm.take().unwrap();
    unit.behavior.fire_weapons.clear();
    unit.behavior.deletions.clear();
    unit.behavior.lifetimes = vec![[75; 2]];
    unit.behavior.firestorms = vec![firestorm::Rules {
        geometry: storm.geometry,
        interval: storm.interval,
        damage: storm.damage,
        height: storm.height,
        particles: std::array::from_fn(|_| None),
        particle_height: storm.particle_height,
        effect: storm.effect,
        scorch: storm.scorch,
    }];
    unit.update_modules.retain(|entry| {
        matches!(
            entry,
            crate::skirmish::module_updates::ModuleRegistration::Helper(_)
        )
    });
    unit.behavior_modules.clear();
    crate::skirmish::tests::register_update(unit, "FirestormDynamicGeometryInfoUpdate");
    crate::skirmish::tests::register_update(unit, "LifetimeUpdate");
}

#[test]
fn zero_health_inactive_fire_keeps_pulsing_and_deletes_at_original_lifetime() {
    let mut game = crate::skirmish::tests::game();
    let kind = field(&mut game, "FireFieldSmall");
    // System.ini's dedicated FireWeaponUpdate bypasses attack-range checks at its own feet.
    assert_eq!(
        game.catalog[kind].behavior.fire_weapons[0]
            .weapon
            .as_ref()
            .unwrap()
            .range
            .to_bits(),
        15.0_f32.to_bits()
    );
    assert!(crate::skirmish::weapons::validate(&game.catalog[kind]).is_ok());
    game.tick = 100;
    let victim = game.spawn(5, 1, [250.0, 250.0], true);
    let fire = game.spawn(kind, 0, [250.0, 250.0], true);
    assert!(!game.entities[fire].alive());
    let before = game.entities[victim].health;
    game.update_ground_contacts();
    for tick in 100..175 {
        game.tick = tick;
        game.run_object_updates();
        game.process_update_destruction();
    }
    assert_eq!(
        game.presentation
            .shots
            .iter()
            .filter(|shot| shot.source == fire)
            .count(),
        10
    );
    assert_eq!(
        game.entities[victim].health.to_bits(),
        (before - 50.0).to_bits()
    );
    assert_eq!(game.entities[victim].damage_state.last_attacker, Some(fire));
    assert!(!game.entities[fire].masked());
    game.tick = 175;
    game.run_object_updates();
    game.process_update_destruction();
    assert!(game.entities[fire].masked());
    game.tick = 176;
    game.run_object_updates();
    game.process_update_destruction();
    assert_eq!(
        game.presentation
            .shots
            .iter()
            .filter(|shot| shot.source == fire)
            .count(),
        10
    );
}

#[test]
fn field_expiry_keeps_unrelated_ground_routes() {
    let (mut game, _) = crate::skirmish::containment::tests::setup(0.0, 0.0);
    let kind = field(&mut game, "FireFieldSmall");
    let fire = game.spawn(kind, 0, [650.0, 450.0], true);
    let mover = game.spawn(5, 0, [305.0, 105.0], true);
    game.move_to(mover, [455.0, 105.0]).unwrap();
    game.process_path_service();
    let route = game.entities[mover].ai.path.raw.clone();
    let goal = game.entities[mover].ai.path_goal;
    assert!(!route.is_empty());
    assert!(game.entities[mover].ai.ground_path.is_some());
    game.tick = u64::from(game.entities[fire].behaviors.deletion_frames[0]);
    game.run_object_updates();
    game.process_update_destruction();
    assert!(game.entities[fire].masked());
    assert!(game.entities[mover].ai.path.raw == route);
    assert!(game.entities[mover].ai.path_goal == goal);
    assert!(game.entities[mover].ai.ground_path.is_some());
}

#[test]
fn immortal_fire_wall_takes_damage_but_never_drops_below_one_health() {
    let mut game = crate::skirmish::tests::game();
    let kind = field(&mut game, "FireWallSegment");
    game.catalog[kind].initial_health = 50.0;
    game.catalog[kind].max_health = 50.0;
    game.catalog[kind].fire.as_mut().unwrap().body = Body::Immortal;
    game.catalog[kind].body_kind = crate::skirmish::BodyKind::Active;
    game.catalog[kind].minimum_health = 1.0;
    let wall = game.spawn(kind, 0, [250.0, 250.0], true);
    let attacker = game.spawn(5, 1, [250.0, 250.0], true);
    game.apply_typed_hit(wall, attacker, 20.0, DeathType::Normal);
    assert_eq!(game.entities[wall].health.to_bits(), 30.0_f32.to_bits());
    game.apply_typed_hit(wall, attacker, 40.0, DeathType::Normal);
    assert_eq!(game.entities[wall].health.to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        game.entities[wall].damage_state.last_attacker,
        Some(attacker)
    );
    assert!(game.entities[wall].alive());
}

#[test]
fn object_creation_keeps_native_choice_lifetime_then_orientation_rng_and_owner() {
    let mut game = crate::skirmish::tests::game();
    field(&mut game, "FireFieldSmall");
    let creation = Creation {
        names: vec!["FireFieldSmall".into()],
        count: 2,
        disposition: Disposition::GroundAligned,
    };
    let mut random = game.random;
    let mut angles = Vec::new();
    for _ in 0..2 {
        angles.push(startup::random_real(
            &mut random,
            0.0,
            std::f32::consts::TAU,
        ));
    }
    let first = game.entities.len();
    game.create_fire_objects(&creation, 1, [230.0, 260.0, 200.0], 1.0, [0.0; 3]);
    assert_eq!(game.random, random);
    for (id, angle) in (first..game.entities.len()).zip(angles) {
        assert_eq!(game.entities[id].owner, 1);
        // Native Thing::setOrientation caches normalizeAngle, while the OCL
        // consumes its random angle from the full [0, 2PI] distribution.
        let heading = crate::skirmish::locomotor::normalize_angle(angle);
        assert_eq!(game.entities[id].facing.to_bits(), heading.to_bits());
        assert_eq!(
            game.object_position(id).map(f32::to_bits),
            [230.0, 260.0, game.ground_height([230.0, 260.0])].map(f32::to_bits)
        );
    }
}

#[test]
fn dynamic_firestorm_obeys_delay_inclusive_reverse_peak_and_damage_height() {
    let mut game = crate::skirmish::tests::game();
    let kind = field(&mut game, "FirestormSmall");
    game.catalog[kind].weapon = WeaponRules::unarmed();
    game.catalog[kind].fire.as_mut().unwrap().storm = Some(Storm {
        geometry: geometry::Rules {
            delay: 3,
            initial: [10.0, 1.0, 0.0],
            final_size: [10.0, 90.0, 0.0],
            transition: 3,
            reverse: true,
        },
        interval: 1.0,
        damage: 2.0,
        height: 20.0,
        particles: Vec::new(),
        particle_height: 0.1,
        effect: String::new(),
        scorch: 100.0,
    });
    storm_modules(&mut game, kind);
    game.tick = 100;
    let near = game.spawn(5, 1, [260.0, 250.0], true);
    let high = game.spawn(5, 1, [260.0, 250.0], true);
    game.entities[high].flight = Some(Box::new(crate::skirmish::flight::Motion::new(21.0)));
    let fire = game.spawn(kind, 0, [250.0, 250.0], true);
    // This isolated normal-phase test needs the preceding partition update,
    // which populates cells before FirestormDynamicGeometryInfoUpdate scans.
    game.update_ground_contacts();
    let before = game.entities[near].health;
    for tick in 100..102 {
        game.tick = tick;
        game.run_object_updates();
        game.process_update_destruction();
    }
    assert!(game.entities[fire].fire_started().is_none());
    assert_eq!(game.entities[near].health.to_bits(), before.to_bits());
    for tick in 102..=105 {
        game.tick = tick;
        game.run_object_updates();
        game.process_update_destruction();
    }
    assert_eq!(game.entities[fire].fire_started(), Some(102));
    assert_eq!(game.entities[fire].fire_reversed(), Some(105));
    assert_eq!(
        game.entities[fire].fire_radius().unwrap().to_bits(),
        90.0_f32.to_bits()
    );
    assert!(game.entities[near].health < before);
    assert_eq!(game.entities[high].health.to_bits(), before.to_bits());
    game.tick = 106;
    game.run_object_updates();
    game.process_update_destruction();
    assert_eq!(
        game.entities[fire].fire_radius().unwrap().to_bits(),
        90.0_f32.to_bits()
    );
}

#[test]
fn firestorm_keeps_exposed_station_riders_and_native_hive_damage_forwarding() {
    use crate::skirmish::{containment::GarrisonRules, hive_body};

    for (exposed, hive) in [(true, false), (false, false), (false, true)] {
        let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 0.0);
        let host = game.entities[carrier].type_index;
        if exposed {
            game.catalog[host].contain.as_mut().unwrap().garrison = Some(GarrisonRules {
                allow_enemies: false,
                allow_neutral: false,
                immune_to_clear: true,
                heal_frames: None,
                stations: Some(vec![[0.0; 3]]),
            });
        }
        let carrier = if exposed {
            // Virtual enclosure and the constructor belong to GarrisonContain;
            // station data on a TransportContain still encloses its passenger.
            crate::skirmish::containment::tests::respawn_contain_family(
                &mut game,
                carrier,
                "GarrisonContain",
            )
        } else {
            carrier
        };
        if hive {
            game.catalog[host].hive_body = Some(hive_body::Rules {
                propagate: 1 << DamageType::Flame as usize,
                swallow: 0,
            });
        }
        game.catalog[host].armor[DamageType::Flame as usize] = 0.0;
        game.catalog[5].armor[DamageType::Flame as usize] = 0.5;
        let position = game.entities[carrier].position;
        let rider = game.spawn(5, 0, position, true);
        game.board(rider, carrier);
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
        assert_eq!(game.entities[rider].stationed(), exposed);
        assert_eq!(game.container_encloses(carrier, rider), !exposed);
        assert_eq!(game.entities[rider].drawable_effectively_hidden(), !exposed);
        let kind = field(&mut game, "FirestormSmall");
        game.catalog[kind].weapon = WeaponRules::unarmed();
        game.catalog[kind].fire.as_mut().unwrap().storm = Some(Storm {
            geometry: geometry::Rules {
                delay: 0,
                initial: [10.0, 50.0, 0.0],
                final_size: [10.0, 50.0, 0.0],
                transition: 10,
                reverse: true,
            },
            interval: 1.0,
            damage: 10.0,
            height: 20.0,
            particles: Vec::new(),
            particle_height: 0.0,
            effect: String::new(),
            scorch: 0.0,
        });
        storm_modules(&mut game, kind);
        game.tick = 100;
        let fire = game.spawn(kind, 1, position, true);
        game.update_ground_contacts();
        assert_eq!(
            game.firestorm_damage_targets(fire, 50.0).contains(&rider),
            exposed
        );
        let before = game.entities[rider].health;
        let host_before = game.entities[carrier].health;
        game.run_object_updates();
        game.process_update_destruction();
        let expected = if exposed || hive {
            before - 5.0
        } else {
            before
        };
        assert_eq!(game.entities[rider].health.to_bits(), expected.to_bits());
        assert_eq!(
            game.entities[carrier].health.to_bits(),
            host_before.to_bits()
        );
    }
}
