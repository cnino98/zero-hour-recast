//! Default unsubdivided `StreakRenderer`: silhouette planes, folds and merging.
use bevy::prelude::*;

struct Segment {
    start: Vec3,
    edge: [Vec3; 2],
}
struct Intersection {
    count: u16,
    next: usize,
    direction: Vec3,
    point: Vec3,
    v: f32,
    fold: bool,
    parallel: bool,
}

pub struct Strip {
    pub vertices: Vec<Vec3>,
    pub uv: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

pub fn geometry(
    points: &mut [Vec3],
    widths: &[f32],
    personalities: &[u32],
) -> Result<Strip, &'static str> {
    if points.len() < 2 || points.len() != widths.len() || points.len() != personalities.len() {
        return Err("invalid streak arrays");
    }
    let texture: Vec<_> = (0..points.len())
        .map(|index| {
            if index == 0 || index + 1 == points.len() {
                0.0
            } else {
                f32::from(u8::try_from(personalities[index + 1] & 1).expect("one bit"))
            }
        })
        .collect();
    geometry_with_texture(points, widths, &texture)
}

pub fn segmented_geometry(
    points: &mut [Vec3],
    radius: f32,
    tile: f32,
) -> Result<Strip, &'static str> {
    if !(2..=20).contains(&points.len()) {
        return Err("invalid original projectile stream point count");
    }
    let texture: Vec<_> = (0..points.len())
        .map(|index| f32::from(u16::try_from(index).expect("bounded stream index")) * tile)
        .collect();
    geometry_with_texture(points, &vec![radius; points.len()], &texture)
}

fn geometry_with_texture(
    points: &mut [Vec3],
    widths: &[f32],
    texture: &[f32],
) -> Result<Strip, &'static str> {
    let count = points.len();
    let (mut segments, folds) = segment_planes(points, widths);
    let mut edges: [Vec<Intersection>; 2] = std::array::from_fn(|_| Vec::new());
    for (edge, list) in edges.iter_mut().enumerate() {
        for index in 0..count {
            let (direction, parallel) = if index == 0 || index == count - 1 {
                let plane = segments[if index == 0 { 1 } else { count - 1 }].edge[edge];
                (
                    (points[index] - plane * plane.dot(points[index])).normalize_or_zero(),
                    false,
                )
            } else {
                intersect(
                    segments[index].edge[edge],
                    segments[index + 1].edge[edge],
                    points[index],
                )
            };
            list.push(Intersection {
                count: 1,
                next: if index == count - 1 { count } else { index + 1 },
                direction,
                point: points[index],
                v: texture[index],
                fold: index == 0 || index == count - 1 || folds[index],
                parallel,
            });
        }
    }
    for index in 0..count {
        let plane = edges[0][index]
            .direction
            .cross(edges[1][index].direction)
            .normalize_or_zero();
        let direction = if index == count - 1 {
            (points[index] - points[index - 1]).normalize_or_zero()
        } else {
            segments[index + 1].start
        };
        let oriented = if direction.dot(plane) > 0.0 {
            plane
        } else {
            -plane
        };
        if index == 0 {
            segments[0] = Segment {
                start: oriented,
                edge: [oriented; 2],
            };
            segments[1].start = oriented;
        } else if index == count - 1 {
            segments[count] = Segment {
                start: oriented,
                edge: [oriented; 2],
            };
        } else {
            segments[index + 1].start = oriented;
        }
    }
    merge(&mut edges, &segments, *widths.last().ok_or("empty streak")?)?;
    Ok(triangulate(points, &edges))
}

fn segment_planes(points: &mut [Vec3], widths: &[f32]) -> (Vec<Segment>, Vec<bool>) {
    let count = points.len();
    let mut segments: Vec<Segment> = (0..=count)
        .map(|_| Segment {
            start: Vec3::ZERO,
            edge: [Vec3::ZERO; 2],
        })
        .collect();
    let mut folds = vec![false; count];
    let mut switched = false;
    for index in 1..count {
        let start = points[index - 1];
        if (start - points[index]).abs().max_element() < 0.0001 {
            points[index].x += 0.001;
        }
        let end = points[index];
        let direction = (end - start).normalize_or_zero();
        let nearest = start - direction * direction.dot(start);
        let offset = direction.cross(nearest).normalize_or_zero() * widths[index];
        let top = (start + offset).cross(direction).normalize_or_zero();
        let bottom = direction.cross(start - offset).normalize_or_zero();
        if index > 1 {
            let previous = points[index - 2].cross(start).normalize_or_zero();
            let current = start.cross(end).normalize_or_zero();
            if previous.dot(current) < 0.0 {
                switched = !switched;
                folds[index - 1] = true;
            }
        }
        segments[index] = Segment {
            start: direction,
            edge: if switched {
                [-bottom, -top]
            } else {
                [top, bottom]
            },
        };
    }
    (segments, folds)
}

