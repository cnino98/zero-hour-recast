use super::{Check, Control, Error, Match, Stage, SystemParam, commands, require};
use crate::{
    control_bar::{CommandAction, UiData},
    model_animation::{AnimatedPart, Animations, Players},
    particles::ParticleInspection,
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::UnitVisual,
    source_material::SourceMaterial,
    weapon_fx::TracerVisual,
};
use bevy::{mesh::VertexAttributeValues, prelude::*, ui_render::ui_material::MaterialNode};
use generals_sim::skirmish::Order;

#[derive(SystemParam)]
pub(super) struct View<'w, 's> {
    data: Res<'w, UiData>,
    icons: Res<'w, Assets<PromotionIcon>>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static ComputedNode,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    roots: Query<'w, 's, (&'static UnitVisual, &'static Players, &'static Children)>,
    parts: Query<
        'w,
        's,
        (
            &'static AnimatedPart,
            &'static ViewVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
    >,
    animations: Res<'w, Animations>,
    tracers: Query<
        'w,
        's,
        (
            Entity,
            &'static TracerVisual,
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static ViewVisibility,
        ),
    >,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    particles: ParticleInspection<'w, 's>,
}

impl View<'_, '_> {
    pub(super) fn progress(
        self,
        check: &mut Check,
        control: &mut Control,
    ) -> Result<(), Box<dyn Error>> {
        match check.stage {
            Stage::Cloak => self.cloak(check, control)?,
            Stage::FactorySelection => {
                control.battle.select_objects(&[check.setup.factory], true);
                focus(control, check.setup.factory);
                check.stage = Stage::ResearchSelection;
            }
            Stage::ResearchSelection => commands::research(check, &self, control)?,
            Stage::Research => {
                let game = &control.battle.simulation;
                if game.has_upgrade(0, "Upgrade_AmericaSentryDroneGun") {
                    require(
                        game.tick - check.research >= 900
                            && game.weapon_available(check.setup.drone, 0),
                        "Sentry research completed early or did not allocate the original gun",
                    )?;
                    control.time.pause();
                    control.battle.select_objects(&[check.setup.drone], true);
                    focus(control, check.setup.drone);
                    check.stage = Stage::DroneSelection;
                }
            }
            Stage::DroneSelection => {
                if self.body(&control.battle, check, true, true)? {
                    commands::guard(check, &self, control)?;
                }
            }
            Stage::Deploy => self.deployed(check, control)?,
            Stage::DeployedCaptured => commands::move_drone(check, control)?,
            Stage::Move => self.moving(check, control)?,
            Stage::Approach => {
                let game = &control.battle.simulation;
                let actor = &game.entities[check.setup.drone];
                let point = actor.position;
                let range = actor.active_weapon(&game.catalog[actor.type_index]).range;
                let target = game.entities[check.setup.target].position;
                let target = Vec2::from_array(target);
                if Vec2::from_array(point).distance(target) < range
                    && target.distance(Vec2::from_array(check.next)) < 15.0
                {
                    commands::attack(check, control)?;
                }
            }
            Stage::Fire => self.firing(check, control)?,
            Stage::Done => {}
        }
        Ok(())
    }

    fn cloak(&self, check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
        let game = &control.battle.simulation;
        require(
            game.entities[check.setup.drone].stealthed()
                && !game.weapon_available(check.setup.drone, 0),
            "paid Sentry must begin concealed and unarmed",
        )?;
        if !self.body(&control.battle, check, true, false)? {
            return Ok(());
        }
        control.time.pause();
        control.capture.intermediate("sentry-concealed.png");
        // Preserve this camera/selection until the intermediate image finishes.
        check.stage = Stage::FactorySelection;
        Ok(())
    }

    fn deployed(&self, check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
        if let [Some(start), Some(end)] = check.transitions[..2] {
            require(
                end - start == 30,
                "Sentry Guard deployment changed its original 30-frame delay",
            )?;
            if self.body(&control.battle, check, true, true)? && check.animated {
                control.time.pause();
                control.capture.intermediate("sentry-deployed.png");
                check.stage = Stage::DeployedCaptured;
            }
        } else {
            self.body(&control.battle, check, false, true)?;
        }
        Ok(())
    }

    fn moving(&self, check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
        let game = &control.battle.simulation;
        let actor = &game.entities[check.setup.drone];
        if actor.moving() && !actor.stealthed() && self.body(&control.battle, check, false, true)? {
            check.moved = true;
        }
        if matches!(actor.ai.order, Order::Idle)
            && !actor.moving()
            && Vec2::from_array(actor.position).distance(Vec2::from_array(check.next)) < 15.0
        {
            require(
                check.moved,
                "Sentry movement never showed its real revealed body",
            )?;
            let [Some(start), Some(end)] = check.transitions[2..] else {
                return Err("Sentry movement skipped its original packing states".into());
            };
            require(
                end - start == 30,
                "Sentry packing changed its original 30-frame delay",
            )?;
            check.next = check.setup.approach_drone(&mut control.battle.simulation)?;
            check.stage = Stage::Approach;
        }
        Ok(())
    }

