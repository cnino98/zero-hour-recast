use super::*;
use generals_assets::{content, virtual_assets::Asset};

fn source(text: &str) -> ContentFile {
    content::scan(Asset {
        path: "weapon.ini".into(),
        origin: "death weapon parser fixture".into(),
        bytes: text.as_bytes().to_vec(),
    })
    .expect("source declarations")
}

#[test]
fn native_terrorist_death_mask_excludes_normal_and_poison_deaths() {
    let file = source(
        "Object TestInfantry\nBehavior=FireWeaponWhenDeadBehavior Death\nDeathWeapon=TestAreaExplosion\nStartsActive=Yes\nDeathTypes=NONE +SUICIDED +CRUSHED +SPLATTED +LASERED +BURNED +EXPLODED\nEnd\nEnd\nWeapon TestAreaExplosion\nPrimaryDamage=17\nPrimaryDamageRadius=9\nDamageType=EXPLOSION\nDeathType=SUICIDED\nClipSize=1\nAutoReloadsClip=No\nDamageDealtAtSelfPosition=Yes\nRadiusDamageAffects=SELF SUICIDE ALLIES ENEMIES NEUTRALS NOT_SIMILAR\nEnd\n",
    );
    let rules = load(std::slice::from_ref(&file), &file.declarations[0]).unwrap();
    assert_eq!(rules.len(), 1);
    let mask = rules[0].death_mask;
    assert_eq!(mask & (1 << DeathType::Normal as u32), 0);
    assert_eq!(mask & (1 << DeathType::Poisoned as u32), 0);
    assert_eq!(mask & (1 << DeathType::PoisonedBeta as u32), 0);
    assert_ne!(mask & (1 << DeathType::Suicided as u32), 0);
    assert_ne!(mask & (1 << DeathType::Crushed as u32), 0);
    assert!(
        rules[0]
            .weapon
            .as_ref()
            .unwrap()
            .area
            .as_ref()
            .unwrap()
            .at_source
    );
    assert_eq!(
        rules[0]
            .weapon
            .as_ref()
            .unwrap()
            .area
            .as_ref()
            .unwrap()
            .affects,
        63
    );
    assert!(matches!(
        rules[0].weapon.as_ref().unwrap().reload,
        generals_sim::skirmish::ClipReload::None
    ));
}

#[test]
fn arbitrary_object_death_weapon_keeps_nullable_template_and_filters() {
    let file = source(
        "Object Projectile\nBehavior=FireWeaponWhenDeadBehavior Death\nDeathWeapon=Missing\nVeterancyLevels=NONE +ELITE\nExemptStatus=UNDER_CONSTRUCTION\nTriggeredBy=First\nTriggeredBy=Final\nEnd\nBehavior=FireWeaponWhenDeadBehavior Other\nDeathWeapon=Pulse\nEnd\nEnd\nWeapon Pulse\nPrimaryDamage=5\nEnd\n",
    );
    let rules = load(std::slice::from_ref(&file), &file.declarations[0]).unwrap();
    assert!(rules[0].weapon.is_none());
    assert_eq!(rules[0].veterancy_mask, 4);
    assert_eq!(rules[0].exempt_status, 8);
    assert_eq!(rules[0].triggers, ["Final"]);
    assert!(!rules[0].starts_active);
    assert_eq!(
        rules[1].weapon.as_ref().unwrap().damage.to_bits(),
        5.0_f32.to_bits()
    );
}
