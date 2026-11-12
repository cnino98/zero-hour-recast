//! Authored `JetAIUpdate` locomotor sets and state timings.
use super::{blocks, definition, field, has_flag, locomotor, scalar, selected_pair};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::aircraft::{LocomotorSet, Rules};
use std::error::Error;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod research_tests;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let Some(module) = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "JetAIUpdate")
        })
    else {
        return Ok(None);
    };
    let taxi = load_set(files, object, "SET_TAXIING")?.ok_or("JetAIUpdate lacks taxi locomotor")?;
    let attacking = special_set(files, object, module, "AttackLocomotorType")?;
    let returning = special_set(files, object, module, "ReturnForAmmoLocomotorType")?;
    let rules = Rules {
        taxi,
        attacking,
        returning,
        needs_runway: boolean(module, "NeedsRunway", true)?,
        keeps_space: boolean(module, "KeepsParkingSpaceWhenAirborne", true)?,
        helipad: has_flag(&object.lines, "KindOf", "PRODUCED_AT_HELIPAD"),
        takeoff_pause: duration(module, "TakeoffPause")?,
        idle_return: duration(module, "ReturnToBaseIdleTime")?,
        out_of_ammo_damage: field(module, 1, "OutOfAmmoDamagePerSecond")
            .map(scalar)
            .transpose()?
            .map_or(Ok(0.0), percent)?,
        min_height: real(module, "MinHeight")?,
        parking_offset: real(module, "ParkingOffset")?,
        attack_locomotor_frames: duration(module, "AttackLocomotorPersistTime")?,
        attackers_miss_frames: duration(module, "AttackersMissPersistTime")?,
        lockon: load_lockon(files, module)?,
        sounds: load_sounds(files, object)?,
        sneaky_offset: field(module, 1, "SneakyOffsetWhenAttacking")
            .map(scalar)
            .transpose()?
            .map_or(Ok(0.0), super::source_real)?,
    };
    Ok(Some(rules))
}

fn special_set(
    files: &[ContentFile],
    object: &Declaration,
    module: &[ContentLine],
    field_name: &str,
) -> Result<Option<LocomotorSet>, Box<dyn Error>> {
    let set = field(module, 1, field_name)
        .map(scalar)
        .transpose()?
        .unwrap_or("SET_NORMAL");
    if set == "SET_NORMAL" {
        return Ok(None);
    }
    load_set(files, object, set)?.map_or_else(
        || Err(format!("missing aircraft locomotor {set}").into()),
        |loco| Ok(Some(loco)),
    )
}

fn load_set(
    files: &[ContentFile],
    object: &Declaration,
    name: &str,
) -> Result<Option<LocomotorSet>, Box<dyn Error>> {
    use generals_sim::skirmish::locomotor_set as set;
    let role = match name {
        "SET_NORMAL" => set::NORMAL,
        "SET_NORMAL_UPGRADED" => set::UPGRADED,
        "SET_FREEFALL" => set::FREEFALL,
        "SET_WANDER" => set::WANDER,
        "SET_PANIC" => set::PANIC,
        "SET_TAXIING" => set::TAXI,
        "SET_SUPERSONIC" => set::SUPERSONIC,
        "SET_SLUGGISH" => set::SLUGGISH,
        _ => return Err(format!("unknown aircraft locomotor set {name}").into()),
    };
    let Some(name) = selected_pair(&object.lines, 0, "Locomotor", name)? else {
        return Ok(None);
    };
    let lines = &definition(files, "Locomotor", name)?.lines;
    let global = &definition(files, "GameData", "")?.lines;
    Ok(Some(LocomotorSet {
        role,
        locomotor: locomotor::load(lines, global, &object.lines)?,
        speed: super::number(lines, 0, "Speed")?,
    }))
}

fn boolean(module: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    match field(module, 1, key).map(scalar).transpose()? {
        Some(value) if value.eq_ignore_ascii_case("Yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("No") => Ok(false),
        None => Ok(default),
        Some(_) => Err(format!("invalid JetAIUpdate {key}").into()),
    }
}

fn duration(module: &[ContentLine], key: &str) -> Result<u32, Box<dyn Error>> {
    field(module, 1, key).map_or(Ok(0), super::duration_frames)
}

fn real(module: &[ContentLine], key: &str) -> Result<f32, Box<dyn Error>> {
    field(module, 1, key)
        .map(scalar)
        .transpose()?
        .map_or(Ok(0.0), super::finite_number)
}

fn percent(value: &str) -> Result<f32, Box<dyn Error>> {
    Ok(super::finite_number(value.trim_end_matches('%'))? * 0.01)
}

fn load_lockon(
    files: &[ContentFile],
    module: &[ContentLine],
) -> Result<generals_sim::skirmish::aircraft::lockon::Rules, Box<dyn Error>> {
    let mut rules = generals_sim::skirmish::aircraft::lockon::Rules::native();
    rules.frames = field(module, 1, "LockonTime")
        .map(scalar)
        .transpose()?
        .map_or(Ok(0), super::source_numbers::duration_frames)?;
    if let Some(misc) = files
        .iter()
        .flat_map(|file| &file.declarations)
        .rev()
        .find(|definition| definition.kind == "MiscAudio")
    {
        field(&misc.lines, 0, "LockonTickSound")
            .map(scalar)
            .transpose()?
            .unwrap_or("")
            .clone_into(&mut rules.tick_sound);
    }
    field(module, 1, "LockonCursor")
        .map(scalar)
        .transpose()?
        .unwrap_or("")
        .clone_into(&mut rules.cursor);
    for (name, output) in [
        ("LockonInitialDist", &mut rules.initial_distance),
        ("LockonFreq", &mut rules.frequency),
    ] {
        if let Some(line) = field(module, 1, name) {
            *output = super::source_real(scalar(line)?)?;
        }
    }
    if let Some(line) = field(module, 1, "LockonAngleSpin") {
        rules.angle_spin = super::source_real(scalar(line)?)? * (std::f32::consts::PI / 180.0);
    }
    rules.blinky = boolean(module, "LockonBlinky", false)?;
    Ok(rules)
}

fn load_sounds(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<generals_sim::skirmish::aircraft::Sounds, Box<dyn Error>> {
    let sounds = blocks(&object.lines, 0, "UnitSpecificSounds")?;
    let sound = |key| -> Result<String, Box<dyn Error>> {
        sounds.first().map_or(Ok(String::new()), |lines| {
            Ok(field(lines, 1, key)
                .map(scalar)
                .transpose()?
                .unwrap_or("")
                .to_owned())
        })
    };
    let misc = files
        .iter()
        .flat_map(|file| &file.declarations)
        .rev()
        .find(|declaration| declaration.kind == "MiscAudio");
    let wheel = misc.map_or(Ok(""), |misc| {
        field(&misc.lines, 0, "AircraftWheelScreech")
            .map(scalar)
            .transpose()
            .map(|name| name.unwrap_or(""))
    })?;
    Ok(generals_sim::skirmish::aircraft::Sounds {
        afterburner: sound("Afterburner")?,
        low_fuel: sound("VoiceLowFuel")?,
        wheel_screech: wheel.to_owned(),
    })
}
