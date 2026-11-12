//! Read-only rendered acceptance checks for real ordered missile exhaust events.
use super::{Emitter, Particles, Style, Template, effects::Nugget, render::ParticleVisual};
use crate::source_material::SourceMaterial;
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

mod additive;
mod drain;
mod smudge;
pub use additive::Quad as AdditiveQuad;
pub use smudge::Fan as SmudgeFan;

type Visuals<'w, 's> = Query<
    'w,
    's,
    (
        &'static ParticleVisual,
        &'static Mesh3d,
        &'static MeshMaterial3d<SourceMaterial>,
        &'static ViewVisibility,
        &'static GlobalTransform,
    ),
>;

#[derive(SystemParam)]
pub struct ParticleInspection<'w, 's> {
    particles: Res<'w, Particles>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    images: Res<'w, Assets<Image>>,
    visuals: Visuals<'w, 's>,
    smudges: Query<
        'w,
        's,
        (
            &'static super::smudge::SmudgeVisual,
            &'static Mesh3d,
            &'static ViewVisibility,
        ),
    >,
}

impl ParticleInspection<'_, '_> {
    pub fn collapse_dust_finished(
        &self,
        generation: u32,
        final_frame: u64,
        now: u64,
    ) -> Result<bool, Box<dyn Error>> {
        drain::collapse_complete(&self.particles, generation, final_frame, now)
    }

    pub fn actor_flash_finished(
        &self,
        name: &str,
        generation: u32,
    ) -> Result<bool, Box<dyn Error>> {
        drain::complete(&self.particles, name, generation)
    }

    pub fn drawable_state(
        &self,
        actor: usize,
        name: &str,
        generation: u32,
    ) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            if emitter
                .drawable_attachment
                .is_some_and(|(object, _)| object == actor)
                && self.particles.templates[emitter.template].name == name
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn object_attached(
        &self,
        actor: usize,
        name: &str,
        generation: u32,
    ) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            if emitter
                .object_attachment
                .as_ref()
                .is_some_and(|(object, _)| *object == actor)
                && self.particles.templates[emitter.template].name == name
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn incendiary(&self, name: &str, generation: u32) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            if self.particles.templates[emitter.template].name == name
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn actor_fx(
        &self,
        name: &str,
        frame: u64,
        generation: u32,
    ) -> Result<bool, Box<dyn Error>> {
        let list = *self
            .particles
            .actor_fx
            .get(name)
            .ok_or("missing original actor FX binding")?;
        let mut checked = 0;
        for nugget in &self.particles.lists[list] {
            let Nugget::Particle { template, .. } = nugget else {
                continue;
            };
            let visible = self
                .particles
                .emitters
                .iter()
                .filter(|emitter| {
                    emitter.template == *template
                        && emitter
                            .particles
                            .iter()
                            .any(|particle| particle.born == frame)
                })
                .try_fold(false, |visible, emitter| {
                    Ok::<_, Box<dyn Error>>(visible || self.visible_mesh(emitter, generation)?)
                })?;
            if !visible {
                return Ok(false);
            }
            checked += 1;
        }
        if checked == 0 {
            return Err("actor FX has no original particle nuggets".into());
        }
        Ok(true)
    }

    pub(crate) fn actor_fx_diagnostic(
        &self,
        name: &str,
        frame: u64,
        generation: u32,
    ) -> Result<String, Box<dyn Error>> {
        let list = *self
            .particles
            .actor_fx
            .get(name)
            .ok_or("missing original actor FX binding")?;
        let mut nuggets = Vec::new();
        for nugget in &self.particles.lists[list] {
            let Nugget::Particle { template, .. } = nugget else {
                continue;
            };
            let rules = &self.particles.templates[*template];
            let mut emitters = Vec::new();
            for emitter in self
                .particles
                .emitters
                .iter()
                .filter(|emitter| emitter.template == *template)
                .rev()
                .take(8)
            {
                let births = emitter.particles.iter().map(|particle| particle.born);
                let earliest = births.clone().min();
                let latest = births.max();
                let exact = emitter
                    .particles
                    .iter()
                    .any(|particle| particle.born == frame);
                emitters.push((
                    emitter.id,
                    emitter.delay,
                    emitter.particles.len(),
                    earliest,
                    latest,
                    exact,
                    self.visible_mesh(emitter, generation)?,
                ));
            }
            nuggets.push(format!(
                "template={} source_initial_delay=({},{}), emitter(id,current_delay,count,first_birth,last_birth,exact_event_birth,visible)={emitters:?}",
                rules.name, rules.initial_delay.low, rules.initial_delay.high
            ));
        }
        Ok(format!(
            "effect={name} event_frame={frame} requested_generation={generation} particle_generation={} particle_frame={:?} processed={:?}; emitter_creation_frame=not_retained; newest_candidates_per_template_limit=8; {nuggets:?}",
            self.particles.generation, self.particles.frame, self.particles.processed
        ))
    }

