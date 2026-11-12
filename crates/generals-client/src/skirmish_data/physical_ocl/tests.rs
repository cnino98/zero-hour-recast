use super::*;
use generals_assets::{content, virtual_assets::Asset};

pub fn installed_outpost(
    assets: &generals_assets::virtual_assets::VirtualAssets,
) -> (
    generals_sim::skirmish::UnitType,
    generals_sim::skirmish::UnitType,
    List,
) {
    use generals_sim::skirmish::{Faction, Kind};
    let files = super::super::source_files(assets).unwrap();
    let debris = super::super::unit_type(
        &files,
        definition(&files, "Object", "GenericDebris").unwrap(),
        Faction::China,
        Kind::Effect,
    )
    .unwrap();
    let outpost = super::super::unit_type(
        &files,
        definition(&files, "Object", "ChinaVehicleListeningOutpost").unwrap(),
        Faction::China,
        Kind::Vehicle,
    )
    .unwrap();
    let list = load(&files, "OCL_InitialListeningOutpostDebris").unwrap();
    (debris, outpost, list)
}

#[test]
fn creation_nuggets_keep_cross_kind_order_native_rates_and_signed_offsets() {
    let file = content::scan(Asset { path: "ocl.ini".into(), origin: "native physical OCL parser fixture".into(), bytes: b"ObjectCreationList Death\nCreateDebris\nModelNames=AVRaptor_D4\nMass=3\nOffset=X:-5.598 Y:8.188 Z:0.04\nDisposition=SEND_IT_FLYING INHERIT_VELOCITY RANDOM_FORCE\nSpinRate=20\nEnd\nCreateObject\nObjectNames=AmericaJetRaptorHulk\nIgnorePrimaryObstacle=Yes\nDisposition=LIKE_EXISTING\nEnd\nCreateDebris\nModelNames=AVRaptor_D2 AVRaptor_D3\nMass=3\nCount=2\nEnd\nEnd\n".to_vec() }).unwrap();
    let list = load(&[file], "Death").unwrap();
    assert_eq!(list.nuggets.len(), 3);
    assert_eq!(list.nuggets[1].names, ["AmericaJetRaptorHulk"]);
    assert!(list.nuggets[1].debris_mass.is_none());
    assert!(list.nuggets[1].ignore_primary_obstacle);
    assert!(!list.nuggets[0].ignore_primary_obstacle);
    assert_eq!(list.nuggets[2].count, 2);
    let first = &list.nuggets[0];
    assert_eq!(
        first.offset.map(f32::to_bits),
        [-5.598_f32, 8.188, 0.04].map(f32::to_bits)
    );
    assert_eq!(
        first.spin_rate.unwrap().to_bits(),
        (20.0_f32.to_radians() * (1.0 / 30.0)).to_bits()
    );
    assert_eq!(
        first.disposition,
        physical_ocl::SEND_FLYING | physical_ocl::INHERIT_VELOCITY | physical_ocl::RANDOM_FORCE
    );
}

