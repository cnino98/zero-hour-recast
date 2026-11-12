use super::{TemplateMetadata, load};
use generals_assets::{
    content::{self, ContentFile},
    virtual_assets::Asset,
};
use std::collections::BTreeMap;

fn scan(bytes: &[u8], name: &str) -> ContentFile {
    content::scan(Asset {
        path: name.into(),
        origin: "source-backed metadata fixture".into(),
        bytes: bytes.to_vec(),
    })
    .unwrap()
}
const DEFAULT: &str = "Object DefaultThingTemplate\nKindOf=NONE\nBody=InactiveBody ModuleTag_DefaultInactiveBody\nEnd\nInheritableModule\nBehavior=AutoHealBehavior ModuleTag_DefaultAutoHealBehavior\nEnd\nEnd\nOverrideableByLikeKind\nBehavior=StealthUpdate ModuleTag_DefaultStealthUpdate\nEnd\nEnd\nEnd\n";
fn fixture(body: &str, overrides: &[&str]) -> BTreeMap<String, TemplateMetadata> {
    let baseline = scan(format!("{DEFAULT}{body}").as_bytes(), "baseline.ini");
    let locals: Vec<_> = overrides
        .iter()
        .enumerate()
        .map(|(index, text)| scan(text.as_bytes(), &format!("local{index}.ini")))
        .collect();
    load(&[baseline], &locals, &["Unit"]).unwrap()
}
fn tags(value: &TemplateMetadata) -> Vec<&str> {
    value.modules[0]
        .iter()
        .map(|entry| entry.tag.as_str())
        .collect()
}

#[ignore = "requires original game installation"]
#[test]
fn fixed_roster_and_required_parent_match_executed_native_update_order() {
    // Actual original INIZH declarations; expected output comes from executed C++.
    let file = scan(
        &std::fs::read(
            generals_assets::game_root()
                .unwrap()
                .join("test-fixtures/module-metadata/installed.ini"),
        )
        .unwrap(),
        "original-installed-subset.ini",
    );
    let oracle = std::fs::read_to_string(
        generals_assets::game_root()
            .unwrap()
            .join("test-fixtures/module-metadata/installed-r2.native.tsv"),
    )
    .unwrap();
    let rows: Vec<_> = oracle
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .collect();
    assert_eq!(rows.len(), 159);
    let names: Vec<_> = rows.iter().map(|row| row[0]).collect();
    let parsed = load(&[file], &[], &names).unwrap();
    for row in rows {
        assert_eq!(
            parsed[row[0]].update_registrations(true, true),
            row[1..],
            "{}",
            row[0]
        );
    }
}

#[test]
fn fields_parsed_after_first_module_do_not_restore_removed_defaults() {
    let early = fixture(
        "Object Unit\nBehavior=PhysicsBehavior early\nEnd\nKindOf=VEHICLE\nIsTrainable=Yes\nBehavior=PhysicsBehavior late\nEnd\nEnd\n",
        &[],
    );
    assert_eq!(
        tags(&early["Unit"]),
        ["ModuleTag_DefaultInactiveBody", "early", "late"]
    );
    let before = fixture(
        "Object Unit\nKindOf=VEHICLE\nIsTrainable=Yes\nBehavior=PhysicsBehavior early\nEnd\nEnd\n",
        &[],
    );
    assert_eq!(
        tags(&before["Unit"]),
        [
            "ModuleTag_DefaultInactiveBody",
            "ModuleTag_DefaultAutoHealBehavior",
            "ModuleTag_DefaultStealthUpdate",
            "early"
        ]
    );
}

#[test]
fn raw_override_add_retains_default_stealth_and_is_not_an_ordinary_reparse() {
    let body = "Object Unit\nKindOf=VEHICLE\nIsTrainable=Yes\nBehavior=PhysicsBehavior physics\nEnd\nEnd\n";
    let overridden = fixture(
        body,
        &["Object Unit\nAddModule\nBehavior=StealthUpdate map_stealth\nEnd\nEnd\nEnd\n"],
    );
    assert_eq!(
        tags(&overridden["Unit"]),
        [
            "ModuleTag_DefaultInactiveBody",
            "ModuleTag_DefaultAutoHealBehavior",
            "ModuleTag_DefaultStealthUpdate",
            "physics",
            "map_stealth"
        ]
    );
    let flattened = fixture(
        "Object Unit\nKindOf=VEHICLE\nIsTrainable=Yes\nBehavior=PhysicsBehavior physics\nEnd\nBehavior=StealthUpdate map_stealth\nEnd\nEnd\n",
        &[],
    );
    assert!(!tags(&flattened["Unit"]).contains(&"ModuleTag_DefaultStealthUpdate"));
}

