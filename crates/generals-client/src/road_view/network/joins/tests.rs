use super::super::{Network, build};
use crate::road_view::{ANGLED, JOIN, Kind, LAST, Road};
use bevy::prelude::*;
use generals_assets::map::MapObject;
use std::collections::BTreeMap;

fn roads() -> [Road; 2] {
    ["RoadA", "RoadB"].map(|name| Road {
        name: name.into(),
        texture: String::new(),
        scale: 10.0,
        width: 1.0,
        id: if name == "RoadA" { 1 } else { 2 },
        stacking: 0,
    })
}

fn records(lines: &[([f32; 2], [f32; 2], &str, u32)]) -> Vec<MapObject> {
    lines
        .iter()
        .flat_map(|&(from, to, template, flags)| {
            [(from, flags | 2), (to, flags | 4)].map(|(point, flags)| MapObject {
                template: template.into(),
                position: [point[0], point[1], 0.0],
                angle: 0.0,
                flags,
                properties: BTreeMap::new(),
            })
        })
        .collect()
}

#[test]
fn three_arm_tee_changes_shared_endpoints_and_appends_one_atlas_section() {
    let objects = records(&[
        ([100.0, 100.0], [200.0, 100.0], "RoadA", 0),
        ([100.0, 100.0], [0.0, 100.0], "RoadA", 0),
        ([100.0, 100.0], [100.0, 200.0], "RoadA", 0),
    ]);
    let segments = build(&objects, &mut roads(), 100).unwrap();
    assert_eq!(segments.len(), 4);
    assert!(matches!(segments[3].kind, Kind::Tee));
    assert_eq!(segments[3].points[0].count, -3);
    assert!(segments[..3].iter().all(|segment| {
        segment
            .points
            .iter()
            .all(|point| point.count == 0 && point.flags & LAST != 0)
    }));
    assert!(segments[..3].iter().all(|segment| {
        segment
            .points
            .iter()
            .all(|point| point.position != Vec2::new(100.0, 100.0))
    }));
}

#[test]
fn four_arm_cross_preserves_arm_order_and_native_width_increase() {
    let objects = records(&[
        ([100.0, 100.0], [200.0, 100.0], "RoadA", 0),
        ([100.0, 100.0], [0.0, 100.0], "RoadA", 0),
        ([100.0, 100.0], [100.0, 200.0], "RoadA", 0),
        ([100.0, 100.0], [100.0, 0.0], "RoadA", 0),
    ]);
    let segments = build(&objects, &mut roads(), 100).unwrap();
    assert_eq!(segments.len(), 5);
    assert!(matches!(segments[4].kind, Kind::FourWay));
    assert_eq!(segments[4].width.to_bits(), 1.03_f32.to_bits());
    assert_eq!(segments[4].points[0].count, -4);
}

#[test]
fn ninety_degree_link_inserts_curve_but_angled_link_uses_miter() {
    let lines = [
        ([0.0, 100.0], [100.0, 100.0], "RoadA", 0),
        ([100.0, 100.0], [100.0, 200.0], "RoadA", 0),
    ];
    let smooth = build(&records(&lines), &mut roads(), 100).unwrap();
    assert!(
        smooth
            .iter()
            .any(|segment| matches!(segment.kind, Kind::Curve))
    );
    let angled = lines.map(|(from, to, name, _)| (from, to, name, 8));
    let sharp = build(&records(&angled), &mut roads(), 100).unwrap();
    assert_eq!(sharp.len(), 2);
    assert!(
        sharp
            .iter()
            .all(|segment| segment.points.iter().all(|point| point.flags & ANGLED != 0))
    );
}

#[test]
fn cross_type_join_promotes_join_texture_above_underlying_road() {
    let mut objects = records(&[
        ([0.0, 100.0], [200.0, 100.0], "RoadA", 0),
        ([100.0, 100.0], [100.0, 200.0], "RoadB", 128),
    ]);
    objects[3].flags &= !128;
    let mut textures = roads();
    let segments = build(&objects, &mut textures, 100).unwrap();
    assert_eq!(
        segments
            .iter()
            .filter(|segment| matches!(segment.kind, Kind::AlphaJoin))
            .count(),
        1
    );
    assert!(textures[1].stacking > textures[0].stacking);
    assert!(
        segments[..2]
            .iter()
            .flat_map(|segment| segment.points)
            .any(|point| point.flags & JOIN != 0)
    );
}

