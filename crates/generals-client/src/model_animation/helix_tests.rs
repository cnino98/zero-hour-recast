use super::*;

#[ignore = "requires original game installation"]
#[test]
fn installed_helix_bomb_wing_is_retained_and_upgrade_overrides_hidden_pose() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    let index =
        crate::strategy_verification::template(&battle.simulation, "ChinaVehicleHelix").unwrap();
    let draw = &animations.draws[index][0];
    assert_eq!(draw.upgrade_show, ["BombWing"]);
    for damaged in [false, true] {
        let flags = if damaged {
            BTreeSet::from(["REALLYDAMAGED".into()])
        } else {
            BTreeSet::new()
        };
        let state = &draw.states[draw.select(&flags)];
        let source = &models[state.model.unwrap()];
        let rendered = &models[state.render_model.unwrap()];
        let mut pose = source.controlled_pose(None, 0.0, &[]).unwrap();
        render_objects::hide_subobjects(source, &mut pose);
        let wings: Vec<_> = source
            .meshes
            .iter()
            .enumerate()
            .filter_map(|(index, mesh)| {
                mesh.name
                    .rsplit('.')
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("BombWing"))
                    .then_some(index)
            })
            .collect();
        assert!(
            !wings.is_empty(),
            "installed Helix model lacks BombWing meshes"
        );
        for &wing in &wings {
            assert!(!pose[wing].visible);
            assert_eq!(rendered.meshes[wing].attributes & 0x1000, 0);
            assert!(!pose[wing].positions.is_empty());
        }
        let overrides: Vec<_> = draw
            .upgrade_show
            .iter()
            .map(|name| generals_sim::skirmish::SubObjectOverride {
                name: name.clone(),
                visible: true,
            })
            .collect();
        render_objects::apply_subobject_overrides(source, &mut pose, &overrides);
        assert!(wings.iter().all(|&wing| pose[wing].visible));
    }
}