fn intersect(a: Vec3, b: Vec3, point: Vec3) -> (Vec3, bool) {
    let dot = a.dot(b);
    if dot.abs() < 0.9 {
        let direction = a.cross(b).normalize_or_zero();
        (
            if direction.dot(point) < 0.0 {
                -direction
            } else {
                direction
            },
            false,
        )
    } else {
        let plane = if dot > 0.0 { a + b } else { a - b }.normalize_or_zero();
        ((point - plane * plane.dot(point)).normalize_or_zero(), true)
    }
}

fn merge(
    edges: &mut [Vec<Intersection>; 2],
    segments: &[Segment],
    radius: f32,
) -> Result<(), &'static str> {
    loop {
        let mut merged = false;
        for (edge, list) in edges.iter_mut().enumerate() {
            let mut index = 0;
            while index + 1 < list.len() {
                let current = &list[index];
                let next = &list[index + 1];
                let previous = &segments[if index == 0 { 0 } else { list[index - 1].next }];
                let here = &segments[current.next];
                let after = &segments[next.next];
                let overlaps_next = !next.fold
                    && current.direction.dot(after.start) > 0.0
                    && current.direction.dot(after.edge[edge]) > 0.0;
                let overlaps_previous = !current.fold
                    && next.direction.dot(-here.start) > 0.0
                    && next.direction.dot(previous.edge[edge]) > 0.0;
                if !(overlaps_next || overlaps_previous) {
                    index += 1;
                    continue;
                }
                let count = current.count + next.count;
                let inverse_count = 1.0 / f32::from(count);
                let factor = inverse_count * f32::from(current.count);
                // Preserve native next_factor=curr_int->PointCount typo.
                let point = current.point * factor + next.point * factor;
                let current_v = current.v * factor;
                let next_v = next.v * factor;
                let v = current_v + next_v;
                let dot = previous.edge[edge].dot(after.edge[edge]);
                let (mut direction, parallel) =
                    intersect(previous.edge[edge], after.edge[edge], point);
                if parallel {
                    if current.parallel {
                        // Native reads an uninitialized new_direction in this
                        // branch. Do not invent a replacement rendered shape.
                        return Err("native streak parallel-merge direction is undefined");
                    }
                    let plane = if dot > 0.0 {
                        previous.edge[edge] + after.edge[edge]
                    } else {
                        previous.edge[edge] - after.edge[edge]
                    }
                    .normalize_or_zero();
                    direction = here.edge[edge].cross(plane).normalize_or_zero();
                }
                let abort = radius * 1.5;
                let abort_squared = abort * abort;
                if (current.point - direction * current.point.dot(direction)).length_squared()
                    > abort_squared
                    || (next.point - direction * next.point.dot(direction)).length_squared()
                        > abort_squared
                {
                    index += 1;
                    continue;
                }
                let fold = current.fold || next.fold;
                let next_id = next.next;
                list[index] = Intersection {
                    count,
                    next: next_id,
                    direction,
                    point,
                    v,
                    fold,
                    parallel,
                };
                list.remove(index + 1);
                merged = true;
            }
        }
        if !merged {
            return Ok(());
        }
    }
}

fn triangulate(points: &[Vec3], edges: &[Vec<Intersection>; 2]) -> Strip {
    let mut result = Strip {
        vertices: Vec::new(),
        uv: Vec::new(),
        indices: Vec::new(),
    };
    let push = |out: &mut Strip, side: usize, index: usize, point: usize| {
        let intersection = &edges[side][index];
        out.vertices
            .push(intersection.direction * points[point].dot(intersection.direction));
        out.uv
            .push([if side == 0 { 0.0 } else { 1.0 }, intersection.v]);
    };
    push(&mut result, 0, 0, 0);
    push(&mut result, 1, 0, 0);
    let mut cursor = [0, 0];
    let mut remaining = [edges[0][0].count, edges[1][0].count];
    let mut last = [0u32, 1];
    let mut point = 0usize;
    loop {
        let skip = remaining[0].min(remaining[1]) - 1;
        remaining[0] -= skip;
        remaining[1] -= skip;
        point += usize::from(skip);
        if cursor[0] + 1 == edges[0].len() && remaining[0] == 1 {
            break;
        }
        let vertex = u32::try_from(result.vertices.len()).expect("bounded streak vertices");
        if remaining == [1, 1] {
            result
                .indices
                .extend([last[0], last[1], vertex, last[1], vertex + 1, vertex]);
            point += 1;
            for side in 0..2 {
                cursor[side] += 1;
                remaining[side] = edges[side][cursor[side]].count;
                push(&mut result, side, cursor[side], point);
            }
            last = [vertex, vertex + 1];
        } else {
            let side = usize::from(remaining[0] > 1);
            let other = 1 - side;
            result.indices.extend([last[0], last[1], vertex]);
            remaining[other] -= 1;
            cursor[side] += 1;
            remaining[side] = edges[side][cursor[side]].count;
            point += 1;
            push(&mut result, side, cursor[side], point);
            last[side] = vertex;
        }
    }
    result
}
