//! Authored OCL timers and contained `DeliverPayload` lists.
use super::{blocks, definition, duration_frames, field, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{deliver_payload, reinforcement_timer};
use std::error::Error;

pub(super) fn timer(
    object: &Declaration,
) -> Result<Option<reinforcement_timer::Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let matching: Vec<_> = modules
        .iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "OCLUpdate")
        })
        .copied()
        .collect();
    let Some(&module) = matching.first() else {
        return Ok(None);
    };
    if matching.len() != 1 {
        return Err(
            "multiple OCLUpdate instances are not represented by this template data".into(),
        );
    }
    if !matches!(
        object.name.as_str(),
        "TechReinforcementPad" | "AmericaSupplyDropZone"
    ) {
        return Err(format!("unsupported timed OCLUpdate object {}", object.name).into());
    }
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        if line.tokens.first().is_some_and(|key| {
            !matches!(
                key.as_str(),
                "OCL"
                    | "FactionOCL"
                    | "MinDelay"
                    | "MaxDelay"
                    | "CreateAtEdge"
                    | "FactionTriggered"
                    | "End"
            )
        }) {
            return Err(format!("{}: unrepresented OCLUpdate field", object.name).into());
        }
    }
    let mut factions = Vec::new();
    for line in module.iter().filter(|line| {
        line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "FactionOCL")
    }) {
        let text = line.tokens[1..].join(" ").replace(':', " ");
        let words: Vec<_> = text.split_whitespace().collect();
        if words.len() != 4
            || !words[0].eq_ignore_ascii_case("Faction")
            || !words[2].eq_ignore_ascii_case("OCL")
        {
            return Err("invalid native FactionOCL declaration".into());
        }
        factions.push(reinforcement_timer::FactionOcl {
            faction: words[1].into(),
            ocl: words[3].into(),
        });
    }
    Ok(Some(reinforcement_timer::Rules {
        min_delay: duration(module, "MinDelay")?,
        max_delay: duration(module, "MaxDelay")?,
        ocl: field(module, 1, "OCL")
            .map(scalar)
            .transpose()?
            .map(str::to_owned),
        faction_ocl: factions,
        create_at_edge: boolean(module, "CreateAtEdge", false)?,
        faction_triggered: boolean(module, "FactionTriggered", false)?,
    }))
}

pub(super) fn lists(
    files: &[ContentFile],
    selected: &[&str],
) -> Result<Vec<deliver_payload::Rules>, Box<dyn Error>> {
    let mut lists = Vec::new();
    if selected.contains(&"TechReinforcementPad") {
        let pad = definition(files, "Object", "TechReinforcementPad")?;
        let timer = timer(pad)?.ok_or("pad lacks original timer")?;
        for entry in timer.faction_ocl {
            let list = delivery(files, &entry.ocl)?;
            // The playable catalog admits the three base sides. Bind a general's
            // original list when its actual payload templates are admitted.
            if matches!(entry.faction.as_str(), "America" | "China" | "GLA")
                || list
                    .payload
                    .iter()
                    .all(|payload| selected.contains(&payload.object.as_str()))
            {
                lists.push(list);
            }
        }
    }
    if selected.contains(&"AmericaSupplyDropZone") {
        let zone = definition(files, "Object", "AmericaSupplyDropZone")?;
        let timer = timer(zone)?.ok_or("Supply Drop Zone lacks original timer")?;
        let ocl = timer
            .ocl
            .ok_or("Supply Drop Zone lacks original delivery OCL")?;
        lists.push(delivery(files, &ocl)?);
    }
    Ok(lists)
}

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut result = Vec::new();
    for list in lists(files, selected)? {
        for name in std::iter::once(&list.transport)
            .chain(std::iter::once(&list.container))
            .chain(list.payload.iter().map(|payload| &payload.object))
            .filter(|name| !name.is_empty())
        {
            let name = definition(files, "Object", name)?.name.as_str();
            if !selected.contains(&name) && !result.contains(&name) {
                result.push(name);
            }
        }
    }
    Ok(result)
}