    pub fn cleanup_systems(
        &self,
        game: &generals_sim::skirmish::Skirmish,
        projectiles: &std::collections::BTreeSet<u64>,
    ) -> Vec<(u64, u64)> {
        self.particles
            .emitters
            .iter()
            .filter(|emitter| self.particles.templates[emitter.template].name == "CleanupTrail01")
            .filter_map(|emitter| {
                let (object, _) = emitter.object_attachment.as_ref()?;
                let projectile = game.entities[*object].projectile_id()?;
                projectiles
                    .contains(&projectile)
                    .then_some((emitter.id, projectile))
            })
            .collect()
    }

    pub fn cleanup_trail(
        &self,
        systems: &std::collections::BTreeSet<u64>,
        generation: u32,
    ) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            if systems.contains(&emitter.id)
                && self.particles.templates[emitter.template].name == "CleanupTrail01"
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn cleanup_details(
        &self,
        systems: &std::collections::BTreeSet<u64>,
        generation: u32,
    ) -> Option<String> {
        let emitters: Vec<_> = self
            .particles
            .emitters
            .iter()
            .filter(|emitter| {
                systems.contains(&emitter.id)
                    && self.particles.templates[emitter.template].name == "CleanupTrail01"
            })
            .map(|emitter| {
                let visuals: Vec<_> = self
                    .visuals
                    .iter()
                    .filter(|(visual, ..)| {
                        visual.system == emitter.id && visual.generation == generation
                    })
                    .map(|(_, mesh, _, visible, _)| {
                        (
                            visible.get(),
                            self.meshes.get(&mesh.0).map(Mesh::count_vertices),
                        )
                    })
                    .collect();
                (
                    emitter.id,
                    emitter
                        .object_attachment
                        .as_ref()
                        .map(|(object, _)| *object),
                    emitter.particles.len(),
                    emitter.lifecycle.destroyed,
                    emitter.shrouded,
                    emitter.particles.first().map(|particle| {
                        (particle.born, particle.color, particle.alpha, particle.size)
                    }),
                    visuals,
                )
            })
            .collect();
        (!emitters.is_empty()).then(|| format!("tracked={systems:?}; emitter(id,object,particles,destroyed,shrouded,first_particle,visible/vertices)={emitters:?}"))
    }

