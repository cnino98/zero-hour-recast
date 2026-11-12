//! Original SCUD detonation OCLs and poison-field module values.
use super::{blocks, definition, duration_frames, field, number, scalar, text};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    UnitType,
    hazards::{PoisonRules, Rules},
};
use std::error::Error;
mod dependencies;
pub(super) use dependencies::field_templates;

pub(super) fn poisoned(object: &Declaration) -> Result<Vec<PoisonRules>, Box<dyn Error>> {
    let mut rules = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "PoisonedBehavior")
        {
            continue;
        }
        rules.push(PoisonRules {
            interval: field(module, 1, "PoisonDamageInterval").map_or(Ok(0), duration_frames)?,
            duration: field(module, 1, "PoisonDuration").map_or(Ok(0), duration_frames)?,
        });
    }
    Ok(rules)
}

pub(super) fn load(files: &[ContentFile], unit: &mut UnitType) -> Result<(), Box<dyn Error>> {
    for index in 0..unit.weapon_template_count() {
        let Some(weapon) = unit.weapon_template_mut(index) else {
            continue;
        };
        if weapon.name.is_empty() {
            continue;
        }
        let declaration = definition(files, "Weapon", &weapon.name)?;
        let detonation = field(&declaration.lines, 0, "ProjectileDetonationOCL");
        let firing = field(&declaration.lines, 0, "FireOCL");
        if firing.is_some() && detonation.is_some() {
            return Err("simultaneous firing and detonation OCLs not translated".into());
        }
        if let Some(line) = detonation.or(firing)
            && weapon
                .incendiary
                .as_ref()
                .is_none_or(|rules| rules.impact.is_none())
        {
            weapon.hazard = Some(hazard(files, scalar(line)?)?);
        }
        if let Some(line) = field(&declaration.lines, 0, "ProjectileObject") {
            let name = scalar(line)?;
            if name.eq_ignore_ascii_case("NONE") {
                continue;
            }
            let object = definition(files, "Object", name)?;
            projectile_upgrade(object, &declaration.lines, weapon)?;
        }
    }
    Ok(())
}

fn projectile_upgrade(
    object: &Declaration,
    _lines: &[ContentLine],
    weapon: &mut generals_sim::skirmish::WeaponRules,
) -> Result<(), Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "WeaponBonusUpgrade")
        {
            let trigger = text(module, 1, "TriggeredBy")?;
            weapon.projectile.upgrade = Some(trigger.into());
        }
    }
    Ok(())
}

