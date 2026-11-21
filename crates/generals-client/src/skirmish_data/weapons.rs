//! Original primary and represented secondary `WeaponSet` templates.
use super::{
    blocks, damage_type, definition, duration_frames, field, normal_set, optional_number, scalar,
    selected_pair, support, text, unsigned, weapon_aim_delta,
};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{ContinuousFire, UnitType, WeaponRules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    load_creation_rules(object, unit)?;
    let Some(normal) = normal_set(&object.lines, "WeaponSet")? else {
        return Ok(());
    };
    let auto_choose = sources(normal, "PRIMARY")?;
    if (auto_choose != 0
        || unit.name == "GLAScudStorm"
        || unit.battle_plan.is_some()
        || !unit.behavior.cleanup.is_empty()
        || !unit.demo_trap.is_empty())
        && let Some(name) = selected_pair(normal, 1, "Weapon", "PRIMARY")?
        && !name.eq_ignore_ascii_case("None")
    {
        name.clone_into(&mut unit.weapon.name);
        parse(&mut unit.weapon, &definition(files, "Weapon", name)?.lines)?;
    }
    unit.weapon.auto_choose = auto_choose;
    unit.weapon.preferred_against = preferred(normal, "PRIMARY");
    if unit.combat_bike.is_some()
        || !unit.demo_trap.is_empty()
        || unit.kind == generals_sim::skirmish::Kind::Builder
        || !unit.behavior.mobs.is_empty()
        || unit
            .contain
            .as_ref()
            .is_some_and(|rules| rules.weapons.upgrade_transport)
        || matches!(
            unit.name.as_str(),
            "GLATankScorpion"
                | "AmericaVehicleComanche"
                | "AmericaVehicleHumvee"
                | "AmericaVehicleSentryDrone"
                | "GLAVehicleRocketBuggy"
                | "GLAVehicleScudLauncher"
                | "GLAVehicleToxinTruck"
                | "ChinaTankDragon"
                | "ChinaVehicleInfernoCannon"
                | "ChinaJetMIG"
                | "GLAVehicleBattleBus"
        )
    {
        return load_research_sets(files, object, unit);
    }
    if !matches!(
        unit.name.as_str(),
        "AmericaInfantryRanger"
            | "AmericaInfantryColonelBurton"
            | "AmericaTankMicrowave"
            | "GLADemoTrap"
            | "AmericaInfantryMissileDefender"
            | "AmericaTankAvenger"
            | "AmericaTankAvengerLaserTurret"
            | "AmericaPatriotBattery"
            | "ChinaVehicleNukeLauncher"
            | "ChinaTankGattling"
            | "ChinaGattlingCannon"
            | "ChinaTankOverlord"
            | "ChinaTankOverlordGattlingCannon"
            | "GLAVehicleQuadCannon"
            | "GLAInfantryJarmenKell"
            | "GLAInfantryAngryMobNexus"
    ) {
        return Ok(());
    }
    let set = alternate_set(object, unit, normal)?;
    unit.weapon.auto_choose = sources(set, "PRIMARY")?;
    for (slot, destination) in [
        ("SECONDARY", &mut unit.secondary_weapon),
        ("TERTIARY", &mut unit.tertiary_weapon),
    ] {
        let Some(name) = selected_pair(set, 1, "Weapon", slot)? else {
            continue;
        };
        let mut weapon = WeaponRules::unarmed();
        name.clone_into(&mut weapon.name);
        parse(&mut weapon, &definition(files, "Weapon", name)?.lines)?;
        weapon.auto_choose = sources(set, slot)?;
        weapon.preferred_against = preferred(set, slot);
        *destination = Some(weapon);
    }
    unit.weapon_set_rules.share_reload = field(set, 1, "ShareWeaponReloadTime")
        .map(scalar)
        .transpose()?
        .is_some_and(|value| value.eq_ignore_ascii_case("Yes"));
    Ok(())
}

fn load_creation_rules(object: &Declaration, unit: &mut UnitType) -> Result<(), Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "LockWeaponCreate")
        {
            unit.weapon_set_rules.initial_lock = Some(match text(module, 1, "SlotToLock")? {
                "PRIMARY" => 0,
                "SECONDARY" => 1,
                "TERTIARY" => 2,
                _ => return Err("invalid LockWeaponCreate slot".into()),
            });
        }
    }
    if !unit.behavior.cleanup.is_empty() && blocks(&object.lines, 0, "WeaponSet")?.len() != 1 {
        return Err("cleanup weapon snapshot with alternative sets is not translated".into());
    }
    Ok(())
}

