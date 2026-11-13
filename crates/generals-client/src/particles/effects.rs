mod bones;
pub(super) mod death;
#[cfg(test)]
mod exhaust_tests;
pub(super) mod rng;
mod scorch;
#[cfg(test)]
mod tests;
use super::{Particles, Pose, Template, random};
use crate::{
    model_animation::{Animations, Players},
    playtest::Match,
    skirmish_data::{blocks, definition, field, scalar},
};
use bevy::prelude::*;
use generals_assets::{
    content::{self, ContentFile, ContentLine},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{ProjectileFxStage, UnitType};
use std::error::Error;

/// The same ordered FX traversal serves buffered position effects and object
/// callbacks with their distinct sound ownership and child-position shroud gate.
pub(super) struct Output<'a> {
    pub random: &'a mut dyn FnMut(rng::Request) -> rng::Value,
    pub sound: &'a mut dyn FnMut(&str, [f32; 3], bool),
    pub clear: &'a dyn Fn([f32; 3]) -> bool,
}

pub(super) struct Callback<'a> {
    pub pose: &'a Pose,
    pub object: Option<(usize, f32)>,
    pub secondary: Option<[f32; 3]>,
}

pub(super) struct LoadedEffects {
    pub bindings: Vec<Vec<Option<WeaponEffects>>>,
    pub lists: Vec<Vec<Nugget>>,
    pub actor_fx: std::collections::BTreeMap<String, usize>,
    pub propaganda: Vec<[Option<usize>; 2]>,
}

pub(super) struct WeaponEffects {
    pub(super) fire: [Option<usize>; 2],
    exhaust: [Option<usize>; 2],
    ignition: Option<usize>,
    detonation: Option<usize>,
    garrison_kill: Option<usize>,
}
pub(super) enum Nugget {
    Particle {
        template: usize,
        count: i32,
        radius: Option<[f32; 2]>,
        height: Option<[f32; 2]>,
        oriented: bool,
        ricochet: bool,
        attached: bool,
        offset: [f32; 3],
        ground: bool,
        caller_radius: bool,
        delay: Option<[f32; 2]>,
    },
    Light {
        color: [f32; 3],
        radius: f32,
        percent: f32,
        increase: u32,
        decrease: u32,
    },
    Sound(String),
    ViewShake(usize),
    Scorch {
        kind: i32,
        radius: f32,
    },
    Tracer,
    AtBones {
        list: usize,
        bone: String,
    },
}

fn source_files(assets: &VirtualAssets) -> Result<Vec<ContentFile>, Box<dyn Error>> {
    let mut files = [
        "weapon",
        "fxlist",
        "object/weaponobjects",
        "object/americainfantry",
        "object/chinainfantry",
        "object/glainfantry",
        "object/civilianunit",
        "object/system",
        "object/hulk",
    ]
    .into_iter()
    .map(|stem| content::scan(assets.read(&format!("data/ini/{stem}.ini"))?))
    .collect::<Result<Vec<_>, generals_assets::AssetError>>()?;
    crate::skirmish_data::resolve_reskins(&mut files)?;
    Ok(files)
}

