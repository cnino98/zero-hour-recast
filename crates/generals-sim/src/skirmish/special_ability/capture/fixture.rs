use super::super::{
    Approach, Effects, Feedback, Objects, Packing, Power, PowerType, Preparation, Rules, Timings,
};
use crate::skirmish::{Skirmish, UnitType, special_power, tests};

pub(in crate::skirmish) fn bind(
    unit: &mut UnitType,
    kind: PowerType,
    frames: [u32; 4],
    paused: bool,
) {
    if unit.locomotor.ground.is_none() {
        unit.locomotor.ground = Some(crate::skirmish::GroundRules {
            wander: None,
            appearance: crate::skirmish::GroundAppearance::Legs,
            is_dozer: false,
            downhill_only: false,
            acceleration: 100.0 / 900.0,
            acceleration_damaged: 50.0 / 900.0,
            braking: 100.0 / 900.0,
            min_speed: 0.0,
            close_enough: 1.0,
            mass: 5.0,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            extra_friction: 0.0,
        });
    }
    let name = bind_power(unit, kind, frames[3], paused);
    unit.behavior.special_abilities.push(Rules {
        power: Some(Power {
            name: name.into(),
            kind,
        }),
        approach: Approach {
            start_range: if paused { 5.0 } else { 150.0 },
            abort_range: 10_000_000.0,
            requires_los: true,
            face_target: true,
            flee_range: 0.0,
        },
        timing: Timings {
            unpack: frames[0],
            preparation: frames[1],
            pack: frames[2],
            persistent: 0,
            unstealth: 0,
            effect: 450,
            variation: 0.0,
        },
        objects: Objects {
            name: if paused {
                String::new()
            } else {
                "BinaryDataStream".into()
            },
            bone: "Muzzle01".into(),
            maximum: 1,
            persistent: false,
            unique_targets: false,
            persist_on_death: false,
        },
        packing: Packing {
            skip_no_target: false,
            flip_after_pack: false,
            flip_after_unpack: false,
        },
        preparation: Preparation {
            always_validate: false,
            capture_fx: kind != PowerType::SpecialBlacklotusDisableVehicleHack,
            persistence_recharges: false,
        },
        effects: Effects {
            value: 1,
            experience: if paused || kind == PowerType::SpecialBlacklotusDisableVehicleHack {
                0
            } else {
                20
            },
            skill_points: -1,
            lose_stealth: false,
        },
        feedback: Feedback {
            disable_particle: Some("DisabledEffectBinaryShower0".into()),
            sounds: std::array::from_fn(|_| String::new()),
            completion_voice: String::new(),
            capture_tick: String::new(),
        },
    });
    tests::register_update(unit, "SpecialAbilityUpdate");
    if paused {
        unit.unpause_special_powers.push(Some(name.into()));
        tests::register_behavior(unit, "UnpauseSpecialPowerUpgrade", 0x80);
        unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Capture".into()];
    }
}

pub(in crate::skirmish) fn stream(game: &mut Skirmish) {
    let mut unit = tests::fixture_type(
        crate::skirmish::Faction::Civilian,
        crate::skirmish::Kind::Effect,
    );
    unit.name = "BinaryDataStream".into();
    unit.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    unit.body_kind = crate::skirmish::BodyKind::Inactive;
    unit.behavior_modules.clear();
    unit.update_modules.clear();
    unit.laser_update = Some(crate::skirmish::laser_update::Rules {
        particles: [String::new(), String::new()],
        punch_through: 0.0,
    });
    game.catalog.push(unit);
}

fn bind_power(
    unit: &mut UnitType,
    kind: PowerType,
    reload_frames: u32,
    paused: bool,
) -> &'static str {
    let index = unit.behavior.special_abilities.len();
    let name = match kind {
        PowerType::SpecialInfantryCaptureBuilding => "InfantryCapture",
        PowerType::SpecialBlacklotusCaptureBuilding => "LotusCapture",
        PowerType::SpecialBlacklotusDisableVehicleHack => "LotusVehicle",
        PowerType::SpecialBlacklotusStealCashHack => "LotusCash",
        _ => unreachable!(),
    };
    let native_kind = match kind {
        PowerType::SpecialInfantryCaptureBuilding => "SPECIAL_INFANTRY_CAPTURE_BUILDING",
        PowerType::SpecialBlacklotusCaptureBuilding => "SPECIAL_BLACKLOTUS_CAPTURE_BUILDING",
        PowerType::SpecialBlacklotusDisableVehicleHack => "SPECIAL_BLACKLOTUS_DISABLE_VEHICLE_HACK",
        PowerType::SpecialBlacklotusStealCashHack => "SPECIAL_BLACKLOTUS_STEAL_CASH_HACK",
        _ => unreachable!(),
    };
    let tag = format!("CapturePower{index}");
    unit.special_powers.push(special_power::Rules {
        tag: tag.clone(),
        name: name.into(),
        power_type: native_kind.into(),
        required_science: String::new(),
        timer: special_power::Timer {
            reload_frames,
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
            paused,
            ready: false,
        },
        script_only: false,
    });
    tests::register_behavior(unit, "SpecialAbility", 0x0100);
    unit.behavior_modules.last_mut().unwrap().tag = tag;
    name
}
