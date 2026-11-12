use super::*;
use generals_assets::{content, virtual_assets::Asset};

#[ignore = "requires original game installation"]
#[test]
fn installed_barrel_final_death_keeps_original_debris_explosion_and_weapon() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = crate::skirmish_data::source_files(&assets).unwrap();
    let object = definition(&files, "Object", "Barrel").unwrap();
    let rules = load(&files, object).unwrap().unwrap();
    assert_eq!(rules.slow.len(), 1);
    let death = &rules.slow[0];
    assert_eq!(death.destruction_delay, [6, 9]);
    assert_eq!(death.fx[2], ["FX_BarrelDeathExplosion"]);
    assert_eq!(death.weapons[2].len(), 1);
    assert_eq!(rules.weapons[death.weapons[2][0]].name, "BarrelWeapon");
    assert_eq!(death.objects[2].len(), 1);
    let Spawn::Physical(list) = &death.objects[2][0] else {
        panic!("original Barrel creates physical debris, not a toxin corpse");
    };
    assert_eq!(list.name, "OCL_CreateDamagedBarrel");
    assert_eq!(list.nuggets.len(), 1);
    let debris = &list.nuggets[0];
    assert_eq!(debris.names, ["PMBarrel01_D1"]);
    assert_eq!(debris.count, 1);
    assert_eq!(debris.debris_mass.unwrap().to_bits(), 2.0_f32.to_bits());
    assert_eq!(debris.offset.map(f32::to_bits), [0.0_f32.to_bits(); 3]);
    assert_eq!(debris.particle, "BlackTrail");
    assert_eq!(
        debris.force_magnitude.map(f32::to_bits),
        [5.0_f32, 7.0].map(f32::to_bits)
    );
    assert!(
        !assets
            .read("art/w3d/PMBarrel01_D1.w3d")
            .unwrap()
            .bytes
            .is_empty()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_humvee_death_keeps_initial_hull_final_debris_and_one_frame_destruction() {
    use crate::skirmish_data::{finish_type, normal_model, source_files, unit_type};
    use generals_assets::virtual_assets::VirtualAssets;
    use generals_sim::skirmish::{Faction, Kind, physical_ocl::Death};
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = source_files(&assets).unwrap();
    let object = definition(&files, "Object", "AmericaVehicleHumvee").unwrap();
    let unit = unit_type(&files, object, Faction::Usa, Kind::Vehicle)
        .and_then(|unit| finish_type(&files, object, unit))
        .unwrap();
    let rules = unit.death_lifecycle.as_ref().unwrap();
    assert_eq!(rules.slow.len(), 1);
    let death = &rules.slow[0];
    assert_eq!(death.probability, 25);
    assert_eq!(death.destruction_delay, [1, 0]);
    assert_eq!(death.fx[2], ["FX_BattleMasterExplosionOneFinal"]);
    assert_eq!(death.objects[0].len(), 1);
    let Spawn::Physical(initial) = &death.objects[0][0] else {
        panic!("original initial Humvee physical OCL");
    };
    assert_eq!(initial.nuggets.len(), 1);
    assert_eq!(initial.nuggets[0].names, ["AmericaVehicleHumveeDeadHull"]);
    assert_eq!(initial.nuggets[0].count, 1);
    assert_eq!(death.objects[2].len(), 1);
    let Spawn::Physical(final_debris) = &death.objects[2][0] else {
        panic!("original final Humvee physical OCL");
    };
    assert_eq!(final_debris.nuggets.len(), 5);
    assert_eq!(
        final_debris
            .nuggets
            .iter()
            .map(|nugget| nugget.count)
            .sum::<u32>(),
        8
    );
    for nugget in &final_debris.nuggets {
        assert!(nugget.debris_mass.is_some());
        for name in &nugget.names {
            let asset = assets.read(&format!("art/w3d/{name}.w3d")).unwrap();
            assert!(!asset.bytes.is_empty());
        }
    }
    let hull = definition(&files, "Object", "AmericaVehicleHumveeDeadHull").unwrap();
    let hull_type = unit_type(&files, hull, Faction::Usa, Kind::Effect)
        .and_then(|unit| finish_type(&files, hull, unit))
        .unwrap();
    let physical = hull_type.physical.as_ref().unwrap();
    assert!(physical.active_body && physical.motion.allow_bouncing);
    assert_eq!(hull_type.behavior.lifetimes, [[0, 0]]);
    let Death::Slow(hull_death) = &physical.death else {
        panic!("original sinking Humvee hull");
    };
    assert_eq!(hull_death.sink_delay, [420, 0]);
    assert_eq!(hull_death.destruction_delay, [600, 0]);
    assert!(!normal_model(&assets, hull).unwrap().meshes.is_empty());
}

fn source(text: &str) -> ContentFile {
    content::scan(Asset {
        path: "death.ini".into(),
        origin: "native death fixture".into(),
        bytes: text.as_bytes().to_vec(),
    })
    .unwrap()
}

#[test]
fn original_defaults_and_overrides_keep_distinct_frame_units() {
    let file = source(
        "GameData\nGravity=-900\nGroundStiffness=0.6\nEnd\nObject GLAInfantryTerrorist\nBehavior=PhysicsBehavior Physics\nMass=5\nEnd\nBehavior=SlowDeathBehavior Death\nDeathTypes=NONE +SUICIDED\nSinkDelay=3000\nSinkRate=0.5\nDestructionDelay=8000\nFlingForce=8\nFlingForceVariance=3\nFlingPitch=60\nFlingPitchVariance=10\nFX=INITIAL FX_TerroristExplode\nEnd\nEnd\n",
    );
    let rules = load(std::slice::from_ref(&file), &file.declarations[1])
        .unwrap()
        .unwrap();
    let gravity = -900.0 * (FRAME_SECONDS * FRAME_SECONDS);
    assert_eq!(
        rules.physics.as_ref().unwrap().gravity.to_bits(),
        gravity.to_bits()
    );
    assert_eq!(
        rules.physics.as_ref().unwrap().forward.to_bits(),
        0.15_f32.to_bits()
    );
    assert_eq!(
        rules.physics.as_ref().unwrap().stiffness.to_bits(),
        0.6_f32.to_bits()
    );
    assert_eq!(rules.slow[0].probability, 10);
    assert_eq!(rules.slow[0].sink_delay, [90, 0]);
    assert_eq!(rules.slow[0].destruction_delay, [240, 0]);
    assert_eq!(
        rules.slow[0].sink_rate.to_bits(),
        (0.5 * FRAME_SECONDS).to_bits()
    );
    assert_eq!(
        rules.slow[0]
            .fling
            .as_ref()
            .unwrap()
            .force
            .map(f32::to_bits),
        [8.0, 3.0].map(f32::to_bits)
    );
}

#[test]
fn original_toxin_ocl_retains_disposition_and_airborne_guard() {
    let file = source(
        "GameData\nEnd\nObject GLAInfantryTerrorist\nBehavior=PhysicsBehavior Physics\nMass=5\nEnd\nBehavior=SlowDeathBehavior Death\nDeathTypes=NONE +POISONED\nOCL=INITIAL OCL_ToxicInfantry_TerroristOnly\nEnd\nEnd\nObjectCreationList OCL_ToxicInfantry_TerroristOnly\nCreateObject\nObjectNames=ToxicInfantry_TerroristOnly\nDisposition=LIKE_EXISTING\nSkipIfSignificantlyAirborne=Yes\nEnd\nEnd\n",
    );
    let rules = load(std::slice::from_ref(&file), &file.declarations[1])
        .unwrap()
        .unwrap();
    assert_eq!(
        rules.physics.as_ref().unwrap().gravity.to_bits(),
        (-1.0_f32).to_bits()
    );
    let Spawn::Object {
        name,
        skip_airborne,
    } = &rules.slow[0].objects[0][0]
    else {
        panic!("native toxic infantry spawn");
    };
    assert_eq!(name, "ToxicInfantry_TerroristOnly");
    assert!(skip_airborne);
}

#[test]
fn missing_phase_weapon_keeps_native_nullable_choice() {
    let file = source(
        "GameData\nEnd\nObject GLAInfantryTerrorist\nBehavior=PhysicsBehavior Physics\nEnd\nBehavior=SlowDeathBehavior Death\nWeapon=INITIAL Unknown\nEnd\nEnd\n",
    );
    let rules = load(std::slice::from_ref(&file), &file.declarations[1])
        .unwrap()
        .unwrap();
    assert_eq!(rules.slow[0].weapons[0], [usize::MAX]);
    assert!(rules.weapons.is_empty());
}

#[test]
fn every_authored_death_branch_remains_in_order_including_nullable_gamma_ocl() {
    let file = source(
        "GameData\nEnd\nObject GLAInfantryTerrorist\nBehavior=PhysicsBehavior Physics\nMass=5\nEnd\nBehavior=SlowDeathBehavior Normal\nDeathTypes=NONE +POISONED\nFX=INITIAL FX_DieByToxinGLA\nEnd\nBehavior=SlowDeathBehavior Beta\nDeathTypes=NONE +POISONED_BETA\nFX=INITIAL FX_DieByToxinGLA\nEnd\nBehavior=SlowDeathBehavior Gamma\nDeathTypes=NONE +POISONED_GAMMA\nOCL=INITIAL OCL_ToxicInfantryGamma_TerroristOnly\nEnd\nEnd\n",
    );
    let rules = load(std::slice::from_ref(&file), &file.declarations[1])
        .unwrap()
        .unwrap();
    assert_eq!(rules.slow.len(), 3);
    assert_eq!(
        rules.slow[0].conditions.death_mask,
        1 << generals_sim::skirmish::DeathType::Poisoned as u32
    );
    assert_eq!(
        rules.slow[1].conditions.death_mask,
        1 << generals_sim::skirmish::DeathType::PoisonedBeta as u32
    );
    assert_eq!(
        rules.slow[2].conditions.death_mask,
        1 << generals_sim::skirmish::DeathType::PoisonedGamma as u32
    );
    assert!(matches!(rules.slow[2].objects[0][0], Spawn::None));
}

#[test]
fn corpse_physics_loads_friction_options_while_registered_ai_owns_dead_movement() {
    for (options, expected) in [
        ("", Some((false, false))),
        ("StickToGround=Yes\n", Some((true, false))),
        ("Apply2DFrictionWhenAirborne=Yes\n", Some((false, true))),
        ("LocomotorWorksWhenDead=Yes\n", Some((false, false))),
    ] {
        let file = source(&format!(
            "GameData\nEnd\nLocomotor Original\n{options}End\nObject ChinaVehicleListeningOutpost\nLocomotor=SET_NORMAL Original\nBehavior=PhysicsBehavior Physics\nMass=50\nEnd\nBehavior=SlowDeathBehavior Death\nDestructionDelay=1789\nEnd\nEnd\n"
        ));
        let result = load(std::slice::from_ref(&file), &file.declarations[2]);
        if let Some((stick, airborne)) = expected {
            let physics = result.unwrap().unwrap().physics.unwrap();
            assert_eq!(physics.stick_to_ground, stick);
            assert_eq!(physics.airborne_friction, airborne);
        } else {
            assert!(
                result.is_err_and(|error| error.to_string().contains("working ground locomotor"))
            );
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_bus_final_callback_keeps_hulk_and_all_three_flying_debris() {
    use crate::skirmish_data::{finish_type, normal_model, source_files, unit_type};
    use generals_assets::virtual_assets::VirtualAssets;
    use generals_sim::skirmish::{Faction, Kind, physical_ocl::Death};
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = source_files(&assets).unwrap();
    let object = definition(&files, "Object", "GLAVehicleBattleBus").unwrap();
    let rules = load(&files, object).unwrap().unwrap();
    let final_objects = &rules.slow[0].objects[2];
    assert_eq!(final_objects.len(), 1);
    let Spawn::Physical(list) = &final_objects[0] else {
        panic!("original final Bus physical OCL");
    };
    assert_eq!(list.nuggets.len(), 4);
    let hulk = &list.nuggets[0];
    assert_eq!(hulk.names, ["DeadBattleBusHulk"]);
    assert_eq!(
        hulk.offset.map(f32::to_bits),
        [-7.962_f32, 0.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(
        hulk.disposition,
        generals_sim::skirmish::physical_ocl::LIKE_EXISTING
    );
    for (nugget, name) in
        list.nuggets[1..]
            .iter()
            .zip(["UVBattBus_D2", "UVBattBus_D3", "UVBattBus_D4"])
    {
        assert_eq!(nugget.names, [name]);
        assert_eq!(nugget.debris_mass.unwrap().to_bits(), 10.0_f32.to_bits());
        assert_eq!(
            nugget.disposition,
            generals_sim::skirmish::physical_ocl::SEND_FLYING
        );
        assert!(
            !assets
                .read(&format!("art/w3d/{name}.w3d"))
                .unwrap()
                .bytes
                .is_empty()
        );
    }
    let object = definition(&files, "Object", "DeadBattleBusHulk").unwrap();
    let unit = unit_type(&files, object, Faction::Gla, Kind::Effect)
        .and_then(|unit| finish_type(&files, object, unit))
        .unwrap();
    let physical = unit.physical.as_ref().unwrap();
    assert!(!physical.active_body);
    assert!(physical.kill_when_resting && physical.motion.allow_bouncing);
    assert_eq!(physical.motion.mass.to_bits(), 100.0_f32.to_bits());
    assert_eq!(unit.behavior.lifetimes, [[45, 48]]);
    let Death::Slow(death) = &physical.death else {
        panic!("original sinking Bus hulk")
    };
    assert_eq!(death.sink_delay, [45, 0]);
    assert_eq!(death.destruction_delay, [360, 0]);
    assert!(!normal_model(&assets, object).unwrap().meshes.is_empty());
}

#[ignore = "requires original game installation"]
#[test]
fn installed_bus_crushed_death_retains_original_crusader_hulk_dependency() {
    use crate::skirmish_data::{finish_type, normal_model, source_files, unit_type};
    use generals_assets::virtual_assets::VirtualAssets;
    use generals_sim::skirmish::{
        DeathType, Faction, Kind,
        physical_ocl::{Death, SEND_FLYING},
    };
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let files = source_files(&assets).unwrap();
    let bus = definition(&files, "Object", "GLAVehicleBattleBus").unwrap();
    let rules = super::super::create_object_die::load(&files, bus).unwrap();
    let mask = (1 << DeathType::Crushed as u32) | (1 << DeathType::Splatted as u32);
    let rule = rules
        .iter()
        .find(|rule| rule.conditions.death_mask == mask)
        .expect("original Bus CreateObjectDie crushed/splatted callback");
    let generals_sim::skirmish::create_object_die::Action::Objects(list) = &rule.actions[0] else {
        panic!("actual generic object nugget")
    };
    assert_eq!(list.objects.len(), 1);
    let hulk = &list.objects[0];
    assert_eq!(hulk.names, ["DeadCrusaderHulk"]);
    assert_eq!(hulk.disposition, SEND_FLYING);
    assert_eq!(hulk.force_intensity.to_bits(), 0.1_f32.to_bits());
    assert_eq!(
        hulk.offset.map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    let object = definition(&files, "Object", "DeadCrusaderHulk").unwrap();
    let unit = unit_type(&files, object, Faction::Usa, Kind::Effect)
        .and_then(|unit| finish_type(&files, object, unit))
        .unwrap();
    let physical = unit.physical.as_ref().unwrap();
    assert!(!physical.active_body);
    assert!(physical.kill_when_resting && physical.motion.allow_bouncing);
    assert_eq!(physical.motion.mass.to_bits(), 100.0_f32.to_bits());
    assert_eq!(unit.behavior.lifetimes, [[45, 48]]);
    let Death::Slow(death) = &physical.death else {
        panic!("source crushed Crusader hulk sinking")
    };
    assert_eq!(death.sink_delay, [45, 0]);
    assert_eq!(death.destruction_delay, [240, 0]);
    assert!(!normal_model(&assets, object).unwrap().meshes.is_empty());
}

#[test]
fn reconciled_default_destroy_removal_and_replacement_reach_real_death_consumer() {
    let baseline_text = "Object DefaultThingTemplate\nBehavior=DestroyDie ModuleTag_DefaultDestroyDie\nDeathTypes=ALL -BURNED\nEnd\nEnd\nObject Civilian\nSide=Civilian\nKindOf=STRUCTURE\nEnd\n";
    let baseline = source(baseline_text);
    for (override_text, expected) in [
        ("", Some(false)),
        (
            "Object Civilian\nRemoveModule=ModuleTag_DefaultDestroyDie\nEnd\n",
            None,
        ),
        (
            "Object Civilian\nReplaceModule=ModuleTag_DefaultDestroyDie\nBehavior=DestroyDie MapDestroy\nDeathTypes=NONE +BURNED\nEnd\nEnd\nEnd\n",
            Some(true),
        ),
    ] {
        let overrides = if override_text.is_empty() {
            Vec::new()
        } else {
            vec![
                content::scan(Asset {
                    path: "maps/death/map.ini".into(),
                    origin: "raw death regression".into(),
                    bytes: override_text.as_bytes().to_vec(),
                })
                .unwrap(),
            ]
        };
        let baseline = std::slice::from_ref(&baseline);
        let metadata =
            crate::skirmish_data::module_metadata::load(baseline, &overrides, &["Civilian"])
                .unwrap();
        let mut files = vec![source(baseline_text)];
        crate::skirmish_data::module_metadata::reconcile(
            &mut files, baseline, &overrides, &metadata,
        )
        .unwrap();
        let object = definition(&files, "Object", "Civilian").unwrap();
        let rules = crate::skirmish_data::death_callbacks::destroy(object).unwrap();
        assert_eq!(rules.len(), usize::from(expected.is_some()));
        if let Some(burned) = expected {
            let death_mask = rules[0].death_mask;
            let burn_bit = 1 << generals_sim::skirmish::DeathType::Burned as u32;
            assert_eq!(death_mask & burn_bit != 0, burned);
            if !override_text.is_empty() {
                assert_eq!(death_mask, burn_bit);
                let entry = &metadata["Civilian"].modules[0][0];
                assert_eq!(entry.tag, "MapDestroy");
                assert_eq!(entry.source.path, "maps/death/map.ini");
                assert_eq!(entry.source.origin, "raw death regression");
                assert_eq!(entry.source.line, 3);
                assert!(!entry.copied_from_default);
            }
        }
    }
}

#[test]
fn replacement_destroy_keeps_native_die_slot_order_after_a_map_added_fx() {
    let baseline_text = "Object DefaultThingTemplate\nBehavior=DestroyDie DefaultDestroy\nEnd\nEnd\nObject Civilian\nSide=Civilian\nEnd\n";
    let baseline = vec![source(baseline_text)];
    let overrides = vec![content::scan(Asset {
        path: "maps/death/map.ini".into(),
        origin: "ordered raw death regression".into(),
        bytes: b"Object Civilian\nAddModule\nBehavior=FXListDie MapFx\nDeathFX=FX_CivilianDie\nEnd\nEnd\nReplaceModule=DefaultDestroy\nBehavior=DestroyDie MapDestroy\nEnd\nEnd\nEnd\n".to_vec(),
    }).unwrap()];
    let metadata =
        crate::skirmish_data::module_metadata::load(&baseline, &overrides, &["Civilian"]).unwrap();
    let mut files = vec![source(baseline_text)];
    crate::skirmish_data::module_metadata::reconcile(&mut files, &baseline, &overrides, &metadata)
        .unwrap();
    files.push(source("FXList FX_CivilianDie\nEnd\n"));
    let object = definition(&files, "Object", "Civilian").unwrap();
    let effects = crate::skirmish_data::death_callbacks::fx(&files, object).unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].effect, "FX_CivilianDie");
    assert_eq!(
        crate::skirmish_data::death_callbacks::destroy(object)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        metadata["Civilian"].modules[0]
            .iter()
            .map(|entry| entry.tag.as_str())
            .collect::<Vec<_>>(),
        ["MapFx", "MapDestroy"]
    );
}

#[test]
fn authored_derived_instances_keep_base_data_and_native_angular_duration_defaults() {
    let file = source(
        "Object Copter\nSide=America\nBehavior=HelicopterSlowDeathBehavior First\nSinkDelay=200\nDestructionDelay=1500\nSpiralOrbitTurnRate=83\nSelfSpinUpdateAmount=7\nMinBladeFlyOffDelay=25\nMaxBladeFlyOffDelay=50\nEnd\nBehavior=HelicopterSlowDeathBehavior Second\nProbabilityModifier=3\nMaxBraking=900\nEnd\nBehavior=JetSlowDeathBehavior Jet\nRollRate=0.2\nDelaySecondaryFromInitialDeath=15\nEnd\nEnd\n",
    );
    let rules = load(std::slice::from_ref(&file), &file.declarations[0])
        .unwrap()
        .unwrap();
    assert_eq!(
        rules
            .slow
            .iter()
            .map(|rule| rule.tag.as_str())
            .collect::<Vec<_>>(),
        ["First", "Second", "Jet"]
    );
    assert_eq!(rules.slow[0].sink_delay, [6, 0]);
    assert_eq!(rules.slow[0].destruction_delay, [45, 0]);
    let Derived::Helicopter(first) = &rules.slow[0].derived else {
        panic!("first helicopter")
    };
    let Derived::Helicopter(second) = &rules.slow[1].derived else {
        panic!("second helicopter")
    };
    assert_eq!(first.braking.to_bits(), 99_999.0_f32.to_bits());
    let angular = 83.0_f32 * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0));
    assert_eq!(first.orbit_turn.to_bits(), angular.to_bits());
    assert_eq!(
        first.blade_delay.map(f32::to_bits),
        [0.75_f32, 1.5].map(f32::to_bits)
    );
    assert_eq!(first.spin_step.to_bits(), 7.0_f32.to_radians().to_bits());
    let braking = 900.0_f32 * ((1.0 / 30.0) * (1.0 / 30.0));
    assert_eq!(second.braking.to_bits(), braking.to_bits());
    let Derived::Jet(jet) = &rules.slow[2].derived else {
        panic!("jet")
    };
    assert_eq!(jet.secondary_delay, 1);
    assert_eq!(jet.roll_rate.to_bits(), 0.2_f32.to_bits());
}
