//! Original `OCLSpecialPower` data and dependency closure.
use super::{blocks, definition, field, scalar};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{WeaponRules, deliver_payload, object_creation, power_ocl};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<(Vec<power_ocl::Rules>, Vec<WeaponRules>), Box<dyn Error>> {
    let mut result = Vec::new();
    let mut weapons = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "OCLSpecialPower")
        {
            continue;
        }
        let power = value(module, "SpecialPowerTemplate", "")?;
        definition(files, "SpecialPower", &power)?;
        let location = match value(module, "CreateLocation", "CREATE_AT_EDGE_NEAR_SOURCE")?.as_str()
        {
            "CREATE_AT_EDGE_NEAR_SOURCE" => power_ocl::Location::NearSource,
            "CREATE_AT_EDGE_NEAR_TARGET" => power_ocl::Location::NearTarget,
            "CREATE_AT_EDGE_FARTHEST_FROM_TARGET" => power_ocl::Location::FarTarget,
            "CREATE_AT_LOCATION" => power_ocl::Location::AtTarget,
            "CREATE_ABOVE_LOCATION" => power_ocl::Location::AboveTarget,
            "USE_OWNER_OBJECT" => power_ocl::Location::Owner,
            _ => return Err("unknown native OCL power creation location".into()),
        };
        let mut upgrades = Vec::new();
        for line in module.iter().filter(|line| {
            line.depth == Some(1) && line.tokens.first().is_some_and(|key| key == "UpgradeOCL")
        }) {
            if line.tokens.len() != 3 {
                return Err("invalid OCL power science/list pair".into());
            }
            upgrades.push((
                line.tokens[1].clone(),
                list(files, &line.tokens[2], &mut weapons)?,
            ));
        }
        result.push(power_ocl::Rules {
            power,
            location,
            reference: value(module, "ReferenceObject", "")?,
            adjust_position: match value(module, "OCLAdjustPositionToPassable", "No")?.as_str() {
                value if value.eq_ignore_ascii_case("Yes") => true,
                value if value.eq_ignore_ascii_case("No") => false,
                _ => return Err("invalid OCLAdjustPositionToPassable".into()),
            },
            default: list(files, &value(module, "OCL", "")?, &mut weapons)?,
            upgrades,
        });
    }
    Ok((result, weapons))
}

fn value(lines: &[ContentLine], key: &str, default: &str) -> Result<String, Box<dyn Error>> {
    Ok(field(lines, 1, key)
        .map(scalar)
        .transpose()?
        .unwrap_or(default)
        .into())
}

fn list(
    files: &[ContentFile],
    name: &str,
    weapons: &mut Vec<WeaponRules>,
) -> Result<power_ocl::List, Box<dyn Error>> {
    let mut result = power_ocl::List {
        name: name.into(),
        nuggets: Vec::new(),
    };
    let Some(declaration) = super::instant_death::nullable(files, "ObjectCreationList", name)?
    else {
        return Ok(result);
    };
    let mut delivery = 0;
    for (index, line) in declaration.lines.iter().enumerate() {
        if line.depth != Some(0) || line.tokens.first().is_none_or(|name| name == "End") {
            continue;
        }
        let end = declaration.lines[index + 1..]
            .iter()
            .position(|line| line.depth == Some(0))
            .map_or(declaration.lines.len(), |end| index + 1 + end);
        let block = &declaration.lines[index..end];
        let nugget = match line.tokens[0].as_str() {
            "CreateObject" => power_ocl::Nugget::Objects(object_creation::List {
                name: name.into(),
                objects: vec![super::object_creation::parse_object(block)?],
            }),
            "DeliverPayload" => {
                let rules = super::reinforcement::delivery_nugget(files, name, delivery)?;
                delivery += 1;
                power_ocl::Nugget::Delivery(rules.name)
            }
            "FireWeapon" => {
                let name = value(block, "Weapon", "")?;
                let source = definition(files, "Weapon", &name)?;
                let mut weapon = WeaponRules::unarmed();
                weapon.name = name;
                super::weapons::parse(&mut weapon, &source.lines)?;
                let ordinal = weapons.len();
                weapons.push(weapon);
                power_ocl::Nugget::FireWeapon(ordinal)
            }
            "Attack" => power_ocl::Nugget::Attack {
                shots: value(block, "NumberOfShots", "1")?.parse()?,
                slot: match value(block, "WeaponSlot", "PRIMARY")?.as_str() {
                    "PRIMARY" => 0,
                    "SECONDARY" => 1,
                    "TERTIARY" => 2,
                    _ => return Err("invalid OCL Attack weapon slot".into()),
                },
                decal: super::spectre::decal(block, "DeliveryDecal")?,
                radius: value(block, "DeliveryDecalRadius", "0")?.parse()?,
            },
            family => return Err(format!("{name}: untranslated power OCL nugget {family}").into()),
        };
        result.nuggets.push(nugget);
    }
    Ok(result)
}

