//! `PolygonTrigger::ParsePolygonTriggersDataChunk`, `RETAIL_COMPATIBLE_CRC=0`.
use super::{ascii, finish, integer};
use crate::{AssetError, binary::Reader};

pub struct PolygonTrigger {
    pub name: String,
    pub layer: String,
    pub id: i32,
    pub water: bool,
    pub river: bool,
    pub river_start: i32,
    pub points: Vec<[i32; 3]>,
}

pub(super) fn read(bytes: &[u8], version: u16) -> Result<Vec<PolygonTrigger>, AssetError> {
    // Version 1 synthesizes a water area from global water extents; that older
    // map dependency is not part of the current Zero Hour map profile.
    if !(2..=4).contains(&version) {
        return Err(AssetError::Invalid("unsupported polygon trigger version"));
    }
    let mut reader = Reader::new(bytes);
    let count = integer(&mut reader)?;
    if !(0..=100_000).contains(&count) {
        return Err(AssetError::Invalid("invalid polygon trigger count"));
    }
    let mut triggers = Vec::new();
    for _ in 0..count {
        let name = ascii(&mut reader)?;
        let layer = if version >= 4 {
            ascii(&mut reader)?
        } else {
            String::new()
        };
        let id = integer(&mut reader)?;
        let water = reader.byte()? != 0;
        let river = version >= 3 && reader.byte()? != 0;
        let river_start = if version >= 3 {
            integer(&mut reader)?
        } else {
            0
        };
        let count = integer(&mut reader)?;
        if !(0..=100_000).contains(&count) {
            return Err(AssetError::Invalid("invalid polygon point count"));
        }
        let mut points = Vec::new();
        for _ in 0..count {
            points.push([
                integer(&mut reader)?,
                integer(&mut reader)?,
                integer(&mut reader)?,
            ]);
        }
        // The selected SuperHackers fix drops degenerate triggers.
        if points.len() >= 2 {
            triggers.push(PolygonTrigger {
                name,
                layer,
                id,
                water,
                river,
                river_start,
                points,
            });
        }
    }
    finish(&reader)?;
    Ok(triggers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_degenerate_polygon_without_losing_the_following_trigger() {
        let mut bytes = 2_i32.to_le_bytes().to_vec();
        for count in [1_i32, 3] {
            bytes.extend(4_u16.to_le_bytes());
            bytes.extend(b"Base");
            bytes.extend(count.to_le_bytes());
            bytes.extend([0, 0]);
            bytes.extend(0_i32.to_le_bytes());
            bytes.extend(count.to_le_bytes());
            for point in 0..count {
                bytes.extend(point.to_le_bytes());
                bytes.extend((point * 2).to_le_bytes());
                bytes.extend(7_i32.to_le_bytes());
            }
        }
        let triggers = read(&bytes, 3).unwrap();
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].points, [[0, 0, 7], [1, 2, 7], [2, 4, 7]]);
        bytes.pop();
        assert!(matches!(
            read(&bytes, 3),
            Err(AssetError::Invalid("truncated input"))
        ));
    }
}
