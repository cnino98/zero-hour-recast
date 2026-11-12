//! Authored original Reinforcement Pad module and contained `DeliverPayload` lists.
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
    // Other OCLUpdate users remain outside this bounded consumer admission.
    if object.name != "TechReinforcementPad" {
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

// The currently selectable base-side closure; all twelve authored timer entries
// remain intact. General-side catalogs are a separate P05 prerequisite.
pub(super) fn lists(
    files: &[ContentFile],
    selected: &[&str],
) -> Result<Vec<deliver_payload::Rules>, Box<dyn Error>> {
    if !selected.contains(&"TechReinforcementPad") {
        return Ok(Vec::new());
    }
    let pad = definition(files, "Object", "TechReinforcementPad")?;
    let timer = timer(pad)?.ok_or("pad lacks original timer")?;
    ["America", "China", "GLA"]
        .into_iter()
        .map(|side| {
            let entry = timer
                .faction_ocl
                .iter()
                .find(|entry| entry.faction == side)
                .ok_or("pad lacks selected native side OCL")?;
            delivery(files, &entry.ocl)
        })
        .collect()
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
    let module = nuggets[0];
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
    let required = |key| -> Result<String, Box<dyn Error>> {
        Ok(scalar(field(module, 1, key).ok_or("delivery required field absent")?)?.into())
    };
    let mut payload = Vec::new();
    for line in module.iter().filter(|line| {
        line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "Payload")
    }) {
        if line.tokens.len() != 3 {
            return Err("invalid native delivery Payload declaration".into());
        }
        payload.push(deliver_payload::Payload {
            object: line.tokens[1].clone(),
            count: line.tokens[2].parse()?,
        });
    }
    let number = |key, default| -> Result<f32, Box<dyn Error>> {
        field(module, 1, key)
            .map(scalar)
            .transpose()?
            .map_or(Ok(default), super::finite_number)
    };
    Ok(deliver_payload::Rules {
        name: name.into(),
        transport: required("Transport")?,
        payload,
        container: required("PutInContainer")?,
        preferred_height: boolean(module, "StartAtPreferredHeight", true)?,
        max_speed: boolean(module, "StartAtMaxSpeed", false)?,
        attempts: field(module, 1, "MaxAttempts")
            .map(scalar)
            .transpose()?
            .unwrap_or("1")
            .parse()?,
        drop_offset: field(module, 1, "DropOffset")
            .map(super::physical_ocl::coordinate)
            .transpose()?
            .unwrap_or([0.0; 3]),
        drop_delay: duration(module, "DropDelay")?,
        delivery_distance: number("DeliveryDistance", 0.0)?,
        pre_open_distance: number("PreOpenDistance", 0.0)?,
        parachute_directly: boolean(module, "ParachuteDirectly", false)?,
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
    if !matches!(
        object.name.as_str(),
        "AmericaJetCargoPlane" | "ChinaJetCargoPlane" | "GLAJetCargoPlane"
    ) {
        return Err(format!("unsupported DeliverPayloadAIUpdate actor {}", object.name).into());
    }
    let contain = modules
        .iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "TransportContain")
        })
        .ok_or("delivery plane requires original TransportContain")?;
    if boolean(contain, "ScatterNearbyOnExit", true)?
        || !boolean(contain, "KeepContainerVelocityOnExit", false)?
        || !boolean(contain, "DestroyRidersWhoAreNotFreeToExit", false)?
        || duration(contain, "DoorOpenTime")? != 0
    {
        return Err("untranslated delivery transport containment variant".into());
    }
    if field(contain, 1, "ExitBone").map(scalar).transpose()? != Some("WeaponA01") {
        return Err("delivery transport requires original WeaponA01 exit bone binding".into());
    }
    Ok(Some(deliver_payload::TransportRules {
        door_delay: duration(module, "DoorDelay")?,
        exit_bone: "WeaponA01".into(),
        exit_pitch_rate: field(contain, 1, "ExitPitchRate")
            .map(scalar)
            .transpose()?
            .map_or(Ok(0.0), super::finite_number)?
            * ((1.0 / 30.0) * (std::f32::consts::PI / 180.0)),
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
            assert_eq!(list.drop_delay, 12);
            assert_eq!(list.attempts, 4);
            assert_eq!(list.container, "LargeParachute");
            assert_eq!(
                list.drop_offset.map(f32::to_bits),
                [0.0_f32, 0.0, -5.0].map(f32::to_bits)
            );
            assert!(list.preferred_height && list.max_speed && list.parachute_directly);
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
    }

    #[test]
    fn unsupported_supply_drop_zone_timer_is_rejected_until_its_real_crate_chain_exists() {
        let files=[generals_assets::content::scan(generals_assets::virtual_assets::Asset {path:"supply.ini".into(),origin:"native non-faction timer dependency".into(),bytes:b"Object AmericaSupplyDropZone\nBehavior = OCLUpdate Timer\nOCL=OCL_AmericaSupplyDropZoneCrateDrop\nMinDelay=120000\nMaxDelay=120000\nCreateAtEdge=Yes\nEnd\nEnd\n".to_vec()}).unwrap()];
        let object = definition(&files, "Object", "AmericaSupplyDropZone").unwrap();
        assert!(
            timer(object)
                .is_err_and(|error| error.to_string().contains("unsupported timed OCLUpdate"))
        );
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
