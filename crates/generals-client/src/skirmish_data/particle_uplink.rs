//! `ParticleUplinkCannonUpdateModuleData` and the orbital `W3DLaserDraw`.
use super::{blocks, damage_type, definition, field, finite_number, scalar, source_numbers};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use generals_sim::skirmish::{
    DamageType, DeathType,
    particle_uplink::{Effects, Rules},
};
use std::error::Error;

type BoxError = Box<dyn Error>;

const FAMILY: &str = "ParticleUplinkCannonUpdate";

pub(super) fn load(files: &[ContentFile], object: &Declaration) -> Result<Vec<Rules>, BoxError> {
    modules(object)?
        .map(|module| {
            let rules = read_rules(files, module)?;
            if rules.damage_pulses == 0 || rules.scorch_marks == 0 || rules.firing_frames == 0 {
                return Err(
                    format!("{} has a zero Particle Uplink firing divisor", object.name).into(),
                );
            }
            if rules.effects.outer_bone_count > 16 {
                return Err(
                    format!("{} has too many Particle Uplink outer nodes", object.name).into(),
                );
            }
            Ok(rules)
        })
        .collect()
}

fn modules(object: &Declaration) -> Result<impl Iterator<Item = &[ContentLine]>, BoxError> {
    Ok(blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|family| family == FAMILY)
        }))
}

fn text<'a>(module: &'a [ContentLine], name: &str) -> Result<&'a str, BoxError> {
    Ok(field(module, 1, name)
        .map(scalar)
        .transpose()?
        .unwrap_or_default())
}

fn read_rules(files: &[ContentFile], module: &[ContentLine]) -> Result<Rules, BoxError> {
    let string = |key| text(module, key).map(ToOwned::to_owned);
    let real = |key, default| {
        field(module, 1, key).map_or(Ok(default), |line| finite_number(scalar(line)?))
    };
    let integer = |key| -> Result<u32, BoxError> {
        field(module, 1, key).map_or(Ok(0), |line| scalar(line)?.parse().map_err(Into::into))
    };
    let duration = |key, default| {
        field(module, 1, key).map_or(Ok(default), |line| {
            source_numbers::duration_frames(scalar(line)?)
        })
    };
    let name = string("SpecialPowerTemplate")?;
    definition(files, "SpecialPower", &name)?;
    let orbital_laser = string("ParticleBeamLaserName")?;
    let orbital_radius = orbital_radius(files, &orbital_laser)?;
    let damage_name = text(module, "DamageType")?;
    let death_name = text(module, "DeathType")?;
    let remnant = super::instant_death::nullable(
        files,
        "Object",
        text(module, "DamagePulseRemnantObjectName")?,
    )?
    .map(|record| record.name.clone());
    Ok(Rules {
        power: name,
        charge_frames: duration("BeginChargeTime", 0)?,
        antenna_frames: duration("RaiseAntennaTime", 0)?,
        ready_delay: duration("ReadyDelayTime", 0)?,
        firing_frames: duration("TotalFiringTime", 0)?,
        growth_frames: duration("WidthGrowTime", 0)?,
        travel_frames: duration("BeamTravelTime", 0)?,
        damage_per_second: real("DamagePerSecond", 0.0)?,
        damage_pulses: integer("TotalDamagePulses")?,
        damage_type: if damage_name.is_empty() {
            DamageType::Laser
        } else {
            damage_type(damage_name)
                .ok_or_else(|| format!("unknown Particle Uplink damage type {damage_name}"))?
        },
        death_type: if death_name.is_empty() {
            DeathType::Lasered
        } else {
            super::death_weapons::death_type(death_name)?
        },
        damage_radius_scale: real("DamageRadiusScalar", 1.0)?,
        scorch_marks: integer("TotalScorchMarks")?,
        scorch_radius_scale: real("ScorchMarkScalar", 1.0)?,
        reveal_range: real("RevealRange", 0.0)?,
        orbital_radius,
        launch_fx_interval: duration("DelayBetweenLaunchFX", 30)?,
        ground_hit_fx: string("GroundHitFX")?,
        beam_launch_fx: string("BeamLaunchFX")?,
        swath_distance: real("SwathOfDeathDistance", 0.0)?,
        swath_amplitude: real("SwathOfDeathAmplitude", 0.0)?,
        driving_speed: real("ManualDrivingSpeed", 0.0)?,
        fast_driving_speed: real("ManualFastDrivingSpeed", 0.0)?,
        double_click_frames: duration("DoubleClickToFastDriveDelay", 500)?,
        remnant,
        sounds: [
            string("PoweringUpSoundLoop")?,
            string("UnpackToIdleSoundLoop")?,
            string("FiringToPackSoundLoop")?,
            string("GroundAnnihilationSoundLoop")?,
        ],
        effects: Effects {
            outer_bone: string("OuterEffectBoneName")?,
            outer_bone_count: integer("OuterEffectNumBones")?,
            connector_bone: string("ConnectorBoneName")?,
            fire_bone: string("FireBoneName")?,
            outer_flares: [
                string("OuterNodesLightFlareParticleSystem")?,
                string("OuterNodesMediumFlareParticleSystem")?,
                string("OuterNodesIntenseFlareParticleSystem")?,
            ],
            connector_lasers: [
                string("ConnectorMediumLaserName")?,
                string("ConnectorIntenseLaserName")?,
            ],
            connector_flares: [
                string("ConnectorMediumFlare")?,
                string("ConnectorIntenseFlare")?,
            ],
            base_flares: [
                string("LaserBaseLightFlareParticleSystemName")?,
                string("LaserBaseMediumFlareParticleSystemName")?,
                string("LaserBaseIntenseFlareParticleSystemName")?,
            ],
            orbital_laser,
        },
    })
}

fn orbital_radius(files: &[ContentFile], orbital_laser: &str) -> Result<f32, BoxError> {
    Ok(
        if orbital_laser.is_empty() || orbital_laser.eq_ignore_ascii_case("NONE") {
            // GameLogic's template radius when there is no orbital drawable.
            13.0
        } else {
            let object = definition(files, "Object", orbital_laser)?;
            let draw = blocks(&object.lines, 0, "Draw")?
                .into_iter()
                .find(|draw| {
                    draw[0]
                        .tokens
                        .get(1)
                        .is_some_and(|name| name == "W3DLaserDraw")
                })
                .ok_or("orbital Particle Uplink laser lacks W3DLaserDraw")?;
            super::number(draw, 1, "OuterBeamWidth")? * 0.5
        },
    )
}

pub(super) fn dependencies<'a>(
    files: &'a [ContentFile],
    selected: &[&str],
) -> Result<Vec<&'a str>, BoxError> {
    let mut result = Vec::new();
    for name in selected {
        let object = definition(files, "Object", name)?;
        for module in modules(object)? {
            let remnant = text(module, "DamagePulseRemnantObjectName")?;
            if let Some(template) = super::instant_death::nullable(files, "Object", remnant)?
                && !selected.contains(&template.name.as_str())
                && !result.contains(&template.name.as_str())
            {
                result.push(template.name.as_str());
            }
        }
    }
    Ok(result)
}
