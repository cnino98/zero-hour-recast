use super::*;
use crate::skirmish::{
    BodyKind, DeathType, Faction, Kind, UnitAi, UnitAiUpdate, UnitType, WeaponRules, special_power,
    sticky_bomb, tests,
};

fn add_charge(game: &mut Skirmish, burton: &mut UnitType, charge: BurtonCharge) -> usize {
    let remote = charge == BurtonCharge::Remote;
    let mut child = tests::fixture_type(Faction::Usa, Kind::Effect);
    child.name = if remote {
        "RemoteC4Charge"
    } else {
        "TimedC4Charge"
    }
    .into();
    child.kind_of = vec!["MINE".into(), "PROJECTILE".into(), "NO_COLLIDE".into()];
    child.ai.update = UnitAiUpdate::Absent;
    child.body_kind = BodyKind::Highlander;
    child.weapon = WeaponRules::unarmed();
    child.behavior_modules.clear();
    child.update_modules = tests::fixture_update_helpers(false);
    if !remote {
        child.behavior.lifetimes.push([600; 2]);
        tests::register_update(&mut child, "LifetimeUpdate");
    }
    child.behavior.sticky_bombs.push(sticky_bomb::Rules {
        attach_bone: String::new(),
        offset_z: 10.0,
        geometry_weapon: None,
        geometry_fx: None,
        created_sound: "BurtonPlaced".into(),
        ping_sound: if remote { "RemoteTick" } else { "TimedTick" }.into(),
    });
    tests::register_update(&mut child, "StickyBombUpdate");
    register_charge_power(burton, charge, &child.name);
    let index = game.catalog.len();
    game.catalog.push(child);
    index
}

fn register_charge_power(burton: &mut UnitType, charge: BurtonCharge, child: &str) {
    use crate::skirmish::special_ability::{
        Approach, Effects, Feedback, Objects, Packing, Power, Preparation, Rules, Timings,
    };
    let remote = charge == BurtonCharge::Remote;
    let tag = format!("Power{}", burton.special_powers.len());
    burton.special_powers.push(special_power::Rules {
        tag: tag.clone(),
        name: charge.power().into(),
        power_type: if remote {
            "SPECIAL_REMOTE_CHARGES"
        } else {
            "SPECIAL_TIMED_CHARGES"
        }
        .into(),
        required_science: String::new(),
        timer: special_power::Timer {
            reload_frames: 0,
            shared: false,
            public: false,
        },
        view_range: 0.0,
        view_frames: 0,
        view_template: None,
        initiate_sound: String::new(),
        target_sound: String::new(),
        module_initiate_sound: String::new(),
        academy_classification: "ACT_NONE".into(),
        update_starts_attack: true,
        startup: special_power::Startup {
            paused: false,
            ready: true,
        },
        script_only: false,
    });
    tests::register_behavior(burton, "SpecialAbility", 0x0100);
    burton.behavior_modules.last_mut().unwrap().tag = tag;
    burton.behavior.special_abilities.push(Rules {
        power: Some(Power {
            name: charge.power().into(),
            kind: charge.kind(),
        }),
        approach: Approach {
            start_range: 0.0,
            abort_range: 10_000_000.0,
            requires_los: true,
            face_target: true,
            flee_range: 100.0,
        },
        timing: Timings {
            preparation: 0,
            persistent: 0,
            pack: 0,
            unpack: 165,
            unstealth: 150,
            effect: 0,
            variation: 0.0,
        },
        objects: Objects {
            name: child.into(),
            bone: String::new(),
            maximum: if remote { 8 } else { 10 },
            persistent: true,
            unique_targets: true,
            persist_on_death: !remote,
        },
        packing: Packing {
            skip_no_target: remote,
            flip_after_pack: false,
            flip_after_unpack: true,
        },
        preparation: Preparation {
            always_validate: remote,
            capture_fx: false,
            persistence_recharges: false,
        },
        effects: Effects {
            value: 1,
            experience: 0,
            skill_points: -1,
            lose_stealth: true,
        },
        feedback: Feedback {
            disable_particle: None,
            sounds: [
                String::new(),
                "BurtonPlant".into(),
                String::new(),
                String::new(),
            ],
            completion_voice: String::new(),
            capture_tick: String::new(),
        },
    });
    tests::register_update(burton, "SpecialAbilityUpdate");
}

