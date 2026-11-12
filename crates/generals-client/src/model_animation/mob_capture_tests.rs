use super::*;

#[ignore = "requires original game installation"]
#[test]
fn installed_mob_capture_names_match_original_bodies_and_active_ak_pose() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    for (template, body_name) in [
        ("GLAInfantryAngryMobPistol01", "UIMOB01_SKN.UIMOB01_SKN"),
        ("GLAInfantryAngryMobRock02", "UIMOB02_SKN.UIMOB02_SKN"),
        ("GLAInfantryAngryMobMolotov02", "UIMOB02_SKN.UIMOB02_SKN"),
        ("GLAInfantryAngryMobPistol03", "UIMOB03_SKN.UIMOB03_SKIN"),
        ("GLAInfantryAngryMobRock04", "UIMOB04_SKN.UIMOB04_SKIN"),
        ("GLAInfantryAngryMobPistol05", "UIMOB05_SKN.UIMOB05_SKIN"),
    ] {
        let index = crate::strategy_verification::template(&battle.simulation, template).unwrap();
        let unit = &battle.simulation.catalog[index];
        let ak_capable = (0..unit.weapon_template_count()).any(|weapon| {
            unit.weapon_template(weapon)
                .is_some_and(|rules| rules.name == "GLAAngryMobAK47Weapon")
        });
        assert_eq!(ak_capable, template != "GLAInfantryAngryMobMolotov02");
        let draw = &animations.draws[index][0];
        for upgraded in [false, true] {
            let flags = if upgraded {
                BTreeSet::from(["WEAPONSET_PLAYER_UPGRADE".into(), "FIRING_B".into()])
            } else {
                BTreeSet::new()
            };
            let state = &draw.states[draw.select(&flags)];
            let model = &models[state.model.unwrap()];
            let body = model
                .meshes
                .iter()
                .position(|mesh| mesh.name.eq_ignore_ascii_case(body_name))
                .unwrap();
            let ak = model
                .meshes
                .iter()
                .position(|mesh| mesh.name.rsplit('.').next() == Some("AK47"))
                .unwrap();
            assert_eq!(model.meshes[body].attributes & 0x1000, 0);
            assert!(!model.meshes[body].indices.is_empty());
            assert!(model.meshes[body].positions.len() > 100);
            let clip = &animations.clips[&state.motions[0].name];
            let pose = model.controlled_pose(Some(clip), 0.0, &[]).unwrap();
            assert!(pose[body].visible);
            assert!(
                pose[body]
                    .positions
                    .iter()
                    .flatten()
                    .all(|coordinate| coordinate.abs() < 100.0)
            );
            assert!(pose[ak].visible);
            if upgraded && ak_capable {
                assert!(
                    pose[ak]
                        .positions
                        .iter()
                        .flatten()
                        .all(|coordinate| coordinate.abs() < 100.0),
                    "active AK remains buried: {template}"
                );
            } else {
                assert!(
                    pose[ak]
                        .positions
                        .iter()
                        .flatten()
                        .any(|coordinate| coordinate.abs() > 1000.0),
                    "original idle AK burial missing: {template}"
                );
            }
        }
    }
}
