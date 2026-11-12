//! Authored `BunkerBusterBehavior` instances belong to the missile Object.
use super::{blocks, duration_frames, field, instant_death, scalar, weapons};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{WeaponRules, bunker_buster::Rules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "BunkerBusterBehavior")
        {
            continue;
        }
        let upgrade = nullable_name(files, module, "UpgradeRequired", "Upgrade")?;
        let occupant = weapon(files, module, "OccupantDamageWeaponTemplate")?;
        let crash_frames =
            field(module, 1, "CrashThroughBunkerFXFrequency").map_or(Ok(4), duration_frames)?;
        if crash_frames == 0 {
            return Err("Bunker Buster crash FX frequency is zero".into());
        }
        result.push(Rules {
            upgrade,
            detonation_fx: nullable_name(files, module, "DetonationFX", "FXList")?
                .unwrap_or_default(),
            crash_fx: nullable_name(files, module, "CrashThroughBunkerFX", "FXList")?
                .unwrap_or_default(),
            crash_frames,
            occupant_damage: occupant.map(|weapon| (weapon.damage_type, weapon.death_type)),
            shockwave: weapon(files, module, "ShockwaveWeaponTemplate")?,
        });
    }
    Ok(result)
}

fn nullable_name(
    files: &[ContentFile],
    module: &[ContentLine],
    key: &str,
    kind: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let Some(line) = field(module, 1, key) else {
        return Ok(None);
    };
    let name = scalar(line)?;
    Ok(instant_death::nullable(files, kind, name)?.map(|_| name.into()))
}

fn weapon(
    files: &[ContentFile],
    module: &[ContentLine],
    key: &str,
) -> Result<Option<WeaponRules>, Box<dyn Error>> {
    let Some(name) = nullable_name(files, module, key, "Weapon")? else {
        return Ok(None);
    };
    let source = super::definition(files, "Weapon", &name)?;
    let mut rules = WeaponRules::unarmed();
    rules.name = name;
    weapons::parse(&mut rules, &source.lines)?;
    Ok(Some(rules))
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn ordered_payloads_keep_nullable_defaults_and_full_temporary_weapon() {
        let file = content::scan(Asset {
            path: "buster.ini".into(), origin: "native Bunker Buster fields".into(),
            bytes: b"Object Missile\nBehavior = BunkerBusterBehavior First\nUpgradeRequired = Upgrade_Buster\nCrashThroughBunkerFXFrequency = 571\nShockwaveWeaponTemplate = Shock\nOccupantDamageWeaponTemplate = Occupant\nEnd\nBehavior = BunkerBusterBehavior Second\nUpgradeRequired = Missing\nEnd\nEnd\nUpgrade Upgrade_Buster\nEnd\nWeapon Shock\nPrimaryDamage = 10\nPrimaryDamageRadius = 50\nDamageType = MELEE\nDeathType = NORMAL\nRadiusDamageAffects = ALLIES ENEMIES NEUTRALS\nShockWaveAmount = 50\nShockWaveRadius = 50\nShockWaveTaperOff = 0.3\nEnd\nWeapon Occupant\nPrimaryDamage = 400\nDamageType = PARTICLE_BEAM\nDeathType = EXPLODED\nEnd\n".to_vec(),
        }).unwrap();
        let files = [file];
        let rules = load(
            &files,
            super::super::definition(&files, "Object", "Missile").unwrap(),
        )
        .unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].upgrade.as_deref(), Some("Upgrade_Buster"));
        assert_eq!(rules[0].crash_frames, 18);
        let (damage, death) = rules[0].occupant_damage.unwrap();
        assert!(matches!(
            damage,
            generals_sim::skirmish::DamageType::ParticleBeam
        ));
        assert!(matches!(death, generals_sim::skirmish::DeathType::Exploded));
        let weapon = rules[0].shockwave.as_ref().unwrap();
        assert_eq!(weapon.damage.to_bits(), 10.0_f32.to_bits());
        assert_eq!(
            weapon.area.as_ref().unwrap().primary_radius.to_bits(),
            50.0_f32.to_bits()
        );
        assert_eq!(
            weapon.shockwave.map(f32::to_bits),
            [50.0_f32, 50.0, 0.3].map(f32::to_bits)
        );
        assert!(rules[1].upgrade.is_none());
        assert_eq!(rules[1].crash_frames, 4);
        assert!(rules[1].shockwave.is_none());
    }
}