fn setup() -> (Skirmish, usize, usize, usize) {
    let mut game = tests::game();
    let mut burton = tests::fixture_type(Faction::Usa, Kind::Infantry);
    burton.name = "AmericaInfantryColonelBurton".into();
    burton.ai.acquisition &= !UnitAi::IDLE;
    let remote = add_charge(&mut game, &mut burton, BurtonCharge::Remote);
    let timed = add_charge(&mut game, &mut burton, BurtonCharge::Timed);
    let actor_type = game.catalog.len();
    game.catalog.push(burton);
    game.catalog[6].ai.acquisition &= !UnitAi::IDLE;
    game.catalog[6].kind_of.push("VEHICLE".into());
    let actor = game.spawn(actor_type, 0, [300.0, 300.0], true);
    game.tick = 10;
    (game, actor, remote, timed)
}

fn child(game: &Skirmish, kind: usize, target: usize) -> Option<usize> {
    game.entities.iter().position(|object| {
        object.type_index == kind && object.producer == Some(target) && object.alive()
    })
}

fn plant(
    game: &mut Skirmish,
    actor: usize,
    target: usize,
    charge: BurtonCharge,
    kind: usize,
) -> usize {
    game.set_object_position(actor, [300.0, 300.0, 0.0]);
    game.set_object_position(target, [302.0, 300.0, 0.0]);
    game.update_ground_contacts();
    game.update_visibility();
    game.burton_order_charge(actor, charge, target).unwrap();
    for _ in 0..230 {
        if let Some(object) = child(game, kind, target) {
            return object;
        }
        game.step();
    }
    panic!("Burton never placed the selected charge");
}

#[test]
fn remote_detonation_retires_remote_children_and_keeps_timed_charge() {
    let (mut game, actor, remote, timed) = setup();
    let first = game.spawn(6, 1, [302.0, 300.0], true);
    let second = game.spawn(6, 1, [340.0, 300.0], true);
    let third = game.spawn(6, 1, [380.0, 300.0], true);
    assert!(!game.burton_can_detonate(actor));
    let a = plant(&mut game, actor, first, BurtonCharge::Remote, remote);
    assert!(!game.burton_charge_target(actor, BurtonCharge::Timed, first));
    assert!(!game.burton_charge_target(actor, BurtonCharge::Remote, first));
    game.set_object_position(first, [270.0, 280.0, 0.0]);
    let b = plant(&mut game, actor, second, BurtonCharge::Remote, remote);
    game.set_object_position(second, [285.0, 280.0, 0.0]);
    let c = plant(&mut game, actor, third, BurtonCharge::Timed, timed);
    assert!(game.burton_can_detonate(actor));
    game.burton_detonate(actor).unwrap();
    for _ in 0..5 {
        game.step();
    }
    assert!(!game.entities[a].alive());
    assert!(!game.entities[b].alive());
    assert!(game.entities[c].alive());
}

#[test]
fn stopping_placement_and_killing_targets_remove_charges_without_detonating() {
    let (mut game, actor, remote, _) = setup();
    let victim = game.spawn(6, 1, [302.0, 300.0], true);
    game.update_ground_contacts();
    game.update_visibility();
    game.burton_order_charge(actor, BurtonCharge::Remote, victim)
        .unwrap();
    for _ in 0..20 {
        game.step();
    }
    game.stop(actor).unwrap();
    for _ in 0..170 {
        game.step();
    }
    assert!(child(&game, remote, victim).is_none());
    let charge = plant(&mut game, actor, victim, BurtonCharge::Remote, remote);
    game.kill_object(victim, DeathType::Normal);
    for _ in 0..5 {
        game.step();
    }
    assert!(!game.entities[charge].exists());
    assert!(!game.burton_can_detonate(actor));
}

#[test]
fn burton_death_retires_remote_children_and_leaves_timed_lifetime_running() {
    let (mut game, actor, remote, timed) = setup();
    let first = game.spawn(6, 1, [302.0, 300.0], true);
    let second = game.spawn(6, 1, [350.0, 300.0], true);
    let a = plant(&mut game, actor, first, BurtonCharge::Remote, remote);
    game.set_object_position(first, [270.0, 280.0, 0.0]);
    let b = plant(&mut game, actor, second, BurtonCharge::Timed, timed);
    game.kill_object(actor, DeathType::Normal);
    for _ in 0..4 {
        game.step();
    }
    assert!(!game.entities[a].exists());
    assert!(game.entities[b].alive());
    for _ in 0..600 {
        game.step();
    }
    assert!(!game.entities[b].alive());
}