fn delivery(files: &[ContentFile], name: &str) -> Result<deliver_payload::Rules, Box<dyn Error>> {
    let object = definition(files, "ObjectCreationList", name)?;
    let nuggets = blocks(&object.lines, 0, "DeliverPayload")?;
    if nuggets.len() != 1
        || object.lines.iter().any(|line| {
            line.depth == Some(0)
                && line
                    .tokens
                    .first()
                    .is_some_and(|key| key != "DeliverPayload" && key != "End")
        })
    {
        return Err(format!("{name}: unsupported mixed delivery nugget family").into());
    }
    delivery_fields(&object.name, nuggets[0])
}

pub(super) fn delivery_nugget(
    files: &[ContentFile],
    name: &str,
    ordinal: usize,
) -> Result<deliver_payload::Rules, Box<dyn Error>> {
    let object = definition(files, "ObjectCreationList", name)?;
    let nuggets = blocks(&object.lines, 0, "DeliverPayload")?;
    let module = nuggets
        .get(ordinal)
        .ok_or("missing DeliverPayload nugget")?;
    delivery_fields(&format!("{}#{ordinal}", object.name), module)
}

fn delivery_fields(
    name: &str,
    module: &[ContentLine],
) -> Result<deliver_payload::Rules, Box<dyn Error>> {
    verify_delivery_fields(name, module)?;
    let text = |key| delivery_text(module, key);
    let payload = delivery_payloads(module)?;
    let number = |key, default| delivery_number(module, key, default);
    let unsigned = |key, default| delivery_unsigned(module, key, default);
    let coordinate = |key| {
        field(module, 1, key)
            .map(super::physical_ocl::coordinate)
            .transpose()
            .map(|point| point.unwrap_or([0.0; 3]))
    };
    let visible = visible_payload(name, module)?;
    let dive = delivery_dive(name, module)?;
    let decal_radius = number("DeliveryDecalRadius", 0.0)?;
    let decal = if decal_radius == 0.0 {
        None
    } else {
        Some((
            decal_radius,
            super::spectre::decal(module, "DeliveryDecal")?,
        ))
    };
    let transport = text("Transport")?;
    if transport.is_empty() {
        return Err(format!("missing Transport for {name}").into());
    }
    Ok(deliver_payload::Rules {
        name: name.into(),
        transport,
        payload,
        container: text("PutInContainer")?,
        transport_start: deliver_payload::TransportStart {
            preferred_height: boolean(module, "StartAtPreferredHeight", true)?,
            max_speed: boolean(module, "StartAtMaxSpeed", false)?,
            delay_delivery_max: duration(module, "DelayDeliveryMax")?,
        },
        release: deliver_payload::Release {
            drop_offset: coordinate("DropOffset")?,
            drop_delay: duration(module, "DropDelay")?,
            parachute_directly: boolean(module, "ParachuteDirectly", false)?,
            drop_variance: coordinate("DropVariance")?,
            inherit_velocity: boolean(module, "InheritTransportVelocity", false)?,
            exit_pitch_rate: number("ExitPitchRate", 0.0)?
                * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0)),
            fire_weapon: boolean(module, "FireWeapon", false)?,
        },
        attempts: field(module, 1, "MaxAttempts")
            .map(scalar)
            .transpose()?
            .unwrap_or("1")
            .parse()?,
        delivery_distance: number("DeliveryDistance", 0.0)?,
        pre_open_distance: number("PreOpenDistance", 0.0)?,
        formation_size: unsigned("FormationSize", 1)?,
        formation_spacing: number("FormationSpacing", 25.0)?,
        convergence: number("WeaponConvergenceFactor", 0.0)?,
        error_radius: number("WeaponErrorRadius", 0.0)?,
        self_destruct: boolean(module, "SelfDestructObject", false)?,
        visible,
        dive,
        decal,
    })
}

fn delivery_text(module: &[ContentLine], key: &str) -> Result<String, Box<dyn Error>> {
    Ok(field(module, 1, key)
        .map(scalar)
        .transpose()?
        .unwrap_or("")
        .into())
}

fn delivery_number(module: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(module, 1, key)
        .map(scalar)
        .transpose()?
        .map_or(Ok(default), super::finite_number)
}

fn delivery_unsigned(
    module: &[ContentLine],
    key: &str,
    default: u32,
) -> Result<u32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(default), |line| {
        scalar(line)?.parse().map_err(Into::into)
    })
}