    pub(super) fn command(
        &self,
        battle: &Match,
        slot: usize,
    ) -> Result<CommandAction, Box<dyn Error>> {
        let command =
            selected_command(battle, &self.data, slot).ok_or("original Sentry command missing")?;
        let expected = self.data.icon(&command.image);
        require(
            self.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(index) if *index == slot)
                    && node.size().min_element() > 0.0
                    && self.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
            }),
            "original Sentry command image was not painted",
        )?;
        Ok(command.action)
    }

    fn body(
        &self,
        battle: &Match,
        check: &mut Check,
        cloaked: bool,
        turret: bool,
    ) -> Result<bool, Box<dyn Error>> {
        let id = check.setup.drone;
        let Some((_, players, children)) = self
            .roots
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(false);
        };
        let mut body_visible = false;
        let mut turret_parts = 0;
        let mut turret_visible = false;
        for child in children {
            let Ok((part, visible, material)) = self.parts.get(*child) else {
                continue;
            };
            let name = self
                .animations
                .mesh_name(part.model, part.binding.mesh)
                .ok_or("Sentry part lost its original W3D name")?;
            if name.to_ascii_uppercase().ends_with("TURRETUP09") {
                turret_parts += 1;
                turret_visible |= visible.get();
            }
            body_visible |= visible.get()
                && self.materials.get(&material.0).is_some_and(|material| {
                    let opacity = material.parameters.opacity;
                    opacity.is_finite()
                        && opacity > 0.0
                        && if cloaked {
                            opacity < 1.0 && material.alpha_mode == AlphaMode::Blend
                        } else {
                            opacity.to_bits() == 1.0_f32.to_bits()
                        }
                });
        }
        if !body_visible {
            return Ok(false);
        }
        require(
            turret_parts > 0 && turret_visible == turret,
            "Sentry original TURRETUP09 visibility disagrees with researched weapon state",
        )?;
        for sample in self
            .animations
            .inspect(players, battle.simulation.entities[id].type_index)?
        {
            if sample.clip == "avsentry.avsentry" && sample.bounds.is_some() {
                check.animated |= check
                    .animation
                    .is_some_and(|previous| previous.to_bits() != sample.frame.to_bits());
                check.animation = Some(sample.frame);
            }
        }
        Ok(true)
    }

    fn firing(&self, check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
        let game = &control.battle.simulation;
        require(
            game.entities[check.setup.drone].alive() && game.entities[check.setup.target].alive(),
            "Sentry firing scene lost a paid actor before capture",
        )?;
        if !check.sounds.iter().all(|heard| *heard)
            || game.entities[check.setup.target].health >= check.target_health
        {
            return Ok(());
        }
        require(
            game.entities[check.setup.drone].deployment_condition() == Some("DEPLOYED"),
            "Sentry gun fired before original deployment finished",
        )?;
        if !self.body(&control.battle, check, false, true)? {
            return Ok(());
        }
        let kind = control.battle.simulation.entities[check.setup.drone].type_index;
        for (entity, tracer, mesh, material, visible) in &self.tracers {
            let Some(shot) = check
                .shots
                .iter()
                .find(|shot| shot.frame == tracer.born && tracer.source == check.setup.drone)
            else {
                continue;
            };
            if !visible.get() || tracer.opacity <= 0.0 {
                continue;
            }
            check_tracer(
                self.meshes
                    .get(&mesh.0)
                    .ok_or("Sentry tracer mesh missing")?,
                self.materials
                    .get(&material.0)
                    .ok_or("Sentry tracer material missing")?,
                tracer,
            )?;
            require(
                Vec3::from_array(tracer.origin).distance(Vec3::from_array(shot.position)) > 0.01,
                "Sentry tracer fell back to body origin instead of original TurretFX",
            )?;
            if !self.particles.weapon_fire(
                kind,
                shot.weapon,
                shot.veterancy,
                tracer.born,
                control.battle.generation,
            )? {
                continue;
            }
            // Native tracers advance per draw even while virtual time is paused.
            // Keep this already-verified mesh for the existing GPU capture readback.
            control.commands.entity(entity).remove::<TracerVisual>();
            control.time.pause();
            control.capture.finish_scenario();
            check.stage = Stage::Done;
            println!(
                "Original Sentry native: paid Gun button, concealed unarmed body, researched TURRETUP09, guard deploy/pack delays, revealed movement, real enemy damage, original tracer/muzzle/animation and advancing command/fire audio passed"
            );
            break;
        }
        Ok(())
    }
}

fn focus(control: &mut Control, id: usize) {
    control.camera.focus = Vec2::from_array(control.battle.simulation.entities[id].position) * 0.1;
}

fn check_tracer(
    mesh: &Mesh,
    material: &SourceMaterial,
    tracer: &TracerVisual,
) -> Result<(), Box<dyn Error>> {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return Err("Sentry tracer has no original prism positions".into());
    };
    require(
        points.len() == 8
            && mesh.indices().is_some_and(|indices| indices.len() == 36)
            && points.iter().flatten().all(|value| value.is_finite())
            && tracer.length.to_bits() == 10.0_f32.to_bits()
            && tracer.width.to_bits() == 0.3_f32.to_bits(),
        "Sentry tracer changed the source Line3D geometry",
    )?;
    require(
        material.texture.is_none()
            && !material.depth_write
            && !material.double_sided
            && material.alpha_mode == AlphaMode::Blend
            && material
                .parameters
                .emissive
                .truncate()
                .to_array()
                .map(f32::to_bits)
                == tracer.color.map(f32::to_bits)
            && material.parameters.diffuse.w.to_bits() == tracer.opacity.to_bits(),
        "Sentry tracer changed its original prelit material",
    )
}
