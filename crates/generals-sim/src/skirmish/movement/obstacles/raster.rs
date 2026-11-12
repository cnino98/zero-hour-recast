//! `Pathfinder::internal_classifyObjectFootprint`, AIPathfind.cpp:4372–4525.
use super::{BTreeSet, Cell, Footprint, Grid};
use crate::collision_geometry::Shape;
use num_traits::ToPrimitive;

pub(in crate::skirmish) fn cells(footprint: &Footprint, grid: &Grid) -> BTreeSet<Cell> {
    match footprint.shape {
        Shape::Box => box_cells(footprint, grid),
        Shape::Sphere | Shape::Cylinder => circle_cells(footprint, grid),
    }
}

/// `Pathfinder::classifyFence` uses a narrow strip anchored by `FenceXOffset`,
/// independently of the ordinary geometry radius or structure admission.
pub(in crate::skirmish) fn fence_cells(
    fence: &crate::skirmish::MapFence,
    grid: &Grid,
) -> BTreeSet<Cell> {
    let (sine, cosine) = fence.angle.sin_cos();
    let columns = step_count(fence.width / 2.0);
    let rows = step_count(1.0);
    let offset_x = fence.x_offset * cosine;
    let offset_y = fence.x_offset * sine;
    let mut left = fence.position[0] - offset_x - sine;
    let mut top = fence.position[1] + cosine - offset_y;
    let x_step = cosine * 5.0;
    let y_step = sine * 5.0;
    let mut cells = BTreeSet::new();
    for _ in 0..rows {
        let (mut x, mut y) = (left, top);
        for _ in 0..columns {
            if let (Some(x), Some(y)) = (
                super::super::grid_coordinate((x + 0.5) / 10.0, grid.width() - 1),
                super::super::grid_coordinate((y + 0.5) / 10.0, grid.height() - 1),
            ) {
                cells.insert(Cell { x, y });
            }
            x += x_step;
            y += y_step;
        }
        left += y_step;
        top -= x_step;
    }
    cells
}

fn box_cells(footprint: &Footprint, grid: &Grid) -> BTreeSet<Cell> {
    let mut cells = BTreeSet::new();
    let (sine, cosine) = footprint.angle.sin_cos();
    let major = footprint.major;
    let minor = footprint.minor;
    let columns = step_count(major);
    let rows = step_count(minor);
    let major_cosine = major * cosine;
    let minor_sine = minor * sine;
    let minor_cosine = minor * cosine;
    let major_sine = major * sine;
    let mut left = footprint.position[0] - major_cosine - minor_sine;
    let mut top = footprint.position[1] + minor_cosine - major_sine;
    let x_step = cosine * 5.0;
    let y_step = sine * 5.0;
    for _ in 0..rows {
        let (mut x, mut y) = (left, top);
        for _ in 0..columns {
            if let (Some(x), Some(y)) = (
                super::super::grid_coordinate((x + 0.5) / 10.0, grid.width() - 1),
                super::super::grid_coordinate((y + 0.5) / 10.0, grid.height() - 1),
            ) {
                cells.insert(Cell { x, y });
            }
            x += x_step;
            y += y_step;
        }
        left += y_step;
        top -= x_step;
    }
    cells
}

fn circle_cells(footprint: &Footprint, grid: &Grid) -> BTreeSet<Cell> {
    let radius = footprint.major;
    let top = footprint
        .position
        .map(|position| crate::native_real::floor(0.5 + (position - radius) / 10.0) - 1);
    let center = footprint.position.map(|position| position / 10.0);
    let mut size = radius / 10.0;
    size += 0.4;
    let squared = size * size;
    let diameter = 2.0 * size;
    let bottom =
        top.map(|value| integer(value.to_f32().expect("native footprint cell") + diameter + 2.0));
    let mut cells = BTreeSet::new();
    for y in top[1]..bottom[1] {
        for x in top[0]..bottom[0] {
            let dx = x.to_f32().expect("native cell fits float") + 0.5 - center[0];
            let dy = y.to_f32().expect("native cell fits float") + 0.5 - center[1];
            let x_squared = dx * dx;
            let y_squared = dy * dy;
            if x_squared + y_squared <= squared
                && let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y))
                && x < grid.width() - 1
                && y < grid.height() - 1
            {
                cells.insert(Cell { x, y });
            }
        }
    }
    cells
}

fn integer(value: f32) -> i32 {
    value
        .to_i32()
        .expect("validated footprint fits native cell integer")
}

fn step_count(radius: f32) -> u32 {
    u32::try_from(crate::native_real::ceil(2.0 * radius / 5.0))
        .expect("validated footprint fits native integer steps")
}
