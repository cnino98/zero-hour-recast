//! Native containers and pristine logical exit/fire/station bones, SH 2bfaea60.
use crate::{
    model_animation::Animations,
    skirmish_data::{blocks, definition, field, finite_number, scalar},
};
use generals_assets::{
    content::{ContentFile, ContentLine, Declaration},
    w3d::Model,
};
use generals_sim::skirmish::{
    UnitType,
    containment::{
        Art, FirePoint, GarrisonRules, HealRules, PassengerWeapons, Payload, RiderDeath, Rules,
        TunnelRules,
    },
};
use std::error::Error;

pub fn load(
    files: &[ContentFile],
    object: &Declaration,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    unit.kind_of =
        field(&object.lines, 0, "KindOf").map_or_else(Vec::new, |line| line.tokens[1..].to_vec());
    unit.transport_slots = field(&object.lines, 0, "TransportSlotCount")
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse()?;
    let Some(module) = container_module(object)? else {
        return Ok(());
    };
    let parachute = module[0]
        .tokens
        .get(1)
        .is_some_and(|name| name == "ParachuteContain");
    let helix = module[0]
        .tokens
        .get(1)
        .is_some_and(|name| name == "HelixContain");
    if unit.preferred_height > 0.0
        && unit.chinook.is_none()
        && unit.deliver_payload.is_none()
        && !parachute
        && !helix
    {
        return Err("air transport lacks a translated landing state machine".into());
    }
    let garrison = garrison_rules(module)?;
    let (cave_index, tunnel) = network_rules(module)?;
    let heal = heal_rules(module)?;
    validate_fields(module, &unit.name, unit.deliver_payload.as_ref())?;
    let number = |key, fallback| -> Result<f32, Box<dyn Error>> {
        field(module, 1, key).map_or(Ok(fallback), |line| finite_number(scalar(line)?))
    };
    let flags = if garrison.is_some() || tunnel.is_some() || heal.is_some() || parachute {
        0
    } else {
        u8::from(boolean(module, "GoAggressiveOnExit", false)?)
            | (u8::from(boolean(module, "ResetMoodCheckTimeOnExit", true)?) << 1)
    };
    let death_damage = field(module, 1, "DamagePercentToUnits").map_or(
        Ok(0.0),
        |line| -> Result<f32, Box<dyn Error>> {
            Ok(finite_number(scalar(line)?.trim_end_matches('%'))? / 100.0)
        },
    )?;
    unit.contain = Some(Rules {
        cave_index,
        payload: initial_payload(module)?,
        payload_templates: module
            .iter()
            .filter(|line| {
                line.depth == Some(1)
                    && line
                        .tokens
                        .first()
                        .is_some_and(|key| key == "PayloadTemplateName")
            })
            .flat_map(|line| line.tokens[1..].iter().cloned())
            .collect(),
        experience_sink_for_rider: boolean(module, "ExperienceSinkForRider", true)?,
        passengers_in_turret: boolean(module, "PassengersInTurret", false)?,
        orient_like_container_on_exit: boolean(module, "OrientLikeContainerOnExit", false)?,
        assault_heal_ratio: assault_ratio(object)?,
        capacity: capacity(files, module)?,
        garrison,
        tunnel,
        allowed: allowed_kinds(module, heal.is_some(), parachute),
        forbidden: field(module, 1, "ForbidInsideKindOf")
            .map_or_else(Vec::new, |line| line.tokens[1..].to_vec()),
        weapons: passenger_weapons(module)?,
        relationships: generals_sim::skirmish::containment::Relationships {
            allies: boolean(module, "AllowAlliesInside", true)?,
            enemies: boolean(module, "AllowEnemiesInside", true)?,
            neutral: boolean(module, "AllowNeutralInside", true)?,
        },
        exit_flags: flags,
        exit_frames: field(module, 1, "ExitDelay").map_or(Ok(0), native_duration)?,
        door_frames: field(module, 1, "DoorOpenTime").map_or(Ok(1), native_duration)?,
        exit_paths: field(module, 1, "NumberOfExitPaths")
            .map(scalar)
            .transpose()?
            .unwrap_or("1")
            .parse()?,
        regen_percent: number("HealthRegen%PerSec", 0.0)?,
        death: RiderDeath {
            damage: death_damage,
            burned: boolean(module, "IsBurnedDeathToUnits", false)?,
            destroy_if_not_free: boolean(module, "DestroyRidersWhoAreNotFreeToExit", false)?,
        },
        heal,
        damage_thresholds: [0.5, 0.1],
        art: std::array::from_fn(|_| Art {
            exits: Vec::new(),
            fire_points: Vec::new(),
        }),
    });
    Ok(())
}

