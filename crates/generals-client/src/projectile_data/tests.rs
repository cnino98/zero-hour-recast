use generals_assets::{content, virtual_assets::Asset};

#[ignore = "requires original game installation"]
#[test]
fn original_sticky_bomb_without_model_draw_keeps_identity_launch_offsets() {
    let (_, battle, animations) = crate::cleanup_verification::tests::installed_with_animations();
    let catalog = &battle.simulation.catalog;
    let index = catalog
        .iter()
        .position(|unit| unit.name == "TNTStickyBomb")
        .unwrap();
    let unit = &catalog[index];
    let weapon = (0..unit.weapon_template_count())
        .filter_map(|index| unit.weapon_template(index))
        .find(|weapon| weapon.name == "TNTDetonationWeapon")
        .unwrap();
    let motion = weapon.motion.as_ref().unwrap();
    for (state, damage) in [None, Some("DAMAGED"), Some("REALLYDAMAGED")]
        .into_iter()
        .enumerate()
    {
        assert!(animations.firing_model_absent(index, damage, 0));
        assert!(animations.firing_fields(index, damage, 0).is_none());
        assert_eq!(motion.launches[state].len(), 1);
        let launch = &motion.launches[state][0];
        assert_eq!(
            launch.position.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
        assert_eq!(
            launch.axes.map(|axis| axis.map(f32::to_bits)),
            generals_sim::skirmish::LaunchTransform::IDENTITY
                .axes
                .map(|axis| axis.map(f32::to_bits))
        );
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_location_buttons_use_only_explicit_disarm_or_validated_missile_slots() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut commands = std::collections::BTreeSet::new();
    for (unit, ui) in battle.simulation.catalog.iter().zip(&data.units) {
        super::validate_location_weapons(unit, &battle.simulation.catalog).unwrap();
        for command in ui.commands.iter().chain(
            ui.upgraded_commands
                .iter()
                .flat_map(|(_, commands)| commands),
        ) {
            match command.action {
                crate::control_bar::CommandAction::ClearMines(slot) => {
                    assert_eq!(slot, 0);
                    assert_eq!(unit.location_weapons, 0);
                    commands.insert((unit.name.as_str(), "DISARM", slot));
                }
                crate::control_bar::CommandAction::FireWeapon(slot) => {
                    assert_ne!(unit.location_weapons & (1 << slot), 0);
                    commands.insert((unit.name.as_str(), "MISSILE", slot));
                }
                _ => {}
            }
        }
    }
    assert_eq!(
        commands,
        std::collections::BTreeSet::from([
            ("AmericaVehicleDozer", "DISARM", 0),
            ("ChinaVehicleDozer", "DISARM", 0),
            ("GLAInfantryWorker", "DISARM", 0),
            ("AmericaVehicleComanche", "MISSILE", 2),
            ("ChinaTankDragon", "MISSILE", 1),
            ("GLAVehicleToxinTruck", "MISSILE", 1),
        ])
    );
    let kind = battle
        .simulation
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaTankDragon")
        .unwrap();
    let unit = &mut battle.simulation.catalog[kind];
    let weapon = unit
        .weapon_set_rules
        .templates
        .iter_mut()
        .find(|weapon| weapon.slot == 1)
        .unwrap();
    weapon.rules.motion = None;
    assert!(
        super::validate_location_weapons(
            &battle.simulation.catalog[kind],
            &battle.simulation.catalog
        )
        .unwrap_err()
        .to_string()
        .contains("explicit non-missile command consumer")
    );
}

#[test]
fn scaled_shell_parser_retains_native_range_adjustment_and_speed_default_units() {
    let parse = |fields: &str| {
        let file = content::scan(Asset {
            path: "weapon.ini".into(),
            origin: "native scaled shell regression".into(),
            bytes: format!("Weapon FireBaseHowitzerGun\n{fields}\nEnd\n").into_bytes(),
        })
        .unwrap();
        super::speed_scale(&file.declarations[0].lines).unwrap()
    };
    assert!(parse("MinWeaponSpeed = 75").is_none());
    let rules = parse(
        "ScaleWeaponSpeed = Yes\nMinWeaponSpeed = 75\nMinimumAttackRange = 50\nAttackRange = 275",
    )
    .unwrap();
    assert_eq!(rules.minimum_speed.to_bits(), 0x4020_0001);
    assert_eq!(rules.minimum_range.to_bits(), 47.5_f32.to_bits());
    assert_eq!(rules.maximum_range.to_bits(), 275.0_f32.to_bits());
    let defaults = parse("ScaleWeaponSpeed = Yes").unwrap();
    assert_eq!(defaults.minimum_speed.to_bits(), 999_999.0_f32.to_bits());
    assert_eq!(defaults.minimum_range.to_bits(), 0.0_f32.to_bits());
}

#[ignore = "requires original game installation"]
#[test]
fn cleanup_missile_keeps_invisible_model_and_rejects_missing_assets_and_normal_state() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let source = "Object CleanupStreamProjectile\nDraw = W3DModelDraw ModuleTag_01\nDefaultConditionState\nModel = NONE\nEnd\nEnd\nEnd\n";
    let load = |text: &str| {
        let file = content::scan(Asset {
            path: "weaponobjects.ini".into(),
            origin: "common projectile model".into(),
            bytes: text.as_bytes().to_vec(),
        })
        .unwrap();
        crate::skirmish_data::normal_model(&assets, &file.declarations[0])
    };
    assert!(load(source).unwrap().meshes.is_empty());
    // Native Model NONE is a draw contract, not a special object-name exception.
    assert!(
        load(&source.replace("CleanupStreamProjectile", "OtherMissile"))
            .unwrap()
            .meshes
            .is_empty()
    );
    assert!(
        load(&source.replace("Model = NONE", "Model = FixtureMissingProjectileModel")).is_err()
    );
    assert!(
        load(&source.replace("DefaultConditionState", "ConditionState = NONE"))
            .unwrap()
            .meshes
            .is_empty()
    );
    assert!(load(&source.replace("DefaultConditionState", "ConditionState = MOVING")).is_err());
}

#[ignore = "requires original game installation"]
#[test]
fn original_invisible_mob_nexus_binds_identity_without_accepting_missing_visible_models() {
    use generals_sim::skirmish::{LaunchTransform, turret::Mount};
    let (_, mut battle, animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let nexus = battle
        .simulation
        .catalog
        .iter()
        .position(|unit| unit.name == "GLAInfantryAngryMobNexus")
        .unwrap();
    let unit = &battle.simulation.catalog[nexus];
    for slot in 0..3 {
        let weapon = unit.weapon_template(slot).unwrap();
        assert_eq!(weapon.name, "GLAAngryMobNexusHarmlessWeapon");
        let motion = weapon
            .motion
            .as_ref()
            .expect("original harmless weapon binding");
        for state in 0..3 {
            assert!(animations.firing_model_absent(
                nexus,
                [None, Some("DAMAGED"), Some("REALLYDAMAGED")][state],
                slot
            ));
            assert_eq!(motion.launches[state].len(), 1);
            assert_eq!(
                motion.launches[state][0].position.map(f32::to_bits),
                LaunchTransform::IDENTITY.position.map(f32::to_bits)
            );
            assert_eq!(
                motion.launches[state][0]
                    .axes
                    .map(|axis| axis.map(f32::to_bits)),
                LaunchTransform::IDENTITY
                    .axes
                    .map(|axis| axis.map(f32::to_bits))
            );
            let mount = &motion.turret_mounts.as_ref().unwrap()[state];
            assert_eq!(
                mount.yaw.map(f32::to_bits),
                Mount::ORIGIN.yaw.map(f32::to_bits)
            );
            assert_eq!(
                mount.pitch.map(f32::to_bits),
                Mount::ORIGIN.pitch.map(f32::to_bits)
            );
        }
    }
    let pistol = battle
        .simulation
        .catalog
        .iter()
        .position(|unit| unit.name == "GLAInfantryAngryMobPistol01")
        .unwrap();
    assert!(!animations.firing_model_absent(pistol, None, 0));
    let error = super::bind_launches(
        &animations,
        &[],
        pistol,
        0,
        &mut battle.simulation.catalog[pistol],
    )
    .unwrap_err();
    assert!(error.to_string().contains("firing model missing"));
}

fn original_projectile_files() -> Vec<content::ContentFile> {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    crate::skirmish_data::source_files::effective(
        &assets,
        &[
            "GLAAngryMobRockProjectileObject",
            "GLAAngryMobMolotovCocktailProjectileObject",
        ],
    )
    .unwrap()
    .0
}

#[ignore = "requires original game installation"]
#[test]
fn original_mob_rocks_and_molotovs_keep_authored_tumble_physics_and_arc_fields() {
    let files = original_projectile_files();
    for (object_name, weapon_name, height) in [
        (
            "GLAAngryMobRockProjectileObject",
            "GLAAngryMobRockProjectileWeapon",
            10.0_f32,
        ),
        (
            "GLAAngryMobMolotovCocktailProjectileObject",
            "GLAAngryMobMolotovCocktailProjectileWeapon",
            30.0,
        ),
    ] {
        let object = super::definition(&files, "Object", object_name).unwrap();
        let weapon = super::definition(&files, "Weapon", weapon_name).unwrap();
        assert_eq!(
            super::scalar(super::field(&weapon.lines, 0, "ProjectileObject").unwrap()).unwrap(),
            object_name
        );
        let module = super::blocks(&object.lines, 0, "Behavior")
            .unwrap()
            .into_iter()
            .find(|module| module[0].tokens[1] == "DumbProjectileBehavior")
            .unwrap();
        let rules = super::shell_rules(&files, object, module).unwrap();
        assert!(rules.tumble && rules.orient_to_flight_path);
        assert_eq!(
            rules.heights.map(f32::to_bits),
            [height; 2].map(f32::to_bits)
        );
        assert_eq!(
            rules.indents.map(f32::to_bits),
            [0.5_f32, 0.9].map(f32::to_bits)
        );
        assert_eq!(
            rules.adjust_per_frame.to_bits(),
            (100.0_f32 / 30.0).to_bits()
        );
        assert_eq!(rules.lifespan, 300);
        let physics = rules.physics.unwrap();
        assert_eq!(physics.mass.to_bits(), 0.1_f32.to_bits());
        assert_eq!(physics.angular_factor.to_bits(), 2.0_f32.to_bits());
        assert_eq!(physics.forward_friction.to_bits(), 0.15_f32.to_bits());
        assert_eq!(physics.lateral_friction.to_bits(), 0.15_f32.to_bits());
        assert_eq!(physics.aerodynamic_friction.to_bits(), 0.0_f32.to_bits());
        assert!(!physics.allow_bouncing);
        let global = super::definition(&files, "GameData", "").unwrap();
        let gravity = super::scalar(super::field(&global.lines, 0, "Gravity").unwrap())
            .unwrap()
            .parse::<f32>()
            .unwrap();
        assert_eq!(
            physics.gravity.to_bits(),
            (gravity * ((1.0_f32 / 30.0) * (1.0 / 30.0))).to_bits()
        );
    }
}

#[test]
fn shell_motion_parser_delegates_body_and_death_modules_to_common_object() {
    for (extra, initial, body) in [
        ("", "100", "ActiveBody"),
        (
            "Behavior=FireWeaponWhenDeadBehavior Extra\nDeathWeapon=ExtraWeapon\nEnd",
            "100",
            "ActiveBody",
        ),
        (
            "Behavior=InstantDeathBehavior Extra\nFX=ExtraFX\nEnd",
            "100",
            "ActiveBody",
        ),
        (
            "Behavior=NeutronBlastBehavior Extra\nBlastRadius=60\nEnd",
            "100",
            "ActiveBody",
        ),
        ("", "101", "ActiveBody"),
        ("", "0", "ActiveBody"),
        ("", "100", "ImmortalBody"),
    ] {
        for (tumble, orient) in [("Yes", "Yes"), ("No", "No")] {
            let file = content::scan(Asset {
            path: "projectile.ini".into(), origin: "native DetonateCallsKill retirement boundary".into(),
            bytes: format!("Object Projectile\nKindOf=PROJECTILE\nBody={body} Body\nMaxHealth=100\nInitialHealth={initial}\nEnd\nBehavior=DumbProjectileBehavior Projectile\nTumbleRandomly={tumble}\nOrientToFlightPath={orient}\nDetonateCallsKill=Yes\nEnd\nBehavior=DestroyDie Die\nEnd\n{extra}\nEnd\nWeapon Launcher\nEnd\n").into_bytes(),
        }).unwrap();
            let result = super::shell_rules(
                std::slice::from_ref(&file),
                &file.declarations[0],
                super::blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0],
            );
            let rules = result.unwrap();
            assert!(rules.detonate_calls_kill);
            assert_eq!(rules.tumble, tumble == "Yes");
            assert_eq!(rules.orient_to_flight_path, orient == "Yes");
        }
    }
}

#[test]
fn native_shell_callbacks_consume_first_value_before_original_inline_comment_tokens() {
    let file = content::scan(Asset {
        path: "projectile.ini".into(), origin: "authored inline-comment parser case".into(),
        bytes: b"Object Shell\nBehavior=DumbProjectileBehavior Tag\nFirstHeight=30 // authored arc note\nSecondHeight=30 // authored arc note\nFirstPercentIndent=50% //arc\nSecondPercentIndent=90% //arc\nOrientToFlightPath=No //orientation\nMaxLifespan=1000 //milliseconds\nEnd\nEnd\nWeapon Gun\nEnd\n".to_vec(),
    }).unwrap();
    let rules = super::shell_rules(
        std::slice::from_ref(&file),
        &file.declarations[0],
        super::blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0],
    )
    .unwrap();
    assert_eq!(
        rules.heights.map(f32::to_bits),
        [30.0_f32; 2].map(f32::to_bits)
    );
    assert_eq!(
        rules.indents.map(f32::to_bits),
        [0.5_f32, 0.9].map(f32::to_bits)
    );
    assert_eq!(rules.lifespan, 30);
    assert!(!rules.orient_to_flight_path && !rules.tumble);
}

