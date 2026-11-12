//! Ground admission from `Pathfinder::classifyObjectFootprint` and
//! `Object::isMobile`; map Z is an offset, not a ground-blocking footprint.

use super::{Declaration, Error, MapObject, field, number, value};
use generals_assets::map::MapValue;

pub(super) fn admitted(
    chain: &[&Declaration],
    record: &MapObject,
    ground: f32,
) -> Result<bool, Box<dyn Error>> {
    let base = chain.last().copied().ok_or("Empty obstacle ancestry")?;
    let kind = field(base, "KindOf").unwrap_or(&[]);
    let has = |flag| kind.iter().any(|value| value == flag);
    if has("MINE") || has("PROJECTILE") || has("BRIDGE_TOWER") {
        return Ok(false);
    }
    if fence(base, record)?.is_some() {
        return Ok(false);
    }
    let disabled = ["objectEnabled", "objectPowered"]
        .into_iter()
        .any(|key| matches!(record.properties.get(key), Some(MapValue::Bool(false))));
    if !has("STRUCTURE") || (!has("IMMOBILE") && !disabled) {
        return Ok(false);
    }
    let small = chain
        .iter()
        .find_map(|object| value(object, "GeometryIsSmall"))
        .is_some_and(|value| value.eq_ignore_ascii_case("Yes"));
    if small {
        return Ok(false);
    }
    // GameLogic adds the map offset to ground height; Thing then subtracts
    // ground height in calculateHeightAboveTerrain. Preserve both operations.
    let world_height = ground + record.position[2];
    let above_terrain = world_height - ground;
    Ok(above_terrain <= 10.0 || has("BLAST_CRATER"))
}

pub(super) fn fence(
    base: &Declaration,
    record: &MapObject,
) -> Result<Option<generals_sim::skirmish::MapFence>, Box<dyn Error>> {
    let width = number(base, "FenceWidth", 0.0)?;
    if width <= 0.0
        || field(base, "KindOf")
            .is_some_and(|kinds| kinds.iter().any(|kind| kind == "DEFENSIVE_WALL"))
    {
        return Ok(None);
    }
    Ok(Some(generals_sim::skirmish::MapFence {
        position: [record.position[0], record.position[1]],
        angle: record.angle,
        width,
        x_offset: number(base, "FenceXOffset", 0.0)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};
    use std::collections::BTreeMap;

    fn object(text: &str) -> Declaration {
        content::scan(Asset {
            path: "data/ini/object/fixture.ini".to_owned(),
            origin: "fixture".to_owned(),
            bytes: text.as_bytes().to_vec(),
        })
        .unwrap()
        .declarations
        .remove(0)
    }

    fn record(height: f32) -> MapObject {
        MapObject {
            template: "FortressWallLargeArch".to_owned(),
            position: [2088.425, 1258.1366, height],
            angle: std::f32::consts::FRAC_PI_2,
            flags: 0,
            properties: BTreeMap::new(),
        }
    }

    #[test]
    fn fence_admission_precedes_structure_height_and_geometry_checks() {
        let fence_object = object(
            "Object Fence\nKindOf=NO_COLLIDE\nFenceWidth=30\nFenceXOffset=15\nGeometryIsSmall=Yes\nEnd\n",
        );
        let record = record(100.0);
        let data = fence(&fence_object, &record).unwrap().unwrap();
        assert_eq!(data.width.to_bits(), 30.0_f32.to_bits());
        assert_eq!(data.x_offset.to_bits(), 15.0_f32.to_bits());
        assert!(!admitted(&[&fence_object], &record, 0.0).unwrap());
        let wall =
            object("Object Wall\nKindOf=STRUCTURE IMMOBILE DEFENSIVE_WALL\nFenceWidth=30\nEnd\n");
        assert!(fence(&wall, &record).unwrap().is_none());
    }

    #[test]
    fn raised_fallen_empire_arches_leave_ground_open_with_exact_height_boundary() {
        let arch = object(
            "Object FortressWallLargeArch\nKindOf=STRUCTURE SELECTABLE IMMOBILE WALK_ON_TOP_OF_WALL\nGeometryIsSmall=NO\nEnd\n",
        );
        for height in [27.0, 30.0, 32.0, 33.0, 34.0] {
            assert!(!admitted(&[&arch], &record(height), 18.75).unwrap());
        }
        assert!(admitted(&[&arch], &record(10.0), 18.75).unwrap());
        assert!(!admitted(&[&arch], &record(10.25), 18.75).unwrap());
        assert!(admitted(&[&arch], &record(-7.0), 18.75).unwrap());
        let crater = object("Object Crater\nKindOf=STRUCTURE IMMOBILE BLAST_CRATER\nEnd\n");
        assert!(admitted(&[&crater], &record(33.0), 18.75).unwrap());
    }

    #[test]
    fn source_mobility_small_reskin_and_kind_exclusions_control_admission() {
        let base = object("Object Base\nKindOf=STRUCTURE IMMOBILE NO_COLLIDE\nEnd\n");
        let small = object("ObjectReskin Small Base\nGeometryIsSmall=Yes\nEnd\n");
        assert!(admitted(&[&base], &record(0.0), 0.0).unwrap());
        assert!(!admitted(&[&small, &base], &record(0.0), 0.0).unwrap());
        let mobile = object("Object Mobile\nKindOf=STRUCTURE\nEnd\n");
        assert!(!admitted(&[&mobile], &record(0.0), 0.0).unwrap());
        for flag in ["objectEnabled", "objectPowered"] {
            let mut disabled = record(0.0);
            disabled
                .properties
                .insert(flag.to_owned(), MapValue::Bool(false));
            assert!(admitted(&[&mobile], &disabled, 0.0).unwrap());
        }
        for flag in ["MINE", "PROJECTILE", "BRIDGE_TOWER"] {
            let excluded = object(&format!(
                "Object Excluded\nKindOf=STRUCTURE IMMOBILE {flag}\nEnd\n"
            ));
            assert!(!admitted(&[&excluded], &record(0.0), 0.0).unwrap());
        }
    }
}
