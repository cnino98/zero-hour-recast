use super::{blocks, duration_frames, field, finite_number, has_flag, optional_number, scalar};
use generals_assets::content::ContentLine;
use generals_sim::collision_geometry::Shape;
use generals_sim::skirmish::{AreaDamage, AutoHeal, SourceScoring, TypeLimit};
use std::error::Error;

pub(super) fn build_rules(
    lines: &[ContentLine],
) -> Result<generals_sim::skirmish::BuildRules, Box<dyn Error>> {
    let requirements = prerequisites(lines)?;
    Ok(generals_sim::skirmish::BuildRules {
        status: buildable(lines)?,
        objects: requirements.objects,
        sciences: requirements.sciences,
        limit: max_simultaneous(lines)?,
        linked_limit: max_simultaneous_link(lines)?,
    })
}

pub(super) fn buildable(
    lines: &[ContentLine],
) -> Result<generals_sim::skirmish::Buildable, Box<dyn Error>> {
    use generals_sim::skirmish::Buildable;
    match field(lines, 0, "Buildable")
        .map(scalar)
        .transpose()?
        .unwrap_or("YES")
        .to_ascii_uppercase()
        .as_str()
    {
        "YES" => Ok(Buildable::Yes),
        "NO" => Ok(Buildable::No),
        "IGNORE_PREREQUISITES" => Ok(Buildable::IgnorePrerequisites),
        "ONLY_BY_AI" => Ok(Buildable::OnlyByAi),
        value => Err(format!("unsupported original Buildable {value}").into()),
    }
}

pub(super) fn build_cost(lines: &[ContentLine]) -> Result<u32, Box<dyn Error>> {
    Ok(u32::from(
        field(lines, 0, "BuildCost")
            .map(scalar)
            .transpose()?
            .unwrap_or("0")
            .parse::<u16>()?,
    ))
}

pub(super) struct Prerequisites {
    pub objects: Vec<Vec<String>>,
    pub sciences: Vec<String>,
}

pub(super) fn max_simultaneous(lines: &[ContentLine]) -> Result<TypeLimit, Box<dyn Error>> {
    let value = field(lines, 0, "MaxSimultaneousOfType")
        .map(scalar)
        .transpose()?
        .unwrap_or("0");
    if value.eq_ignore_ascii_case("DeterminedBySuperweaponRestriction") {
        Ok(TypeLimit::Superweapon)
    } else {
        Ok(TypeLimit::Count(value.parse()?))
    }
}

pub(super) fn max_simultaneous_link(
    lines: &[ContentLine],
) -> Result<Option<String>, Box<dyn Error>> {
    field(lines, 0, "MaxSimultaneousLinkKey")
        .map(|line| scalar(line).map(str::to_owned))
        .transpose()
}

pub(super) fn prerequisites(lines: &[ContentLine]) -> Result<Prerequisites, Box<dyn Error>> {
    let mut objects = Vec::new();
    let mut sciences = Vec::new();
    for block in blocks(lines, 0, "Prerequisites")? {
        for line in block.iter().filter(|line| line.depth == Some(1)) {
            match line.tokens.first().map(String::as_str) {
                Some("Object") if line.tokens.len() > 1 => objects.push(line.tokens[1..].to_vec()),
                Some("Science") => sciences.push(scalar(line)?.to_owned()),
                Some("End") => {}
                _ => return Err(format!("unsupported prerequisite at line {}", line.line).into()),
            }
        }
    }
    Ok(Prerequisites { objects, sciences })
}

pub(super) fn selectable(lines: &[ContentLine]) -> bool {
    has_flag(lines, "KindOf", "SELECTABLE")
}

pub(super) fn scoring(lines: &[ContentLine]) -> SourceScoring {
    SourceScoring {
        create: scores(lines, "SCORE_CREATE"),
        destroy: scores(lines, "SCORE_DESTROY"),
        victory: has_flag(lines, "KindOf", "STRUCTURE")
            && has_flag(lines, "KindOf", "MP_COUNT_FOR_VICTORY"),
    }
}

fn scores(lines: &[ContentLine], explicit: &str) -> bool {
    (has_flag(lines, "KindOf", "SCORE") || has_flag(lines, "KindOf", explicit))
        && ["STRUCTURE", "INFANTRY", "VEHICLE"]
            .iter()
            .any(|kind| has_flag(lines, "KindOf", kind))
}