#[ignore = "requires original game installation"]
#[test]
fn actual_shell_admission_rejects_an_unbound_update_and_die_consumer() {
    use generals_sim::skirmish::module_updates::{
        BehaviorRegistration, ModuleRegistration, ModuleSource, validate_registrations,
    };
    let (_, mut battle) = crate::cleanup_verification::tests::installed();
    let tank = battle
        .simulation
        .catalog
        .iter()
        .find(|unit| unit.name == "AmericaTankCrusader")
        .unwrap();
    let shell = tank
        .weapon
        .motion
        .as_ref()
        .unwrap()
        .projectile_type
        .unwrap();
    let unit = &mut battle.simulation.catalog[shell];
    validate_registrations(unit).unwrap();
    // Actual NeutronBlastBehavior registry mask is UPDATE|DIE=0x3.
    unit.behavior_modules.push(BehaviorRegistration {
        family: "NeutronBlastBehavior".into(),
        tag: "UnboundBlast".into(),
        interfaces: 3,
        upgrade: generals_sim::skirmish::module_updates::UpgradeData::none(),
        source: ModuleSource {
            path: "fixture.ini".into(),
            origin: "unbound native DIE consumer".into(),
            line: 1,
        },
    });
    unit.update_modules.push(ModuleRegistration::Behavior {
        family: "NeutronBlastBehavior".into(),
        tag: "UnboundBlast".into(),
        source: ModuleSource {
            path: "fixture.ini".into(),
            origin: "unbound native UPDATE and DIE consumer".into(),
            line: 1,
        },
    });
    assert!(
        validate_registrations(unit)
            .unwrap_err()
            .contains("NeutronBlastBehavior")
    );
}
