use super::*;
use generals_assets::{content, virtual_assets::Asset};

fn file(source: &str) -> ContentFile {
    content::scan(Asset {
        path: "death.ini".into(),
        origin: "native InstantDeath regression".into(),
        bytes: source.as_bytes().to_vec(),
    })
    .unwrap()
}

#[test]
fn repeated_fields_append_choices_and_unknown_names_keep_null_rng_slots() {
    let files = [file(
        "Object Projectile\nBehavior=InstantDeathBehavior Death\nFX=Missing Blast\nFX=NONE Blast\nOCL=Missing NONE\nWeapon=Missing NONE\nEnd\nEnd\nFXList Blast\nEnd\n",
    )];
    let rules = load(&files, definition(&files, "Object", "Projectile").unwrap()).unwrap();
    assert_eq!(
        rules[0].effects,
        [None, Some("Blast".into()), None, Some("Blast".into())]
    );
    assert_eq!(rules[0].objects.len(), 2);
    assert!(rules[0].objects.iter().all(Option::is_none));
    assert_eq!(rules[0].weapons.len(), 2);
    assert!(rules[0].weapons.iter().all(Option::is_none));
}

#[test]
fn valid_status_conditions_and_unbound_producers_are_distinguished() {
    let files = [file(
        "Object Projectile\nBehavior=InstantDeathBehavior Death\nRequiredStatus=UNDER_CONSTRUCTION\nExemptStatus=SOLD MASKED\nVeterancyLevels=NONE +ELITE +HEROIC\nEnd\nEnd\n",
    )];
    let rules = load(&files, definition(&files, "Object", "Projectile").unwrap()).unwrap();
    assert_eq!(rules[0].required_status, 1 << 3);
    assert_eq!(rules[0].exempt_status, (1 << 19) | (1 << 22));
    assert_eq!(rules[0].veterancy_mask, 12);
    let files = [file(
        "Object Projectile\nBehavior=InstantDeathBehavior Death\nRequiredStatus=WET\nEnd\nEnd\n",
    )];
    assert!(
        load(&files, definition(&files, "Object", "Projectile").unwrap())
            .err()
            .unwrap()
            .to_string()
            .contains("unbound DieMux status producer WET")
    );
}

#[test]
fn malformed_existing_effect_is_not_treated_as_a_null_lookup() {
    let files = [file(
        "Object Projectile\nBehavior=InstantDeathBehavior Death\nFX=Ambiguous\nEnd\nEnd\nFXList Ambiguous\nParticleSystem\nName=Trial\nEnd\nFXList Ambiguous\nEnd\n",
    )];
    assert!(load(&files, definition(&files, "Object", "Projectile").unwrap()).is_err());
}
