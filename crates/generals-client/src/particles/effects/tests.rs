use super::{Nugget, Particles, Pose, parse_list, rng};
use crate::particles::{
    Range, Volume,
    tests::{system, template},
};
use generals_assets::{content, virtual_assets::Asset};
use std::cell::RefCell;

fn load(particles: &mut Particles, text: &str) {
    let file = content::scan(Asset {
        path: "fx.ini".into(),
        origin: "native FX fixture".into(),
        bytes: text.as_bytes().to_vec(),
    })
    .unwrap();
    let list = parse_list(
        std::slice::from_ref(&file),
        &file.declarations[0].lines,
        &particles.templates,
        &mut particles.lists,
        true,
    )
    .unwrap();
    particles.lists.push(list);
}

#[test]
fn malformed_earlier_duplicate_fxlist_is_rejected_before_the_final_definition() {
    let file = content::scan(Asset {
        path: "data/ini/fxlist.ini".into(),
        origin: "malformed prior native FXList declaration regression".into(),
        bytes: b"FXList Duplicate\nParticleSystem\nName=Earlier\nEnd\nFXList Duplicate\nEnd\n"
            .to_vec(),
    })
    .unwrap();
    assert!(crate::skirmish_data::fx_definition(&[file], "Duplicate").is_err());
}