fn load_research_sets(
    files: &[ContentFile],
    object: &Declaration,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    let module = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "WeaponSetUpgrade")
        });
    if let Some(module) = module {
        unit.weapon_upgrade_triggers = super::salvage::words(module, 1, "TriggeredBy");
    } else if unit.demo_trap.is_empty()
        && unit.kind != generals_sim::skirmish::Kind::Builder
        && unit
            .contain
            .as_ref()
            .is_none_or(|rules| !rules.weapons.upgrade_transport)
    {
        return Err("researched weapons lack WeaponSetUpgrade".into());
    }
    for set in blocks(&object.lines, 0, "WeaponSet")? {
        let conditions = field(set, 1, "Conditions").ok_or("weapon set lacks conditions")?;
        let mut flags = 0;
        for name in &conditions.tokens[1..] {
            flags |= match name.as_str() {
                "None" | "NONE" => 0,
                "PLAYER_UPGRADE" => 1,
                "VETERAN" => 32,
                "CRATEUPGRADE_ONE" => 2,
                "CRATEUPGRADE_TWO" => 4,
                "MINE_CLEARING_DETAIL" if unit.kind == generals_sim::skirmish::Kind::Builder => 16,
                name if unit.combat_bike.is_some() => {
                    generals_sim::skirmish::combat_bike::rider_weapon_flag(name)
                        .ok_or_else(|| format!("untranslated bike weapon condition {name}"))?
                }
                _ => return Err(format!("untranslated research weapon condition {name}").into()),
            };
        }
        let shared_reload = field(set, 1, "ShareWeaponReloadTime")
            .map(scalar)
            .transpose()?
            .is_some_and(|value| value.eq_ignore_ascii_case("Yes"));
        if flags == 0 {
            unit.weapon_set_rules.share_reload = shared_reload;
        } else if shared_reload != unit.weapon_set_rules.share_reload {
            return Err("weapon set changes shared reload policy".into());
        }
        let mut indices = [None; 3];
        for (slot, name) in ["PRIMARY", "SECONDARY", "TERTIARY"].into_iter().enumerate() {
            let Some(weapon) = selected_pair(set, 1, "Weapon", name)?
                .filter(|name| !name.eq_ignore_ascii_case("None"))
            else {
                continue;
            };
            let mut rules = WeaponRules::unarmed();
            weapon.clone_into(&mut rules.name);
            parse(&mut rules, &definition(files, "Weapon", weapon)?.lines)?;
            rules.auto_choose = sources(set, name)?;
            rules.preferred_against = preferred(set, name);
            if flags == 0 {
                indices[slot] = Some(slot);
                match slot {
                    0 => unit.weapon = rules,
                    1 => unit.secondary_weapon = Some(rules),
                    _ => unit.tertiary_weapon = Some(rules),
                }
            } else if flags == 2 || flags == 4 {
                let index = 2 + slot * 2 + usize::from(flags / 2);
                let original = unit
                    .weapon_template_mut(index)
                    .ok_or("crate set template absent")?;
                *original = rules;
                indices[slot] = Some(index);
            } else {
                // Conditional templates precede autonomous/death weapons in
                // UnitType's namespace, regardless of their load order.
                indices[slot] = Some(unit.weapon_set_rules.next_template_index());
                unit.weapon_set_rules
                    .templates
                    .push(generals_sim::skirmish::ConditionalWeapon { rules, flags, slot });
            }
        }
        unit.weapon_set_rules
            .replacements
            .push(generals_sim::skirmish::WeaponTemplateSet {
                flags,
                weapons: indices,
                share_lock: field(set, 1, "WeaponLockSharedAcrossSets")
                    .map(scalar)
                    .transpose()?
                    .is_some_and(|value| value.eq_ignore_ascii_case("Yes")),
            });
    }
    Ok(())
}

fn alternate_set<'a>(
    object: &'a Declaration,
    unit: &mut UnitType,
    normal: &'a [ContentLine],
) -> Result<&'a [ContentLine], Box<dyn Error>> {
    if !matches!(
        unit.name.as_str(),
        "AmericaInfantryRanger" | "ChinaTankOverlord"
    ) {
        return Ok(normal);
    }
    let upgraded = blocks(&object.lines, 0, "WeaponSet")?
        .into_iter()
        .find(|set| {
            field(set, 1, "Conditions").is_some_and(|line| line.tokens[1..] == ["PLAYER_UPGRADE"])
        })
        .ok_or("Ranger lacks upgraded weapon set")?;
    let module = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|v| v == "WeaponSetUpgrade")
        })
        .ok_or("Ranger lacks source weapon-set upgrade")?;
    unit.weapon_upgrade_triggers = super::salvage::words(module, 1, "TriggeredBy");
    Ok(upgraded)
}

fn preferred(set: &[ContentLine], slot: &str) -> Vec<String> {
    set.iter()
        .find(|line| {
            line.depth == Some(1)
                && line
                    .tokens
                    .first()
                    .is_some_and(|key| key == "PreferredAgainst")
                && line.tokens.get(1).is_some_and(|name| name == slot)
        })
        .map_or_else(Vec::new, |line| line.tokens[2..].to_vec())
}

fn sources(set: &[ContentLine], slot: &str) -> Result<u8, Box<dyn Error>> {
    let Some(line) = set.iter().rev().find(|line| {
        line.depth == Some(1)
            && line
                .tokens
                .first()
                .is_some_and(|value| value == "AutoChooseSources")
            && line.tokens.get(1).is_some_and(|value| value == slot)
    }) else {
        // WeaponTemplateSet initializes every command-source bit to one.
        return Ok(u8::MAX);
    };
    if line.tokens.len() < 3 {
        return Err("AutoChooseSources lacks flags".into());
    }
    let mut sources = 0;
    for name in &line.tokens[2..] {
        sources |= match name.to_ascii_uppercase().as_str() {
            "NONE" if line.tokens.len() == 3 => 0,
            "FROM_PLAYER" => 1,
            "FROM_SCRIPT" => 2,
            "FROM_AI" => 4,
            "FROM_DOZER" => 8,
            "DEFAULT_SWITCH_WEAPON" => 16,
            _ => return Err(format!("unported weapon command source {name}").into()),
        };
    }
    Ok(sources)
}

