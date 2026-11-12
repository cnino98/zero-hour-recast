use super::*;
use generals_sim::skirmish::object_fx;

fn event() -> Event {
    Event {
        pose: object_fx::Event {
            frame: 1,
            object: 2,
            type_index: 0,
            owner: 0,
            position: [100.0, 200.0, 20.0],
            basis: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: 0.0,
            override_radius: 0.0,
            visible_players: 0,
            clear_cells: 0,
            effect: String::new(),
        },
        aflame: false,
        draws: vec![
            (1_u16..=4)
                .map(|index| (format!("fires{index:02}"), [f32::from(index), 0.0, 0.0]))
                .collect(),
        ],
    }
}
fn fixture() -> Particles {
    let mut template = super::super::tests::template("NativeFire");
    template.wind_start = super::super::Range {
        low: 0.1,
        high: 1.1,
    };
    template.wind_end = super::super::Range {
        low: 2.0,
        high: 3.0,
    };
    let mut particles = super::super::tests::system(vec![template]);
    particles.bodies.bindings = std::array::from_fn(|index| Binding {
        prefix: if index == 0 {
            "FireS".into()
        } else {
            "Absent".into()
        },
        template: Some(0),
        maximum: 2,
    });
    particles
}
#[test]
fn native_subset_integer_draw_precedes_constructor_and_refresh_retires_old_ids() {
    for seed in 0..12 {
        let mut actual = fixture();
        let mut expected = fixture();
        let mut random = Animations::effect_fixture(seed);
        let mut expected_random = Animations::effect_fixture(seed);
        let event = event();
        actual.refresh_body_particles(&event, 1, &mut random);
        let mut remaining = vec![1.0_f32, 2.0];
        let mut expected_local = Vec::new();
        for count in [2_u32, 1] {
            let index = usize::try_from(expected_random.effect_integer(1, [0, count - 1])).unwrap();
            expected_local.push(remaining.remove(index));
            expected.create(
                0,
                Pose::at([0.0; 3]),
                None,
                &mut super::super::random(&mut expected_random, 1),
            );
        }
        let actual_local: Vec<_> = actual
            .emitters
            .iter()
            .map(|emitter| emitter.object_attachment.as_ref().unwrap().1.position[0].to_bits())
            .collect();
        assert_eq!(
            actual_local,
            expected_local
                .into_iter()
                .map(f32::to_bits)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            random.effect_random(1).to_bits(),
            expected_random.effect_random(1).to_bits()
        );
        for emitter in &actual.emitters {
            let local = emitter.object_attachment.as_ref().unwrap().1.position;
            assert_eq!(
                emitter.transform.position.map(f32::to_bits),
                [100.0_f32, 200.0 + local[0], 20.0].map(f32::to_bits)
            );
            assert_eq!(
                emitter.wind_origin.unwrap().map(f32::to_bits),
                [100.0_f32 + local[0], 200.0, 20.0].map(f32::to_bits)
            );
        }
        let first_ids = actual.bodies.systems[&2].clone();
        actual.refresh_body_particles(&event, 1, &mut random);
        assert!(first_ids.iter().all(|id| {
            actual
                .emitters
                .iter()
                .find(|emitter| emitter.id == *id)
                .unwrap()
                .lifecycle
                .destroyed
        }));
        assert!(
            actual.bodies.systems[&2]
                .iter()
                .all(|id| !first_ids.contains(id))
        );
        actual.clear_body_particles(2);
        assert!(
            actual
                .emitters
                .iter()
                .all(|emitter| emitter.lifecycle.destroyed)
        );
    }
}

#[test]
fn aflame_uses_native_seven_prefix_substitutions_and_doubled_limits() {
    let templates = [
        "SmallFire",
        "MediumFire",
        "LargeFire",
        "SmokeS",
        "SmokeM",
        "SmokeL",
        "Aflame",
    ]
    .map(super::super::tests::template)
    .into_iter()
    .collect();
    let mut particles = super::super::tests::system(templates);
    particles.bodies.bindings = std::array::from_fn(|index| Binding {
        prefix: format!("Prefix{index}"),
        template: Some(index),
        maximum: 1,
    });
    let mut event = event();
    event.aflame = true;
    event.draws = vec![
        (0_u16..7)
            .flat_map(|group| {
                (1_u16..=2).map(move |index| {
                    (
                        format!("prefix{group}{index:02}"),
                        [f32::from(group), f32::from(index), 0.0],
                    )
                })
            })
            .collect(),
    ];
    let mut animations = Animations::effect_fixture(4);
    particles.refresh_body_particles(&event, 1, &mut animations);
    let names: Vec<_> = particles
        .emitters
        .iter()
        .map(|emitter| particles.templates[emitter.template].name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "MediumFire",
            "MediumFire",
            "LargeFire",
            "LargeFire",
            "LargeFire",
            "LargeFire",
            "SmallFire",
            "SmallFire",
            "SmallFire",
            "SmallFire",
            "SmallFire",
            "SmallFire",
            "Aflame",
            "Aflame"
        ]
    );
}

#[test]
fn native_null_template_table_is_safe_and_consumes_no_client_rng() {
    let mut particles = super::super::tests::system(Vec::new());
    let mut animations = Animations::effect_fixture(3);
    let mut expected = Animations::effect_fixture(3);
    particles.refresh_body_particles(&event(), 1, &mut animations);
    assert!(particles.emitters.is_empty());
    assert_eq!(
        animations.effect_random(1).to_bits(),
        expected.effect_random(1).to_bits()
    );
}