pub(super) fn hazard(files: &[ContentFile], name: &str) -> Result<Rules, Box<dyn Error>> {
    let ocl = definition(files, "ObjectCreationList", name)?;
    let creates = blocks(&ocl.lines, 0, "CreateObject")?;
    if creates.len() != 1 || text(creates[0], 1, "Disposition")? != "ON_GROUND_ALIGNED" {
        return Err("untranslated detonation OCL".into());
    }
    let object = definition(files, "Object", text(creates[0], 1, "ObjectNames")?)?;
    if field(&object.lines, 0, "KindOf")
        .is_none_or(|line| !line.tokens[1..].iter().any(|flag| flag == "CLEANUP_HAZARD"))
    {
        return Err("hazard OCL does not create a cleanup hazard Object".into());
    }
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let updates: Vec<_> = modules
        .iter()
        .copied()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "FireWeaponUpdate")
        })
        .collect();
    if updates.len() != 2 {
        return Err("hazard field requires original damage and cleanup weapons".into());
    }
    let weapon = &definition(files, "Weapon", text(updates[0], 1, "Weapon")?)?.lines;
    let core = &definition(files, "Weapon", text(updates[1], 1, "Weapon")?)?.lines;
    if !matches!(text(weapon, 0, "DamageType")?, "POISON" | "RADIATION")
        || text(core, 0, "DamageType")? != "HAZARD_CLEANUP"
    {
        return Err("invalid original hazard damage types".into());
    }
    let life = modules
        .iter()
        .copied()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "LifetimeUpdate")
        })
        .ok_or("hazard lacks lifetime")?;
    let lifetime = duration_frames(field(life, 1, "MinLifetime").ok_or("missing field lifetime")?)?;
    if lifetime
        != duration_frames(field(life, 1, "MaxLifetime").ok_or("missing maximum lifetime")?)?
        || duration_frames(field(core, 0, "ClipReloadTime").ok_or("missing core reload")?)?
            <= lifetime
    {
        return Err("untranslated variable hazard lifetime or repeating cleanup".into());
    }
    let gravity = text(&definition(files, "GameData", "")?.lines, 0, "Gravity")?.parse::<f32>()?;
    if !gravity.is_finite() || gravity >= 0.0 {
        return Err("invalid hazard gravity".into());
    }
    let death = modules
        .iter()
        .copied()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "FXListDie")
        })
        .ok_or("hazard lacks death FX")?;
    let body = blocks(&object.lines, 0, "Body")?;
    let body = body.first().ok_or("hazard lacks body")?;
    Ok(Rules {
        name: object.name.clone(),
        damage: number(weapon, 0, "PrimaryDamage")?,
        radius: number(weapon, 0, "PrimaryDamageRadius")?,
        interval: duration_frames(
            field(weapon, 0, "DelayBetweenShots").ok_or("hazard lacks interval")?,
        )?,
        airborne_height: -9.0 * (gravity * (1.0 / 30.0) * (1.0 / 30.0)),
        lifetime,
        health: number(body, 1, "InitialHealth")?,
        geometry_radius: super::support::geometry_bounds(&object.lines)?.sphere_radius,
        geometry: super::support::geometry(&object.lines)?,
        core_radius: number(core, 0, "PrimaryDamageRadius")?,
        core_damage: number(core, 0, "PrimaryDamage")?,
        death_type: super::death_weapons::death_type(text(weapon, 0, "DeathType")?)?,
        fire_fx: text(weapon, 0, "FireFX")?.into(),
        death_fx: text(death, 1, "DeathFX")?.into(),
        ambient: text(&object.lines, 0, "SoundAmbient")?.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_reactor_radiation_uses_its_authored_damage_and_cleanup_modules() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = crate::skirmish_data::source_files(&assets).unwrap();
        let rules = hazard(&files, "OCL_RadiationFieldMedium").unwrap();
        assert_eq!(rules.name, "RadiationFieldMedium");
        assert_eq!(rules.lifetime, 900);
        assert_eq!(rules.interval, 23);
        assert_eq!(rules.fire_fx, "WeaponFX_MediumRadiationFieldWeapon");
        assert_eq!(rules.death_fx, "FX_RadiationPoolDie");
        let object = definition(&files, "Object", &rules.name).unwrap();
        let modules = crate::skirmish_data::fire::weapon_update::load(&files, object).unwrap();
        assert_eq!(modules.len(), 2);
        let damage = modules[0].weapon.as_ref().unwrap();
        assert_eq!(damage.name, "MediumRadiationFieldWeapon");
        assert!(matches!(
            damage.damage_type,
            generals_sim::skirmish::DamageType::Radiation
        ));
        assert_eq!(damage.damage.to_bits(), 15.0_f32.to_bits());
        let cleanup = modules[1].weapon.as_ref().unwrap();
        assert_eq!(cleanup.name, "HazardFieldCoreWeapon");
        assert!(matches!(
            cleanup.damage_type,
            generals_sim::skirmish::DamageType::HazardCleanup
        ));
    }

    #[test]
    fn existing_uranium_and_new_ap_rocket_projectiles_share_native_bonus_consumer() {
        for upgrade in ["Upgrade_ChinaUraniumShells", "Upgrade_GLAAPRockets"] {
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "projectile.ini".into(), origin: "installed projectile upgrade module".into(),
                bytes: format!("Object Missile\nBehavior=WeaponBonusUpgrade A\nTriggeredBy={upgrade}\nEnd\nEnd\nWeapon Gun\nWeaponBonus=PLAYER_UPGRADE DAMAGE 125%\nEnd\n").into_bytes(),
            }).unwrap();
            let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
            super::super::weapons::parse(&mut weapon, &file.declarations[1].lines).unwrap();
            projectile_upgrade(
                &file.declarations[0],
                &file.declarations[1].lines,
                &mut weapon,
            )
            .unwrap();
            assert_eq!(weapon.projectile.upgrade.as_deref(), Some(upgrade));
            assert_eq!(weapon.bonuses.template[5][0].to_bits(), 1.25_f32.to_bits());
        }
    }
}