fn verify_delivery_fields(name: &str, module: &[ContentLine]) -> Result<(), Box<dyn Error>> {
    let supported = [
        "Transport",
        "StartAtPreferredHeight",
        "StartAtMaxSpeed",
        "MaxAttempts",
        "DropOffset",
        "DropDelay",
        "PutInContainer",
        "Payload",
        "ParachuteDirectly",
        "DeliveryDistance",
        "PreOpenDistance",
        "FormationSize",
        "FormationSpacing",
        "WeaponConvergenceFactor",
        "WeaponErrorRadius",
        "DelayDeliveryMax",
        "DropVariance",
        "InheritTransportVelocity",
        "ExitPitchRate",
        "FireWeapon",
        "SelfDestructObject",
        "VisibleItemsDroppedPerInterval",
        "VisibleDropBoneBaseName",
        "VisibleSubObjectBaseName",
        "VisibleNumBones",
        "VisiblePayloadTemplateName",
        "VisiblePayloadWeaponTemplate",
        "DiveStartDistance",
        "DiveEndDistance",
        "StrafingWeaponSlot",
        "StrafeWeaponFX",
        "StrafeLength",
        "DeliveryDecal",
        "DeliveryDecalRadius",
    ];
    for line in module.iter().filter(|line| line.depth == Some(1)) {
        if line
            .tokens
            .first()
            .is_some_and(|key| key != "End" && !supported.contains(&key.as_str()))
        {
            return Err(format!("{name}: untranslated DeliverPayload field").into());
        }
    }
    Ok(())
}

fn delivery_payloads(
    module: &[ContentLine],
) -> Result<Vec<deliver_payload::Payload>, Box<dyn Error>> {
    let mut payload = Vec::new();
    for line in module.iter().filter(|line| {
        line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "Payload")
    }) {
        if !(2..=3).contains(&line.tokens.len()) {
            return Err("invalid native delivery Payload declaration".into());
        }
        payload.push(deliver_payload::Payload {
            object: line.tokens[1].clone(),
            count: line.tokens.get(2).map_or(Ok(1), |count| count.parse())?,
        });
    }
    Ok(payload)
}

fn visible_payload(
    name: &str,
    module: &[ContentLine],
) -> Result<Option<deliver_payload::VisiblePayload>, Box<dyn Error>> {
    let text = |key| delivery_text(module, key);
    let unsigned = |key, default| delivery_unsigned(module, key, default);
    let visible_count = unsigned("VisibleNumBones", 0)?;
    Ok(if visible_count == 0 {
        None
    } else {
        let per_interval = unsigned("VisibleItemsDroppedPerInterval", 0)?;
        if per_interval == 0 {
            return Err(format!("{name} never releases its visible payload").into());
        }
        Some(deliver_payload::VisiblePayload {
            count: visible_count,
            per_interval,
            bone: text("VisibleDropBoneBaseName")?,
            sub_object: text("VisibleSubObjectBaseName")?,
            object: text("VisiblePayloadTemplateName")?,
            weapon: text("VisiblePayloadWeaponTemplate")?,
        })
    })
}

fn delivery_dive(
    name: &str,
    module: &[ContentLine],
) -> Result<Option<deliver_payload::Dive>, Box<dyn Error>> {
    let text = |key| delivery_text(module, key);
    let number = |key, default| delivery_number(module, key, default);
    let dive_start = number("DiveStartDistance", 0.0)?;
    Ok(if dive_start > 0.0 {
        let slot = match text("StrafingWeaponSlot")?.as_str() {
            "" => None,
            "PRIMARY" => Some(0),
            "SECONDARY" => Some(1),
            "TERTIARY" => Some(2),
            _ => return Err(format!("{name} has an invalid strafing slot").into()),
        };
        Some(deliver_payload::Dive {
            start: dive_start,
            end: number("DiveEndDistance", 0.0)?,
            weapon_slot: slot,
            effect: text("StrafeWeaponFX")?,
        })
    } else {
        None
    })
}