#[test]
fn generated_junction_capacity_is_an_explicit_error() {
    let objects = records(&[
        ([100.0, 100.0], [200.0, 100.0], "RoadA", 0),
        ([100.0, 100.0], [0.0, 100.0], "RoadA", 0),
        ([100.0, 100.0], [100.0, 200.0], "RoadA", 0),
    ]);
    assert!(build(&objects, &mut roads(), 3).is_err());
}

#[test]
fn duplicate_map_segment_keeps_native_count_side_effect_before_rejection() {
    let objects = records(&[
        ([0.0, 100.0], [100.0, 100.0], "RoadA", 0),
        ([0.0, 100.0], [100.0, 100.0], "RoadA", 0),
    ]);
    let mut textures = roads();
    let mut network = Network {
        segments: Vec::new(),
        roads: &mut textures,
        limit: 100,
    };
    network.add_map_objects(&objects).unwrap();
    assert_eq!(network.segments.len(), 1);
    assert!(
        network.segments[0]
            .points
            .iter()
            .all(|point| point.count == 1)
    );
}

#[test]
fn pinned_native_graphs_match_ordered_topology_and_float_bits() {
    let fixture = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/reference/road-network-reference.txt"
    ))
    .unwrap();
    let mut lines = fixture.lines();
    let mut graphs = 0;
    while let Some(count) = lines.next() {
        let count: usize = count.parse().unwrap();
        let mut objects = Vec::new();
        for _ in 0..count {
            let input: Vec<_> = lines.next().unwrap().split_whitespace().collect();
            let coordinate = |index: usize| input[index].parse::<f32>().unwrap();
            let id: u32 = input[4].parse().unwrap();
            let flags: u32 = input[5].parse().unwrap();
            let template = if id == 1 { "RoadA" } else { "RoadB" };
            let mut pair = records(&[(
                [coordinate(0), coordinate(1)],
                [coordinate(2), coordinate(3)],
                template,
                flags & (8 | 64),
            )]);
            pair[0].flags |= flags & 128;
            pair[1].flags |= (flags & 256) / 2;
            objects.extend(pair);
        }
        let mut textures = roads();
        let segments = build(&objects, &mut textures, 512).unwrap();
        let expected: usize = lines.next().unwrap().parse().unwrap();
        assert_eq!(segments.len(), expected, "graph {graphs} section count");
        for (index, segment) in segments.iter().enumerate() {
            let expected: Vec<i64> = lines
                .next()
                .unwrap()
                .split_whitespace()
                .map(|word| word.parse().unwrap())
                .collect();
            let kind = match segment.kind {
                Kind::Segment => 0,
                Kind::Curve => 1,
                Kind::Tee => 2,
                Kind::FourWay => 3,
                Kind::ThreeWayY => 4,
                Kind::ThreeWayH => 5,
                Kind::ThreeWayHFlip => 6,
                Kind::AlphaJoin => 7,
            };
            let mut actual = vec![
                kind,
                i64::from(textures[segment.road].id),
                i64::from(segment.radius.to_bits()),
                i64::from(segment.scale.to_bits()),
                i64::from(segment.width.to_bits()),
            ];
            for point in segment.points {
                for vector in [point.position, point.top, point.bottom] {
                    actual.extend([i64::from(vector.x.to_bits()), i64::from(vector.y.to_bits())]);
                }
                actual.extend([i64::from(point.count), i64::from(point.flags)]);
            }
            assert_eq!(actual, expected, "graph {graphs} section {index}");
        }
        let stacking: Vec<u32> = lines
            .next()
            .unwrap()
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect();
        assert_eq!(
            textures
                .iter()
                .map(|road| road.stacking)
                .collect::<Vec<_>>(),
            stacking[..2]
        );
        graphs += 1;
    }
    assert_eq!(graphs, 20);
}
