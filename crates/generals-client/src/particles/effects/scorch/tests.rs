use super::super::{Nugget, parse_list, rng};
use crate::particles::{Particles, Pose, tests::system};
use generals_assets::{content, virtual_assets::Asset};

fn load(text: &str) -> Particles {
    let files = [content::scan(Asset {
        path: "fx.ini".into(),
        origin: "native scorch FX fixture".into(),
        bytes: text.as_bytes().to_vec(),
    })
    .unwrap()];
    let mut particles = system(Vec::new());
    let list = parse_list(
        &files,
        &files[0].declarations[0].lines,
        &[],
        &mut particles.lists,
        true,
    )
    .unwrap();
    particles.lists.push(list);
    particles
}

#[test]
fn original_scorch_defaults_and_all_fixed_types_are_retained_without_random_draws() {
    for (name, expected) in [
        ("SCORCH_1", 0),
        ("SCORCH_2", 1),
        ("SCORCH_3", 2),
        ("SCORCH_4", 3),
        ("SHADOW_SCORCH", 4),
    ] {
        let mut particles = load(&format!(
            "FXList Scorch\nTerrainScorch\nType={name}\nRadius=15\nEnd\nEnd\n"
        ));
        particles.fire(
            0,
            &Pose::at([12.0, 34.0, 56.0]),
            &[],
            &|_| panic!("FX scorch defers terrain fitting to its renderer"),
            99.0,
            &mut |_| panic!("fixed authored scorch consumes no RNG"),
        );
        let mark = particles.scorches.marks().next().unwrap();
        assert_eq!(mark.kind, expected);
        assert_eq!(mark.radius.to_bits(), 15.0_f32.to_bits());
        assert_eq!(
            mark.position.map(f32::to_bits),
            [12.0_f32, 34.0, 56.0].map(f32::to_bits)
        );
    }
    for field in ["", "Type=RANDOM\n"] {
        let particles = load(&format!("FXList Default\nTerrainScorch\n{field}End\nEnd\n"));
        let Nugget::Scorch { kind, radius } = particles.lists[0][0] else {
            panic!("original scorch nugget")
        };
        assert_eq!(kind, -1);
        assert_eq!(radius.to_bits(), 0.0_f32.to_bits());
    }
}

#[test]
fn scorch_integer_draw_remains_between_authored_real_nuggets_and_precedes_deduplication() {
    let mut particles = load(
        "FXList Explosion\nViewShake\nType=NORMAL\nEnd\nTerrainScorch\nType=RANDOM\nRadius=15\nEnd\nViewShake\nType=STRONG\nEnd\nEnd\n",
    );
    let mut requests = Vec::new();
    for _ in 0..2 {
        particles.fire(
            0,
            &Pose::at([10.0, 20.0, 30.0]),
            &[],
            &|_| panic!("scorch does not sample ground at FX dispatch"),
            100.0,
            &mut |request| match request {
                rng::Request::Real(low, high) => {
                    assert_eq!(low.to_bits(), 0.0_f32.to_bits());
                    assert_eq!(high.to_bits(), std::f32::consts::TAU.to_bits());
                    requests.push("real");
                    rng::Value::Real(0.0)
                }
                rng::Request::Scorch => {
                    requests.push("integer");
                    rng::Value::Scorch(3)
                }
            },
        );
    }
    assert_eq!(
        requests,
        ["real", "integer", "real", "real", "integer", "real"]
    );
    let marks = particles.scorches.marks().collect::<Vec<_>>();
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].kind, 3);
}

#[test]
fn authored_scorch_fields_overwrite_in_order_with_native_case_insensitive_lookup() {
    let particles = load(
        "FXList Fields\nTerrainScorch\nType=RANDOM\ntYpe=scorch_2\nRadius=15\nrADius=12.5\nEnd\nEnd\n",
    );
    let Nugget::Scorch { kind, radius } = particles.lists[0][0] else {
        panic!("original scorch nugget")
    };
    assert_eq!(kind, 1);
    assert_eq!(radius.to_bits(), 12.5_f32.to_bits());
}