fn allowed_kinds(module: &[ContentLine], healing: bool, parachute: bool) -> Vec<String> {
    field(module, 1, "AllowInsideKindOf").map_or_else(
        || {
            if healing
                || parachute
                || module[0]
                    .tokens
                    .get(1)
                    .is_some_and(|name| name == "OpenContain")
            {
                Vec::new()
            } else {
                vec!["INFANTRY".into()]
            }
        },
        |line| line.tokens[1..].to_vec(),
    )
}

fn container_module(object: &Declaration) -> Result<Option<&[ContentLine]>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let mut containers = modules.into_iter().filter(|module| {
        module[0].tokens.get(1).is_some_and(|name| {
            matches!(
                name.as_str(),
                "OpenContain"
                    | "TransportContain"
                    | "RiderChangeContain"
                    | "OverlordContain"
                    | "HelixContain"
                    | "ParachuteContain"
                    | "GarrisonContain"
                    | "InternetHackContain"
                    | "TunnelContain"
                    | "CaveContain"
                    | "HealContain"
            )
        })
    });
    let Some(module) = containers.next() else {
        return Ok(None);
    };
    if containers.next().is_some() {
        return Err("Object::init rejects duplicate containers".into());
    }
    Ok(Some(module))
}

fn validate_fields(
    module: &[ContentLine],
    name: &str,
    delivery: Option<&generals_sim::skirmish::deliver_payload::TransportRules>,
) -> Result<(), Box<dyn Error>> {
    if boolean(module, "OrientLikeContainerOnExit", false)?
        && !module[0].tokens.get(1).is_some_and(|family| {
            matches!(
                family.as_str(),
                "TransportContain"
                    | "InternetHackContain"
                    | "OverlordContain"
                    | "HelixContain"
                    | "RiderChangeContain"
            )
        })
    {
        return Err(
            format!("{name}: orientation-on-exit requires TransportContain inheritance").into(),
        );
    }
    // The original cargo reader validates its exact exit bone/physics contract;
    // the actual delivery removal callback consumes these fields.
    if delivery.is_none() {
        if boolean(module, "KeepContainerVelocityOnExit", false)? {
            return Err(
                format!("{name}: unsupported container field KeepContainerVelocityOnExit").into(),
            );
        }
        for unsupported in ["ExitBone", "ExitPitchRate"] {
            if field(module, 1, unsupported).is_some() {
                return Err(format!("{name}: unsupported container field {unsupported}").into());
            }
        }
    }
    Ok(())
}

fn passenger_weapons(module: &[ContentLine]) -> Result<PassengerWeapons, Box<dyn Error>> {
    Ok(PassengerWeapons {
        pass_bonuses: boolean(module, "WeaponBonusPassedToPassengers", false)?,
        fire: boolean(module, "PassengersAllowedToFire", false)?,
        upgrade_transport: boolean(module, "ArmedRidersUpgradeMyWeaponSet", false)?,
    })
}

fn initial_payload(module: &[ContentLine]) -> Result<Option<Payload>, Box<dyn Error>> {
    let Some(line) = field(module, 1, "InitialPayload") else {
        return Ok(None);
    };
    let template = line
        .tokens
        .get(1)
        .ok_or("InitialPayload template is missing")?
        .clone();
    let count = line
        .tokens
        .get(2)
        .map_or(Ok(1), |value| value.parse::<i32>())?;
    if line.tokens.len() > 3 {
        return Err("InitialPayload has extra values".into());
    }
    Ok(Some(Payload { template, count }))
}

fn assault_ratio(object: &Declaration) -> Result<Option<f32>, Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "AssaultTransportAIUpdate")
        {
            return Ok(Some(
                field(module, 1, "MembersGetHealedAtLifeRatio")
                    .map_or(Ok(0.0), |line| finite_number(scalar(line)?))?,
            ));
        }
    }
    Ok(None)
}