fn prefire(lines: &[ContentLine]) -> Result<generals_sim::skirmish::Prefire, Box<dyn Error>> {
    use generals_sim::skirmish::Prefire;
    match field(lines, 0, "PreAttackType")
        .map(scalar)
        .transpose()?
        .unwrap_or("PER_SHOT")
    {
        value if value.eq_ignore_ascii_case("PER_SHOT") => Ok(Prefire::Shot),
        value if value.eq_ignore_ascii_case("PER_ATTACK") => Ok(Prefire::Attack),
        value if value.eq_ignore_ascii_case("PER_CLIP") => Ok(Prefire::Clip),
        _ => Err("invalid original PreAttackType".into()),
    }
}

fn shot_delay(line: &ContentLine) -> Result<[u32; 2], Box<dyn Error>> {
    let words: Vec<_> = line.tokens[1..]
        .iter()
        .flat_map(|word| word.split(':').filter(|part| !part.is_empty()))
        .collect();
    let as_frames = |milliseconds| -> Result<u32, Box<dyn Error>> {
        super::source_numbers::duration_frames(milliseconds)
    };
    let bounds = match words.as_slice() {
        [minimum_tag, minimum, maximum_tag, maximum, ..]
            if minimum_tag.eq_ignore_ascii_case("min")
                && maximum_tag.eq_ignore_ascii_case("max") =>
        {
            [as_frames(minimum)?, as_frames(maximum)?]
        }
        [tag, value, ..] if tag.eq_ignore_ascii_case("min") => [as_frames(value)?; 2],
        [value, ..] => [as_frames(value)?; 2],
        [] => return Err(format!("missing DelayBetweenShots at line {}", line.line).into()),
    };
    if bounds[0] > bounds[1] {
        return Err(format!("reversed DelayBetweenShots range at line {}", line.line).into());
    }
    Ok(bounds)
}

