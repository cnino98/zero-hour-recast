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

#[test]
fn tech_providers_without_collapse_keep_authored_damage_transitions() {
    let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
        path: "tech.ini".into(),
        origin: "tech provider transition regression".into(),
        bytes: b"Object Provider\nBehavior=TechBuildingBehavior Ownership\nEnd\nBehavior=TransitionDamageFX Damage\nDamagedParticleSystem1=Bone:None RandomBone:No PSys:Smoke\nReallyDamagedParticleSystem1=Bone:None RandomBone:No PSys:Smoke\nReallyDamagedParticleSystem2=Bone:None RandomBone:No PSys:Explosion\nEnd\nEnd".to_vec(),
    }).unwrap();
    let object = &file.declarations[0];
    let rules = damage_transitions(object).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].particles[1][0].template, "Smoke");
    assert_eq!(rules[0].particles[2][1].template, "Explosion");
    assert!(
        load(std::slice::from_ref(&file), object, None)
            .unwrap()
            .is_none()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn installed_seven_tech_providers_keep_every_authored_transition_and_death_effect() {
    let root = generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let names = [
        "TechHospital",
        "TechOilDerrick",
        "TechOilRefinery",
        "TechRepairbay",
        "TechRepairPad",
        "TechArtilleryPlatform",
        "TechReinforcementPad",
    ];
    let mut roster = names
        .iter()
        .map(|name| {
            (
                String::from(*name),
                generals_sim::skirmish::Faction::Civilian,
                generals_sim::skirmish::Kind::TechBuilding,
            )
        })
        .collect();
    let (files, _) =
        super::super::source_files::with_runtime_dependencies(&assets, &mut roster).unwrap();
    for name in names {
        let object = definition(&files, "Object", name).unwrap();
        let unit = super::super::unit_type(
            &files,
            object,
            generals_sim::skirmish::Faction::Civilian,
            generals_sim::skirmish::Kind::TechBuilding,
        )
        .unwrap();
        let authored = transitions::load(&object.lines).unwrap();
        assert!(
            !authored.is_empty(),
            "{name} must exercise an authored transition"
        );
        assert_eq!(
            unit.behavior.damage_transitions.len(),
            authored.len(),
            "{name}"
        );
        for (actual, expected) in unit.behavior.damage_transitions.iter().zip(&authored) {
            for state in 0..4 {
                let names = |rules: &generals_sim::skirmish::structure_death::TransitionRules| {
                    rules.particles[state]
                        .iter()
                        .map(|particle| particle.template.clone())
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    names(actual),
                    names(expected),
                    "{name}, damage state {state}"
                );
            }
        }
        for module in blocks(&object.lines, 0, "Behavior").unwrap() {
            if module[0]
                .tokens
                .get(1)
                .is_some_and(|family| family == "FXListDie")
            {
                let effect = scalar(field(module, 1, "DeathFX").unwrap()).unwrap();
                assert!(
                    super::super::fx_definition(&files, effect)
                        .unwrap()
                        .is_some(),
                    "{name}: missing authored death effect {effect}"
                );
            }
        }
    }
}

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
            assert_eq!(unit.behavior.damage_transitions.len(), 2);
            assert!(unit.behavior.damage_transitions[0].suppress_water);
            assert!(!unit.behavior.damage_transitions[1].suppress_water);
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
fn transition_slot_order_and_numbered_bones_follow_native_fields() {
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
    let randomized = scan(&text.replace("Bone:None RandomBone:No", "Bone:Smoke RandomBone:Yes"));
    let rules = transitions::load(&randomized.declarations[0].lines).unwrap();
    assert!(
        rules[0].particles[3]
            .iter()
            .all(|particle| particle.random_bone && particle.bone == "Smoke")
    );
    let mixed = scan(&text.replace(
        "RubbleParticleSystem4=Bone:None RandomBone:No PSys:Flare",
        "RubbleFXList4=Loc: X:1 Y:-2 Z:3 FXList:Flare",
    ));
    let rules = transitions::load(&mixed.declarations[0].lines).unwrap();
    let effect = &rules[0].effects[3][0];
    assert_eq!(effect.slot, 4);
    assert_eq!(
        effect.local.map(f32::to_bits),
        [1.0_f32, -2.0, 3.0].map(f32::to_bits)
    );
}
