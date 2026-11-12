//! Aggregate original-roster parser admission; this does not prove runtime parity.
use super::*;

#[ignore = "requires original game installation"]
#[test]
fn installed_roster_collects_every_per_object_parser_admission_error() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut roster: Vec<_> = ROSTER
        .iter()
        .map(|(name, faction, kind)| ((*name).to_owned(), *faction, *kind))
        .collect();
    // Original pad is a real map catalog consumer, and brings all three P01
    // original delivery transport/payload closures into the same admission run.
    let map = generals_assets::map::MapData::parse(
        &assets
            .read("maps/rocky rampage/rocky rampage.map")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let original = source_files(&assets).unwrap();
    let pads: Vec<_> = map
        .objects
        .into_iter()
        .filter(|record| record.template == "TechReinforcementPad")
        .collect();
    assert_eq!(pads.len(), 1);
    roster.extend(
        garrisons::templates(&original, &pads)
            .unwrap()
            .into_iter()
            .map(|(name, kind)| (name.to_owned(), Faction::Civilian, kind)),
    );
    let (files, metadata) = source_files::with_runtime_dependencies(&assets, &mut roster).unwrap();
    let repulsors_enabled = module_repulsors_enabled(&assets).unwrap();
    let mut errors = Vec::new();
    for (name, faction, kind) in roster {
        let admitted = (|| -> Result<(), Box<dyn Error>> {
            let object = definition(&files, "Object", &name)?;
            let unit = unit_type(&files, object, faction, kind)?;
            let mut unit = finish_type(&files, object, unit)?;
            veterancy::load(&files, object, &mut unit)?;
            fake_buildings::load(&files, object, &mut unit)?;
            let model = normal_model(&assets, object)?;
            bind_authored_model(object, &model, &mut unit, &name)?;
            let ordered = metadata
                .get(&name)
                .ok_or("missing ordered module metadata")?;
            unit.update_modules = ordered.runtime_registrations(repulsors_enabled);
            unit.behavior_modules = ordered.runtime_behaviors(&files)?;
            generals_sim::skirmish::module_updates::validate_registrations(&unit)?;
            Ok(())
        })();
        if let Err(error) = admitted {
            errors.push(format!("{name}: {error}"));
        }
    }
    assert!(
        errors.is_empty(),
        "original per-object admission failures ({}):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