pub(super) fn parse(stats: &mut WeaponRules, lines: &[ContentLine]) -> Result<(), Box<dyn Error>> {
    super::death_weapons::weapon_fields(stats, lines)?;
    stats.shockwave = [
        optional_number(lines, "ShockWaveAmount", 0.0)?,
        optional_number(lines, "ShockWaveRadius", 0.0)?,
        optional_number(lines, "ShockWaveTaperOff", 0.0)?,
    ];
    stats.projectile.calls_on_die = match field(lines, 0, "MissileCallsOnDie")
        .map(scalar)
        .transpose()?
    {
        None => false,
        Some(value) if value.eq_ignore_ascii_case("Yes") => true,
        Some(value) if value.eq_ignore_ascii_case("No") => false,
        Some(_) => return Err("invalid MissileCallsOnDie boolean".into()),
    };
    stats.laser = field(lines, 0, "LaserName")
        .map(scalar)
        .transpose()?
        .is_some_and(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"));
    stats.bonuses.template = bonus_table(lines)?;
    stats.allow_attack_garrisoned = match field(lines, 0, "AllowAttackGarrisonedBldgs")
        .map(scalar)
        .transpose()?
    {
        None => false,
        Some(value) if value.eq_ignore_ascii_case("Yes") => true,
        Some(value) if value.eq_ignore_ascii_case("No") => false,
        Some(_) => return Err("invalid AllowAttackGarrisonedBldgs boolean".into()),
    };
    stats.anti_mask = anti_mask(lines)?;
    stats.request_assist_range = optional_number(lines, "RequestAssistRange", 0.0)?;
    stats.damage = optional_number(lines, "PrimaryDamage", 0.0)?;
    stats.area = support::area_damage(lines)?;
    stats.scatter = optional_number(lines, "ScatterRadius", 0.0)?;
    stats.scatter_targets = super::weapons_scatter::load(lines)?;
    stats.scatter_target_scalar = optional_number(lines, "ScatterTargetScalar", 0.0)?;
    stats.infantry_scatter = optional_number(lines, "ScatterRadiusVsInfantry", 0.0)?;
    stats.range = optional_number(lines, "AttackRange", 0.0)?;
    stats.min_range = optional_number(lines, "MinimumAttackRange", 0.0)?;
    stats.aim_delta = weapon_aim_delta(lines)?;
    stats.target_pitch = [
        target_pitch(lines, "MinTargetPitch", -std::f32::consts::PI)?,
        target_pitch(lines, "MaxTargetPitch", std::f32::consts::PI)?,
    ];
    stats.delay_frames = field(lines, 0, "DelayBetweenShots")
        .map(shot_delay)
        .transpose()?;
    stats.delay = stats.delay_frames.map_or(0.0, |[minimum, _]| {
        num_traits::ToPrimitive::to_f32(&minimum).expect("native frame count") / 30.0
    });
    stats.projectile.stream = field(lines, 0, "ProjectileStreamName")
        .map(scalar)
        .transpose()?
        .filter(|name| !name.eq_ignore_ascii_case("NONE"))
        .map(str::to_owned);
    stats.pre_attack_frames = field(lines, 0, "PreAttackDelay").map_or(Ok(0), duration_frames)?;
    stats.pre_attack_type = prefire(lines)?;
    stats.continue_attack_range = optional_number(lines, "ContinueAttackRange", 0.0)?;
    stats.leech_range = field(lines, 0, "LeechRangeWeapon")
        .map(|line| -> Result<bool, Box<dyn Error>> {
            match scalar(line)?.to_ascii_lowercase().as_str() {
                "yes" => Ok(true),
                "no" => Ok(false),
                _ => Err("invalid LeechRangeWeapon".into()),
            }
        })
        .transpose()?
        .unwrap_or(false);
    stats.auto_reload_idle_frames =
        field(lines, 0, "AutoReloadWhenIdle").map_or(Ok(0), duration_frames)?;
    stats.fire_sound_loop_frames =
        field(lines, 0, "FireSoundLoopTime").map_or(Ok(0), duration_frames)?;
    stats.clip_size =
        field(lines, 0, "ClipSize").map_or(Ok(0), |_| unsigned(lines, 0, "ClipSize"))?;
    stats.reload = reload_policy(lines)?;
    // WeaponTemplate's constructor leaves omitted clip reload duration at zero.
    let reload = field(lines, 0, "ClipReloadTime").map_or(Ok(0.0), |line| {
        super::finite_number(line.tokens.get(1).ok_or("missing ClipReloadTime value")?)
    })?;
    stats.reload_seconds = (reload * 30.0 / 1000.0).ceil() / 30.0;
    // WeaponTemplate constructor stores 999999 distance/frame; rules use distance/second.
    stats.speed = optional_number(lines, "WeaponSpeed", 999_999.0 * 30.0)?;
    let damage = field(lines, 0, "DamageType")
        .map(scalar)
        .transpose()?
        .unwrap_or("EXPLOSION");
    stats.damage_type = damage_type(damage).ok_or("unsupported selected weapon damage type")?;
    if damage == "STATUS" && text(lines, 0, "DamageStatusType")? != "FAERIE_FIRE" {
        return Err("untranslated weapon DamageStatusType".into());
    }
    load_continuous_fire(stats, lines)?;
    Ok(())
}

fn reload_policy(
    lines: &[ContentLine],
) -> Result<generals_sim::skirmish::ClipReload, Box<dyn Error>> {
    use generals_sim::skirmish::ClipReload;
    match field(lines, 0, "AutoReloadsClip").map(scalar).transpose()? {
        None => Ok(ClipReload::Automatic),
        Some(value) if value.eq_ignore_ascii_case("Yes") => Ok(ClipReload::Automatic),
        Some(value) if value.eq_ignore_ascii_case("No") => Ok(ClipReload::None),
        Some(value) if value.eq_ignore_ascii_case("RETURN_TO_BASE") => Ok(ClipReload::ReturnToBase),
        Some(_) => Err("invalid AutoReloadsClip mode".into()),
    }
}

fn target_pitch(lines: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    let Some(line) = field(lines, 0, key) else {
        return Ok(default);
    };
    // INI::parseAngleReal accepts signed scanReal, including a C float suffix.
    // It does not clamp or normalize the authored angle.
    let text = scalar(line)?;
    let degrees: f32 = text.strip_suffix(['f', 'F']).unwrap_or(text).parse()?;
    let radians = degrees * (std::f32::consts::PI / 180.0);
    if !radians.is_finite() {
        return Err(format!("non-finite {key}").into());
    }
    Ok(radians)
}

fn anti_mask(lines: &[ContentLine]) -> Result<u8, Box<dyn Error>> {
    let mut mask = 0x02;
    for (key, bit) in [
        ("AntiAirborneVehicle", 0x01),
        ("AntiGround", 0x02),
        ("AntiProjectile", 0x04),
        ("AntiSmallMissile", 0x08),
        ("AntiMine", 0x10),
        ("AntiAirborneInfantry", 0x20),
        ("AntiBallisticMissile", 0x40),
        ("AntiParachute", 0x80),
    ] {
        let Some(line) = field(lines, 0, key) else {
            continue;
        };
        let value = scalar(line)?;
        if value.eq_ignore_ascii_case("Yes") {
            mask |= bit;
        } else if value.eq_ignore_ascii_case("No") {
            mask &= !bit;
        } else {
            return Err(format!("invalid weapon {key} boolean {value}").into());
        }
    }
    Ok(mask)
}

fn bonus_table(lines: &[ContentLine]) -> Result<[[f32; 5]; 27], Box<dyn Error>> {
    let mut result = [[1.0; 5]; 27];
    for line in lines.iter().filter(|line| {
        line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "WeaponBonus")
    }) {
        let [_, condition, field, amount] = line.tokens.as_slice() else {
            return Err("invalid source WeaponBonus".into());
        };
        let condition = generals_sim::skirmish::containment::bonuses::NAMES
            .iter()
            .position(|name| name.eq_ignore_ascii_case(condition))
            .ok_or("unknown WeaponBonus condition")?;
        let field = ["DAMAGE", "RADIUS", "RANGE", "RATE_OF_FIRE", "PRE_ATTACK"]
            .iter()
            .position(|name| name.eq_ignore_ascii_case(field))
            .ok_or("unknown WeaponBonus field")?;
        let value = super::finite_number(amount)? / 100.0;
        result[condition][field] = value;
    }
    Ok(result)
}

pub(super) fn global_bonuses(
    files: &[ContentFile],
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    let table = bonus_table(&definition(files, "GameData", "")?.lines)?;
    for index in 0..unit.weapon_template_count() {
        if let Some(weapon) = unit.weapon_template_mut(index) {
            weapon.bonuses.global = table;
        }
    }
    Ok(())
}