#[test]
fn override_can_remove_and_replace_inherited_tags_in_native_append_order() {
    let parsed = fixture(
        "Object Unit\nKindOf=VEHICLE\nIsTrainable=Yes\nBehavior=PhysicsBehavior physics\nEnd\nEnd\n",
        &[
            "Object Unit\nRemoveModule ModuleTag_DefaultAutoHealBehavior\nReplaceModule ModuleTag_DefaultStealthUpdate\nBehavior=StealthUpdate replacement\nEnd\nEnd\nAddModule\nBehavior=PhysicsBehavior added\nEnd\nEnd\nEnd\n",
        ],
    );
    assert_eq!(
        tags(&parsed["Unit"]),
        [
            "ModuleTag_DefaultInactiveBody",
            "physics",
            "replacement",
            "added"
        ]
    );
}

#[test]
fn each_override_declaration_marks_copied_entries_and_clears_first_weapon_set() {
    let parsed = fixture(
        "Object Unit\nKindOf=VEHICLE\nIsTrainable=Yes\nBody=ActiveBody body\nEnd\nWeaponSet\nConditions=NONE\nWeapon=PRIMARY Gun\nEnd\nEnd\n",
        &[
            "Object Unit\nAddModule\nBehavior=PhysicsBehavior added\nEnd\nEnd\nWeaponSet\nConditions=NONE\nWeapon=PRIMARY None\nEnd\nWeaponSet\nConditions=PLAYER_UPGRADE\nWeapon=SECONDARY Gun\nEnd\nEnd\n",
            "Object Unit\nWeaponSet\nConditions=NONE\nWeapon=PRIMARY None\nEnd\nEnd\n",
        ],
    );
    let unit = &parsed["Unit"];
    assert!(!unit.can_have_any_weapon());
    assert!(
        unit.modules[0]
            .iter()
            .all(|entry| entry.copied_from_default)
    );
    assert!(
        !unit
            .update_registrations(true, true)
            .iter()
            .any(|entry| entry == "FiringTracker")
    );
}

#[test]
fn ai_data_replaces_only_prior_ai_instances_and_reskin_marks_every_copy() {
    let file = scan(format!("{DEFAULT}Object Parent\nKindOf=INFANTRY\nIsTrainable=Yes\nBehavior=AIUpdateInterface first\nEnd\nBehavior=AIUpdateInterface second\nEnd\nDraw=W3DModelDraw old_draw\nEnd\nEnd\nObjectReskin Unit Parent\nDraw=W3DModelDraw child_draw\nEnd\nEnd\n").as_bytes(), "reskins.ini");
    let parsed = load(&[file], &[], &["Unit"]).unwrap();
    let child = &parsed["Unit"];
    assert!(!tags(child).contains(&"first"));
    assert!(tags(child).contains(&"second"));
    assert!(
        child.modules[0]
            .iter()
            .all(|entry| entry.copied_from_default)
    );
    assert_eq!(child.modules[1].len(), 1);
    assert_eq!(child.modules[1][0].tag, "child_draw");
    assert!(!child.modules[1][0].copied_from_default);
}

#[test]
fn helper_selection_uses_actual_inactive_body_ai_and_any_effective_weapon_set() {
    let parsed = fixture("Object Unit\nKindOf=CAN_BE_REPULSED SHRUBBERY\nEnd\n", &[]);
    let unit = &parsed["Unit"];
    // Native does not run default filtering merely because KindOf changes;
    // no new module was parsed, so the authored inherited updates remain.
    assert_eq!(
        tags(unit),
        [
            "ModuleTag_DefaultInactiveBody",
            "ModuleTag_DefaultAutoHealBehavior",
            "ModuleTag_DefaultStealthUpdate"
        ]
    );
    assert_eq!(
        unit.helper_prefix(false, true)
            .into_iter()
            .map(super::diagnostic::helper_name)
            .collect::<Vec<_>>(),
        ["ObjectSMCHelper"]
    );
    assert_eq!(
        unit.helper_prefix(true, false)
            .into_iter()
            .map(super::diagnostic::helper_name)
            .collect::<Vec<_>>(),
        ["ObjectSMCHelper"]
    );
    assert_eq!(
        unit.helper_prefix(true, true)
            .into_iter()
            .map(super::diagnostic::helper_name)
            .collect::<Vec<_>>(),
        ["ObjectSMCHelper", "ObjectRepulsorHelper"]
    );
}

