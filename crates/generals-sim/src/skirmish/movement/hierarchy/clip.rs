//! `Line2D.cpp::ClipLine2D`, integer division truncates toward zero.
use crate::grid::Cell;
use num_traits::ToPrimitive;

pub(super) fn world_line(from: [f32; 3], to: [f32; 3], extent: Cell) -> ([f32; 3], [f32; 3]) {
    let cell = |position: [f32; 3]| {
        [
            (position[0] / 10.0)
                .floor()
                .to_i32()
                .expect("admitted native world X"),
            (position[1] / 10.0)
                .floor()
                .to_i32()
                .expect("admitted native world Y"),
        ]
    };
    let start = cell(from);
    let end = cell(to);
    let Some([first, second]) = line(
        start,
        end,
        [i32::from(extent.x) - 1, i32::from(extent.y) - 1],
    ) else {
        return (from, to);
    };
    let world = |mut position: [f32; 3], original: [i32; 2], clipped: [i32; 2]| {
        if original != clipped {
            let horizontal = clipped[0].to_f32().expect("bounded clip X") * 10.0;
            let vertical = clipped[1].to_f32().expect("bounded clip Y") * 10.0;
            position[0] = horizontal + 0.05;
            position[1] = vertical + 0.05;
        }
        position
    };
    (world(from, start, first), world(to, end, second))
}

const fn code([x, y]: [i32; 2], [right, bottom]: [i32; 2]) -> u8 {
    let horizontal = if x < 0 {
        1
    } else if x > right {
        2
    } else {
        0
    };
    let vertical = if y < 0 {
        8
    } else if y > bottom {
        4
    } else {
        0
    };
    horizontal | vertical
}

fn line(mut first: [i32; 2], mut second: [i32; 2], extent: [i32; 2]) -> Option<[[i32; 2]; 2]> {
    let first_code = code(first, extent);
    let second_code = code(second, extent);
    if first_code | second_code == 0 {
        return Some([first, second]);
    }
    if first_code & second_code != 0 {
        return None;
    }
    if first_code != 0 {
        clip_endpoint(&mut first, second, first_code, extent)?;
    }
    if second_code != 0 {
        clip_endpoint(&mut second, first, second_code, extent)?;
    }
    (code(first, extent) | code(second, extent) == 0).then_some([first, second])
}

const fn clip_endpoint(
    point: &mut [i32; 2],
    other: [i32; 2],
    code: u8,
    [right, bottom]: [i32; 2],
) -> Option<()> {
    if code & 0b1100 != 0 {
        let boundary = if code & 8 != 0 { 0 } else { bottom };
        let difference = other[1] - point[1];
        if difference == 0 {
            return None;
        }
        point[0] += (other[0] - point[0]) * (boundary - point[1]) / difference;
        point[1] = boundary;
    }
    if point[0] > right || point[0] < 0 {
        let boundary = if point[0] > right { right } else { 0 };
        let difference = other[0] - point[0];
        if difference == 0 {
            return None;
        }
        point[1] += (other[1] - point[1]) * (boundary - point[0]) / difference;
        point[0] = boundary;
    }
    Some(())
}