#[cfg(test)]
mod transport_tests;

#[cfg(test)]
mod builder_tests;

#[cfg(test)]
mod tests {
    use generals_assets::content::ContentLine;

    #[test]
    fn between_shot_delay_rounds_both_authored_bounds_before_sampling() {
        let line = |words: &[&str]| ContentLine {
            line: 1,
            tokens: std::iter::once("DelayBetweenShots")
                .chain(words.iter().copied())
                .map(str::to_owned)
                .collect(),
            raw: String::new(),
            assigned: true,
            depth: Some(0),
        };
        assert_eq!(
            super::shot_delay(&line(&["Min:100", "Max:1000"])).unwrap(),
            [3, 30]
        );
        assert_eq!(
            super::shot_delay(&line(&["min:", "101", "MAX:", "124"])).unwrap(),
            [4, 4]
        );
        assert_eq!(super::shot_delay(&line(&["250"])).unwrap(), [8, 8]);
        assert!(super::shot_delay(&line(&["Min:1000", "Max:100"])).is_err());
    }

    #[test]
    fn missile_death_flag_uses_authored_boolean_and_constructor_default() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(),
            origin: "WeaponTemplate MissileCallsOnDie parser contract".into(),
            bytes: b"Weapon Default\nEnd\nWeapon Enabled\nMissileCallsOnDie=yEs\nEnd\nWeapon Disabled\nMissileCallsOnDie=No\nEnd\nWeapon Invalid\nMissileCallsOnDie=1\nEnd\n".to_vec(),
        }).unwrap();
        for (index, expected) in [false, true, false].into_iter().enumerate() {
            let mut rules = generals_sim::skirmish::WeaponRules::unarmed();
            super::parse(&mut rules, &file.declarations[index].lines).unwrap();
            assert_eq!(rules.projectile.calls_on_die, expected);
        }
        let mut rules = generals_sim::skirmish::WeaponRules::unarmed();
        assert!(super::parse(&mut rules, &file.declarations[3].lines).is_err());
    }

    #[test]
    fn player_upgrade_damage_is_authored_per_weapon_not_inherited_from_primary() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(),
            origin: "Jarmen rifle and pilot sniper AP distinction".into(),
            bytes: b"Weapon Rifle\nPrimaryDamage=180\nDamageType=SNIPER\nWeaponBonus=PLAYER_UPGRADE DAMAGE 125%\nEnd\nWeapon Pilot\nPrimaryDamage=1\nDamageType=KILL_PILOT\nEnd\nWeapon Unsupported\nWeaponBonus=PLAYER_UPGRADE RANGE 125%\nEnd\n".to_vec(),
        }).unwrap();
        for (index, expected) in [1.25_f32, 1.0].into_iter().enumerate() {
            let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
            super::parse(&mut weapon, &file.declarations[index].lines).unwrap();
            assert_eq!(weapon.bonuses.template[5][0].to_bits(), expected.to_bits());
        }
        let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
        super::parse(&mut weapon, &file.declarations[2].lines).unwrap();
        assert_eq!(weapon.bonuses.template[5][2].to_bits(), 1.25_f32.to_bits());
    }

    #[test]
    fn original_creation_weapon_preserves_constructor_defaults() {
        for source in [
            "Weapon FirestormSmallCreationWeapon\nDamageType=FLAME\nDeathType=BURNED\nFireOCL=OCL_FirestormSmall\nEnd\n",
            "Weapon SmallFireFieldCreationWeapon\nFireOCL=OCL_FireFieldSmall\nPrimaryDamage=0.0\nPrimaryDamageRadius=1.0\nEnd\n",
        ] {
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "weapon.ini".into(),
                origin: "original OCL-only weapon fields".into(),
                bytes: source.as_bytes().to_vec(),
            })
            .unwrap();
            let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
            super::parse(&mut weapon, &file.declarations[0].lines).unwrap();
            assert_eq!(weapon.damage.to_bits(), 0.0_f32.to_bits());
            assert_eq!(weapon.range.to_bits(), 0.0_f32.to_bits());
            assert_eq!(weapon.delay.to_bits(), 0.0_f32.to_bits());
        }
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_gattling_keeps_ground_and_air_weapons_for_native_target_admission() {
        use generals_assets::virtual_assets::VirtualAssets;
        use generals_sim::skirmish::{Faction, Kind};

        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        let object = super::definition(&files, "Object", "ChinaTankGattling").unwrap();
        let unit = super::super::unit_type(&files, object, Faction::China, Kind::Tank)
            .and_then(|unit| super::super::finish_type(&files, object, unit))
            .unwrap();
        assert_eq!(unit.weapon.name, "GattlingTankGun");
        assert_eq!(unit.weapon.anti_mask, 0x02);
        let air = unit.secondary_weapon.as_ref().expect("native AA slot");
        assert_eq!(air.name, "GattlingTankGunAir");
        assert_eq!(air.anti_mask, 0x01 | 0x20);
        assert_eq!(air.range.to_bits(), 350.0_f32.to_bits());
        assert_eq!(air.damage.to_bits(), 12.0_f32.to_bits());
        assert_eq!(air.auto_choose, u8::MAX);
        assert!(unit.tertiary_weapon.is_none());
        assert_eq!(unit.turret.as_ref().unwrap().controlled_slots, 0x01 | 0x02);
    }

    #[test]
    fn original_switch_source_mask_is_retained_for_locked_scud_and_separate_slots() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "object.ini".into(), origin: "original SCUD and command-source table".into(),
            bytes: b"Object Scud\nWeaponSet\nConditions=None\nAutoChooseSources=PRIMARY DEFAULT_SWITCH_WEAPON\nAutoChooseSources=SECONDARY NONE\nEnd\nEnd\nObject Explicit\nWeaponSet\nAutoChooseSources=PRIMARY FROM_PLAYER FROM_SCRIPT FROM_AI FROM_DOZER\nEnd\nEnd\nObject Default\nWeaponSet\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let mask = |object: usize, slot: &str| {
            let set = super::blocks(&file.declarations[object].lines, 0, "WeaponSet").unwrap()[0];
            super::sources(set, slot).unwrap()
        };
        assert_eq!(mask(0, "PRIMARY"), 16);
        assert_eq!(mask(0, "SECONDARY"), 0);
        assert_eq!(mask(1, "PRIMARY"), 15);
        assert_eq!(mask(2, "PRIMARY"), u8::MAX);
    }

    #[test]
    fn avenger_painter_keeps_status_duration_and_distinct_target_bonus() {
        let source = "Weapon AvengerTargetDesignator\nPrimaryDamage=200\nAttackRange=200\nDelayBetweenShots=200\nClipSize=0\nDamageType=STATUS\nDamageStatusType=FAERIE_FIRE\nLaserName=AvengerTargetingLaserBeam\nWeaponBonus=TARGET_FAERIE_FIRE RATE_OF_FIRE 150%\nWeaponBonus=GARRISONED RATE_OF_FIRE 110%\nEnd\n";
        for (text, valid) in [
            (source.to_owned(), true),
            (
                source.replace("DamageStatusType=FAERIE_FIRE", "DamageStatusType=DISABLED"),
                false,
            ),
        ] {
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "Weapon.ini".into(),
                origin: "original Avenger painter".into(),
                bytes: text.into_bytes(),
            })
            .unwrap();
            let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
            let result = super::parse(&mut weapon, &file.declarations[0].lines);
            assert_eq!(result.is_ok(), valid);
            if valid {
                assert!(weapon.laser);
                assert!(matches!(
                    weapon.damage_type,
                    generals_sim::skirmish::DamageType::Status
                ));
                assert_eq!(weapon.damage.to_bits(), 200.0_f32.to_bits());
                assert_eq!(weapon.bonuses.template[22][3].to_bits(), 1.5_f32.to_bits());
                assert_eq!(weapon.bonuses.template[0][3].to_bits(), 1.1_f32.to_bits());
            }
        }
    }
    #[test]
    fn continuous_fire_keeps_signed_thresholds_and_independent_native_defaults() {
        for (fields, expected) in [
            ("ContinuousFireOne=-1", [-1, i32::MAX]),
            ("ContinuousFireTwo=-2", [i32::MAX, -2]),
            ("ContinuousFireCoast=100", [i32::MAX, i32::MAX]),
        ] {
            let source = format!("Weapon Test\nPrimaryDamage=1\nAttackRange=10\n{fields}\nEnd\n");
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "Weapon.ini".into(),
                origin: "WeaponTemplate signed thresholds".into(),
                bytes: source.into_bytes(),
            })
            .unwrap();
            let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
            super::parse(&mut weapon, &file.declarations[0].lines).unwrap();
            let continuous_fire = weapon.continuous_fire.unwrap();
            assert_eq!(
                [continuous_fire.mean_shots, continuous_fire.fast_shots],
                expected
            );
            let coast = if fields.starts_with("ContinuousFireCoast") {
                3.0_f32
            } else {
                0.0_f32
            };
            assert_eq!(continuous_fire.coast_frames.to_bits(), coast.to_bits());
        }
    }

    #[test]
    fn omitted_speed_preserves_constructor_distance_per_frame() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(), origin: "Technical RPG default speed".into(),
            bytes: b"Weapon RPG\nPrimaryDamage=50\nAttackRange=150\nDelayBetweenShots=1000\nClipSize=0\nClipReloadTime=2000 how long to reload\nDamageType=EXPLOSION\nEnd\n".to_vec(),
        }).unwrap();
        let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
        super::parse(&mut weapon, &file.declarations[0].lines).unwrap();
        assert_eq!((weapon.speed / 30.0).to_bits(), 999_999.0_f32.to_bits());
        assert_eq!(weapon.reload_seconds.to_bits(), 2.0_f32.to_bits());
        assert_eq!(
            weapon.target_pitch.map(f32::to_bits),
            [-std::f32::consts::PI, std::f32::consts::PI].map(f32::to_bits)
        );
    }

    #[test]
    fn jet_missile_clip_returns_to_base_without_automatic_reload() {
        let source = "Weapon RaptorJetMissileWeapon\nPrimaryDamage=100\nAttackRange=150\nDelayBetweenShots=500\nClipSize=4\nClipReloadTime=8000\nAutoReloadsClip=RETURN_TO_BASE\nDamageType=JET_MISSILES\nEnd\n";
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(),
            origin: "native Raptor clip policy".into(),
            bytes: source.as_bytes().to_vec(),
        })
        .unwrap();
        let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
        super::parse(&mut weapon, &file.declarations[0].lines).unwrap();
        assert!(matches!(
            weapon.reload,
            generals_sim::skirmish::ClipReload::ReturnToBase
        ));
        assert!(matches!(
            weapon.damage_type,
            generals_sim::skirmish::DamageType::JetMissiles
        ));
        assert_eq!(weapon.clip_size, 4);
        assert_eq!(weapon.reload_seconds.to_bits(), 8.0_f32.to_bits());
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(),
            origin: "invalid reload policy is rejected".into(),
            bytes: source
                .replace("RETURN_TO_BASE", "SOMETHING_ELSE")
                .into_bytes(),
        })
        .unwrap();
        assert!(super::parse(&mut weapon, &file.declarations[0].lines).is_err());
    }

    #[test]
    fn strategy_gun_without_clip_reload_uses_native_zero_default_and_shot_delay() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(),
            origin: "Installed StrategyCenterGun omitted ClipReloadTime".into(),
            bytes: b"Weapon StrategyCenterGun\nPrimaryDamage=200\nAttackRange=400\nMinTargetPitch=45\nMaxTargetPitch=80\nDelayBetweenShots=7000\nClipSize=0\nDamageType=EXPLOSION\nEnd\n".to_vec(),
        }).unwrap();
        let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
        super::parse(&mut weapon, &file.declarations[0].lines).unwrap();
        assert_eq!(weapon.reload_seconds.to_bits(), 0.0_f32.to_bits());
        assert_eq!(weapon.delay.to_bits(), 7.0_f32.to_bits());
        assert_eq!(weapon.clip_size, 0);
        assert_eq!(
            weapon.target_pitch.map(f32::to_bits),
            [45.0_f32.to_radians(), 80.0_f32.to_radians()].map(f32::to_bits)
        );
    }

    #[test]
    fn target_pitch_accepts_original_negative_crusader_angle_and_rejects_nonfinite() {
        for (minimum, valid) in [("-15f", true), ("NaN", false), ("-inf", false)] {
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "Weapon.ini".into(),
                origin: "Crusader target pitch".into(),
                bytes: format!("Weapon CrusaderTankGun\nPrimaryDamage=60\nAttackRange=150\nMinTargetPitch={minimum}\nMaxTargetPitch=15\nDelayBetweenShots=1000\nClipSize=0\nDamageType=ARMOR_PIERCING\nEnd\n").into_bytes(),
            }).unwrap();
            let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
            let result = super::parse(&mut weapon, &file.declarations[0].lines);
            assert_eq!(result.is_ok(), valid);
            if valid {
                assert_eq!(
                    weapon.target_pitch.map(f32::to_bits),
                    [-15.0_f32.to_radians(), 15.0_f32.to_radians()].map(f32::to_bits)
                );
            }
        }
    }

    #[test]
    fn artillery_timing_keeps_original_frame_rounding_and_idle_reload() {
        for (mode, valid) in [
            ("PER_SHOT", true),
            ("PER_CLIP", true),
            ("PER_ATTACK", true),
            ("INVALID", false),
        ] {
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "Weapon.ini".into(),
                origin: "Tomahawk pre-fire and Buggy idle reload fields".into(),
                bytes: format!("Weapon Artillery\nPrimaryDamage=20\nAttackRange=300\nDelayBetweenShots=200\nClipSize=6\nClipReloadTime=6000\nDamageType=EXPLOSION\nPreAttackDelay=250\nPreAttackType={mode}\nAutoReloadWhenIdle=6100\nEnd\n").into_bytes(),
            }).unwrap();
            let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
            let parsed = super::parse(&mut weapon, &file.declarations[0].lines);
            assert_eq!(parsed.is_ok(), valid);
            if valid {
                assert_eq!(weapon.pre_attack_frames, 8);
                assert_eq!(weapon.auto_reload_idle_frames, 183);
                assert_eq!(weapon.clip_size, 6);
            }
        }
    }

    #[test]
    fn native_ground_default_and_explicit_air_only_masks_are_preserved() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(),
            origin: "Original anti-mask parser fixture".into(),
            bytes: b"Weapon Rifle\nEnd\nWeapon AirOnly\nAntiAirborneVehicle=Yes\nAntiGround=No\nEnd\nWeapon Invalid\nAntiGround=Maybe\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(super::anti_mask(&file.declarations[0].lines).unwrap(), 0x02);
        assert_eq!(super::anti_mask(&file.declarations[1].lines).unwrap(), 0x01);
        assert!(super::anti_mask(&file.declarations[2].lines).is_err());
    }
}