fn capacity(files: &[ContentFile], module: &[ContentLine]) -> Result<u32, Box<dyn Error>> {
    let kind = module[0].tokens.get(1).map(String::as_str);
    if kind == Some("ParachuteContain") {
        return Ok(1);
    }
    if matches!(kind, Some("OpenContain" | "HealContain")) {
        // OpenContain defaults to CONTAIN_MAX_UNKNOWN and does not check capacity.
        return Ok(field(module, 1, "ContainMax")
            .map(scalar)
            .transpose()?
            .map_or(Ok(u32::MAX), |value| {
                value
                    .parse::<i32>()
                    .map(|count| u32::try_from(count).unwrap_or(u32::MAX))
            })?);
    }
    if matches!(kind, Some("TunnelContain" | "CaveContain")) {
        return crate::skirmish_data::unsigned(
            &definition(files, "GameData", "")?.lines,
            0,
            "MaxTunnelCapacity",
        );
    }
    let key = if kind == Some("GarrisonContain") {
        "ContainMax"
    } else {
        "Slots"
    };
    Ok(field(module, 1, key)
        .map(scalar)
        .transpose()?
        .ok_or("container capacity is required")?
        .parse()?)
}

fn heal_rules(module: &[ContentLine]) -> Result<Option<HealRules>, Box<dyn Error>> {
    if module[0]
        .tokens
        .get(1)
        .is_none_or(|name| name != "HealContain")
    {
        return Ok(None);
    }
    Ok(Some(HealRules {
        full_heal_frames: field(module, 1, "TimeForFullHeal").map_or(Ok(0), native_duration)?,
        allow_enemies: boolean(module, "AllowEnemiesInside", true)?,
        allow_neutral: boolean(module, "AllowNeutralInside", true)?,
    }))
}

fn tunnel_rules(module: &[ContentLine]) -> Result<Option<TunnelRules>, Box<dyn Error>> {
    if module[0]
        .tokens
        .get(1)
        .is_none_or(|name| name != "TunnelContain")
    {
        return Ok(None);
    }
    let frames = field(module, 1, "TimeForFullHeal").map_or(
        Ok(1.0),
        |line| -> Result<f32, Box<dyn Error>> {
            Ok(crate::skirmish_data::source_real(scalar(line)?)? * (30.0 / 1000.0))
        },
    )?;
    Ok(Some(TunnelRules {
        full_heal_frames: frames,
        conditions: crate::skirmish_data::death_callbacks::conditions(module)?,
    }))
}

pub fn bind(
    unit: &mut UnitType,
    index: usize,
    models: &[Model],
    animations: &Animations,
    thresholds: [f32; 2],
) -> Result<(), Box<dyn Error>> {
    let Some(rules) = &mut unit.contain else {
        return Ok(());
    };
    rules.damage_thresholds = thresholds;
    if let Some(garrison) = &mut rules.garrison
        && let Some(stations) = &mut garrison.stations
    {
        let model = animations
            .garrison_model(models, index, None)
            .ok_or("station garrison pristine model missing")?;
        for bone in model
            .public_bone_indices("STATION")
            .values()
            .take(rules.capacity as usize)
        {
            stations.push(
                model
                    .bone_position(&model.bone_names[*bone])
                    .ok_or("garrison station bone transform missing")?,
            );
        }
        if stations.len() != rules.capacity as usize || stations.len() > 40 {
            return Err("non-enclosing garrison lacks its authored station capacity".into());
        }
        return Ok(());
    }
    for (state, damage) in [None, Some("DAMAGED"), Some("REALLYDAMAGED")]
        .into_iter()
        .enumerate()
    {
        let model = if rules.garrison.is_some() {
            animations.garrison_model(models, index, damage)
        } else {
            animations.firing_model(models, index, damage, 0)
        }
        .ok_or("container model missing")?;
        if rules.garrison.is_some() {
            rules.art[state].fire_points = garrison_firepoints(model)?;
            continue;
        }
        for path in 1..=rules.exit_paths {
            let point = |prefix: &str| {
                let name = if rules.exit_paths == 1 {
                    prefix.to_owned()
                } else {
                    format!("{prefix}{path:02}")
                };
                // Object::getSingleLogicalBonePosition explicitly returns the
                // object transform when no draw module has this logical bone.
                animations
                    .logical_bone_position(models, index, damage, &name)
                    .unwrap_or([0.0; 3])
            };
            rules.art[state]
                .exits
                .push((point("ExitStart"), point("ExitEnd")));
        }
        rules.art[state].fire_points = model
            .public_bone_indices("FirePoint")
            .iter()
            .filter(|(name, _)| name.as_str() != "firepoint")
            .take(32)
            .map(|(_, bone)| pristine_firepoint(model, &model.bone_names[*bone]))
            .collect::<Result<_, _>>()?;
    }
    Ok(())
}