pub(super) fn geometry_bounds(
    lines: &[ContentLine],
) -> Result<generals_sim::skirmish::GeometryBounds, Box<dyn Error>> {
    let major = optional_number(lines, "GeometryMajorRadius", 1.0)?;
    let minor = optional_number(lines, "GeometryMinorRadius", 1.0)?;
    let height = optional_number(lines, "GeometryHeight", 1.0)?;
    let half_height = height * 0.5;
    // GeometryInfo::getZDeltaToCenterPosition and calcBoundingStuff.
    let (center_height, sphere_radius, max_height) = match geometry_shape(lines)? {
        Shape::Sphere => (0.0, major, major),
        Shape::Cylinder => (half_height, half_height.max(major), height),
        Shape::Box => {
            let xy = major * major + minor * minor;
            (half_height, (xy + half_height * half_height).sqrt(), height)
        }
    };
    Ok(generals_sim::skirmish::GeometryBounds {
        center_height,
        sphere_radius,
        max_height,
    })
}

pub(super) fn area_damage(lines: &[ContentLine]) -> Result<Option<AreaDamage>, Box<dyn Error>> {
    let primary_radius = optional_number(lines, "PrimaryDamageRadius", 0.0)?;
    let secondary_radius = optional_number(lines, "SecondaryDamageRadius", 0.0)?;
    let mut affects = 7;
    if let Some(line) = field(lines, 0, "RadiusDamageAffects") {
        affects = 0;
        for flag in &line.tokens[1..] {
            affects |= match flag.as_str() {
                "ALLIES" => 1,
                "ENEMIES" => 2,
                "NEUTRALS" => 4,
                "SELF" => 8,
                "NOT_SIMILAR" => 16,
                "SUICIDE" => 32,
                "NOT_AIRBORNE" => 64,
                _ => return Err(format!("unsupported RadiusDamageAffects {flag}").into()),
            };
        }
    }
    if primary_radius == 0.0 && secondary_radius == 0.0 && affects & 32 == 0 {
        return Ok(None);
    }
    Ok(Some(AreaDamage {
        at_source: match field(lines, 0, "DamageDealtAtSelfPosition")
            .map(scalar)
            .transpose()?
        {
            None => false,
            Some(value) if value.eq_ignore_ascii_case("Yes") => true,
            Some(value) if value.eq_ignore_ascii_case("No") => false,
            Some(_) => return Err("invalid DamageDealtAtSelfPosition".into()),
        },
        primary_radius,
        secondary_radius,
        affects,
        secondary_damage: optional_number(lines, "SecondaryDamage", 0.0)?,
    }))
}

pub(super) fn kind_flags(line: Option<&ContentLine>) -> u8 {
    line.map_or(0, |line| {
        line.tokens.iter().skip(1).fold(0, |bits, token| {
            bits | match token.as_str() {
                "INFANTRY" => 1,
                "VEHICLE" => 2,
                "AIRCRAFT" => 4,
                _ => 0,
            }
        })
    })
}

fn boolean(lines: &[ContentLine], key: &str) -> Result<bool, Box<dyn Error>> {
    let Some(line) = field(lines, 1, key) else {
        return Ok(false);
    };
    match scalar(line)? {
        "Yes" | "YES" | "yes" => Ok(true),
        "No" | "NO" | "no" => Ok(false),
        _ => Err(format!("invalid AutoHealBehavior {key}").into()),
    }
}