#[test]
fn non_metadata_block_ends_do_not_terminate_object_before_later_body_modules() {
    // Default/Object.ini places ArmorSet and UnitSpecificSounds before its Body.
    // Include both empty and populated blocks so End cannot become Object End.
    let file = scan(b"Object DefaultThingTemplate\nArmorSet\nEnd\nUnitSpecificSounds\nVoiceMove=NoSound\nEnd\nBody=InactiveBody inactive\nEnd\nBehavior=DestroyDie destroy\nEnd\nEnd\nObject Unit\nKindOf=CRATE\nPrerequisites\nEnd\nBehavior=PhysicsBehavior physics\nEnd\nEnd\n", "block-order.ini");
    let parsed = load(&[file], &[], &["Unit"]).unwrap();
    let unit = &parsed["Unit"];
    assert_eq!(tags(unit), ["inactive", "destroy", "physics"]);
    assert_eq!(
        unit.update_registrations(true, true),
        [
            "ObjectSMCHelper",
            "ObjectDefectionHelper",
            "PhysicsBehavior:physics"
        ]
    );
}

#[test]
fn native_release_duplicates_remain_instances_but_invalid_operations_fail() {
    let parsed = fixture(
        "Object Unit\nKindOf=VEHICLE\nIsTrainable=Yes\nBehavior=PhysicsBehavior same\nEnd\nBehavior=PhysicsBehavior same\nEnd\nEnd\n",
        &[],
    );
    assert_eq!(
        tags(&parsed["Unit"])
            .iter()
            .filter(|tag| **tag == "same")
            .count(),
        2
    );
    for change in [
        "Object Unit\nRemoveModule absent\nEnd\n",
        "Object Unit\nBehavior=PhysicsBehavior direct\nEnd\nEnd\n",
        "Object Unit\nReplaceModule physics\nBehavior=PhysicsBehavior physics\nEnd\nEnd\nEnd\n",
        "Object Unit\nReplaceModule physics\nBehavior=StealthUpdate different_family\nEnd\nEnd\nEnd\n",
    ] {
        let file = scan(
            format!("{DEFAULT}Object Unit\nBehavior=PhysicsBehavior physics\nEnd\nEnd\n")
                .as_bytes(),
            "baseline.ini",
        );
        let override_file = scan(change.as_bytes(), "map.ini");
        assert!(load(&[file], &[override_file], &["Unit"]).is_err());
    }
}

#[test]
fn unknown_admitted_families_and_invalid_kindof_tokens_fail_explicitly() {
    for body in [
        "Behavior=UnrepresentedModule unknown\nEnd\n",
        "KindOf=+VEHICLE STRUCTURE\n",
        "KindOf=UNKNOWN_FLAG\n",
    ] {
        let file = scan(
            format!("{DEFAULT}Object Unit\n{body}End\n").as_bytes(),
            "unknown.ini",
        );
        assert!(load(&[file], &[], &["Unit"]).is_err());
    }
}

#[test]
fn inherited_native_factories_replace_ai_in_both_directions_and_override_add() {
    // These rows were executed with actual inherited static factories and native
    // virtual AI marker bodies, not with guessed FamilyModuleData/boolean inputs.
    let oracle = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/reference/module-metadata-reference-r2.native.tsv"
    ));
    for family in ["TransportAIUpdate", "WanderAIUpdate"] {
        for (label, first, second, overriding) in [
            ("forward", "AIUpdateInterface", family, false),
            ("reverse", family, "AIUpdateInterface", false),
            ("override_add", "AIUpdateInterface", family, true),
        ] {
            let body = format!(
                "Object Unit\nKindOf=VEHICLE\nIsTrainable=Yes\nBehavior=PhysicsBehavior unrelated\nEnd\nBehavior={first} first\nEnd\n{}End\n",
                if overriding {
                    String::new()
                } else {
                    format!("Behavior={second} second\nEnd\n")
                }
            );
            let change =
                format!("Object Unit\nAddModule\nBehavior={second} second\nEnd\nEnd\nEnd\n");
            let parsed = if overriding {
                fixture(&body, &[&change])
            } else {
                fixture(&body, &[])
            };
            let unit = &parsed["Unit"];
            let ai_entries: Vec<_> = unit.modules[0]
                .iter()
                .filter(|entry| entry.ai_data)
                .collect();
            assert_eq!(ai_entries.len(), 1, "{family} {label}");
            assert_eq!(ai_entries[0].family, second);
            assert_eq!(ai_entries[0].tag, "second");
            assert!(
                unit.modules[0]
                    .iter()
                    .any(|entry| entry.family == "PhysicsBehavior"
                        && entry.tag == "unrelated"
                        && !entry.ai_data)
            );
            let key = format!("controlled_inherited_factory_{family}_{label}");
            let expected = oracle
                .lines()
                .find(|line| line.split('\t').next() == Some(key.as_str()))
                .unwrap()
                .split('\t')
                .skip(1)
                .collect::<Vec<_>>();
            assert_eq!(unit.update_registrations(true, true), expected, "{key}");
        }
    }
}

