//! Source object sight, radar providers and global partition/shroud fields.

use crate::skirmish_data::{blocks, field, finite_number, number, scalar};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::visibility::{RadarPriority, Sight, VisibilityRules};
use std::error::Error;

pub fn rules(
    assets: &generals_assets::virtual_assets::VirtualAssets,
    lines: &[ContentLine],
) -> Result<VisibilityRules, Box<dyn Error>> {
    let file = generals_assets::content::scan(assets.read("data/ini/multiplayer.ini")?)?;
    let settings =
        crate::skirmish_data::definition(std::slice::from_ref(&file), "MultiplayerSettings", "")?;
    let use_shroud = crate::skirmish_data::text(&settings.lines, 0, "UseShroud")?;
    let use_shroud = if use_shroud.eq_ignore_ascii_case("Yes") {
        true
    } else if use_shroud.eq_ignore_ascii_case("No") {
        false
    } else {
        return Err("invalid UseShroud value".into());
    };
    let duration = number(lines, 0, "UnlookPersistDuration")?;
    let frames = (duration * 30.0 / 1000.0).ceil();
    Ok(VisibilityRules {
        cell_size: number(lines, 0, "PartitionCellSize")?,
        unlook_persist_frames: frames.to_string().parse()?,
        use_shroud,
    })
}

pub fn radar_updates(
    lines: &[ContentLine],
) -> Result<Vec<generals_sim::skirmish::radar_update::Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "RadarUpdate")
        {
            continue;
        }
        let milliseconds = field(module, 1, "RadarExtendTime")
            .map_or(Ok(0.0), |line| finite_number(scalar(line)?))
            .map_err(|error| {
                format!(
                    "RadarUpdate at line {}: unsupported duration; {error}",
                    module[0].line
                )
            })?;
        let extend_frames = milliseconds * (30.0 / 1000.0);
        if !extend_frames.is_finite() {
            return Err("RadarUpdate extension duration exceeds Real frames".into());
        }
        if !(0.0..4_294_967_296.0).contains(&extend_frames) {
            return Err(format!("RadarUpdate at line {}: unsupported duration; port supports only finite nonnegative durations below 2^32 frames", module[0].line).into());
        }
        result.push(generals_sim::skirmish::radar_update::Rules { extend_frames });
    }
    Ok(result)
}

pub fn radar_upgrades(
    lines: &[ContentLine],
) -> Result<Vec<generals_sim::skirmish::radar_update::Upgrade>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "RadarUpgrade")
        {
            continue;
        }
        let words = |key| {
            field(module, 1, key).map_or_else(Vec::new, |line| {
                line.tokens.iter().skip(1).cloned().collect()
            })
        };
        if !words("RemovesUpgrades").is_empty()
            || field(module, 1, "FXListUpgrade").is_some_and(|line| {
                line.tokens
                    .get(1)
                    .is_some_and(|name| !name.eq_ignore_ascii_case("NONE"))
            })
        {
            return Err(format!("RadarUpgrade at line {} requires authored cross-family UpgradeMux FX/removal dispatch", module[0].line).into());
        }
        let boolean = |key| -> Result<bool, Box<dyn Error>> {
            match field(module, 1, key).map(scalar).transpose()? {
                None | Some("No" | "NO" | "no") => Ok(false),
                Some("Yes" | "YES" | "yes") => Ok(true),
                _ => Err(format!("invalid RadarUpgrade {key}").into()),
            }
        };
        result.push(generals_sim::skirmish::radar_update::Upgrade {
            triggers: words("TriggeredBy"),
            conflicts: words("ConflictsWith"),
            requires_all: boolean("RequiresAllTriggers")?,
            disable_proof: boolean("DisableProof")?,
        });
    }
    Ok(result)
}