fn garrison_firepoints(model: &Model) -> Result<Vec<FirePoint>, Box<dyn Error>> {
    let mut points: Vec<_> = (0..40)
        .map(|_| FirePoint {
            position: [0.0; 3],
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        })
        .collect();
    for (slot, bone) in model
        .public_bone_indices("FirePoint")
        .values()
        .take(40)
        .enumerate()
    {
        points[slot] = pristine_firepoint(model, &model.bone_names[*bone])?;
    }
    Ok(points)
}

fn pristine_firepoint(model: &Model, name: &str) -> Result<FirePoint, Box<dyn Error>> {
    let position = model
        .bone_position(name)
        .ok_or("container fire bone position missing")?;
    let columns = model
        .bone_axes(name)
        .ok_or("container fire bone basis missing")?;
    Ok(FirePoint {
        position,
        basis: std::array::from_fn(|row| std::array::from_fn(|column| columns[column][row])),
    })
}

fn boolean(lines: &[ContentLine], key: &str, fallback: bool) -> Result<bool, Box<dyn Error>> {
    let Some(line) = field(lines, 1, key) else {
        return Ok(fallback);
    };
    match scalar(line)?.to_ascii_lowercase().as_str() {
        "yes" => Ok(true),
        "no" => Ok(false),
        _ => Err(format!("invalid container boolean {key}").into()),
    }
}