#[test]
fn weapon_slots_follow_native_case_insensitive_index_lookup_and_last_slot_write() {
    let parsed = fixture(
        "Object Unit\nBody=ActiveBody body\nEnd\nWeaponSet\nWeapon=PRIMARY BaselineGun\nEnd\nEnd\n",
        &[
            "Object Unit\nWeaponSet\nWeapon=primary Gun\nWeapon=pRiMaRy nOnE\nWeapon=SeCoNdArY Gun\nWeapon=secondary NONE\nWeapon=tertiary Gun\nEnd\nWeaponSet\nWeapon=PRIMARY None\nEnd\nEnd\n",
        ],
    );
    let unit = &parsed["Unit"];
    assert_eq!(
        unit.weapon_sets,
        [[false, false, true], [false, false, false]]
    );
    assert!(unit.can_have_any_weapon());
    assert!(
        unit.helper_prefix(true, true)
            .iter()
            .any(|helper| matches!(helper, super::diagnostic::Helper::FiringTracker))
    );
    let file = scan(
        format!("{DEFAULT}Object Unit\nEnd\n").as_bytes(),
        "baseline.ini",
    );
    let invalid = scan(
        b"Object Unit\nWeaponSet\nWeapon=auxiliary Gun\nEnd\nEnd\n",
        "map.ini",
    );
    assert!(load(&[file], &[invalid], &["Unit"]).is_err());
}

#[test]
fn delivery_ai_map_override_replaces_only_existing_ai_metadata() {
    let parsed = fixture(
        "Object Unit\nKindOf=AIRCRAFT VEHICLE\nIsTrainable=Yes\nBehavior=PhysicsBehavior physics\nEnd\nBehavior=JetAIUpdate old_ai\nEnd\nEnd\n",
        &["Object Unit\nAddModule\nBehavior=DeliverPayloadAIUpdate map_delivery\nEnd\nEnd\nEnd\n"],
    );
    let unit = &parsed["Unit"];
    let ai: Vec<_> = unit.modules[0]
        .iter()
        .filter(|entry| entry.ai_data)
        .collect();
    assert_eq!(ai.len(), 1);
    assert_eq!(ai[0].family, "DeliverPayloadAIUpdate");
    assert_eq!(ai[0].tag, "map_delivery");
    assert_eq!(ai[0].interface_mask, 0x0001);
    assert!(unit.modules[0].iter().any(|entry| entry.tag == "physics"));
    assert!(!tags(unit).contains(&"old_ai"));
    assert!(
        unit.update_registrations(true, true)
            .iter()
            .any(|entry| { entry == "DeliverPayloadAIUpdate:map_delivery" })
    );
}

#[test]
fn original_tree_draw_and_sway_metadata_keep_native_categories_and_masks() {
    let parsed = fixture(
        "Object Unit\nKindOf=SHRUBBERY\nDraw=W3DTreeDraw tree\nEnd\nClientUpdate=SwayClientUpdate sway\nEnd\nBehavior=ToppleUpdate topple\nEnd\nEnd\n",
        &[],
    );
    let unit = &parsed["Unit"];
    assert_eq!(unit.modules[1].len(), 1);
    assert_eq!(unit.modules[1][0].family, "W3DTreeDraw");
    assert_eq!(unit.modules[1][0].interface_mask, 0x0400);
    assert_eq!(unit.modules[2].len(), 1);
    assert_eq!(unit.modules[2][0].family, "SwayClientUpdate");
    assert_eq!(unit.modules[2][0].interface_mask, 0x0800);
    assert!(!unit.modules[2][0].ai_data);
    let topple = unit.modules[0]
        .iter()
        .find(|entry| entry.tag == "topple")
        .unwrap();
    assert_eq!(topple.interface_mask, 0x0011);
    assert!(!topple.ai_data);
    assert!(
        unit.update_registrations(true, true)
            .iter()
            .any(|entry| { entry == "ToppleUpdate:topple" })
    );
    assert!(!unit.update_registrations(true, true).iter().any(|entry| {
        entry.starts_with("SwayClientUpdate:") || entry.starts_with("W3DTreeDraw:")
    }));
}

#[test]
fn excluded_native_registration_guards_remain_unknown() {
    for family in [
        "POWTruckAIUpdate",
        "PrisonBehavior",
        "DemoralizeSpecialPower",
    ] {
        let file = scan(
            format!("{DEFAULT}Object Unit\nBehavior={family} guarded\nEnd\nEnd\n").as_bytes(),
            "guarded.ini",
        );
        assert!(load(&[file], &[], &["Unit"]).is_err(), "{family}");
    }
}
