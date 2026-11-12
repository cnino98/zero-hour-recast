//! `StickyBombUpdateModuleData` fields and nullable native template pointers.
use super::{blocks, field, scalar, source_real, weapons};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{WeaponRules, sticky_bomb::Rules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    let sounds = blocks(&object.lines, 0, "UnitSpecificSounds")?;
    let sound = |name: &str| -> Result<String, Box<dyn Error>> {
        Ok(sounds
            .iter()
            .filter_map(|block| field(block, 1, name))
            .next_back()
            .map(scalar)
            .transpose()?
            .unwrap_or("NoSound")
            .into())
    };
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "StickyBombUpdate")
        {
            continue;
        }
        let mut rules = Rules {
            attach_bone: String::new(),
            offset_z: 10.0,
            geometry_weapon: None,
            geometry_fx: None,
            created_sound: sound("StickyBombCreated")?,
            ping_sound: sound("UnitBombPing")?,
        };
        // Assign every authored occurrence, including a later null lookup.
        for line in module.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("AttachToTargetBone") => rules.attach_bone = scalar(line)?.into(),
                Some("OffsetZ") => rules.offset_z = source_real(scalar(line)?)?,
                Some("GeometryBasedDamageWeapon") => {
                    rules.geometry_weapon =
                        super::instant_death::nullable(files, "Weapon", scalar(line)?)?
                            .map(|template| {
                                let mut weapon = WeaponRules::unarmed();
                                weapon.name.clone_from(&template.name);
                                weapons::parse(&mut weapon, &template.lines)?;
                                Ok::<_, Box<dyn Error>>(weapon)
                            })
                            .transpose()?;
                }
                Some("GeometryBasedDamageFX") => {
                    rules.geometry_fx =
                        super::instant_death::nullable(files, "FXList", scalar(line)?)?
                            .map(|template| template.name.clone());
                }
                _ => {}
            }
        }
        result.push(rules);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_instances_keep_defaults_signed_offsets_and_resolved_null_overrides() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "sticky.ini".into(), origin: "native StickyBomb parser regression".into(),
            bytes: b"Weapon GeometryDamage\nPrimaryDamage=7\nPrimaryDamageRadius=10\nSecondaryDamage=3\nSecondaryDamageRadius=20\nEnd\nFXList BombFX\nEnd\nObject Bomb\nUnitSpecificSounds\nStickyBombCreated=Created\nUnitBombPing=Ping\nEnd\nBehavior=StickyBombUpdate First\nOffsetZ=-7\nAttachToTargetBone=UnusedSourceBone\nGeometryBasedDamageWeapon=GeometryDamage\nGeometryBasedDamageFX=BombFX\nEnd\nBehavior=StickyBombUpdate Second\nGeometryBasedDamageWeapon=GeometryDamage\nGeometryBasedDamageWeapon=Missing\nGeometryBasedDamageFX=None\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let files = [file];
        let rules = load(
            &files,
            super::super::definition(&files, "Object", "Bomb").unwrap(),
        )
        .unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].offset_z.to_bits(), (-7.0_f32).to_bits());
        assert_eq!(rules[0].attach_bone, "UnusedSourceBone");
        assert_eq!(rules[0].created_sound, "Created");
        assert_eq!(rules[0].ping_sound, "Ping");
        assert_eq!(
            rules[0].geometry_weapon.as_ref().unwrap().damage.to_bits(),
            7.0_f32.to_bits()
        );
        assert_eq!(rules[0].geometry_fx.as_deref(), Some("BombFX"));
        assert_eq!(rules[1].offset_z.to_bits(), 10.0_f32.to_bits());
        assert!(rules[1].geometry_weapon.is_none());
        assert!(rules[1].geometry_fx.is_none());
    }
}