pub(super) fn auto_heal(lines: &[ContentLine]) -> Result<Vec<AutoHeal>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "AutoHealBehavior")
        {
            continue;
        }
        let words = |key: &str| field(module, 1, key).map(|line| line.tokens[1..].to_vec());
        if field(module, 1, "RemovesUpgrades").is_some()
            || field(module, 1, "FXListUpgrade").is_some()
        {
            return Err("AutoHeal upgrade-removal/FX callback needs a concrete binding".into());
        }
        let amount = if let Some(line) = field(module, 1, "HealingAmount") {
            scalar(line)?.parse::<i32>()?
        } else {
            0
        };
        result.push(AutoHeal {
            amount: num_traits::ToPrimitive::to_f32(&amount).expect("INI Int converts to Real"),
            delay: field(module, 1, "HealingDelay").map_or(Ok(u32::MAX), duration_frames)?,
            damage_delay: field(module, 1, "StartHealingDelay").map_or(Ok(0), duration_frames)?,
            starts_active: boolean(module, "StartsActive")?,
            triggers: words("TriggeredBy").unwrap_or_default(),
            conflicts: words("ConflictsWith").unwrap_or_default(),
            requires_all: boolean(module, "RequiresAllTriggers")?,
            targets: generals_sim::skirmish::AutoHealTargets {
                radius: field(module, 1, "Radius")
                    .map_or(Ok(0.0), |line| finite_number(scalar(line)?))?,
                affects_whole_player: boolean(module, "AffectsWholePlayer")?,
                kinds: words("KindOf"),
                forbidden: words("ForbiddenKindOf").unwrap_or_default(),
                skip_self: boolean(module, "SkipSelfForHealing")?,
            },
            single_burst: boolean(module, "SingleBurst")?,
        });
    }
    Ok(result)
}

// ThingTemplate initializes GeometryInfo as SPHERE; each INI radius remains independent.
pub(super) fn geometry_shape(
    lines: &[ContentLine],
) -> Result<generals_sim::collision_geometry::Shape, Box<dyn Error>> {
    use generals_sim::collision_geometry::Shape;
    match field(lines, 0, "Geometry")
        .map(scalar)
        .transpose()?
        .unwrap_or("SPHERE")
        .to_ascii_uppercase()
        .as_str()
    {
        "SPHERE" => Ok(Shape::Sphere),
        "CYLINDER" => Ok(Shape::Cylinder),
        "BOX" => Ok(Shape::Box),
        name => Err(format!("unsupported object geometry {name}").into()),
    }
}

pub fn geometry(
    lines: &[ContentLine],
) -> Result<generals_sim::collision_geometry::Geometry, Box<dyn Error>> {
    Ok(generals_sim::collision_geometry::Geometry {
        shape: geometry_shape(lines)?,
        is_small: field(lines, 0, "GeometryIsSmall")
            .map(scalar)
            .transpose()?
            .is_some_and(|value| value.eq_ignore_ascii_case("Yes")),
        major: optional_number(lines, "GeometryMajorRadius", 1.0)?,
        minor: optional_number(lines, "GeometryMinorRadius", 1.0)?,
        height: optional_number(lines, "GeometryHeight", 1.0)?,
    })
}