fn garrison_rules(module: &[ContentLine]) -> Result<Option<GarrisonRules>, Box<dyn Error>> {
    if module[0]
        .tokens
        .get(1)
        .is_none_or(|name| name != "GarrisonContain")
    {
        return Ok(None);
    }
    if boolean(module, "MobileGarrison", false)? || field(module, 1, "InitialRoster").is_some() {
        return Err("mobile/initial-roster garrison is not translated".into());
    }
    let heal_frames = if boolean(module, "HealObjects", false)? {
        // parseDurationReal scales milliseconds without unsigned frame rounding.
        let frames = field(module, 1, "TimeForFullHeal").map_or(
            Ok(1.0),
            |line| -> Result<f32, Box<dyn Error>> {
                Ok(crate::skirmish_data::source_real(scalar(line)?)? * (30.0 / 1000.0))
            },
        )?;
        if frames <= 0.0 {
            return Err("garrison healing duration must be positive".into());
        }
        Some(frames)
    } else {
        None
    };
    Ok(Some(GarrisonRules {
        allow_enemies: boolean(module, "AllowEnemiesInside", true)?,
        allow_neutral: boolean(module, "AllowNeutralInside", true)?,
        immune_to_clear: boolean(module, "ImmuneToClearBuildingAttacks", false)?,
        heal_frames,
        stations: (!boolean(module, "IsEnclosingContainer", true)?).then(Vec::new),
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        blocks, boolean, capacity, container_module, garrison_rules, heal_rules, initial_payload,
        tunnel_rules, validate_fields,
    };
    use generals_assets::{content, virtual_assets::Asset};

    #[ignore = "requires original game installation"]
    #[test]
    fn original_overlord_turret_passengers_and_false_default_are_preserved() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let file = content::scan(assets.read("Data/INI/Object/ChinaVehicle.ini").unwrap()).unwrap();
        let object = file
            .declarations
            .iter()
            .find(|object| object.name == "ChinaTankOverlord")
            .unwrap();
        let module = container_module(object).unwrap().unwrap();
        validate_fields(module, &object.name, None).unwrap();
        assert!(boolean(module, "PassengersInTurret", false).unwrap());
        for (field, expected) in [
            ("", false),
            ("PassengersInTurret=No", false),
            ("PassengersInTurret=Yes", true),
        ] {
            let file = content::scan(Asset {
                path: "contain.ini".into(),
                origin: "OpenContain default".into(),
                bytes: format!(
                    "Object Carrier\nBehavior=OverlordContain Contain\n{field}\nEnd\nEnd\n"
                )
                .into_bytes(),
            })
            .unwrap();
            let module = container_module(&file.declarations[0]).unwrap().unwrap();
            validate_fields(module, "Carrier", None).unwrap();
            assert_eq!(
                boolean(module, "PassengersInTurret", false).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn initial_payload_preserves_default_count_and_native_signed_empty_count() {
        for (value, expected) in [("Guard", 1), ("Guard 8", 8), ("Guard -1", -1)] {
            let file = content::scan(Asset { path: "payload.ini".into(), origin: "source parser fixture".into(), bytes: format!("Object Transport\nBehavior=TransportContain Tag\nInitialPayload={value}\nEnd\nEnd\n").into_bytes() }).unwrap();
            let module = blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0];
            let payload = initial_payload(module).unwrap().unwrap();
            assert_eq!(payload.template, "Guard");
            assert_eq!(payload.count, expected);
        }
    }

    #[test]
    fn garrison_defaults_immunity_and_fractional_heal_duration_follow_source() {
        let file = content::scan(Asset {
            path: "garrison.ini".into(), origin: "source parser fixture".into(),
            bytes: b"Object Default\nBehavior=GarrisonContain Tag\nContainMax=10\nEnd\nEnd\nObject Bunker\nBehavior=GarrisonContain Tag\nContainMax=5\nImmuneToClearBuildingAttacks=Yes\nAllowEnemiesInside=No\nHealObjects=Yes\nTimeForFullHeal=1250\nEnd\nEnd\nObject Mobile\nBehavior=GarrisonContain Tag\nMobileGarrison=Yes\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let modules = file
            .declarations
            .iter()
            .map(|object| blocks(&object.lines, 0, "Behavior").unwrap()[0])
            .collect::<Vec<_>>();
        let defaults = garrison_rules(modules[0]).unwrap().unwrap();
        assert!(defaults.allow_enemies && defaults.allow_neutral);
        assert!(!defaults.immune_to_clear);
        assert!(defaults.heal_frames.is_none());
        let bunker = garrison_rules(modules[1]).unwrap().unwrap();
        assert!(!bunker.allow_enemies);
        assert!(bunker.immune_to_clear);
        assert_eq!(bunker.heal_frames.unwrap().to_bits(), 37.5_f32.to_bits());
        assert!(garrison_rules(modules[2]).is_err());
    }

    #[test]
    fn tunnel_uses_global_capacity_and_real_duration_including_immediate_healing() {
        for (duration, expected) in [
            (None, 1.0_f32),
            (Some("0"), 0.0),
            (Some("1250"), 37.5),
            (Some("-1000"), -30.0),
        ] {
            let duration =
                duration.map_or_else(String::new, |value| format!("TimeForFullHeal={value}\n"));
            let file = content::scan(Asset {
                path: "tunnel.ini".into(),
                origin: "source parser fixture".into(),
                bytes: format!("GameData\nMaxTunnelCapacity=10\nEnd\nObject Tunnel\nBehavior=TunnelContain Tag\nSlots=99\n{duration}End\nEnd\n").into_bytes(),
            }).unwrap();
            let module = blocks(&file.declarations[1].lines, 0, "Behavior").unwrap()[0];
            assert_eq!(capacity(std::slice::from_ref(&file), module).unwrap(), 10);
            assert_eq!(
                tunnel_rules(module)
                    .unwrap()
                    .unwrap()
                    .full_heal_frames
                    .to_bits(),
                expected.to_bits()
            );
        }
    }

    #[test]
    fn tunnel_rejects_nonfinite_healing_duration() {
        for duration in ["NaN", "inf"] {
            let file = content::scan(Asset {
                path: "tunnel.ini".into(),
                origin: "invalid duration fixture".into(),
                bytes: format!("Object Tunnel\nBehavior=TunnelContain Tag\nTimeForFullHeal={duration}\nEnd\nEnd\n").into_bytes(),
            }).unwrap();
            let module = blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0];
            assert!(tunnel_rules(module).is_err());
        }
    }
    #[test]
    fn heal_container_uses_open_contain_capacity_and_unsigned_duration_defaults() {
        for (duration, expected) in [(None, 0), (Some("2000"), 60), (Some("1250"), 38)] {
            let duration =
                duration.map_or_else(String::new, |value| format!("TimeForFullHeal={value}\n"));
            let file = content::scan(Asset {
                path: "heal.ini".into(),
                origin: "HealContain parser fixture".into(),
                bytes: format!("Object Barracks\nBehavior=HealContain Heal\nAllowAlliesInside=Yes\nAllowEnemiesInside=No\nAllowNeutralInside=No\nAllowInsideKindOf=INFANTRY\n{duration}End\nEnd\n").into_bytes(),
            }).unwrap();
            let module = blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0];
            assert_eq!(
                capacity(std::slice::from_ref(&file), module).unwrap(),
                u32::MAX
            );
            let rules = heal_rules(module).unwrap().unwrap();
            assert_eq!(rules.full_heal_frames, expected);
            assert!(!rules.allow_enemies && !rules.allow_neutral);
        }
    }
}

#[cfg(test)]
mod passenger_bonus_tests {
    use super::*;

    #[test]
    fn original_open_contain_passes_conditions_only_when_authored() {
        for (field, expected) in [
            ("", false),
            ("WeaponBonusPassedToPassengers=Yes", true),
            ("WeaponBonusPassedToPassengers=No", false),
        ] {
            let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
                path: "Object.ini".into(), origin: "Battle Bus OpenContain fields".into(),
                bytes: format!("Object GLAVehicleBattleBus\nBehavior=TransportContain Tag\nSlots=8\nPassengersAllowedToFire=Yes\n{field}\nEnd\nEnd\n").into_bytes(),
            }).unwrap();
            let module = blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0];
            validate_fields(module, "GLAVehicleBattleBus", None).unwrap();
            let rules = passenger_weapons(module).unwrap();
            assert_eq!(rules.pass_bonuses, expected);
            assert!(rules.fire);
        }
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "Object.ini".into(), origin: "invalid native boolean".into(),
            bytes: b"Object GLAVehicleBattleBus\nBehavior=TransportContain Tag\nWeaponBonusPassedToPassengers=Maybe\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let module = blocks(&file.declarations[0].lines, 0, "Behavior").unwrap()[0];
        assert!(passenger_weapons(module).is_err());
    }
}