pub(super) fn load(
    assets: &VirtualAssets,
    types: &[UnitType],
    templates: &[Template],
) -> Result<LoadedEffects, Box<dyn Error>> {
    let files = source_files(assets)?;
    let mut bindings = Vec::new();
    let mut lists = Vec::new();
    for unit in types {
        let mut slots = (0..unit.weapon_template_count())
            .map(|_| None)
            .collect::<Vec<_>>();
        for (slot, output) in slots.iter_mut().enumerate() {
            let Some(rules) = unit
                .weapon_template(slot)
                .filter(|rules| !rules.name.is_empty())
            else {
                continue;
            };
            let weapon = &definition(&files, "Weapon", &rules.name)?.lines;
            let resolve = |name: &str,
                           lists: &mut Vec<Vec<Nugget>>|
             -> Result<Option<usize>, Box<dyn Error>> {
                if name.eq_ignore_ascii_case("NONE") {
                    return Ok(None);
                }
                let fx = crate::skirmish_data::fx_definition(&files, name)?
                    .ok_or_else(|| format!("missing FXList {name}"))?;
                let list = parse_list(&files, &fx.lines, templates, lists, true)?;
                let index = lists.len();
                lists.push(list);
                Ok(Some(index))
            };
            let normal = value(weapon, "FireFX")?.unwrap_or("NONE");
            let heroic = veterancy(weapon, "VeterancyFireFX")?.unwrap_or(normal);
            let fire = [resolve(normal, &mut lists)?, resolve(heroic, &mut lists)?];
            let normal = value(weapon, "ProjectileExhaust")?.unwrap_or("NONE");
            let heroic = veterancy(weapon, "VeterancyProjectileExhaust")?.unwrap_or(normal);
            let exhaust = [
                template_index(templates, normal),
                template_index(templates, heroic),
            ];
            let projectile = value(weapon, "ProjectileObject")?.unwrap_or("NONE");
            let ignition = if projectile == "NONE" {
                None
            } else {
                ignition_fx(&files, projectile)?
                    .map(|name| resolve(name, &mut lists))
                    .transpose()?
                    .flatten()
            };
            let detonation = resolve(
                value(weapon, "ProjectileDetonationFX")?.unwrap_or("NONE"),
                &mut lists,
            )?;
            let garrison_kill = resolve(garrison_fx(&files, projectile)?, &mut lists)?;
            *output = Some(WeaponEffects {
                fire,
                exhaust,
                ignition,
                detonation,
                garrison_kill,
            });
        }
        bindings.push(slots);
    }
    let propaganda = super::object_effects::load(&files, types, templates, &mut lists)?;
    let actor_fx = super::actor_fx::load(&files, types, templates, &mut lists)?;
    Ok(LoadedEffects {
        bindings,
        lists,
        actor_fx,
        propaganda,
    })
}

fn ignition_fx<'a>(
    files: &'a [ContentFile],
    name: &str,
) -> Result<Option<&'a str>, Box<dyn Error>> {
    let projectile = definition(files, "Object", name)?;
    let module = blocks(&projectile.lines, 0, "Behavior")?
        .into_iter()
        .find(|lines| {
            lines[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "MissileAIUpdate")
        });
    module
        .and_then(|lines| field(lines, 1, "IgnitionFX"))
        .map(scalar)
        .transpose()
}

pub(super) fn parse_list(
    files: &[ContentFile],
    lines: &[ContentLine],
    templates: &[Template],
    lists: &mut Vec<Vec<Nugget>>,
    object_form: bool,
) -> Result<Vec<Nugget>, Box<dyn Error>> {
    let mut result = Vec::new();
    // Native FXList preserves authored nugget order.
    for (start, line) in lines.iter().enumerate() {
        if line.depth != Some(0) || line.tokens.is_empty() || line.tokens[0] == "End" {
            continue;
        }
        let end = lines[start + 1..]
            .iter()
            .position(|l| l.depth == Some(0))
            .map_or(lines.len(), |i| start + 1 + i);
        let fields = &lines[start..end];
        let value = |key: &str| field(fields, 1, key).map(scalar).transpose();
        let nugget =
            match line.tokens[0].as_str() {
                "ParticleSystem" => {
                    let Some(template) =
                        template_index(templates, value("Name")?.ok_or("particle FX lacks name")?)
                    else {
                        continue;
                    };
                    for f in &fields[1..] {
                        if !matches!(
                            f.tokens[0].as_str(),
                            "Name"
                                | "Count"
                                | "Radius"
                                | "OrientToObject"
                                | "Ricochet"
                                | "AttachToObject"
                                | "Offset"
                                | "CreateAtGroundHeight"
                                | "UseCallersRadius"
                                | "Height"
                                | "InitialDelay"
                        ) {
                            return Err(format!(
                                "FX particle {} requires unported {}",
                                line.line, f.tokens[0]
                            )
                            .into());
                        }
                    }
                    Nugget::Particle {
                        template,
                        count: value("Count")?.unwrap_or("1").parse()?,
                        radius: particle_range(fields, "Radius")?,
                        height: particle_range(fields, "Height")?,
                        oriented: particle_boolean(fields, "OrientToObject")?,
                        ricochet: particle_boolean(fields, "Ricochet")?,
                        attached: particle_boolean(fields, "AttachToObject")?,
                        offset: field(fields, 1, "Offset")
                            .map(|line| super::data::coordinate(&line.tokens[1..]))
                            .transpose()?
                            .unwrap_or([0.0; 3]),
                        ground: particle_boolean(fields, "CreateAtGroundHeight")?,
                        caller_radius: particle_boolean(fields, "UseCallersRadius")?,
                        delay: particle_delay(fields)?,
                    }
                }
                "FXListAtBonePos" if object_form => bones::parse(files, fields, templates, lists)?,
                "FXListAtBonePos" => return Err(
                    "nested bone FX requires an object, but native child FX receives a position"
                        .into(),
                ),
                "Tracer" => Nugget::Tracer,
                "Sound" => Nugget::Sound(value("Name")?.unwrap_or("").to_owned()),
                "LightPulse" => light(fields)?,
                "ViewShake" => Nugget::ViewShake(super::view_shake::kind(value("Type")?)?),
                "TerrainScorch" => scorch::parse(fields)?,
                name => {
                    return Err(
                        format!("FX nugget {name} at line {} not translated", line.line).into(),
                    );
                }
            };
        if result.iter().any(|entry| matches!(entry, Nugget::Tracer)) {
            return Err("selected Tracer FX must be the final nugget".into());
        }
        result.push(nugget);
    }
    Ok(result)
}
fn particle_boolean(fields: &[ContentLine], key: &str) -> Result<bool, Box<dyn Error>> {
    match field(fields, 1, key).map(scalar).transpose()? {
        None => Ok(false),
        Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
        Some(value) => Err(format!("invalid particle FX {key}: {value}").into()),
    }
}