// ThingTemplate constructor initializes EnergyProduction to zero.
pub(super) fn energy(lines: &[ContentLine]) -> Result<i32, Box<dyn Error>> {
    Ok(field(lines, 0, "EnergyProduction")
        .map(scalar)
        .transpose()?
        .unwrap_or("0")
        .parse()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn original_buildable_status_accepts_all_four_case_insensitive_values() {
        use generals_sim::skirmish::Buildable;
        let file = content::scan(Asset {
            path: "object.ini".into(),
            origin: "ThingTemplate BuildableStatusNames".into(),
            bytes: b"Object Default\nEnd\nObject Ai\nBuildable=Only_By_AI\nEnd\nObject Override\nBuildable=Ignore_Prerequisites\nEnd\nObject Hidden\nBuildable=No\nEnd\nObject Invalid\nBuildable=Maybe\nEnd\n".to_vec(),
        }).unwrap();
        assert!(matches!(
            buildable(&file.declarations[0].lines).unwrap(),
            Buildable::Yes
        ));
        assert!(matches!(
            buildable(&file.declarations[1].lines).unwrap(),
            Buildable::OnlyByAi
        ));
        assert!(matches!(
            buildable(&file.declarations[2].lines).unwrap(),
            Buildable::IgnorePrerequisites
        ));
        assert!(matches!(
            buildable(&file.declarations[3].lines).unwrap(),
            Buildable::No
        ));
        assert!(buildable(&file.declarations[4].lines).is_err());
    }

    #[test]
    fn source_geometry_enum_is_case_insensitive_without_changing_sphere_bounds() {
        let file = content::scan(Asset {
            path: "object.ini".into(), origin: "native enum parsing".into(),
            bytes: b"Object Jet\nGeometry=Box\nGeometryMajorRadius=14\nGeometryMinorRadius=7\nGeometryHeight=5\nEnd\nObject Ball\nGeometry=Sphere\nGeometryMajorRadius=3\nGeometryHeight=99\nEnd\n".to_vec(),
        }).unwrap();
        let box_lines = &file.declarations[0].lines;
        assert!(matches!(
            geometry_shape(box_lines).unwrap(),
            generals_sim::collision_geometry::Shape::Box
        ));
        let bounds = geometry_bounds(box_lines).unwrap();
        assert_eq!(bounds.center_height.to_bits(), 2.5_f32.to_bits());
        assert_eq!(bounds.max_height.to_bits(), 5.0_f32.to_bits());
        let bounds = geometry_bounds(&file.declarations[1].lines).unwrap();
        assert_eq!(
            [
                bounds.center_height,
                bounds.sphere_radius,
                bounds.max_height
            ]
            .map(f32::to_bits),
            [0.0_f32, 3.0, 3.0].map(f32::to_bits)
        );
    }

    #[test]
    fn original_type_limits_retain_dynamic_flag_link_key_and_ushort_range() {
        let file = content::scan(Asset {
            path: "object.ini".into(),
            origin: "source template limit fields".into(),
            bytes: b"Object Default\nEnd\nObject Fixed\nMaxSimultaneousOfType=1\nEnd\nObject Linked\nMaxSimultaneousOfType=DeterminedBySuperweaponRestriction\nMaxSimultaneousLinkKey=WeaponGroup\nEnd\nObject Negative\nMaxSimultaneousOfType=-1\nEnd\nObject Overflow\nMaxSimultaneousOfType=65536\nEnd\n".to_vec(),
        }).unwrap();
        assert!(matches!(
            max_simultaneous(&file.declarations[0].lines).unwrap(),
            TypeLimit::Count(0)
        ));
        assert!(matches!(
            max_simultaneous(&file.declarations[1].lines).unwrap(),
            TypeLimit::Count(1)
        ));
        assert!(matches!(
            max_simultaneous(&file.declarations[2].lines).unwrap(),
            TypeLimit::Superweapon
        ));
        assert_eq!(
            max_simultaneous_link(&file.declarations[2].lines)
                .unwrap()
                .as_deref(),
            Some("WeaponGroup")
        );
        assert!(
            max_simultaneous_link(&file.declarations[0].lines)
                .unwrap()
                .is_none()
        );
        assert!(max_simultaneous(&file.declarations[3].lines).is_err());
        assert!(max_simultaneous(&file.declarations[4].lines).is_err());
    }

    #[test]
    fn repeated_prerequisite_rows_preserve_and_and_or_boundaries() {
        let file = content::scan(Asset {
            path: "object.ini".into(), origin: "fixture".into(),
            bytes: b"Object Tank\nPrerequisites\nObject=Factory AlternativeFactory\nObject=TechCenter\nScience=TankScience\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let rules = prerequisites(&file.declarations[0].lines).unwrap();
        assert_eq!(
            rules.objects,
            [vec!["Factory", "AlternativeFactory"], vec!["TechCenter"]]
        );
        assert_eq!(rules.sciences, ["TankScience"]);
    }

    #[test]
    fn active_radius_heal_accepts_float_suffix_and_preserves_zero_delay() {
        let source = "Object Medic\nBehavior=AutoHealBehavior Tag\nStartsActive=Yes\nHealingAmount=5\nHealingDelay=1000\nRadius=100.0f\nKindOf=VEHICLE\nForbiddenKindOf=AIRCRAFT\nSkipSelfForHealing=Yes\nEnd\nEnd\n";
        let parse = |text: &str| {
            content::scan(Asset {
                path: "object.ini".into(),
                origin: "fixture".into(),
                bytes: text.as_bytes().to_vec(),
            })
            .unwrap()
        };
        let file = parse(source);
        let heal = auto_heal(&file.declarations[0].lines).unwrap();
        assert_eq!(heal.len(), 1);
        assert_eq!(heal[0].delay, 30);
        assert_eq!(heal[0].targets.kinds.as_ref().unwrap(), &["VEHICLE"]);
        assert_eq!(heal[0].targets.forbidden, ["AIRCRAFT"]);
        assert!((heal[0].targets.radius - 100.0).abs() < f32::EPSILON);
        let immediate = parse(&source.replace("HealingDelay=1000", "HealingDelay=0"));
        let heal = auto_heal(&immediate.declarations[0].lines).unwrap();
        assert_eq!(heal[0].delay, 0);
    }
}