#[cfg(test)]
mod garrison_tests {
    #[test]
    fn original_bonus_fields_remain_separate_and_last_declaration_wins() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "garrison_weapon.ini".into(), origin: "source bonus fixture".into(),
            bytes: b"GameData\nWeaponBonus=GARRISONED DAMAGE 125%\nWeaponBonus=GARRISONED RANGE 150%\nWeaponBonus=GARRISONED RATE_OF_FIRE 75%\nWeaponBonus=VETERAN DAMAGE 110%\nEnd\nWeapon Rifle\nWeaponBonus=GARRISONED DAMAGE 200%\nWeaponBonus=GARRISONED DAMAGE 175%\nEnd\nWeapon Unsupported\nWeaponBonus=GARRISONED RADIUS 150%\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            super::bonus_table(&file.declarations[0].lines).unwrap()[0].map(f32::to_bits),
            [1.25_f32, 1.0, 1.5, 0.75, 1.0].map(f32::to_bits)
        );
        assert_eq!(
            super::bonus_table(&file.declarations[1].lines).unwrap()[0].map(f32::to_bits),
            [1.75_f32, 1.0, 1.0, 1.0, 1.0].map(f32::to_bits)
        );
        assert_eq!(
            super::bonus_table(&file.declarations[2].lines).unwrap()[0][1].to_bits(),
            1.5_f32.to_bits()
        );
    }
    #[test]
    fn battle_plan_weapon_bonuses_keep_source_order_and_reject_unknown_field_names() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(), origin: "source battle-plan fixture".into(),
            bytes: b"Weapon Gun\nWeaponBonus=BATTLEPLAN_BOMBARDMENT DAMAGE 120%\nWeaponBonus=BATTLEPLAN_BOMBARDMENT DAMAGE 140%\nWeaponBonus=BATTLEPLAN_SEARCHANDDESTROY RANGE 120%\nEnd\nWeapon Unsupported\nWeaponBonus=BATTLEPLAN_HOLDTHELINE PREATTACK 50%\nEnd\n".to_vec(),
        }).unwrap();
        let bonuses = super::bonus_table(&file.declarations[0].lines).unwrap();
        assert_eq!(bonuses[12][0].to_bits(), 1.4_f32.to_bits());
        assert_eq!(bonuses[14][2].to_bits(), 1.2_f32.to_bits());
        assert_eq!(bonuses[13][0].to_bits(), 1.0_f32.to_bits());
        assert!(super::bonus_table(&file.declarations[1].lines).is_err());
    }
}