fn network_rules(
    module: &[ContentLine],
) -> Result<(Option<usize>, Option<TunnelRules>), Box<dyn Error>> {
    let cave = module[0]
        .tokens
        .get(1)
        .is_some_and(|name| name == "CaveContain");
    let cave_index = if cave {
        Some(usize::try_from(
            field(module, 1, "CaveIndex")
                .map(scalar)
                .transpose()?
                .unwrap_or("0")
                .parse::<i32>()?,
        )?)
    } else {
        None
    };
    let tunnel = if cave {
        Some(TunnelRules {
            full_heal_frames: 0.0,
            conditions: crate::skirmish_data::death_callbacks::conditions(module)?,
        })
    } else {
        tunnel_rules(module)?
    };
    Ok((cave_index, tunnel))
}

// ThingTemplate inherits GlobalData's three-second default, then parses its own override.
pub fn occlusion_delay(files: &[ContentFile], object: &Declaration) -> Result<u32, Box<dyn Error>> {
    if let Some(line) = field(&object.lines, 0, "OcclusionDelay") {
        return native_duration(line);
    }
    if files.iter().any(|file| {
        file.declarations
            .iter()
            .any(|declaration| declaration.kind == "GameData")
    }) {
        let global = definition(files, "GameData", "")?;
        if let Some(line) = field(&global.lines, 0, "DefaultOcclusionDelay") {
            return native_duration(line);
        }
    }
    Ok(90)
}

fn native_duration(line: &ContentLine) -> Result<u32, Box<dyn Error>> {
    use num_traits::ToPrimitive;
    let milliseconds: u32 = scalar(line)?.parse()?;
    Ok(
        (milliseconds.to_f32().expect("native UInt converts to Real") * (30.0_f32 / 1000.0))
            .ceil()
            .to_u32()
            .expect("native duration fits UInt"),
    )
}
