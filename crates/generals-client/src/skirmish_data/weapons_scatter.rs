//! Original fixed scatter offsets retain authored order, including repeated points.
use super::ContentLine;
use std::error::Error;

pub(super) fn load(lines: &[ContentLine]) -> Result<Vec<[f32; 2]>, Box<dyn Error>> {
    let mut points = Vec::new();
    for line in lines.iter().filter(|line| {
        line.depth == Some(0)
            && line
                .tokens
                .first()
                .is_some_and(|name| name == "ScatterTarget")
    }) {
        let mut point = [0.0; 2];
        for (axis, prefix) in ["X:", "Y:"].into_iter().enumerate() {
            let (index, value) = line
                .tokens
                .iter()
                .enumerate()
                .find_map(|(index, token)| token.strip_prefix(prefix).map(|value| (index, value)))
                .ok_or("scatter target coordinate missing")?;
            let value = if value.is_empty() {
                line.tokens
                    .get(index + 1)
                    .map(String::as_str)
                    .ok_or("scatter target value missing")?
            } else {
                value
            };
            let value: f32 = value.parse()?;
            if !value.is_finite() {
                return Err("nonfinite scatter target coordinate".into());
            }
            point[axis] = value;
        }
        points.push(point);
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn source_offsets_preserve_negative_coordinates_and_repeated_entries() {
        let source = content::scan(Asset {
            path: "weapon.ini".into(),
            origin: "native rocket pod pattern fields".into(),
            bytes: b"Weapon Pods\nScatterTargetScalar=50\nScatterTarget=X:0.133 Y:-0.200\nScatterTarget=X: -0.067 Y: 0.667\nScatterTarget=X:0.133 Y:-0.200\nEnd\n".to_vec(),
        }).unwrap();
        let points = load(&source.declarations[0].lines).unwrap();
        let expected = [[0.133_f32, -0.2], [-0.067, 0.667], [0.133, -0.2]];
        assert_eq!(
            points
                .into_iter()
                .map(|point| point.map(f32::to_bits))
                .collect::<Vec<_>>(),
            expected.map(|point| point.map(f32::to_bits))
        );
    }
}