pub(super) fn transport(
    object: &Declaration,
) -> Result<Option<deliver_payload::TransportRules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let Some(module) = modules.iter().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "DeliverPayloadAIUpdate")
    }) else {
        return Ok(None);
    };
    let contain = modules
        .iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "TransportContain")
        })
        .copied()
        .unwrap_or_default();
    let unit_sounds = blocks(&object.lines, 0, "UnitSpecificSounds")?;
    Ok(Some(deliver_payload::TransportRules {
        door_delay: duration(module, "DoorDelay")?,
        exit_bone: field(contain, 1, "ExitBone")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
        exit_pitch_rate: field(contain, 1, "ExitPitchRate")
            .map(scalar)
            .transpose()?
            .map_or(Ok(0.0), super::finite_number)?
            * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0)),
        dive_sound: unit_sounds
            .first()
            .and_then(|sounds| field(sounds, 1, "StartDive"))
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .into(),
        strafe_effects: Vec::new(),
    }))
}

fn duration(module: &[ContentLine], key: &str) -> Result<u32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(0), duration_frames)
}

fn boolean(module: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    match field(module, 1, key).map(scalar).transpose()? {
        None => Ok(default),
        Some(value) if value.eq_ignore_ascii_case("Yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("No") => Ok(false),
        _ => Err("invalid delivery boolean".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::definition;
    use super::*;
    use generals_assets::virtual_assets::VirtualAssets;

    #[test]
    fn authored_nuggets_keep_independent_release_lists_and_primary_weapon_bindings() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "power-delivery.ini".into(),
            origin: "delivery shape fixture".into(),
            bytes: br"
ObjectCreationList Paradrop
  DeliverPayload
    Transport = CargoPlane
    Payload = Ranger 10
    PutInContainer = Parachute
    StartAtMaxSpeed = Yes
    ParachuteDirectly = Yes
    DeliveryDistance = 0
    PreOpenDistance = 300
  End
  DeliverPayload
    Transport = CargoPlane
    Payload = Ranger 10
    PutInContainer = Parachute
    ParachuteDirectly = Yes
  End
End
ObjectCreationList AirStrike
  DeliverPayload
    Transport = A10
    FormationSize = 3
    FormationSpacing = 35
    WeaponConvergenceFactor = 0.5
    VisibleItemsDroppedPerInterval = 2
    VisibleNumBones = 6
    VisibleDropBoneBaseName = WeaponA
    VisibleSubObjectBaseName = Missile
    VisiblePayloadTemplateName = A10Missile
    VisiblePayloadWeaponTemplate = A10Weapon
    DiveStartDistance = 500
    DiveEndDistance = 300
    StrafingWeaponSlot = PRIMARY
    DeliveryDecalRadius = 50
    DeliveryDecal
      Texture = SCCA10Strike_USA
      OpacityMin = 25%
      OpacityMax = 50%
      OpacityThrobTime = 500
      OnlyVisibleToOwningPlayer = Yes
    End
  End
End
ObjectCreationList Bomb
  DeliverPayload
    Transport = Bomber
    Payload = Bomb
  End
End
"
            .to_vec(),
        })
        .unwrap();
        let files = [file];
        let lead = delivery_nugget(&files, "Paradrop", 0).unwrap();
        let wing = delivery_nugget(&files, "Paradrop", 1).unwrap();
        assert_eq!(lead.name, "Paradrop#0");
        assert_eq!(wing.name, "Paradrop#1");
        assert!(lead.transport_start.max_speed && !wing.transport_start.max_speed);
        assert!(lead.release.parachute_directly && wing.release.parachute_directly);
        assert_eq!(lead.payload[0].count + wing.payload[0].count, 20);
        let strike = delivery_nugget(&files, "AirStrike", 0).unwrap();
        assert_eq!(strike.formation_size, 3);
        assert_eq!(strike.visible.as_ref().unwrap().count, 6);
        assert_eq!(strike.visible.as_ref().unwrap().weapon, "A10Weapon");
        assert_eq!(strike.dive.as_ref().unwrap().weapon_slot, Some(0));
        assert_eq!(strike.decal.unwrap().1.texture, "SCCA10Strike_USA");
        let bomb = delivery_nugget(&files, "Bomb", 0).unwrap();
        assert_eq!(bomb.payload[0].count, 1);
        assert!(bomb.container.is_empty());
    }

    const PAD_DELIVERIES: [(&str, &str, &str, i32); 12] = [
        (
            "America",
            "OCL_ReinforcementPadUSAVehicle",
            "AmericaTankCrusader",
            1,
        ),
        (
            "AmericaSuperWeaponGeneral",
            "OCL_SWGen_ReinforcementPadUSAVehicle",
            "SupW_AmericaVehicleTomahawk",
            1,
        ),
        (
            "AmericaLaserGeneral",
            "OCL_LGen_ReinforcementPadUSAVehicle",
            "Lazr_AmericaTankCrusader",
            1,
        ),
        (
            "AmericaAirForceGeneral",
            "OCL_AFGen_ReinforcementPadUSAVehicle",
            "AirF_AmericaVehicleHumvee",
            1,
        ),
        (
            "China",
            "OCL_ReinforcementPadCHIVehicle",
            "ChinaTankBattleMaster",
            1,
        ),
        (
            "ChinaTankGeneral",
            "OCL_TnkGen_ReinforcementPadCHIVehicle",
            "Tank_ChinaTankBattleMaster",
            1,
        ),
        (
            "ChinaInfantryGeneral",
            "OCL_InfGen_ReinforcementPadCHIVehicle",
            "Infa_ChinaVehicleTroopCrawler",
            1,
        ),
        (
            "ChinaNukeGeneral",
            "OCL_NukeGen_ReinforcementPadCHIVehicle",
            "Nuke_ChinaTankBattleMaster",
            1,
        ),
        (
            "GLA",
            "OCL_ReinforcementPadGLAVehicle",
            "GLATankScorpion",
            1,
        ),
        (
            "GLAToxinGeneral",
            "OCL_ToxGen_ReinforcementPadGLAVehicle",
            "Chem_GLAVehicleToxinTruck",
            1,
        ),
        (
            "GLADemolitionGeneral",
            "OCL_DemoGen_ReinforcementPadGLAVehicle",
            "Demo_GLAVehicleCombatBike",
            2,
        ),
        (
            "GLAStealthGeneral",
            "OCL_StlthGen_ReinforcementPadGLAVehicle",
            "Slth_GLAVehicleQuadCannon",
            1,
        ),
    ];

    #[ignore = "requires original game installation"]
    #[test]
    fn original_cargo_container_loads_validated_exit_fields_and_actual_orientation_flag() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let mut files = super::super::source_files(&assets).unwrap();
        for (name, faction) in [
            ("AmericaJetCargoPlane", generals_sim::skirmish::Faction::Usa),
            ("ChinaJetCargoPlane", generals_sim::skirmish::Faction::China),
            ("GLAJetCargoPlane", generals_sim::skirmish::Faction::Gla),
        ] {
            for enabled in [true, false] {
                for file in &mut files {
                    for object in &mut file.declarations {
                        if object.kind == "Object" && object.name == name {
                            for line in &mut object.lines {
                                if line
                                    .tokens
                                    .first()
                                    .is_some_and(|key| key == "OrientLikeContainerOnExit")
                                {
                                    line.tokens[1] = if enabled { "Yes" } else { "No" }.into();
                                }
                            }
                        }
                    }
                }
                let object = definition(&files, "Object", name).unwrap();
                let unit = super::super::unit_type(
                    &files,
                    object,
                    faction,
                    generals_sim::skirmish::Kind::Vehicle,
                )
                .unwrap();
                let unit = super::super::finish_type(&files, object, unit).unwrap();
                assert!(unit.preferred_height > 0.0);
                let rules = unit.contain.as_ref().unwrap();
                assert_eq!(rules.orient_like_container_on_exit, enabled);
                assert_eq!(rules.capacity, 100);
                assert_eq!(rules.door_frames, 0);
                assert!(rules.death.destroy_if_not_free);
                assert_eq!(
                    unit.deliver_payload.as_ref().unwrap().exit_bone,
                    "WeaponA01"
                );
            }
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_delivery_lists_preserve_twelve_exact_sides_two_demo_bikes_and_original_transport_data()
     {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        let pad = timer(definition(&files, "Object", "TechReinforcementPad").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(pad.faction_ocl.len(), 12);
        for (entry, (side, ocl, object, count)) in pad.faction_ocl.iter().zip(PAD_DELIVERIES) {
            assert_eq!(entry.faction, side);
            assert_eq!(entry.ocl, ocl);
            let list = delivery(&files, &entry.ocl).unwrap();
            assert_eq!(list.payload.len(), 1);
            assert_eq!(list.payload[0].object, object);
            assert_eq!(list.payload[0].count, count);
            assert_eq!(list.release.drop_delay, 12);
            assert_eq!(list.attempts, 4);
            assert_eq!(list.container, "LargeParachute");
            assert_eq!(
                list.release.drop_offset.map(f32::to_bits),
                [0.0_f32, 0.0, -5.0].map(f32::to_bits)
            );
            assert!(
                list.transport_start.preferred_height
                    && list.transport_start.max_speed
                    && list.release.parachute_directly
            );
        }
        for plane in [
            "AmericaJetCargoPlane",
            "ChinaJetCargoPlane",
            "GLAJetCargoPlane",
        ] {
            let object = definition(&files, "Object", plane).unwrap();
            let rules = transport(object).unwrap().unwrap();
            assert_eq!(rules.door_delay, 15);
            assert_eq!(rules.exit_bone, "WeaponA01");
            let model = super::super::normal_model(&assets, object).unwrap();
            assert!(model.bone_position("WeaponA01").is_some());
        }
        let loaded = lists(&files, &["TechReinforcementPad"]).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].payload[0].object, "AmericaTankCrusader");
        assert_eq!(loaded[1].payload[0].object, "ChinaTankBattleMaster");
        assert_eq!(loaded[2].payload[0].object, "GLATankScorpion");

        let mut selected = vec!["TechReinforcementPad", "AmericaSupplyDropZone"];
        selected.extend(PAD_DELIVERIES.map(|(_, _, payload, _)| payload));
        let loaded = lists(&files, &selected).unwrap();
        assert_eq!(loaded.len(), 13);
        for (list, (_, ocl, object, count)) in loaded.iter().zip(PAD_DELIVERIES) {
            assert_eq!(list.name, ocl);
            assert_eq!(list.payload[0].object, object);
            assert_eq!(list.payload[0].count, count);
        }
        let supply = loaded.last().unwrap();
        assert_eq!(supply.name, "OCL_AmericaSupplyDropZoneCrateDrop");
        assert_eq!(supply.transport, "AmericaJetCargoPlane");
        assert_eq!(supply.container, "AmericaCrateParachute");
        assert_eq!(supply.payload[0].object, "SupplyDropZoneCrate");
        assert_eq!(supply.payload[0].count, 6);
        assert_eq!(supply.release.drop_delay, 11);
        assert_eq!(supply.delivery_distance.to_bits(), 410.0_f32.to_bits());
    }

    #[test]
    fn supply_drop_zone_timer_reads_non_faction_ocl() {
        let files=[generals_assets::content::scan(generals_assets::virtual_assets::Asset {path:"supply.ini".into(),origin:"native non-faction timer dependency".into(),bytes:b"Object AmericaSupplyDropZone\nBehavior = OCLUpdate Timer\nOCL=OCL_AmericaSupplyDropZoneCrateDrop\nMinDelay=120000\nMaxDelay=120000\nCreateAtEdge=Yes\nEnd\nEnd\n".to_vec()}).unwrap()];
        let object = definition(&files, "Object", "AmericaSupplyDropZone").unwrap();
        let rules = timer(object).unwrap().unwrap();
        assert_eq!(
            rules.ocl.as_deref(),
            Some("OCL_AmericaSupplyDropZoneCrateDrop")
        );
        assert_eq!([rules.min_delay, rules.max_delay], [3600, 3600]);
        assert!(rules.create_at_edge && !rules.faction_triggered);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_pad_timer_retains_three_authored_base_faction_list_names() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        let object = definition(&files, "Object", "TechReinforcementPad").unwrap();
        let rules = timer(object).unwrap().unwrap();
        assert_eq!(rules.min_delay, 3600);
        assert_eq!(rules.max_delay, 3600);
        assert!(rules.create_at_edge && rules.faction_triggered);
        for (side, list) in [
            ("America", "OCL_ReinforcementPadUSAVehicle"),
            ("China", "OCL_ReinforcementPadCHIVehicle"),
            ("GLA", "OCL_ReinforcementPadGLAVehicle"),
        ] {
            assert_eq!(
                rules
                    .faction_ocl
                    .iter()
                    .find(|entry| entry.faction == side)
                    .unwrap()
                    .ocl,
                list
            );
        }
    }
}
