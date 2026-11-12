use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use std::collections::BTreeSet;

#[ignore = "requires original game installation"]
#[test]
fn original_tournament_b_streetlight_keeps_its_missing_damaged_draw_invisible() {
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let bytes = assets
        .read("maps/tournamentb/tournamentb.map")
        .unwrap()
        .bytes;
    let objects: Vec<_> = MapData::parse(&bytes)
        .unwrap()
        .objects
        .into_iter()
        .filter(|object| object.template == "StreetLight03")
        .collect();
    assert!(!objects.is_empty());
    assert!(matches!(
        assets.read("art/w3d/pmlitpol01_d.w3d"),
        Err(generals_assets::AssetError::Missing(_))
    ));
    let mut data = crate::skirmish_data::load(&assets, &objects).unwrap();
    let animation = super::load(&assets, &mut data.models, &data.types).unwrap();
    let index = data
        .types
        .iter()
        .position(|unit| unit.name == "StreetLight03")
        .unwrap();
    let draw = &animation.draws[index][0];
    let normal = &draw.states[draw.select(&BTreeSet::new())];
    assert!(normal.model.is_some());
    assert!(normal.render_model.is_some());
    for conditions in [
        BTreeSet::from(["DAMAGED".into()]),
        BTreeSet::from(["DAMAGED".into(), "REALLYDAMAGED".into()]),
    ] {
        let state = &draw.states[draw.select(&conditions)];
        assert!(state.name.eq_ignore_ascii_case("PMLitPol01_D"));
        assert!(state.model.is_none());
        assert!(state.render_model.is_none());
    }
    // The original template declares DAMAGED only; it has no REALLYDAMAGED alias.
    let unmatched = &draw.states[draw.select(&BTreeSet::from(["REALLYDAMAGED".into()]))];
    assert_eq!(unmatched.model, normal.model);
}

#[ignore = "requires original game installation"]
#[test]
fn original_outhouse_mesh_ignores_its_authored_hierarchy_animation() {
    let (assets, _) = crate::cleanup_verification::tests::installed();
    let bytes = assets
        .read("maps/tournament plains/tournament plains.map")
        .unwrap()
        .bytes;
    let objects: Vec<_> = MapData::parse(&bytes)
        .unwrap()
        .objects
        .into_iter()
        .filter(|object| object.template == "Outhouse02")
        .collect();
    assert!(!objects.is_empty());
    let mut data = crate::skirmish_data::load(&assets, &objects).unwrap();
    let animation = super::load(&assets, &mut data.models, &data.types).unwrap();
    let index = data
        .types
        .iter()
        .position(|unit| unit.name == "Outhouse02")
        .unwrap();
    let draw = &animation.draws[index][0];
    let state = &draw.states[draw.select(&BTreeSet::from(["GARRISONED".into()]))];
    assert!(state.name.eq_ignore_ascii_case("PMOuthouse02"));
    assert_eq!(state.motions.len(), 1);
    assert!(
        state.motions[0]
            .name
            .eq_ignore_ascii_case("PMOUTHOUSE02_G.PMOUTHOUSE02_G")
    );
    let original = assets.read("art/w3d/pmouthouse02.w3d").unwrap();
    assert!(
        generals_assets::w3d::Model::hierarchy_name(&original.bytes)
            .unwrap()
            .is_none()
    );
    let model = &data.models[state.model.unwrap()];
    let clip = &animation.clips[&state.motions[0].name];
    for frame in [
        0.0,
        f32::from(clip.frame_count - 1) * 0.5,
        f32::from(clip.frame_count - 1),
    ] {
        let poses = model.pose(clip, frame).unwrap();
        assert_eq!(poses.len(), model.meshes.len());
        for (pose, mesh) in poses.iter().zip(&model.meshes) {
            assert!(pose.visible);
            assert_eq!(pose.positions, mesh.positions);
            assert_eq!(pose.normals, mesh.normals);
            assert_eq!(pose.bound_transform, mesh.bound_transform);
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn original_map_civilian_props_load_models_audio_ui_and_death_effect_dependencies() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let names = [
        "55GallonDrum",
        "Barrel",
        "FirePit",
        "MarketVender01",
        "JunkDealer",
    ];
    let mut objects = Vec::new();
    for map in [
        crate::playtest::MAP_NAME,
        "maps/final crusade/final crusade.map",
        "maps/golden oasis/golden oasis.map",
        "maps/eastern everglades/eastern everglades.map",
    ] {
        let bytes = assets.read(map).unwrap().bytes;
        objects.extend(
            MapData::parse(&bytes)
                .unwrap()
                .objects
                .into_iter()
                .filter(|object| names.contains(&object.template.as_str())),
        );
    }
    let actual: BTreeSet<_> = objects
        .iter()
        .map(|object| object.template.as_str())
        .collect();
    assert_eq!(actual, names.into_iter().collect());
    let mut data = crate::skirmish_data::load(&assets, &objects).unwrap();
    let animation = super::load(&assets, &mut data.models, &data.types).unwrap();
    assert_original_models(&animation, &data.models, &data.types);
    crate::projectile_data::load(&assets, &mut data.types, &data.models, &animation).unwrap();
    crate::particles::load(&assets, &data.types).unwrap();
    let sounds = crate::particles::object_fx_sounds(&assets, &data.types).unwrap();
    assert_eq!(sounds["FX_BarrelDeathExplosion"], ["ExplosionBarrel"]);
    assert!(sounds.contains_key("FX_PropExplode"));
    assert_eq!(sounds["FX_PropCrush"], ["CarCrush"]);
    let options = root.join("UserData/Options.ini");
    crate::source_audio::load(&assets, &data.types, &options).unwrap();
    let ui = crate::control_bar::load(&assets, &data.types, &options).unwrap();
    for name in names {
        let index = data
            .types
            .iter()
            .position(|unit| unit.name == name)
            .unwrap();
        assert!(
            !ui.units[index].name.is_empty(),
            "missing original display name for {name}"
        );
    }
}

fn assert_original_models(
    animation: &super::Animations,
    models: &[generals_assets::w3d::Model],
    types: &[generals_sim::skirmish::UnitType],
) {
    for (name, model, damaged) in [
        ("55GallonDrum", "PMgaldrum", Some("PMgaldrum_D")),
        ("Barrel", "PMBarrel01", None),
        ("FirePit", "PMFirepit", Some("PMFirepit_D")),
        ("MarketVender01", "PMOpmarkV1", None),
        ("JunkDealer", "PAJunkdlr", None),
    ] {
        let index = types.iter().position(|unit| unit.name == name).unwrap();
        let draw = &animation.draws[index][0];
        let state = &draw.states[draw.select(&BTreeSet::new())];
        assert!(
            state.name.eq_ignore_ascii_case(model),
            "wrong original model for {name}"
        );
        assert!(
            state
                .model
                .is_some_and(|index| !models[index].meshes.is_empty())
        );
        if let Some(damaged) = damaged {
            let flags = BTreeSet::from(["DAMAGED".into()]);
            let state = &draw.states[draw.select(&flags)];
            assert!(state.name.eq_ignore_ascii_case(damaged));
            assert!(
                state
                    .model
                    .is_some_and(|index| !models[index].meshes.is_empty())
            );
        }
    }
}