#[test]
#[ignore = "requires original game installation"]
fn duplicate_original_gattling_explosion_uses_last_authored_fx_nuggets() {
    let root = generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let file = content::scan(assets.read("data/ini/fxlist.ini").unwrap()).unwrap();
    let files = [file];
    let selected = crate::skirmish_data::fx_definition(&files, "FX_GattlingExplosionOneFinal")
        .unwrap()
        .unwrap();
    assert!(super::definition(&files, "FXList", "FX_GattlingExplosionOneFinal").is_err());
    let templates = [
        "BattleMasterTankExplosionSmoke",
        "BattleMasterTankExplosionDebris",
        "BattleMasterTankExplosionShockwave",
        "BattleMasterTankExplosionLenzflare",
        "BattleMasterTankSubExplosionSmoke",
    ]
    .into_iter()
    .map(template)
    .collect::<Vec<_>>();
    let mut particles = system(templates);
    let nuggets = parse_list(
        &files,
        &selected.lines,
        &particles.templates,
        &mut particles.lists,
        true,
    )
    .unwrap();
    let particle_names = nuggets
        .iter()
        .filter_map(|nugget| match nugget {
            Nugget::Particle { template, .. } => Some(particles.templates[*template].name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        particle_names,
        [
            "BattleMasterTankExplosionSmoke",
            "BattleMasterTankExplosionDebris",
            "BattleMasterTankExplosionShockwave",
            "BattleMasterTankExplosionLenzflare",
        ]
    );
    assert!(
        nuggets
            .iter()
            .any(|nugget| matches!(nugget, Nugget::Sound(sound) if sound == "TankDie"))
    );
    assert!(
        nuggets
            .iter()
            .any(|nugget| matches!(nugget, Nugget::ViewShake(_)))
    );
    assert!(
        nuggets
            .iter()
            .any(|nugget| matches!(nugget, Nugget::Scorch { .. }))
    );
}

#[test]
fn absent_particle_fx_keeps_following_sound_and_draws_no_constructor_randomness() {
    let mut particles = system(Vec::new());
    load(
        &mut particles,
        "FXList Missing\nParticleSystem\nName=ScudMissleLauncherSmoke\nEnd\nSound\nName=FollowingSound\nEnd\nEnd\n",
    );
    let mut draws = 0;
    particles.fire(
        0,
        &Pose::at([1.0, 2.0, 3.0]),
        &[],
        &|_| 0.0,
        0.0,
        &mut rng::real_only(|low, _| {
            draws += 1;
            low
        }),
    );
    assert!(particles.emitters.is_empty());
    assert_eq!(particles.next_id, 0);
    assert_eq!(draws, 0);
    assert_eq!(particles.sounds.len(), 1);
    assert_eq!(particles.sounds[0].0, "FollowingSound");
}

#[test]
fn ground_projection_follows_rotated_offset_and_constructor_randomness_without_z_bias() {
    let mut rules = template("fixture");
    rules.wind_start = Range {
        low: 1.0,
        high: 2.0,
    };
    rules.wind_end = Range {
        low: 3.0,
        high: 4.0,
    };
    let mut particles = system(vec![rules]);
    load(
        &mut particles,
        "FXList Ground\nParticleSystem\nName=fixture\nOffset=X:4 Y:5 Z:6\nEnd\nParticleSystem\nName=fixture\nOffset=X:4 Y:5 Z:6\nOrientToObject=Yes\nCreateAtGroundHeight=Yes\nEnd\nEnd\n",
    );
    let pose = Pose {
        position: [10.0, 20.0, 300.0],
        axes: [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
    };
    let events = RefCell::new(Vec::new());
    particles.fire(
        0,
        &pose,
        &[],
        &|point| {
            events.borrow_mut().push("ground");
            assert_eq!(point.map(f32::to_bits), [16.0_f32, 24.0].map(f32::to_bits));
            let x = point[0] * 0.25;
            let y = point[1] * 0.5;
            x + y
        },
        0.0,
        &mut rng::real_only(|low, high| {
            if low < high {
                events
                    .borrow_mut()
                    .push(if high.to_bits() == std::f32::consts::TAU.to_bits() {
                        "placement"
                    } else {
                        "constructor"
                    });
            }
            low
        }),
    );
    assert_eq!(
        *events.borrow(),
        [
            "constructor",
            "constructor",
            "constructor",
            "placement",
            "constructor",
            "constructor",
            "constructor",
            "placement",
            "ground"
        ]
    );
    let airborne = &particles.emitters[0].transform;
    assert_eq!(
        airborne.position.map(f32::to_bits),
        [16.0_f32, 24.0, 305.0].map(f32::to_bits)
    );
    assert_eq!(
        airborne.axes.map(|axis| axis.map(f32::to_bits)),
        Pose::at([0.0; 3]).axes.map(|axis| axis.map(f32::to_bits))
    );
    let grounded = &particles.emitters[1].transform;
    assert_eq!(
        grounded.position.map(f32::to_bits),
        [16.0_f32, 24.0, 16.0].map(f32::to_bits)
    );
    assert_eq!(
        grounded.axes.map(|axis| axis.map(f32::to_bits)),
        pose.axes.map(|axis| axis.map(f32::to_bits))
    );
}

#[test]
fn caller_radius_changes_live_cylinder_and_sphere_emission_without_mutating_template_or_slave() {
    for volume in [
        Volume::Cylinder {
            radius: 2.0,
            length: 0.0,
            hollow: true,
        },
        Volume::Sphere {
            radius: 2.0,
            hollow: true,
        },
    ] {
        let mut master = template("fixture");
        master.volume = volume;
        master.slave = Some(1);
        let mut particles = system(vec![master, template("slave")]);
        load(
            &mut particles,
            "FXList Blast\nParticleSystem\nName=fixture\nUseCallersRadius=Yes\nEnd\nEnd\n",
        );
        let mut random = |low: f32, high: f32| if low < 0.0 && high > 0.0 { high } else { low };
        for radius in [7.0, 0.0] {
            particles.fire(
                0,
                &Pose::at([0.0; 3]),
                &[],
                &|_| panic!("no ground projection"),
                radius,
                &mut rng::real_only(&mut random),
            );
        }
        assert_eq!(particles.emitters.len(), 4);
        assert!(particles.emitters[0].radius.is_none());
        assert!(particles.emitters[2].radius.is_none());
        assert!(particles.emitters[3].radius.is_none());
        particles.logic(0, &mut random, &|_| 0.0);
        for (index, expected) in [(1, 7.0), (3, 2.0)] {
            let point = particles.emitters[index].particles[0].position;
            let squares = point.map(|value| value * value);
            let distance = (squares[0] + squares[1] + squares[2]).sqrt();
            assert!((distance - expected).abs() < 0.000_001);
        }
    }
}

#[test]
fn bone_child_projects_on_ground_but_does_not_inherit_callers_radius() {
    let mut rules = template("fixture");
    rules.volume = Volume::Sphere {
        radius: 2.0,
        hollow: true,
    };
    let mut particles = system(vec![rules]);
    load(
        &mut particles,
        "FXList Parent\nFXListAtBonePos\nFX=Child\nBoneName=FIREPOINT\nEnd\nEnd\nFXList Child\nParticleSystem\nName=fixture\nCreateAtGroundHeight=Yes\nUseCallersRadius=Yes\nEnd\nEnd\n",
    );
    assert!(matches!(
        particles.lists[1][0],
        Nugget::AtBones { list: 0, .. }
    ));
    particles.fire(
        1,
        &Pose::at([0.0; 3]),
        &[(0, vec![Pose::at([40.0, 50.0, 60.0])])],
        &|point| point[0] + point[1],
        10.0,
        &mut rng::real_only(|low, _| low),
    );
    assert_eq!(
        particles.emitters[0].transform.position.map(f32::to_bits),
        [40.0_f32, 50.0, 90.0].map(f32::to_bits)
    );
    assert!(particles.emitters[0].radius.is_none());
}

#[test]
fn fx_delay_overrides_after_constructor_and_placement_and_preserves_slave_lifetime() {
    let mut master = template("master");
    master.initial_delay = Range {
        low: 17.0,
        high: 18.0,
    };
    master.slave = Some(1);
    let mut slave = template("slave");
    slave.initial_delay = Range {
        low: 7.0,
        high: 7.0,
    };
    let mut particles = system(vec![master, slave]);
    load(
        &mut particles,
        "FXList Delayed\nParticleSystem\nName=master\nInitialDelay=100 100 UNIFORM\nEnd\nEnd\n",
    );
    let draws = RefCell::new(Vec::new());
    particles.fire(
        0,
        &Pose::at([1.0, 2.0, 3.0]),
        &[],
        &|_| 0.0,
        0.0,
        &mut rng::real_only(|low, high| {
            draws.borrow_mut().push([low, high]);
            low
        }),
    );
    let draws = draws.into_inner();
    assert_eq!(
        draws[0].map(f32::to_bits),
        [17.0_f32, 18.0].map(f32::to_bits)
    );
    assert_eq!(
        draws[draws.len() - 2].map(f32::to_bits),
        [0.0, std::f32::consts::TAU].map(f32::to_bits)
    );
    assert_eq!(
        draws[draws.len() - 1].map(f32::to_bits),
        [100.0_f32; 2].map(f32::to_bits)
    );
    let lifetime = particles.emitters[1].remaining;
    assert_eq!(particles.emitters[0].delay.to_bits(), 7.0_f32.to_bits());
    assert_eq!(particles.emitters[1].delay.to_bits(), 3.0_f32.to_bits());
    for frame in 0..3 {
        particles.logic(frame, &mut |low, _| low, &|_| 0.0);
        assert!(particles.emitters[1].particles.is_empty());
        assert_eq!(
            particles.emitters[1].remaining.to_bits(),
            lifetime.to_bits()
        );
    }
    particles.logic(3, &mut |low, _| low, &|_| 0.0);
    assert_eq!(particles.emitters[1].particles.len(), 1);
    assert_eq!(particles.emitters[1].particles[0].born, 3);
    assert_eq!(
        particles.emitters[1].remaining.to_bits(),
        (lifetime - 1.0).max(0.0).to_bits()
    );
}

#[test]
fn fx_delay_preserves_negative_defaults_rounds_fractional_frames_and_restart_cancels() {
    let mut rules = template("fixture");
    rules.initial_delay = Range {
        low: 9.0,
        high: 9.0,
    };
    let mut particles = system(vec![rules]);
    load(
        &mut particles,
        "FXList Delays\nParticleSystem\nName=fixture\nEnd\nParticleSystem\nName=fixture\nInitialDelay=-1 -1 CONSTANT\nEnd\nParticleSystem\nName=fixture\nInitialDelay=101 199 UNIFORM\nEnd\nEnd\n",
    );
    particles.begin(1, 0);
    particles.fire(
        0,
        &Pose::at([0.0; 3]),
        &[],
        &|_| 0.0,
        0.0,
        &mut rng::real_only(|low, _| low),
    );
    assert_eq!(
        particles
            .emitters
            .iter()
            .map(|emitter| emitter.delay.to_bits())
            .collect::<Vec<_>>(),
        [9.0_f32, 9.0, 4.0].map(f32::to_bits)
    );
    particles.emitters[2].lifecycle.destroyed = true;
    for frame in 0..5 {
        particles.logic(frame, &mut |low, _| low, &|_| 0.0);
        assert!(
            particles
                .emitters
                .iter()
                .all(|emitter| emitter.particles.is_empty())
        );
    }
    particles.begin(2, 0);
    assert!(particles.emitters.is_empty());
    particles.logic(0, &mut |low, _| low, &|_| 0.0);
    assert!(particles.emitters.is_empty());
}

#[test]
fn attached_fx_keeps_native_local_matrix_and_ignores_placement_offset() {
    let mut particles = system(vec![template("fixture")]);
    load(
        &mut particles,
        "FXList Attached\nParticleSystem\nName=fixture\nAttachToObject=yes\nOffset=X:99 Y:88 Z:77\nEnd\nParticleSystem\nName=fixture\nAttachToObject=YES\nOrientToObject=yes\nOffset=X:99 Y:88 Z:77\nEnd\nEnd\n",
    );
    let pose = Pose {
        position: [10.0, 20.0, 30.0],
        axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let mut draws = Vec::new();
    particles.fire_from(
        0,
        (&pose, Some((9, 0.0))),
        &[],
        &|_| panic!("no ground projection"),
        0.0,
        &mut rng::real_only(|low, high| {
            draws.push([low.to_bits(), high.to_bits()]);
            low
        }),
    );
    let attached_draws = draws.clone();
    let plain = &particles.emitters[0];
    assert_eq!(plain.object_attachment.as_ref().unwrap().0, 9);
    assert_eq!(
        plain.transform.position.map(f32::to_bits),
        pose.position.map(f32::to_bits)
    );
    let oriented = &particles.emitters[1];
    let expected = Pose {
        position: [-10.0, 30.0, 60.0],
        axes: [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    assert_eq!(
        oriented.transform.position.map(f32::to_bits),
        expected.position.map(f32::to_bits)
    );
    assert_eq!(
        oriented.transform.axes.map(|axis| axis.map(f32::to_bits)),
        expected.axes.map(|axis| axis.map(f32::to_bits))
    );
    assert_eq!(
        oriented.wind_origin.unwrap().map(f32::to_bits),
        [20.0_f32, 40.0, 60.0].map(f32::to_bits)
    );
    draws.clear();
    particles.fire(
        0,
        &pose,
        &[],
        &|_| panic!("no ground projection"),
        0.0,
        &mut rng::real_only(|low, high| {
            draws.push([low.to_bits(), high.to_bits()]);
            low
        }),
    );
    assert_eq!(draws, attached_draws);
    for emitter in &particles.emitters[2..] {
        assert!(emitter.object_attachment.is_none());
        assert_eq!(
            emitter.transform.position.map(f32::to_bits),
            [-78.0_f32, 119.0, 107.0].map(f32::to_bits)
        );
    }
}

#[test]
fn particle_fx_rejects_invalid_booleans_instead_of_silently_disabling_behavior() {
    let file = content::scan(Asset {
        path: "fx.ini".into(),
        origin: "invalid attached FX".into(),
        bytes: b"FXList Invalid\nParticleSystem\nName=fixture\nAttachToObject=Perhaps\nEnd\nEnd\n"
            .to_vec(),
    })
    .unwrap();
    let error = parse_list(
        std::slice::from_ref(&file),
        &file.declarations[0].lines,
        &[template("fixture")],
        &mut Vec::new(),
        true,
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("invalid particle FX AttachToObject")
    );
}

#[test]
fn repeated_death_fx_samples_constructor_radius_angle_height_delay_in_source_order() {
    for grounded in [false, true] {
        let mut rule = template("smoke");
        rule.initial_delay = Range {
            low: 5.0,
            high: 6.0,
        };
        let mut particles = system(vec![rule]);
        load(
            &mut particles,
            &format!(
                "FXList Repeated\nParticleSystem\nName=smoke\nCount=3\nRadius=15 30 UNIFORM\nHeight=0 20 UNIFORM\nInitialDelay=167 667 UNIFORM\nCreateAtGroundHeight={}\nEnd\nParticleSystem\nName=smoke\nCount=0\nEnd\nParticleSystem\nName=smoke\nCount=-2\nEnd\nEnd\n",
                if grounded { "Yes" } else { "No" },
            ),
        );
        let mut draws = Vec::new();
        particles.fire(
            0,
            &Pose::at([1.0, 2.0, 3.0]),
            &[],
            &|_| 40.0,
            0.0,
            &mut rng::real_only(|low, high| {
                if low < high {
                    draws.push([low.to_bits(), high.to_bits()]);
                }
                low
            }),
        );
        let mut expected = vec![[5.0_f32, 6.0], [15.0, 30.0], [0.0, std::f32::consts::TAU]];
        if !grounded {
            expected.push([0.0, 20.0]);
        }
        expected.push([167.0, 667.0]);
        let expected = expected
            .into_iter()
            .map(|pair| pair.map(f32::to_bits))
            .collect::<Vec<_>>();
        assert_eq!(draws, expected.repeat(3));
        assert_eq!(particles.emitters.len(), 3);
        for emitter in &particles.emitters {
            assert_eq!(
                emitter.transform.position.map(f32::to_bits),
                [16.0_f32, 2.0, if grounded { 40.0 } else { 3.0 }].map(f32::to_bits)
            );
            assert_eq!(emitter.delay.to_bits(), 6.0_f32.to_bits());
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_aurora_and_neutral_artillery_keep_original_weapon_effects() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let catalog = &battle.simulation.catalog;
    let particles = crate::particles::load(&assets, catalog).unwrap();
    for name in ["AmericaJetAurora", "TechArtilleryPlatform"] {
        let index = catalog.iter().position(|unit| unit.name == name).unwrap();
        let effects = particles.bindings[index][0].as_ref().unwrap();
        assert!(effects.fire[0].is_some(), "{name} launch FX");
        assert!(effects.detonation.is_some(), "{name} impact FX");
        if name == "AmericaJetAurora" {
            let launch = &particles.lists[effects.fire[0].unwrap()];
            assert!(launch.iter().any(
                |nugget| matches!(nugget, Nugget::Sound(sound) if sound == "DaisyCutterWeapon")
            ));
            let impact = &particles.lists[effects.detonation.unwrap()];
            assert!(impact.iter().any(
                |nugget| matches!(nugget, Nugget::Sound(sound) if sound == "ExplosionCarpetBomb")
            ));
            for name in [
                "CarpetBombExplosion",
                "CarpetBombWave",
                "CarpetBombExplosionPuff",
            ] {
                assert!(impact.iter().any(|nugget| matches!(nugget, Nugget::Particle { template, .. } if particles.templates[*template].name == name)), "{name}");
            }
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_microwave_smudge_uses_scene_distortion_without_loading_a_sentinel_texture() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let catalog = &battle.simulation.catalog;
    let kind = catalog
        .iter()
        .position(|unit| unit.name == "AmericaTankMicrowave")
        .unwrap();
    let unit = &catalog[kind];
    let weapon_index = unit.fire_weapon_index(0).unwrap();
    let weapon = unit.weapon_template(weapon_index).unwrap();
    assert_eq!(weapon.name, "MicrowaveTankEmitterWeapon");
    assert_eq!(
        weapon.area.as_ref().unwrap().primary_radius.to_bits(),
        100.0_f32.to_bits()
    );
    let files = super::source_files(&assets).unwrap();
    let weapon = super::definition(&files, "Weapon", &weapon.name).unwrap();
    let fire = super::scalar(super::field(&weapon.lines, 0, "FireFX").unwrap()).unwrap();
    assert_eq!(fire, "FX_MicrowaveTankEmitter");
    let fx = crate::skirmish_data::fx_definition(&files, fire)
        .unwrap()
        .unwrap();
    let blocks = super::blocks(&fx.lines, 0, "ParticleSystem").unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        super::scalar(super::field(blocks[0], 1, "Name").unwrap()).unwrap(),
        "MicrowaveEmitter"
    );
    assert_eq!(
        super::scalar(super::field(blocks[0], 1, "UseCallersRadius").unwrap()).unwrap(),
        "Yes"
    );
    let definitions = content::scan(assets.read("data/ini/particlesystem.ini").unwrap()).unwrap();
    let emitter = super::definition(
        std::slice::from_ref(&definitions),
        "ParticleSystem",
        "MicrowaveEmitter",
    )
    .unwrap();
    assert_eq!(
        super::scalar(super::field(&emitter.lines, 0, "Type").unwrap()).unwrap(),
        "PARTICLE"
    );
    assert_eq!(
        super::scalar(super::field(&emitter.lines, 0, "ParticleName").unwrap()).unwrap(),
        "SMUDGE.tga"
    );
    let particles = crate::particles::load(&assets, catalog).unwrap();
    assert!(particles.bindings[kind][weapon_index].is_some());
    let template = particles
        .templates
        .iter()
        .find(|template| template.name == "MicrowaveEmitter")
        .unwrap();
    assert!(matches!(
        template.drawing.style,
        crate::particles::Style::Smudge
    ));
    let texture = &particles.textures[template.texture];
    assert!(texture.image.is_none() && texture.handle.is_none());
}

#[test]
fn native_light_threshold_and_object_radius_use_real_pulse_pool_without_rng() {
    let mut particles = system(Vec::new());
    load(
        &mut particles,
        "FXList Small\nLightPulse\nRadius=19\nEnd\nSound\nName=AfterSmall\nEnd\nEnd\n",
    );
    particles.fire(0, &Pose::at([1.0; 3]), &[], &|_| 0.0, 0.0, &mut |_| {
        panic!("LightPulse consumes no client RNG")
    });
    assert_eq!(particles.sounds, [("AfterSmall".into(), [1.0; 3])]);
    assert!(particles.pulses.lights().is_empty());
    load(
        &mut particles,
        "FXList Accepted\nLightPulse\nColor=R:255 G:128 B:0\nRadius=20\nRadiusAsPercentOfObjectSize=150%\nIncreaseTime=100\nDecreaseTime=100\nEnd\nEnd\n",
    );
    let index = particles.lists.len() - 1;
    particles.fire(
        index,
        &Pose::at([3.0, 4.0, 5.0]),
        &[],
        &|_| 0.0,
        999.0,
        &mut |_| panic!("LightPulse consumes no client RNG"),
    );
    assert_eq!(
        particles.pulses.lights()[0].outer_radius.to_bits(),
        21.0_f32.to_bits()
    );
    particles.fire_from(
        index,
        (&Pose::at([6.0, 7.0, 8.0]), Some((9, 40.0))),
        &[],
        &|_| 0.0,
        999.0,
        &mut |_| panic!("LightPulse consumes no client RNG"),
    );
    let light = &particles.pulses.lights()[0];
    assert_eq!(
        light.position.map(f32::to_bits),
        [6.0_f32, 7.0, 8.0].map(f32::to_bits)
    );
    assert_eq!(light.outer_radius.to_bits(), 61.0_f32.to_bits());
    assert_eq!(
        light.color.map(f32::to_bits),
        [1.0_f32, 128.0 / 255.0, 0.0].map(f32::to_bits)
    );
    particles.pulses.advance_frame();
    assert_eq!(
        particles.pulses.lights()[0].color[0].to_bits(),
        (1.0_f32 / 3.0).to_bits()
    );
}

#[test]
fn native_none_fxlist_is_null_even_when_a_declaration_uses_the_reserved_name() {
    let file = content::scan(Asset {
        path: "data/ini/fxlist.ini".into(),
        origin: "FXListStore::findFXList unconditional case-insensitive sentinel".into(),
        bytes: b"FXList NONE\nSound\nName=TankDie\nEnd\nEnd\nFXList Present\nEnd\n".to_vec(),
    })
    .unwrap();
    let files = [file];
    for name in ["NONE", "None", "nOnE"] {
        assert!(
            crate::skirmish_data::fx_definition(&files, name)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(
        crate::skirmish_data::fx_definition(&files, "Present")
            .unwrap()
            .unwrap()
            .name,
        "Present"
    );
}