#[cfg(test)]
mod passenger_bonus_tests {
    #[test]
    fn each_native_condition_keeps_all_five_weapon_fields_and_global_local_tables() {
        use generals_sim::skirmish::containment::bonuses::NAMES;
        let mut source = String::from("GameData\n");
        for name in NAMES {
            for (field, amount) in [
                ("DAMAGE", 110),
                ("RADIUS", 125),
                ("RANGE", 150),
                ("RATE_OF_FIRE", 200),
                ("PRE_ATTACK", 50),
            ] {
                use std::fmt::Write;
                writeln!(source, "WeaponBonus={name} {field} {amount}%").unwrap();
            }
        }
        source.push_str("End\nWeapon Local\nWeaponBonus=VETERAN DAMAGE 125%\nWeaponBonus=VETERAN DAMAGE 130%\nWeaponBonus=ELITE RADIUS 175%\nWeaponBonus=HERO PRE_ATTACK 0%\nEnd\n");
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(),
            origin: "native bonus table parser fixture".into(),
            bytes: source.into_bytes(),
        })
        .unwrap();
        let global = super::bonus_table(&file.declarations[0].lines).unwrap();
        let mut weapon = generals_sim::skirmish::WeaponRules::unarmed();
        super::parse(&mut weapon, &file.declarations[1].lines).unwrap();
        for row in global {
            assert_eq!(
                row.map(f32::to_bits),
                [1.1_f32, 1.25, 1.5, 2.0, 0.5].map(f32::to_bits)
            );
        }
        assert_eq!(weapon.bonuses.template[9][0].to_bits(), 1.3_f32.to_bits());
        assert_eq!(weapon.bonuses.template[10][1].to_bits(), 1.75_f32.to_bits());
        assert_eq!(weapon.bonuses.template[11][4].to_bits(), 0.0_f32.to_bits());
    }
}