fn particle_delay(fields: &[ContentLine]) -> Result<Option<[f32; 2]>, Box<dyn Error>> {
    let delay = particle_range(fields, "InitialDelay")?;
    if delay.is_some_and(|range| range.iter().any(|value| *value * 0.03 > 16_777_216.0)) {
        return Err("FX particle delay exceeds exact frame counter range".into());
    }
    Ok(delay)
}

fn particle_range(fields: &[ContentLine], key: &str) -> Result<Option<[f32; 2]>, Box<dyn Error>> {
    let Some(line) = field(fields, 1, key) else {
        return Ok(None);
    };
    if !(3..=4).contains(&line.tokens.len())
        || line
            .tokens
            .get(3)
            .is_some_and(|value| value != "UNIFORM" && value != "CONSTANT")
    {
        return Err(format!("unsupported FX particle {key} distribution").into());
    }
    let delay = [
        super::data::real(&line.tokens[1])?,
        super::data::real(&line.tokens[2])?,
    ];
    Ok(Some(delay))
}

fn light(fields: &[ContentLine]) -> Result<Nugget, Box<dyn Error>> {
    for line in fields.iter().filter(|line| line.depth == Some(1)) {
        if !matches!(
            line.tokens[0].as_str(),
            "Color"
                | "Radius"
                | "RadiusAsPercentOfObjectSize"
                | "IncreaseTime"
                | "DecreaseTime"
                | "End"
        ) {
            return Err(format!("unsupported LightPulse field {}", line.tokens[0]).into());
        }
    }
    let value = |key: &str| field(fields, 1, key).map(scalar).transpose();
    let color = field(fields, 1, "Color")
        .map(|line| super::data::rgb(&line.tokens[1..]))
        .transpose()?
        .unwrap_or([0.0; 3]);
    let duration = |key| {
        field(fields, 1, key)
            .map(crate::skirmish_data::duration_frames)
            .transpose()
            .map(|value| value.unwrap_or(0))
    };
    Ok(Nugget::Light {
        color,
        radius: super::data::real(value("Radius")?.unwrap_or("0"))?,
        percent: super::data::real(
            value("RadiusAsPercentOfObjectSize")?
                .unwrap_or("0")
                .trim_end_matches('%'),
        )? / 100.0,
        increase: duration("IncreaseTime")?,
        decrease: duration("DecreaseTime")?,
    })
}

fn template_index(templates: &[Template], name: &str) -> Option<usize> {
    templates.iter().position(|template| template.name == name)
}

fn value<'a>(lines: &'a [ContentLine], key: &str) -> Result<Option<&'a str>, Box<dyn Error>> {
    field(lines, 0, key).map(scalar).transpose()
}
fn veterancy<'a>(lines: &'a [ContentLine], key: &str) -> Result<Option<&'a str>, Box<dyn Error>> {
    field(lines, 0, key)
        .map(|line| {
            if line.tokens.len() != 3 || line.tokens[1] != "HEROIC" {
                return Err("unsupported veterancy FX".into());
            }
            Ok(line.tokens[2].as_str())
        })
        .transpose()
}

