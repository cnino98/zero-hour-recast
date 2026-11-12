use super::*;
use generals_assets::virtual_assets::VirtualAssets;

const COLLAPSE: [&str; 9] = [
    "AsianBank",
    "AsianOffice02",
    "AsianSkyScraper01",
    "AsianTVStation",
    "CivilianHighrise01",
    "CivilianHighrise02",
    "EuroHighrise",
    "InternationalHotel01",
    "MogadishuHighrise01",
];
const TOPPLE: [&str; 5] = [
    "MogadishuTower04",
    "MogadishuTower05",
    "StanTallTower",
    "StanWatchTowerTall",
    "TallTower02",
];

#[ignore = "requires original game installation"]
#[test]
fn installed_fourteen_structures_keep_phase_objects_crushing_weapons_and_damage_effects() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let mut roster = COLLAPSE
        .into_iter()
        .chain(TOPPLE)
        .map(|name| {
            (
                name.into(),
                generals_sim::skirmish::Faction::Civilian,
                generals_sim::skirmish::Kind::TechBuilding,
            )
        })
        .collect();
    let (files, metadata) =
        super::super::source_files::with_runtime_dependencies(&assets, &mut roster).unwrap();
    let defaults = definition(&files, "Object", "DefaultThingTemplate").unwrap();
    let default_scale = field(&defaults.lines, 0, "Scale")
        .map(scalar)
        .transpose()
        .unwrap()
        .unwrap_or("1.0")
        .parse::<f32>()
        .unwrap();
    for name in COLLAPSE.into_iter().chain(TOPPLE) {
        let object = definition(&files, "Object", name).unwrap();
        let scale = field(&object.lines, 0, "Scale")
            .map(scalar)
            .transpose()
            .unwrap()
            .map(str::parse::<f32>)
            .transpose()
            .unwrap()
            .unwrap_or(default_scale);
        assert_eq!(
            scale.to_bits(),
            1.0_f32.to_bits(),
            "{name} requires SourceScale in actor presentation"
        );
        let mut lifecycle = super::super::death_lifecycle::load(&files, object)
            .unwrap()
            .unwrap();
        let rules = load(&files, object, Some(&mut lifecycle)).unwrap().unwrap();
        let mut unit = super::super::unit_type(
            &files,
            object,
            generals_sim::skirmish::Faction::Civilian,
            generals_sim::skirmish::Kind::TechBuilding,
        )
        .unwrap();
        unit.structure_death = Some(rules);
        unit.behavior_modules = metadata[name].runtime_behaviors(&files).unwrap();
        unit.update_modules = metadata[name].runtime_registrations(true);
        generals_sim::skirmish::module_updates::validate_registrations(&unit).unwrap();
        let expected = if COLLAPSE.contains(&name) {
            "StructureCollapseUpdate"
        } else {
            "StructureToppleUpdate"
        };
        assert_eq!(unit.update_modules.iter().filter(|entry| matches!(entry, generals_sim::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == expected)).count(), 1);
        let rules = unit.structure_death.as_ref().unwrap();
        verify_phase_effects(&assets, rules, &lifecycle);
        if matches!(name, "MogadishuTower04" | "MogadishuTower05") {
            let bones = super::super::bone_fx::load(&files, object).unwrap();
            assert_eq!(bones[0].slots[3][0].fx.as_ref().unwrap().delay, 90);
            assert_eq!(bones[0].slots[3][0].particle.as_ref().unwrap().delay, 90);
            assert_eq!(rules.transitions.len(), 2);
            assert!(rules.transitions[0].suppress_water);
            assert!(!rules.transitions[1].suppress_water);
        } else {
            assert!(
                super::super::bone_fx::load(&files, object)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

fn verify_phase_effects(
    assets: &VirtualAssets,
    rules: &Rules,
    lifecycle: &generals_sim::skirmish::death_lifecycle::Rules,
) {
    match &rules.module {
        Module::Collapse(module) => {
            assert_eq!(module.delay, [15, 30]);
            assert_eq!(module.burst_delay, [8, 24]);
            assert_eq!(rules.physical_lists().count(), 3);
            assert_eq!(rules.phases[2].effects, ["FX_StructureCollapseBurst"]);
            for model in rules
                .physical_lists()
                .flat_map(|list| &list.nuggets)
                .flat_map(|nugget| &nugget.names)
            {
                assert!(
                    !assets
                        .read(&format!("art/w3d/{model}.w3d"))
                        .unwrap()
                        .bytes
                        .is_empty()
                );
            }
        }
        Module::Topple(module) => {
            assert_eq!(module.delay, [8, 15]);
            assert_eq!(module.burst_delay, [45, 75]);
            assert_eq!(
                lifecycle.weapons[module.crushing_weapon].name,
                "ToppledStructureWeapon"
            );
            assert!(module.suppress_water_fx);
        }
    }
}

#[test]
fn transition_slot_order_follows_native_offsets_and_rejects_untranslated_random_bones() {
    let text = "Object Tower\nBehavior=TransitionDamageFX FX\nRubbleParticleSystem4=Bone:None RandomBone:No PSys:Flare\nRubbleParticleSystem1=Bone:None RandomBone:No PSys:Smoke\nEnd\nEnd";
    let scan = |text: &str| {
        generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "structure.ini".into(),
            origin: "authored slots".into(),
            bytes: text.as_bytes().to_vec(),
        })
        .unwrap()
    };
    let file = scan(text);
    let rules = transitions::load(&file.declarations[0].lines).unwrap();
    assert_eq!(
        rules[0].particles[3]
            .iter()
            .map(|particle| particle.template.as_str())
            .collect::<Vec<_>>(),
        ["Smoke", "Flare"]
    );
    let unsupported = scan(&text.replace("RandomBone:No", "RandomBone:Yes"));
    assert!(transitions::load(&unsupported.declarations[0].lines).is_err());
}