pub fn radar_grants(
    files: &[generals_assets::content::ContentFile],
    lines: &[ContentLine],
) -> Result<Vec<generals_sim::skirmish::radar_update::Grant>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "GrantUpgradeCreate")
        {
            continue;
        }
        let name = field(module, 1, "UpgradeToGrant")
            .map(scalar)
            .transpose()?
            .unwrap_or("");
        let definition = crate::skirmish_data::definition(files, "Upgrade", name).ok();
        let player = match definition
            .map(|definition| field(&definition.lines, 0, "Type").map(scalar).transpose())
            .transpose()?
            .flatten()
        {
            None | Some("PLAYER") => true,
            Some("OBJECT") => false,
            _ => return Err("invalid grant upgrade type".into()),
        };
        let academy_radar = definition
            .and_then(|definition| field(&definition.lines, 0, "AcademyClassify"))
            .map(scalar)
            .transpose()?
            .is_some_and(|name| name == "ACT_UPGRADE_RADAR");
        let exempt = field(module, 1, "ExemptStatus").map_or(Ok(0), |line| {
            crate::skirmish_data::instant_death::status(line, 0)
        })?;
        result.push(generals_sim::skirmish::radar_update::Grant {
            name: definition.map(|_| name.into()),
            player,
            on_created: exempt & (1 << 3) != 0,
            academy_radar,
        });
    }
    Ok(result)
}

pub fn sight(lines: &[ContentLine]) -> Result<Sight, Box<dyn Error>> {
    let value = |key, default| -> Result<f32, Box<dyn Error>> {
        field(lines, 0, key).map_or(Ok(default), |line| finite_number(scalar(line)?))
    };
    let kind = |flag| {
        field(lines, 0, "KindOf")
            .is_some_and(|line| line.tokens.iter().skip(1).any(|token| token == flag))
    };
    let vision = value("VisionRange", 0.0)?;
    let clearing_range = match field(lines, 0, "ShroudClearingRange") {
        Some(line) if scalar(line)? != "-1" => finite_number(scalar(line)?)?,
        _ => vision,
    };
    let radar_priority = match field(lines, 0, "RadarPriority")
        .map(scalar)
        .transpose()?
        .unwrap_or("NOT_ON_RADAR")
    {
        "INVALID" | "NOT_ON_RADAR" => RadarPriority::None,
        "UNIT" => RadarPriority::Unit,
        "LOCAL_UNIT_ONLY" => RadarPriority::LocalUnit,
        "STRUCTURE" => RadarPriority::Structure,
        other => return Err(format!("unsupported radar priority {other}").into()),
    };
    Ok(Sight {
        clearing_range,
        reveal_to_all_range: value("ShroudRevealToAllRange", 0.0)?,
        reveal_to_all: kind("REVEAL_TO_ALL"),
        immobile: kind("IMMOBILE"),
        mine: kind("MINE"),
        radar_priority,
    })
}

