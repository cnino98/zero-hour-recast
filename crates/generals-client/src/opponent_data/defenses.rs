//! `AIData::SideInfo` defense selection and all authored build-list geometry.
use crate::skirmish_data::{blocks, definition, field, scalar};
use generals_assets::{
    content::{self, ContentFile, ContentLine},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::AiBuilding;
use std::error::Error;

pub(super) fn templates(assets: &VirtualAssets) -> Result<Vec<ContentFile>, Box<dyn Error>> {
    let mut files = vec![content::scan(
        assets.read("data/ini/object/factionbuilding.ini")?,
    )?];
    crate::skirmish_data::resolve_reskins(&mut files)?;
    Ok(files)
}

pub(super) fn buildings(
    lines: &[ContentLine],
    side: &str,
    files: &[ContentFile],
) -> Result<Vec<AiBuilding>, Box<dyn Error>> {
    let build_data = super::named_block(lines, "SkirmishBuildList", side)?;
    let mut buildings = blocks(build_data, 1, "Structure")?
        .into_iter()
        .map(super::building)
        .collect::<Result<Vec<_>, _>>()?;
    for building in &mut buildings {
        if !building.name.is_empty() {
            geometry(files, building)?;
        }
    }
    Ok(buildings)
}

fn geometry(files: &[ContentFile], building: &mut AiBuilding) -> Result<(), Box<dyn Error>> {
    let object = definition(files, "Object", &building.name)?;
    let shape = crate::skirmish_data::geometry(&object.lines)?;
    building.bounding_radius =
        if matches!(shape.shape, generals_sim::collision_geometry::Shape::Box) {
            let squared = [shape.major * shape.major, shape.minor * shape.minor];
            (squared[0] + squared[1]).sqrt()
        } else {
            shape.major
        };
    Ok(())
}

pub(super) fn template(lines: &[ContentLine], side: &str) -> Result<String, Box<dyn Error>> {
    let mut result = String::new();
    for block in blocks(lines, 0, "SideInfo")? {
        if block[0].tokens.get(1).is_some_and(|name| name == side)
            && let Some(line) = field(block, 1, "BaseDefenseStructure1")
        {
            scalar(line)?.clone_into(&mut result);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;

    #[test]
    fn side_defense_overrides_merge_without_erasing_unmentioned_fields() {
        let file = content::scan(Asset { path: "aidata.ini".into(), origin: "fixture".into(),
            bytes: b"AIData\nSideInfo America\nBaseDefenseStructure1=AmericaPatriotBattery\nEnd\nSideInfo China\nBaseDefenseStructure1=ChinaGattlingCannon\nEnd\nSideInfo America\nResourceGatherersEasy=3\nEnd\nEnd\n".to_vec() }).unwrap();
        let lines = &file.declarations[0].lines;
        assert_eq!(template(lines, "America").unwrap(), "AmericaPatriotBattery");
        assert_eq!(template(lines, "China").unwrap(), "ChinaGattlingCannon");
        assert!(template(lines, "GLA").unwrap().is_empty());
    }

    #[test]
    fn unrepresented_build_list_templates_still_supply_original_base_circle_geometry() {
        let file = content::scan(Asset { path: "factionbuilding.ini".into(), origin: "fixture".into(),
            bytes: b"Object UnrepresentedAirfield\nGeometry=BOX\nGeometryMajorRadius=30\nGeometryMinorRadius=40\nGeometryHeight=10\nEnd\n".to_vec() }).unwrap();
        let mut building = AiBuilding {
            name: "UnrepresentedAirfield".into(),
            bounding_radius: 0.0,
            position: [0.0; 2],
            angle: 0.0,
            rebuilds: 0,
            automatic: false,
        };
        geometry(&[file], &mut building).unwrap();
        assert_eq!(building.bounding_radius.to_bits(), 50.0_f32.to_bits());
    }
}