impl Particles {
    pub fn projectile_event(
        &mut self,
        simulation: &generals_sim::skirmish::Skirmish,
        generation: u32,
        event: &generals_sim::skirmish::ProjectileFxEvent,
        players: Option<&Players>,
        animations: &mut Animations,
    ) {
        self.begin(generation, simulation.tick);
        let Some(binding) = self.bindings[event.weapon_type][event.weapon_index].as_ref() else {
            return;
        };
        let pose = Pose {
            position: event.position,
            axes: event.axes,
        };
        if matches!(event.stage, ProjectileFxStage::Created) {
            // MissileAIUpdate creates its exhaust at ignition, not launch.
            return;
        }
        if matches!(event.stage, ProjectileFxStage::Ignition) {
            let ignition = binding.ignition;
            let exhaust = binding.exhaust[usize::from(event.veterancy == 3)];
            self.ignite_projectile(simulation, generation, event, ignition, exhaust, animations);
            return;
        }
        if matches!(event.stage, ProjectileFxStage::Lasered) {
            return;
        }
        if !matches!(
            simulation.visibility_cell(0, [event.position[0], event.position[1]]),
            generals_sim::skirmish::visibility::Status::Clear
        ) {
            return;
        }
        let index = match event.stage {
            ProjectileFxStage::Detonation => binding.detonation,
            ProjectileFxStage::GarrisonKill => binding.garrison_kill,
            ProjectileFxStage::Created => unreachable!("creation handled before FX visibility"),
            ProjectileFxStage::Ignition => unreachable!("ignition handled before FX visibility"),
            ProjectileFxStage::Lasered => unreachable!("laser death handled separately"),
        };
        if let Some(index) = index {
            let bones = event
                .object
                .zip(players)
                .map_or_else(Vec::new, |(id, players)| {
                    self.effect_bones(index, &simulation.entities[id], players, animations, &pose)
                        .unwrap_or_else(|error| {
                            error!("Original building FX bones: {error}");
                            Vec::new()
                        })
                });
            self.fire_from(
                index,
                (&pose, event.object.zip(event.object_geometry_radius)),
                &bones,
                &|point| simulation.ground_height(point),
                if matches!(event.stage, ProjectileFxStage::Detonation) {
                    simulation.catalog[event.weapon_type]
                        .weapon_template(event.weapon_index)
                        .and_then(|weapon| weapon.area.as_ref())
                        .map_or(0.0, |area| area.primary_radius)
                } else {
                    0.0
                },
                &mut rng::source(animations, generation),
            );
        }
    }

    fn ignite_projectile(
        &mut self,
        simulation: &generals_sim::skirmish::Skirmish,
        generation: u32,
        event: &generals_sim::skirmish::ProjectileFxEvent,
        ignition: Option<usize>,
        exhaust: Option<usize>,
        animations: &mut Animations,
    ) {
        let pose = Pose {
            position: event.position,
            axes: event.axes,
        };
        // MissileAIUpdate::doIgnitionState (480–484): visible IgnitionFX first,
        // then unconditional attached-system creation. Shroud suppresses later
        // exhaust emission through the attached missile, not its constructor.
        if let Some(index) = ignition
            && death::visible(simulation, event)
        {
            self.fire(
                index,
                &pose,
                &[],
                &|point| simulation.ground_height(point),
                0.0,
                &mut rng::source(animations, generation),
            );
        }
        if let Some(template) = exhaust {
            self.create(
                template,
                pose,
                Some(event.projectile),
                &mut random(animations, generation),
            );
        }
    }

    pub(crate) fn flush_effect_sounds(&mut self, audio: &mut crate::source_audio::SourceAudio) {
        for (name, position) in std::mem::take(&mut self.sounds) {
            audio.fx(&name, position);
        }
    }