    pub fn poison_field(&self, generation: u32) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            if self.particles.templates[emitter.template].name == "AnthraxFieldMedium"
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn contamination(&self, generation: u32, continuous: bool) -> Result<bool, Box<dyn Error>> {
        let name = if continuous {
            "ToxinPuddleContinuous"
        } else {
            "PoisonFieldMedium"
        };
        for emitter in &self.particles.emitters {
            if self.particles.templates[emitter.template].name == name
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn drone_welding(self, generation: u32) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            if self.particles.templates[emitter.template].name == "BlueSparks"
                && emitter.lifetime.is_some()
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn rifle(
        &self,
        type_index: usize,
        veterancy: u8,
        frame: u64,
        generation: u32,
    ) -> Result<bool, Box<dyn Error>> {
        self.weapon_fire(type_index, 0, veterancy, frame, generation)
    }

    pub fn weapon_fire(
        &self,
        type_index: usize,
        weapon_index: usize,
        veterancy: u8,
        frame: u64,
        generation: u32,
    ) -> Result<bool, Box<dyn Error>> {
        let list = self.particles.bindings[type_index][weapon_index]
            .as_ref()
            .and_then(|binding| binding.fire[usize::from(veterancy == 3)])
            .ok_or("weapon has no authored firing FX list")?;
        let mut count = 0;
        for nugget in &self.particles.lists[list] {
            let Nugget::Particle { template, .. } = nugget else {
                continue;
            };
            let Some(emitter) = self.particles.emitters.iter().find(|emitter| {
                emitter.template == *template
                    && emitter
                        .particles
                        .iter()
                        .any(|particle| particle.born == frame)
            }) else {
                return Ok(false);
            };
            if !self.visible_mesh(emitter, generation)? {
                return Ok(false);
            }
            count += 1;
        }
        if count == 0 {
            return Err("weapon firing list contained no authored particles".into());
        }
        eprintln!(
            "Original weapon particles rendered: {count} authored firing effects at frame {frame}"
        );
        Ok(true)
    }

    pub fn weapon_fire_above_hud(
        &self,
        source: (usize, usize, u8),
        frame: u64,
        generation: u32,
        view: ((&Camera, &GlobalTransform), Rect),
    ) -> Result<bool, Box<dyn Error>> {
        let (type_index, weapon_index, veterancy) = source;
        if !self.weapon_fire(type_index, weapon_index, veterancy, frame, generation)? {
            return Ok(false);
        }
        let list = self.particles.bindings[type_index][weapon_index]
            .as_ref()
            .and_then(|binding| binding.fire[usize::from(veterancy == 3)])
            .ok_or("weapon has no authored firing FX list")?;
        let ((camera, camera_transform), viewport) = view;
        for nugget in &self.particles.lists[list] {
            let Nugget::Particle { template, .. } = nugget else {
                continue;
            };
            let emitter = self
                .particles
                .emitters
                .iter()
                .find(|emitter| {
                    emitter.template == *template
                        && emitter
                            .particles
                            .iter()
                            .any(|particle| particle.born == frame)
                })
                .ok_or("checked weapon emitter disappeared")?;
            if matches!(
                self.particles.templates[emitter.template].drawing.style,
                Style::Smudge
            ) {
                let Some((mesh, range)) = self.smudge_mesh(emitter.id, generation) else {
                    return Ok(false);
                };
                let Some(VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    return Err("smudge positions absent".into());
                };
                if points[range].iter().any(|point| {
                    !camera
                        .world_to_viewport(camera_transform, Vec3::from_array(*point))
                        .is_ok_and(|position| viewport.contains(position))
                }) {
                    return Ok(false);
                }
                continue;
            }
            let (_, handle, _, _, transform) = self
                .visuals
                .iter()
                .find(|(visual, ..)| visual.system == emitter.id && visual.generation == generation)
                .ok_or("checked weapon particle drawable disappeared")?;
            let mesh = self
                .meshes
                .get(&handle.0)
                .ok_or("checked weapon particle mesh disappeared")?;
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return Err("weapon particle mesh has no original positions".into());
            };
            if points.is_empty()
                || points.iter().any(|point| {
                    !camera
                        .world_to_viewport(
                            camera_transform,
                            transform.transform_point(Vec3::from_array(*point)),
                        )
                        .is_ok_and(|position| position.is_finite() && viewport.contains(position))
                })
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn jammed_missile(&self, projectile: u64, generation: u32) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            if matches!(emitter.fire_attachment, Some(super::fire::Attachment::Projectile(id)) if id == projectile)
                && self.particles.templates[emitter.template].name == "SparksMedium"
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn missile(&self, projectile: u64, generation: u32) -> Result<bool, Box<dyn Error>> {
        let Some(master) = self
            .particles
            .emitters
            .iter()
            .find(|emitter| emitter.attachment == Some(projectile))
        else {
            // The headless setup's first shot can precede the client FX stream.
            // Wait for another ordinary shot; never synthesize its birth event.
            return Ok(false);
        };
        let template = &self.particles.templates[master.template];
        if !matches!(template.drawing.style, Style::Streak) {
            return Err("launcher exhaust did not use its authored STREAK template".into());
        }
        let Some(slave) = self
            .particles
            .emitters
            .iter()
            .find(|emitter| Some(emitter.id) == master.slave)
        else {
            return Ok(false);
        };
        if Some(slave.template) != template.slave || !slave.master {
            return Err("missile flare does not match the authored exhaust slave".into());
        }
        if master.particles.len() < 2
            || !self.visible_mesh(master, generation)?
            || !self.visible_mesh(slave, generation)?
        {
            return Ok(false);
        }
        eprintln!(
            "Original missile particles rendered: projectile {projectile}, {} STREAK ({} particles) and {} slave flare",
            template.name,
            master.particles.len(),
            self.particles.templates[slave.template].name
        );
        Ok(true)
    }

    pub fn disabled(&self, target: usize, generation: u32) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            if emitter
                .object_attachment
                .as_ref()
                .is_none_or(|(id, _)| *id != target)
            {
                continue;
            }
            let template = &self.particles.templates[emitter.template];
            if template.name != "DisabledEffectBinaryShower0" {
                return Err("disable FX used an unexpected original template".into());
            }
            let Some(slave) = self
                .particles
                .emitters
                .iter()
                .find(|other| Some(other.id) == emitter.slave)
            else {
                continue;
            };
            if Some(slave.template) != template.slave || !slave.master {
                return Err("disable shower lost its original binary slave".into());
            }
            // Capture after the authored green ramp reaches a clearly visible
            // value, rather than accepting the nearly black birth frame.
            if emitter
                .particles
                .iter()
                .any(|particle| particle.color[1] >= 0.5)
                && self.visible_mesh(emitter, generation)?
                && self.visible_mesh(slave, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn propaganda(&self, generation: u32) -> Result<bool, Box<dyn Error>> {
        for emitter in &self.particles.emitters {
            let template = &self.particles.templates[emitter.template];
            if !matches!(template.name.as_str(), "SonicRange" | "SonicRangeUpgraded") {
                continue;
            }
            if !template.drawing.ground_aligned {
                return Err("speaker pulse lost its ground alignment".into());
            }
            // At birth the original ring has zero size and is hidden beneath
            // the tank. Wait until its authored growth clears the chassis.
            if emitter
                .particles
                .iter()
                .any(|particle| particle.size >= 80.0)
                && self.visible_mesh(emitter, generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn smudge_mesh(&self, system: u64, generation: u32) -> Option<(&Mesh, std::ops::Range<usize>)> {
        self.smudges.iter().find_map(|(visual, mesh, visible)| {
            if visual.generation != generation || !visible.get() {
                return None;
            }
            let range = visual
                .ranges
                .iter()
                .find(|(id, _)| *id == system)?
                .1
                .clone();
            Some((self.meshes.get(&mesh.0)?, range))
        })
    }

    fn visible_mesh(&self, emitter: &Emitter, generation: u32) -> Result<bool, Box<dyn Error>> {
        if matches!(
            self.particles.templates[emitter.template].drawing.style,
            Style::Smudge
        ) {
            let Some((mesh, range)) = self.smudge_mesh(emitter.id, generation) else {
                return Ok(false);
            };
            let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                return Err("smudge colors absent".into());
            };
            return Ok(colors[range].iter().any(|color| color[3] > 0.0));
        }
        let Some((_, mesh, material, visible, _)) = self
            .visuals
            .iter()
            .find(|(visual, ..)| visual.system == emitter.id && visual.generation == generation)
        else {
            return Ok(false);
        };
        if !visible.get() {
            return Ok(false);
        }
        let (Some(mesh), Some(material)) =
            (self.meshes.get(&mesh.0), self.materials.get(&material.0))
        else {
            return Ok(false);
        };
        let template = &self.particles.templates[emitter.template];
        check_mesh(mesh)?;
        check_material(material, template)?;
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            return Err("checked particle mesh lost its source colors".into());
        };
        if !matches!(template.drawing.shader, super::Shader::AlphaTest)
            && !colors.iter().any(|color| {
                color[..3].iter().any(|&component| component > 0.0)
                    && (!matches!(template.drawing.shader, super::Shader::Alpha) || color[3] > 0.0)
            })
        {
            return Ok(false);
        }
        let texture = self.particles.textures[template.texture]
            .handle
            .as_ref()
            .ok_or("rendered particle has no loaded original texture")?;
        if material.texture.as_ref() != Some(texture) || self.images.get(texture).is_none() {
            return Err("particle material does not use its loaded original texture".into());
        }
        Ok(true)
    }
}

const fn check_material(
    material: &SourceMaterial,
    template: &Template,
) -> Result<(), &'static str> {
    let blend = match template.drawing.shader {
        super::Shader::Additive => matches!(material.alpha_mode, AlphaMode::Add),
        super::Shader::Alpha => matches!(material.alpha_mode, AlphaMode::Blend),
        super::Shader::AlphaTest => matches!(material.alpha_mode, AlphaMode::Mask(_)),
    };
    let tested = matches!(template.drawing.shader, super::Shader::AlphaTest);
    if material.parameters.vertex_diffuse != 3
        || material.double_sided == tested
        || material.depth_write != tested
        || material.parameters.primary_gradient != if tested { 0 } else { 1 }
        || material.parameters.alpha_test.to_bits()
            != if tested {
                (96.0_f32 / 255.0).to_bits()
            } else {
                (-1.0_f32).to_bits()
            }
        || material.depth_compare.is_some()
        || !blend
    {
        return Err("particle fixed-function material differs from its authored draw mode");
    }
    Ok(())
}

fn check_mesh(mesh: &Mesh) -> Result<(), &'static str> {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return Err("particle mesh has no positions");
    };
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        return Err("particle mesh has no original texture coordinates");
    };
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        return Err("particle mesh has no source vertex colors");
    };
    if points.len() < 4
        || uv.len() != points.len()
        || colors.len() != points.len()
        || !points.iter().flatten().all(|value| value.is_finite())
        || !uv.iter().flatten().all(|value| value.is_finite())
        || !colors.iter().flatten().all(|value| value.is_finite())
    {
        return Err("particle mesh has invalid geometry, texture coordinates or colors");
    }
    if mesh.indices().is_none_or(|indices| {
        indices.len() < 6
            || !indices.len().is_multiple_of(3)
            || indices.iter().any(|index| index >= points.len())
    }) {
        return Err("particle mesh has invalid triangle indices");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_mesh;
    use bevy::{
        asset::RenderAssetUsages, mesh::Indices, prelude::*,
        render::render_resource::PrimitiveTopology,
    };

    fn quad() -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD,
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]; 4])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
    }

    #[test]
    fn rendered_acceptance_rejects_nonfinite_vertices_and_invalid_triangles() {
        assert!(check_mesh(&quad()).is_ok());
        let mut mesh = quad();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[f32::NAN, 0.0, 0.0]; 4]);
        assert!(check_mesh(&mesh).is_err());
        let mut mesh = quad();
        mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 4]));
        assert!(check_mesh(&mesh).is_err());
    }
}