#[cfg(test)]
mod radar_tests {
    #[test]
    fn ordered_radar_modules_keep_fractional_real_duration_and_zero_default() {
        use generals_assets::{content, virtual_assets::Asset};
        let file = content::scan(Asset { path: "radar.ini".into(), origin: "native RadarUpdate duration".into(),
            bytes: b"Object Radar\nBehavior=RadarUpdate First\nRadarExtendTime=83.5\nEnd\nBehavior=RadarUpdate Second\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules = super::radar_updates(&file.declarations[0].lines).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(
            rules[0].extend_frames.to_bits(),
            (83.5_f32 * (30.0 / 1000.0)).to_bits()
        );
        assert_eq!(rules[1].extend_frames.to_bits(), 0.0_f32.to_bits());
    }
    #[test]
    fn repeated_upgrade_mux_and_native_create_grant_fields_are_preserved() {
        use generals_assets::{content, virtual_assets::Asset};
        let file = content::scan(Asset { path: "radar.ini".into(), origin: "RadarUpgrade authored mux".into(), bytes: b"Upgrade Upgrade_RadarA\nType=OBJECT\nEnd\nObject Radar\nBehavior=RadarUpgrade First\nTriggeredBy=Upgrade_RadarA Upgrade_RadarB\nConflictsWith=Upgrade_NoRadar\nRequiresAllTriggers=Yes\nDisableProof=Yes\nEnd\nBehavior=RadarUpgrade Second\nTriggeredBy=Upgrade_RadarB\nEnd\nBehavior=GrantUpgradeCreate Grant\nUpgradeToGrant=Upgrade_RadarA\nExemptStatus=UNDER_CONSTRUCTION\nEnd\nEnd\n".to_vec() }).unwrap();
        let lines = &file.declarations[1].lines;
        let rules = super::radar_upgrades(lines).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].triggers, ["Upgrade_RadarA", "Upgrade_RadarB"]);
        assert_eq!(rules[0].conflicts, ["Upgrade_NoRadar"]);
        assert!(rules[0].requires_all && rules[0].disable_proof);
        assert!(!rules[1].requires_all && !rules[1].disable_proof);
        let grants = super::radar_grants(std::slice::from_ref(&file), lines).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].name.as_deref(), Some("Upgrade_RadarA"));
        assert!(grants[0].on_created);
        assert!(!grants[0].player);
    }

    #[test]
    fn upgrade_fx_and_removals_require_the_authored_cross_family_dispatcher() {
        use generals_assets::{content, virtual_assets::Asset};
        for field in ["FXListUpgrade=FX_Radar", "RemovesUpgrades=Upgrade_OldRadar"] {
            let file = content::scan(Asset { path: "radar.ini".into(), origin: "native UpgradeMux boundary".into(), bytes: format!("Object Radar\nBehavior=RadarUpgrade First\nTriggeredBy=Upgrade_Radar\n{field}\nEnd\nEnd\n").into_bytes() }).unwrap();
            let error = super::radar_upgrades(&file.declarations[0].lines)
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains("cross-family UpgradeMux"));
        }
    }

    #[test]
    fn unsupported_port_duration_domain_is_an_explicit_admission_error() {
        use generals_assets::{content, virtual_assets::Asset};
        for (duration, cause) in [
            ("-83.5", "invalid nonnegative source number -83.5"),
            ("143165587456", "below 2^32 frames"),
            ("1e35", "below 2^32 frames"),
        ] {
            let file = content::scan(Asset { path: "radar.ini".into(), origin: "undefined native UInt conversion".into(), bytes: format!("Object Radar\nBehavior=RadarUpdate First\nRadarExtendTime={duration}\nEnd\nEnd\n").into_bytes() }).unwrap();
            let error = super::radar_updates(&file.declarations[0].lines)
                .err()
                .unwrap()
                .to_string();
            assert!(
                error.contains("RadarUpdate at line 2: unsupported duration"),
                "{duration}: {error}"
            );
            assert!(error.contains(cause), "{duration}: {error}");
        }
    }

    #[test]
    fn source_real_rounding_keeps_the_preceding_duration_inside_the_port_domain() {
        use generals_assets::{content, virtual_assets::Asset};
        let file = content::scan(Asset {
            path: "radar.ini".into(),
            origin: "native binary32 duration boundary".into(),
            bytes: b"Object Radar\nBehavior=RadarUpdate First\nRadarExtendTime=143165576534\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules = super::radar_updates(&file.declarations[0].lines).unwrap();
        assert_eq!(rules.len(), 1);
        // Real milliseconds round to 143165571072; the source float product is 2^32 - 256.
        assert_eq!(rules[0].extend_frames.to_bits(), 0x4f7f_ffff);
    }
    #[test]
    fn grant_create_keeps_nonradar_missing_and_default_name_slots_in_authored_order() {
        use generals_assets::{content, virtual_assets::Asset};
        let file = content::scan(Asset {
            path: "grant.ini".into(), origin: "native GrantUpgradeCreate regression".into(),
            bytes: b"Upgrade Ordinary\nType=OBJECT\nEnd\nUpgrade Radar\nAcademyClassify=ACT_UPGRADE_RADAR\nEnd\nObject Carrier\nBehavior=GrantUpgradeCreate Missing\nUpgradeToGrant=Missing\nEnd\nBehavior=GrantUpgradeCreate ObjectGrant\nUpgradeToGrant=Ordinary\nExemptStatus=UNDER_CONSTRUCTION\nEnd\nBehavior=GrantUpgradeCreate Default\nEnd\nBehavior=GrantUpgradeCreate PlayerGrant\nUpgradeToGrant=Radar\nExemptStatus=UNDER_CONSTRUCTION\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules =
            super::radar_grants(std::slice::from_ref(&file), &file.declarations[2].lines).unwrap();
        assert_eq!(rules.len(), 4);
        assert!(rules[0].name.is_none());
        assert_eq!(rules[1].name.as_deref(), Some("Ordinary"));
        assert!(!rules[1].player);
        assert!(rules[1].on_created);
        assert!(!rules[1].academy_radar);
        assert!(rules[2].name.is_none());
        assert_eq!(rules[3].name.as_deref(), Some("Radar"));
        assert!(rules[3].player && rules[3].academy_radar && rules[3].on_created);
    }
}