    pub fn fire_weapon(
        &mut self,
        simulation: &generals_sim::skirmish::Skirmish,
        shot: &generals_sim::skirmish::Shot,
        position: [f32; 3],
        animations: &mut Animations,
        stamp: (u32, u64),
    ) {
        let (generation, frame) = stamp;
        self.begin(generation, frame);
        let type_index = shot.weapon_type;
        let Some(index) = self
            .bindings
            .get(type_index)
            .and_then(|slots| slots[shot.weapon_index].as_ref())
            .and_then(|binding| binding.fire[usize::from(shot.veterancy == 3)])
        else {
            return;
        };
        let (s, c) = shot.source_facing.sin_cos();
        self.fire(
            index,
            &Pose {
                position,
                axes: [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]],
            },
            &[],
            &|point| simulation.ground_height(point),
            simulation.catalog[type_index]
                .weapon_template(shot.weapon_index)
                .and_then(|weapon| weapon.area.as_ref())
                .map_or(0.0, |area| area.primary_radius),
            &mut rng::source(animations, generation),
        );
    }
    fn fire_particle(
        &mut self,
        effect: (usize, usize),
        source: &Callback<'_>,
        ground_height: &impl Fn([f32; 2]) -> f32,
        override_radius: f32,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        let pose = source.pose;
        let object = source.object.map(|(id, _)| id);
        let secondary = source.secondary;
        let (list, nugget) = effect;
        let Nugget::Particle {
            template,
            radius: placement_radius,
            height,
            oriented,
            ricochet,
            attached,
            offset,
            ground,
            caller_radius,
            delay,
            ..
        } = &self.lists[list][nugget]
        else {
            unreachable!("particle FX dispatch")
        };
        let redirected = if *ricochet && object.is_some() {
            secondary.map(|secondary| ricochet_pose(pose.position, secondary))
        } else {
            None
        };
        let matrix = redirected.as_ref().unwrap_or(pose);
        let shifted = matrix.rotate(*offset);
        let mut position = std::array::from_fn(|axis| pose.position[axis] + shifted[axis]);
        let axes = if *oriented {
            matrix.axes
        } else {
            Pose::at([0.0; 3]).axes
        };
        let attachment = object.filter(|_| *attached).map(|id| {
            // FXList does not apply the placement offset when attaching.
            // OrientToObject retains its complete matrix as the local transform.
            (
                id,
                if *oriented {
                    matrix.clone()
                } else {
                    Pose::at([0.0; 3])
                },
            )
        });
        let ground = *ground;
        let placement_radius = *placement_radius;
        let height = *height;
        let delay = *delay;
        let radius = (*caller_radius
            && override_radius != 0.0
            && matches!(
                self.templates[*template].volume,
                super::Volume::Sphere { .. } | super::Volume::Cylinder { .. }
            ))
        .then_some(override_radius);
        self.create(*template, Pose::at([0.0; 3]), None, random);
        // The system constructor precedes radius, angle, height and delay draws.
        // FXList draws its radial angle even when the radius is zero.
        let radius_value = placement_radius.map_or(0.0, |[low, high]| random(low, high));
        let angle = random(0.0, std::f32::consts::TAU);
        let radial = [radius_value * angle.cos(), radius_value * angle.sin()];
        position[0] += radial[0];
        position[1] += radial[1];
        if ground {
            // Native destination layer replaces all prior Z, including the offset.
            position[2] = ground_height([position[0], position[1]]);
        } else if let Some([low, high]) = height {
            position[2] += random(low, high);
        }
        // Source samples the override after construction and placement. Negative
        // values retain the template delay; the slave is not reconfigured.
        let delay = delay.map(|[low, high]| random(low, high));
        let emitter = self.emitters.last_mut().expect("created FX emitter");
        emitter.transform = Pose { position, axes };
        if let Some((id, local)) = attachment {
            emitter.transform = super::model_states::compose(pose, &local);
            emitter.wind_origin = Some(std::array::from_fn(|axis| {
                pose.position[axis] + local.position[axis]
            }));
            emitter.object_attachment = Some((id, local));
        }
        if let Some(milliseconds) = delay.filter(|value| *value >= 0.0) {
            emitter.delay = (milliseconds * 0.03).ceil();
        }
        emitter.radius = radius;
    }

    pub(super) fn fire(
        &mut self,
        index: usize,
        pose: &Pose,
        bones: &[(usize, Vec<Pose>)],
        ground_height: &impl Fn([f32; 2]) -> f32,
        override_radius: f32,
        random: &mut impl FnMut(rng::Request) -> rng::Value,
    ) {
        self.fire_from(
            index,
            (pose, None),
            bones,
            ground_height,
            override_radius,
            random,
        );
    }

    pub(super) fn fire_from(
        &mut self,
        index: usize,
        source: (&Pose, Option<(usize, f32)>),
        bones: &[(usize, Vec<Pose>)],
        ground_height: &impl Fn([f32; 2]) -> f32,
        override_radius: f32,
        random: &mut impl FnMut(rng::Request) -> rng::Value,
    ) {
        let mut sounds = Vec::new();
        self.fire_ordered(
            index,
            &Callback {
                pose: source.0,
                object: source.1,
                secondary: None,
            },
            bones,
            ground_height,
            override_radius,
            &mut Output {
                random,
                sound: &mut |name, position, _| sounds.push((name.to_owned(), position)),
                clear: &|_| true,
            },
        );
        self.sounds.extend(sounds);
    }

    pub(super) fn fire_ordered(
        &mut self,
        index: usize,
        source: &Callback<'_>,
        bones: &[(usize, Vec<Pose>)],
        ground_height: &impl Fn([f32; 2]) -> f32,
        override_radius: f32,
        output: &mut Output<'_>,
    ) {
        let pose = source.pose;
        let object = source.object;
        for nugget in 0..self.lists[index].len() {
            match &self.lists[index][nugget] {
                Nugget::Particle { count, .. } => {
                    let count = *count;
                    for _ in 0..count {
                        self.fire_particle(
                            (index, nugget),
                            source,
                            ground_height,
                            override_radius,
                            &mut |low, high| rng::real(&mut output.random, low, high),
                        );
                    }
                }
                Nugget::Light {
                    color,
                    radius,
                    percent,
                    increase,
                    decrease,
                } => {
                    let radius = source
                        .object
                        .filter(|_| *percent > 0.0)
                        .map_or(*radius, |(_, geometry_radius)| geometry_radius * percent);
                    self.pulses
                        .create(pose.position, *color, radius, *increase, *decrease);
                }
                Nugget::Sound(name) => {
                    if !name.is_empty() {
                        (output.sound)(name, pose.position, object.is_some());
                    }
                }
                Nugget::ViewShake(kind) => {
                    self.shake.impulse(
                        *kind,
                        pose.position,
                        rng::real(&mut output.random, 0.0, std::f32::consts::TAU),
                    );
                }
                Nugget::Scorch { kind, radius } => {
                    let kind = if *kind < 0 {
                        let rng::Value::Scorch(value) = (output.random)(rng::Request::Scorch)
                        else {
                            unreachable!("integer client scorch request")
                        };
                        value
                    } else {
                        *kind
                    };
                    self.scorches.add(pose.position, *radius, kind);
                }
                Nugget::Tracer => {} // existing weapon_fx runs this final nugget.
                Nugget::AtBones { list, .. } => {
                    let child = *list;
                    if let Some((_, poses)) = bones.iter().find(|(entry, _)| *entry == nugget) {
                        for pose in poses {
                            if (output.clear)(pose.position) {
                                self.fire_ordered(
                                    child,
                                    &Callback {
                                        pose,
                                        object: None,
                                        secondary: None,
                                    },
                                    &[],
                                    ground_height,
                                    0.0,
                                    output,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

// ParticleSystemFXNugget::doFXObj uses an identity matrix rotated in XY.
// Its zero translation matters when reallyDoFX attaches the local transform.
fn ricochet_pose(primary: [f32; 3], secondary: [f32; 3]) -> Pose {
    let angle = (primary[1] - secondary[1]).atan2(primary[0] - secondary[0]);
    let (sine, cosine) = angle.sin_cos();
    let mut pose = Pose::at([0.0; 3]);
    for axis in 0..3 {
        let first = pose.axes[0][axis];
        let second = pose.axes[1][axis];
        let first_cosine = cosine * first;
        let second_sine = sine * second;
        let first_sine = -sine * first;
        let second_cosine = cosine * second;
        pose.axes[0][axis] = first_cosine + second_sine;
        pose.axes[1][axis] = first_sine + second_cosine;
    }
    pose
}

pub fn step(
    battle: Res<Match>,
    mut particles: ResMut<Particles>,
    mut animations: ResMut<Animations>,
    mut audio: ResMut<crate::source_audio::SourceAudio>,
) {
    let battle = battle.into_inner();
    if !battle.started {
        return;
    }
    let simulation = &battle.simulation;
    particles.begin(battle.generation, simulation.tick);
    if particles.processed == Some(simulation.tick) {
        return;
    }
    particles.processed = Some(simulation.tick);
    let scans = particles.detector_scans(simulation, &animations);
    let projectiles: std::collections::BTreeMap<_, _> = simulation
        .projectiles
        .iter()
        .map(|projectile| (projectile.id, projectile))
        .collect();
    let mut destroyed = Vec::new();
    for emitter in &mut particles.emitters {
        if let Some(id) = emitter.attachment {
            if let Some((pose, shrouded)) = projectiles
                .get(&id)
                .and_then(|projectile| attached_pose(simulation, projectile))
            {
                emitter.previous = Some(emitter.transform.position);
                emitter.transform = pose;
                emitter.shrouded = shrouded;
            } else {
                emitter.lifecycle.destroyed = true;
                emitter.wind_origin = Some([0.0; 3]);
                destroyed.extend(emitter.slave);
            }
        }
    }
    for emitter in &mut particles.emitters {
        if destroyed.contains(&emitter.id) {
            emitter.lifecycle.destroyed = true;
        }
    }
    particles.object_effects(
        simulation,
        &mut rng::source(&mut animations, battle.generation),
    );
    let mut random = random(&mut animations, battle.generation);
    particles.emit_detector_scans(scans, &mut random);
    particles.shake.advance();
    particles.flush_effect_sounds(&mut audio);
}

/// `GameClient::update` advances the manager after current drawable callbacks.
pub fn advance(
    battle: Res<Match>,
    mut particles: ResMut<Particles>,
    mut animations: ResMut<Animations>,
    mut audio: ResMut<crate::source_audio::SourceAudio>,
) {
    use super::model_states::WheelSound;
    let battle = battle.into_inner();
    let game = &battle.simulation;
    for sound in std::mem::take(&mut particles.model_states.wheel_sounds) {
        match sound {
            WheelSound::Draw {
                object,
                landing,
                sliding,
            } => audio.wheel_sounds(
                game,
                object,
                landing,
                sliding,
                battle.observer_player.unwrap_or(0),
            ),
            WheelSound::Retire(object) => audio.retire_wheel_sounds(object),
        }
    }
    if !battle.started || battle.paused || particles.processed != Some(battle.simulation.tick) {
        return;
    }
    particles.advance_frame(
        game.tick,
        &mut random(&mut animations, battle.generation),
        &|point| game.ground_height(point),
    );
}

pub(super) fn attached_pose(
    simulation: &generals_sim::skirmish::Skirmish,
    projectile: &generals_sim::skirmish::Projectile,
) -> Option<(Pose, bool)> {
    attached_pose_for_player(simulation, projectile, 0)
}

pub(super) fn attached_pose_for_player(
    simulation: &generals_sim::skirmish::Skirmish,
    projectile: &generals_sim::skirmish::Projectile,
    observer: usize,
) -> Option<(Pose, bool)> {
    use generals_sim::{collision_geometry::Body, skirmish::visibility::Status};
    let (position, axes) = projectile.effect_transform(&simulation.entities)?;
    let actor = &simulation.entities[projectile.object?];
    let geometry = &simulation.catalog[actor.type_index].geometry;
    let body = Body {
        position,
        angle: axes[0][1].atan2(axes[0][0]),
        geometry,
    };
    let shrouded = !matches!(
        simulation.visibility_footprint(observer, &body),
        Status::Clear | Status::Partial
    );
    Some((Pose { position, axes }, shrouded))
}

fn garrison_fx<'a>(files: &'a [ContentFile], projectile: &str) -> Result<&'a str, Box<dyn Error>> {
    if projectile.eq_ignore_ascii_case("NONE") {
        return Ok("NONE");
    }
    let object = definition(files, "Object", projectile)?;
    let module = blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .find(|lines| {
            lines[0].tokens.get(1).is_some_and(|value| {
                matches!(value.as_str(), "DumbProjectileBehavior" | "MissileAIUpdate")
            })
        });
    module
        .and_then(|lines| field(lines, 1, "GarrisonHitKillFX"))
        .map_or(Ok("NONE"), scalar)
}
