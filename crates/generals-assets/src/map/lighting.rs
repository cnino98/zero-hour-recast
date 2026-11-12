//! `WorldHeightMap::ParseLightingDataChunk`, pinned revision cited in `map`.

use super::{finish, real};
use crate::{AssetError, binary::Reader};

/// RGB light values and native world-space light direction/position.
pub struct MapLight {
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub position: [f32; 3],
}

/// Terrain and object lights are separate arrays in the original map.
pub struct LightingSet {
    pub terrain: [MapLight; 3],
    pub objects: [MapLight; 3],
}

pub struct MapLighting {
    /// Source enum: 1 morning, 2 afternoon, 3 evening, 4 night.
    pub time_of_day: u32,
    /// Morning, afternoon, evening, night, in file order.
    pub times: [LightingSet; 4],
    pub shadow_color: Option<u32>,
}

impl MapLighting {
    pub(super) fn parse(bytes: &[u8], version: u16) -> Result<Self, AssetError> {
        if !(1..=3).contains(&version) {
            return Err(AssetError::Invalid("unsupported map lighting version"));
        }
        let mut reader = Reader::new(bytes);
        let time_of_day = reader.le_u32()?;
        if !(1..=4).contains(&time_of_day) {
            return Err(AssetError::Invalid("invalid map time of day"));
        }
        let mut times = std::array::from_fn(|_| LightingSet {
            terrain: std::array::from_fn(|_| unlit()),
            objects: std::array::from_fn(|_| unlit()),
        });
        for time in &mut times {
            time.terrain[0] = read_light(&mut reader)?;
            time.objects[0] = read_light(&mut reader)?;
            if version >= 2 {
                for light in &mut time.objects[1..] {
                    *light = read_light(&mut reader)?;
                }
            }
            if version >= 3 {
                for light in &mut time.terrain[1..] {
                    *light = read_light(&mut reader)?;
                }
            }
        }
        let shadow_color = if reader.is_empty() {
            None
        } else {
            Some(reader.le_u32()?)
        };
        finish(&reader)?;
        Ok(Self {
            time_of_day,
            times,
            shadow_color,
        })
    }

    #[must_use]
    pub fn active(&self) -> Option<&LightingSet> {
        self.time_of_day
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.times.get(index))
    }
}

const fn unlit() -> MapLight {
    MapLight {
        ambient: [0.0; 3],
        diffuse: [0.0; 3],
        position: [0.0, 0.0, -1.0],
    }
}

fn read_light(reader: &mut Reader<'_>) -> Result<MapLight, AssetError> {
    Ok(MapLight {
        ambient: [real(reader)?, real(reader)?, real(reader)?],
        diffuse: [real(reader)?, real(reader)?, real(reader)?],
        position: [real(reader)?, real(reader)?, real(reader)?],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighting_versions_preserve_order_and_default_missing_extra_lights() {
        let mut bytes = 2_u32.to_le_bytes().to_vec();
        for time in 0..4_u8 {
            for light in 0..2_u8 {
                let value = f32::from(time * 2 + light);
                for _ in 0..9 {
                    bytes.extend(value.to_le_bytes());
                }
            }
        }
        let lighting = MapLighting::parse(&bytes, 1).expect("old lighting");
        let active = lighting.active().expect("afternoon");
        assert_eq!(
            active.terrain[0].ambient.map(f32::to_bits),
            [2.0_f32.to_bits(); 3]
        );
        assert_eq!(
            active.objects[0].ambient.map(f32::to_bits),
            [3.0_f32.to_bits(); 3]
        );
        assert_eq!(
            active.objects[1].position.map(f32::to_bits),
            [0.0_f32, 0.0, -1.0].map(f32::to_bits)
        );
        assert_eq!(
            active.terrain[2].diffuse.map(f32::to_bits),
            [0.0_f32.to_bits(); 3]
        );
        assert!(MapLighting::parse(&bytes[..bytes.len() - 1], 1).is_err());
        assert!(MapLighting::parse(&bytes, 3).is_err());
    }

    #[test]
    fn version_three_distinguishes_extra_object_and_terrain_lights() {
        let mut bytes = 1_u32.to_le_bytes().to_vec();
        for _ in 0..4 {
            for light in 0..6_u8 {
                for _ in 0..9 {
                    bytes.extend(f32::from(light).to_le_bytes());
                }
            }
        }
        bytes.extend(0xff00_0080_u32.to_le_bytes());
        let lighting = MapLighting::parse(&bytes, 3).expect("six lights per time of day");
        let active = lighting.active().expect("morning");
        assert_eq!(
            active.objects[1].diffuse.map(f32::to_bits),
            [2.0_f32.to_bits(); 3]
        );
        assert_eq!(
            active.terrain[1].diffuse.map(f32::to_bits),
            [4.0_f32.to_bits(); 3]
        );
        assert_eq!(lighting.shadow_color, Some(0xff00_0080));
        bytes.push(0);
        assert!(MapLighting::parse(&bytes, 3).is_err());
    }
}