pub(super) fn deliveries(
    files: &[ContentFile],
    selected: &[&str],
) -> Result<Vec<deliver_payload::Rules>, Box<dyn Error>> {
    let mut result: Vec<deliver_payload::Rules> = Vec::new();
    for name in selected {
        for rules in load(files, definition(files, "Object", name)?)?.0 {
            for list in std::iter::once(rules.default)
                .chain(rules.upgrades.into_iter().map(|(_, list)| list))
            {
                let mut ordinal = 0;
                for nugget in list.nuggets {
                    if let power_ocl::Nugget::Delivery(name) = nugget {
                        if !result.iter().any(|existing| existing.name == name) {
                            result.push(super::reinforcement::delivery_nugget(
                                files, &list.name, ordinal,
                            )?);
                        }
                        ordinal += 1;
                    }
                }
            }
        }
    }
    Ok(result)
}

pub(super) fn append_delivery_weapons(
    files: &[ContentFile],
    deliveries: &[deliver_payload::Rules],
    unit: &mut generals_sim::skirmish::UnitType,
) -> Result<(), Box<dyn Error>> {
    for delivery in deliveries
        .iter()
        .filter(|delivery| delivery.transport == unit.name)
    {
        if let Some(dive) = &delivery.dive
            && let Some(transport) = &mut unit.deliver_payload
            && !dive.effect.is_empty()
            && !dive.effect.eq_ignore_ascii_case("NONE")
            && !transport.strafe_effects.contains(&dive.effect)
        {
            transport.strafe_effects.push(dive.effect.clone());
        }
        let Some(visible) = &delivery.visible else {
            continue;
        };
        if visible.weapon.is_empty()
            || visible.weapon.eq_ignore_ascii_case("NONE")
            || unit
                .behavior
                .power_weapons
                .iter()
                .any(|weapon| weapon.name == visible.weapon)
        {
            continue;
        }
        let source = definition(files, "Weapon", &visible.weapon)?;
        let mut weapon = WeaponRules::unarmed();
        weapon.name.clone_from(&visible.weapon);
        super::weapons::parse(&mut weapon, &source.lines)?;
        unit.behavior.power_weapons.push(weapon);
    }
    Ok(())
}

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let mut names = Vec::new();
    for name in selected {
        let (rules, weapons) = load(files, definition(files, "Object", name)?)?;
        for rules in rules {
            if !rules.reference.is_empty() {
                names.push(rules.reference);
            }
            for list in std::iter::once(rules.default)
                .chain(rules.upgrades.into_iter().map(|(_, list)| list))
            {
                for nugget in list.nuggets {
                    if let power_ocl::Nugget::Objects(list) = nugget {
                        for object in list.objects {
                            names.extend(object.names);
                            if !object.container.is_empty() {
                                names.push(object.container);
                            }
                        }
                    }
                }
            }
        }
        for weapon in weapons {
            let declaration = definition(files, "Weapon", &weapon.name)?;
            if let Some(line) = field(&declaration.lines, 0, "ProjectileObject") {
                names.push(scalar(line)?.into());
            }
        }
    }
    for delivery in deliveries(files, selected)? {
        names.push(delivery.transport);
        if let Some(visible) = delivery.visible
            && !visible.object.is_empty()
        {
            names.push(visible.object);
        }
        if !delivery.container.is_empty() {
            names.push(delivery.container);
        }
        names.extend(delivery.payload.into_iter().map(|payload| payload.object));
    }
    let mut result = Vec::new();
    for name in names {
        if name.eq_ignore_ascii_case("NONE") {
            continue;
        }
        let name = definition(files, "Object", &name)?.name.as_str();
        if !selected.contains(&name) && !result.contains(&name) {
            result.push(name);
        }
    }
    Ok(result)
}
