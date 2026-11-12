//! `AI::parseSideInfo` repeated-key merge and `AIPlayer` missing-side quota.
use super::BoxError;
use generals_assets::{AssetError, content, virtual_assets::VirtualAssets};

pub(super) fn load(assets: &VirtualAssets) -> Result<[i32; 3], BoxError> {
    let mut result = None;
    for prefix in ["data/ini/default/aidata", "data/ini/aidata"] {
        let standalone = format!("{prefix}.ini");
        match assets.read(&standalone) {
            Ok(asset) => merge_file(content::scan(asset)?, &mut result)?,
            Err(AssetError::Missing(_)) => {}
            Err(error) => return Err(error.into()),
        }
        for path in assets.list(prefix)? {
            if std::path::Path::new(&path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
            {
                merge_file(content::scan(assets.read(&path)?)?, &mut result)?;
            }
        }
    }
    // checkForSupplyCenter leaves desiredGatherers zero when its side has no row.
    Ok(result.unwrap_or([0; 3]))
}

fn merge_file(file: content::ContentFile, result: &mut Option<[i32; 3]>) -> Result<(), BoxError> {
    for declaration in file.declarations {
        if declaration.kind == "AIData" {
            merge(&declaration.lines, result)?;
        }
    }
    Ok(())
}

fn merge(lines: &[content::ContentLine], result: &mut Option<[i32; 3]>) -> Result<(), BoxError> {
    use crate::skirmish_data::{blocks, field, scalar};
    for block in blocks(lines, 0, "SideInfo")? {
        if block[0].tokens.get(1).is_none_or(|side| side != "Civilian") {
            continue;
        }
        let gatherers = result.get_or_insert([0, 1, 2]);
        for (index, key) in [
            "ResourceGatherersEasy",
            "ResourceGatherersNormal",
            "ResourceGatherersHard",
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(line) = field(block, 1, key) {
                gatherers[index] = scalar(line)?.parse()?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;
    #[test]
    fn civilian_side_creation_defaults_and_later_fields_merge_without_resetting() {
        let file = content::scan(Asset {path: "aidata.ini".into(), origin: "native side-info fixture".into(), bytes: b"AIData\nSideInfo America\nResourceGatherersNormal=5\nEnd\nSideInfo Civilian\nResourceGatherersNormal=3\nEnd\nSideInfo Civilian\nResourceGatherersHard=7\nEnd\nEnd\n".to_vec()}).unwrap();
        let mut result = None;
        merge(&file.declarations[0].lines, &mut result).unwrap();
        assert_eq!(result, Some([0, 3, 7]));
    }
}