#[cfg(test)]
mod bonus_scanner_tests {
    #[test]
    fn native_bonus_names_ignore_case_and_percent_values_use_source_real_prefix() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Weapon.ini".into(), origin: "INI scanIndexList and scanPercentToReal".into(),
            bytes: b"Weapon Gun\nWeaponBonus=veteran damage +125%\nWeaponBonus=elite radius 150\nWeaponBonus=hero pre_attack 5e1percent\nEnd\n".to_vec(),
        }).unwrap();
        let table = super::bonus_table(&file.declarations[0].lines).unwrap();
        assert_eq!(table[9][0].to_bits(), 1.25_f32.to_bits());
        assert_eq!(table[10][1].to_bits(), 1.5_f32.to_bits());
        assert_eq!(table[11][4].to_bits(), 0.5_f32.to_bits());
    }
}

fn load_continuous_fire(
    stats: &mut WeaponRules,
    lines: &[ContentLine],
) -> Result<(), Box<dyn Error>> {
    let continuous = [
        "ContinuousFireOne",
        "ContinuousFireTwo",
        "ContinuousFireCoast",
    ];
    if continuous.iter().any(|key| field(lines, 0, key).is_some()) {
        let threshold = |key| -> Result<i32, Box<dyn Error>> {
            field(lines, 0, key).map_or(Ok(i32::MAX), |line| Ok(scalar(line)?.parse()?))
        };
        stats.continuous_fire = Some(ContinuousFire {
            mean_shots: threshold(continuous[0])?,
            fast_shots: threshold(continuous[1])?,
            coast_frames: field(lines, 0, continuous[2]).map_or(Ok(0.0), |line| {
                Ok::<_, Box<dyn Error>>(
                    (super::finite_number(scalar(line)?)? * 30.0 / 1000.0).ceil(),
                )
            })?,
        });
    }
    Ok(())
}