#[test]
fn unsupported_containment_is_not_silently_admitted_as_ordinary_debris() {
    let file = content::scan(Asset { path: "ocl.ini".into(), origin: "native container dependency rejection".into(), bytes: b"ObjectCreationList Pilot\nCreateObject\nObjectNames=AmericaInfantryPilot\nPutInContainer=AmericaParachute\nEnd\nEnd\n".to_vec() }).unwrap();
    assert!(
        load(&[file], "Pilot").is_err_and(|error| error.to_string().contains("PutInContainer"))
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_aurora_hulk_inherits_default_body_and_real_physical_retirement() {
    use generals_assets::virtual_assets::VirtualAssets;
    use generals_sim::skirmish::{Faction, Kind};
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = super::super::source_files(&assets).unwrap();
    let object = definition(&files, "Object", "AmericaJetAuroraHulk").unwrap();
    assert!(blocks(&object.lines, 0, "Body").unwrap().is_empty());
    let unit = super::super::unit_type(&files, object, Faction::Usa, Kind::Effect).unwrap();
    let unit = super::super::finish_type(&files, object, unit).unwrap();
    let rules = unit.physical.as_ref().unwrap();
    assert!(!rules.active_body);
    assert_eq!(rules.motion.mass.to_bits(), 2.0_f32.to_bits());
    assert!(!rules.motion.allow_bouncing);
    assert!(rules.kill_when_resting);
    assert_eq!(unit.behavior.lifetimes, [[45, 48]]);
    assert!(unit.no_collide());
    let Death::Slow(death) = &rules.death else {
        panic!("Aurora hulk lacks authored SlowDeathBehavior");
    };
    assert_eq!(death.sink_delay, [45, 0]);
    assert_eq!(death.destruction_delay, [240, 0]);
    assert_eq!(
        death.sink_rate.to_bits(),
        (2.0_f32 * (1.0 / 30.0)).to_bits()
    );
    let list = load(&files, "OCL_AuroraDeathFinalBlowUp").unwrap();
    assert_eq!(list.nuggets.len(), 1);
    let hulk = &list.nuggets[0];
    assert_eq!(hulk.names, ["AmericaJetAuroraHulk"]);
    assert!(hulk.debris_mass.is_none());
    assert_eq!(
        hulk.disposition,
        physical_ocl::SEND_FLYING | physical_ocl::INHERIT_VELOCITY | physical_ocl::RANDOM_FORCE
    );
}

#[ignore = "requires original game installation"]
#[test]
fn original_physical_templates_bind_each_authored_update_once() {
    use generals_assets::virtual_assets::VirtualAssets;
    use generals_sim::skirmish::{Faction, Kind};
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let (files, metadata) = super::super::source_files::effective(&assets, &OBJECTS).unwrap();
    let repulsors = super::super::module_repulsors_enabled(&assets).unwrap();
    for name in OBJECTS {
        let object = definition(&files, "Object", name).unwrap();
        let unit = super::super::unit_type(&files, object, Faction::Usa, Kind::Effect).unwrap();
        let mut unit = super::super::finish_type(&files, object, unit).unwrap();
        unit.update_modules = metadata[name].runtime_registrations(repulsors);
        unit.behavior_modules = metadata[name].runtime_behaviors(&files).unwrap();
        generals_sim::skirmish::module_updates::validate_registrations(&unit).unwrap();
        assert!(
            unit.physical.is_some(),
            "{name} lost its physical lifecycle"
        );
        // The physical body and authored SlowDeath data coexist. The shared
        // scheduler owns the actual module; the legacy body loop is guarded.
        for (family, expected) in [
            ("PhysicsBehavior", 1),
            ("LifetimeUpdate", unit.behavior.lifetimes.len()),
            (
                "SlowDeathBehavior",
                unit.death_lifecycle
                    .as_ref()
                    .map_or(0, |rules| rules.slow.len()),
            ),
        ] {
            let registrations: Vec<_> = unit
                .update_modules
                .iter()
                .filter_map(|module| match module {
                    generals_sim::skirmish::module_updates::ModuleRegistration::Behavior {
                        family: actual,
                        tag,
                        ..
                    } if actual == family => Some(tag),
                    _ => None,
                })
                .collect();
            assert_eq!(registrations.len(), expected, "{name} {family}");
            assert_eq!(
                registrations
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                expected,
                "{name} duplicated {family}"
            );
        }
        if name == "GenericDebris" {
            assert_eq!(unit.max_health.to_bits(), 0.0_f32.to_bits());
            assert!(!unit.physical.as_ref().unwrap().active_body);
            let Death::Slow(death) = &unit.physical.as_ref().unwrap().death else {
                panic!("GenericDebris native SlowDeath");
            };
            assert_eq!(death.sink_delay, [30, 0]);
            assert_eq!(death.destruction_delay, [60, 0]);
        }
    }
    let barrel = definition(&files, "Object", "Barrel").unwrap();
    let rules = super::super::death_lifecycle::load(&files, barrel)
        .unwrap()
        .unwrap();
    assert_eq!(rules.slow.len(), 1);
    assert!(
        matches!(&rules.slow[0].objects[2][0], generals_sim::skirmish::death_lifecycle::Spawn::Physical(list) if list.name == "OCL_CreateDamagedBarrel")
    );
}

fn physical_fixture(body: &str) -> Vec<ContentFile> {
    vec![content::scan(Asset {
        path: "physical.ini".into(), origin: "physical callback ownership fixture".into(),
        bytes: format!("GameData\nGravity=-1\nEnd\nObject GenericDebris\nBehavior=PhysicsBehavior Physics\nVehicleCrashesIntoBuildingWeaponTemplate=NONE\nVehicleCrashesIntoNonBuildingWeaponTemplate=NONE\nEnd\n{body}End\nFXList NativeInitial\nEnd\n").into_bytes(),
    }).unwrap()]
}

#[test]
fn physical_owner_retains_exact_single_native_slow_and_rejects_other_die_mixes() {
    for extra in [
        "Behavior=FXListDie Extra\nDeathFX=Extra\nEnd\n",
        "Behavior=SlowDeathBehavior Second\nDestructionDelay=1000\nEnd\n",
        "Behavior=InstantDeathBehavior Extra\nEnd\n",
    ] {
        let files = physical_fixture(&format!(
            "Behavior=SlowDeathBehavior Death\nSinkDelay=1000\nDestructionDelay=2000\nEnd\n{extra}"
        ));
        let object = definition(&files, "Object", "GenericDebris").unwrap();
        let lifecycle = super::super::death_lifecycle::load(&files, object)
            .unwrap()
            .unwrap();
        assert!(rules(&files, object, Some(&lifecycle)).is_err());
    }
}

#[test]
fn physical_owner_uses_real_parsed_slow_selector_fling_and_initial_fx() {
    let files = physical_fixture(
        "Behavior=SlowDeathBehavior Death\nSinkDelay=1000\nDestructionDelay=2000\nSinkRate=2\nModifierBonusPerOverkillPercent=10%\nExemptStatus=UNDER_CONSTRUCTION\nDeathTypes=NONE +NORMAL\nFlingForce=10\nFlingPitch=20\nFX=INITIAL NativeInitial\nEnd\n",
    );
    let object = definition(&files, "Object", "GenericDebris").unwrap();
    let mut lifecycle = super::super::death_lifecycle::load(&files, object)
        .unwrap()
        .unwrap();
    let parsed = &lifecycle.slow[0];
    assert_eq!(parsed.overkill_bonus.to_bits(), (10.0_f32 * 0.01).to_bits());
    assert_ne!(parsed.conditions.exempt_status, 0);
    assert_eq!(
        parsed.conditions.death_mask,
        1 << generals_sim::skirmish::DeathType::Normal as u32
    );
    assert_eq!(
        parsed.fling.as_ref().unwrap().force[0].to_bits(),
        10.0_f32.to_bits()
    );
    assert_eq!(
        parsed.fling.as_ref().unwrap().pitch[0].to_bits(),
        20.0_f32.to_radians().to_bits()
    );
    assert_eq!(parsed.fx[0], ["NativeInitial"]);
    let actual = rules(&files, object, Some(&lifecycle)).unwrap().unwrap();
    let Death::Slow(actual) = actual.death else {
        panic!("actual SlowDeath binding");
    };
    assert_eq!(actual.sink_delay, parsed.sink_delay);
    assert_eq!(actual.destruction_delay, parsed.destruction_delay);
    assert_eq!(actual.sink_rate.to_bits(), parsed.sink_rate.to_bits());
    assert!(rules(&files, object, None).is_err());
    lifecycle.slow[0].tag = "DifferentInstance".into();
    assert!(rules(&files, object, Some(&lifecycle)).is_err());
}

#[ignore = "requires original game installation"]
#[test]
fn original_technical_chassis_keep_actual_force_and_debris_animation_setup() {
    use generals_sim::skirmish::{Faction, Kind};
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let files = super::super::source_files(&assets).unwrap();
    for name in [
        "GLAVehicleTechnicalChassisOne",
        "GLAVehicleTechnicalChassisTwo",
        "GLAVehicleTechnicalChassisThree",
    ] {
        let unit = super::super::unit_type(
            &files,
            definition(&files, "Object", name).unwrap(),
            Faction::Gla,
            Kind::Vehicle,
        )
        .unwrap();
        let list = generals_sim::skirmish::physical_ocl::lists(&unit)
            .find(|list| list.name == "OCL_TechnicalAirDeathStart")
            .unwrap();
        let force = list.initial_force.as_ref().unwrap();
        assert_eq!(
            force.magnitude.map(f32::to_bits),
            [60.0_f32, 100.0].map(f32::to_bits)
        );
        let draw = &list.nuggets[0];
        assert_eq!(draw.names, ["UITech_Man_SKN"]);
        assert_eq!(draw.draw.animations.len(), 3);
        assert_eq!(
            draw.draw.animations[0],
            [
                "UITech_Man_SKL.UITech_Man_DTA1",
                "UITech_Man_SKL.UITech_Man_DTA2",
                "UITech_Man_SKL.UITech_Man_DTA3"
            ]
        );
        assert_eq!(draw.draw.final_fx, "FX_TechnicalGunnerHitsGround");
        assert!(draw.draw.keep_color && draw.orient_in_force_direction);
    }
}

#[test]
fn physical_list_rejects_repeated_or_late_source_force_without_reordering() {
    for body in [
        "ApplyRandomForce\nEnd\nApplyRandomForce\nEnd",
        "CreateDebris\nModelNames=UITech_Man_SKN\nMass=5\nEnd\nApplyRandomForce\nEnd",
    ] {
        let file = content::scan(Asset {
            path: "ocl.ini".into(),
            origin: "source OCL declaration-order boundary".into(),
            bytes: format!("ObjectCreationList Test\n{body}\nEnd\n").into_bytes(),
        })
        .unwrap();
        assert!(load(&[file], "Test").is_err());
    }
}

pub fn technical_list(assets: &generals_assets::virtual_assets::VirtualAssets) -> List {
    let files = super::super::source_files(assets).unwrap();
    load(&files, "OCL_TechnicalAirDeathStart").unwrap()
}