#[cfg(test)]
mod cleanup_tests;

#[cfg(test)]
mod collapse_delay_tests {
    use crate::particles::{Pose, effects::Nugget, load_with_position_effects};
    use generals_assets::virtual_assets::VirtualAssets;
    use std::collections::BTreeSet;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_collapse_flash_emits_after_event_frame_while_initial_effect_has_no_delay() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let mut particles = load_with_position_effects(
            &assets,
            &[],
            &BTreeSet::from([
                "FX_StructureCollapseInitial".into(),
                "FX_StructureCollapseBurst".into(),
            ]),
        )
        .unwrap();
        let initial = particles.actor_fx["FX_StructureCollapseInitial"];
        let templates: Vec<_> = particles.lists[initial]
            .iter()
            .filter_map(|nugget| {
                if let Nugget::Particle { template, .. } = nugget {
                    Some(*template)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(templates.len(), 2);
        for &template in &templates {
            let delay = &particles.templates[template].initial_delay;
            assert_eq!(delay.low.to_bits(), 0.0_f32.to_bits());
            assert_eq!(delay.high.to_bits(), 0.0_f32.to_bits());
        }
        let flash = particles
            .templates
            .iter()
            .position(|template| template.name == "StructureCollapseBurstFlash")
            .unwrap();
        let burst = particles.actor_fx["FX_StructureCollapseBurst"];
        assert!(particles.lists[burst].iter().any(
            |nugget| matches!(nugget, Nugget::Particle { template, .. } if *template == flash)
        ));
        let delay = &particles.templates[flash].initial_delay;
        assert_eq!(delay.low.to_bits(), 0.0_f32.to_bits());
        assert_eq!(delay.high.to_bits(), 10.0_f32.to_bits());
        let event_frame = 100;
        particles.begin(1, event_frame);
        let mut random = |low: f32, high: f32| (high - low).mul_add(0.25, low);
        for &template in &templates {
            particles.create(template, Pose::at([0.0; 3]), None, &mut random);
        }
        let flash_system = particles.create(flash, Pose::at([0.0; 3]), None, &mut random);
        particles.logic(event_frame, &mut random, &|_| 0.0);
        for template in templates {
            assert!(particles.emitters.iter().any(|emitter| {
                emitter.template == template
                    && emitter
                        .particles
                        .iter()
                        .any(|particle| particle.born == event_frame)
            }));
        }
        let flash = particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == flash_system)
            .unwrap();
        assert!(
            flash.particles.is_empty(),
            "source InitialDelay must suppress event-frame emission"
        );
        for frame in event_frame + 1..=event_frame + 3 {
            particles.logic(frame, &mut random, &|_| 0.0);
        }
        let flash = particles
            .emitters
            .iter()
            .find(|emitter| emitter.id == flash_system)
            .unwrap();
        assert!(!flash.particles.is_empty());
        assert!(
            flash
                .particles
                .iter()
                .all(|particle| particle.born > event_frame)
        );
    }
}
