use super::{Check, Control, Kind, Phase};
use crate::{
    capture_framing::AircraftFrame,
    input::{self, CommandMode, picking},
    particles::ParticleInspection,
    playtest::Match,
    playtest_ui::{dispatch_command, selected_command},
    playtest_view::Placement,
    radius_cursor::RadiusVisual,
    skirmish_verification::require,
    source_material::SourceMaterial,
};
use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use generals_sim::skirmish::{Skirmish, visibility::Status};
use std::error::Error;

#[derive(SystemParam)]
pub(super) struct View<'w, 's> {
    commands: Commands<'w, 's>,
    frame: AircraftFrame<'w, 's>,
    picking: picking::View<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    radius: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static ViewVisibility,
        ),
        With<RadiusVisual>,
    >,
    placement: Query<
        'w,
        's,
        (
            &'static Placement,
            &'static Children,
            &'static InheritedVisibility,
        ),
    >,
    preview_parts: Query<'w, 's, (&'static Mesh3d, &'static MeshMaterial3d<SourceMaterial>)>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
}

impl View<'_, '_> {
    pub(super) fn progress(
        &mut self,
        check: &mut Check,
        control: &mut Control,
    ) -> Result<bool, Box<dyn Error>> {
        let battle = &mut control.battle;
        let target = Vec3::from_array(check.scene.target);
        let point = target.truncate();
        let world = Vec3::new(target.x, target.z, -target.y) * 0.1;
        let cursor = self
            .picking
            .camera
            .0
            .world_to_viewport(self.picking.camera.1, world)?;
        match check.phase {
            Phase::Button => {
                let command = (1..=14)
                    .filter_map(|slot| selected_command(battle, &self.picking.ui, slot))
                    .find(|command| command.action.power() == Some(check.kind.power()))
                    .ok_or("original GLA power button is missing")?;
                dispatch_command(battle, &mut control.mouse, command.action);
                require(
                    battle.placing.is_some()
                        || matches!(control.mouse.pending_command, CommandMode::Power { .. }),
                    "original GLA button failed to enter targeting mode",
                )?;
                if matches!(check.kind, Kind::Radar) {
                    require(
                        !matches!(
                            battle.simulation.visibility_cell(0, point.to_array()),
                            Status::Clear
                        ),
                        "Radar target was visible before the scan",
                    )?;
                    self.move_cursor(cursor);
                }
                check.phase = Phase::Target;
            }
            Phase::Target if matches!(check.kind, Kind::Sneak) => {
                let mut pressed = ButtonInput::default();
                pressed.press(MouseButton::Left);
                input::placement::pointer(
                    battle,
                    &mut control.mouse,
                    &self.picking,
                    &pressed,
                    cursor,
                    false,
                );
                control.time.unpause();
                check.phase = Phase::Release;
            }
            Phase::Release => {
                if !self.placement.iter().any(|(placement, children, visible)| {
                    placement.legal
                        && visible.get()
                        && children.iter().any(|child| {
                            self.preview_parts
                                .get(child)
                                .is_ok_and(|(mesh, material)| self.textured(mesh, material))
                        })
                }) {
                    return Ok(false);
                }
                let mut released = ButtonInput::default();
                released.press(MouseButton::Left);
                released.clear_just_pressed(MouseButton::Left);
                released.release(MouseButton::Left);
                input::placement::pointer(
                    battle,
                    &mut control.mouse,
                    &self.picking,
                    &released,
                    cursor,
                    false,
                );
                require(
                    battle.placing.is_none(),
                    "original anchored Sneak Attack placement was rejected",
                )?;
                check.phase = Phase::Effect;
            }
            Phase::Target => {
                if matches!(check.kind, Kind::Radar)
                    && !self.radius.iter().any(|(mesh, material, visible)| {
                        visible.get() && self.textured(mesh, material)
                    })
                {
                    return Ok(false);
                }
                let object = matches!(check.kind, Kind::Booby).then_some(check.scene.subject);
                input::targeted_order(battle, &mut control.mouse, point, object);
                require(
                    matches!(control.mouse.pending_command, CommandMode::Normal),
                    "original GLA target was rejected",
                )?;
                check.phase = Phase::Effect;
                control.time.unpause();
            }
            Phase::Effect => return self.effect(check, battle),
            Phase::Done => {}
        }
        Ok(false)
    }

    fn move_cursor(&mut self, cursor: Vec2) {
        self.commands.queue(move |world: &mut World| {
            let mut window = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
            if let Ok(mut window) = window.single_mut(world) {
                window.set_cursor_position(Some(cursor));
            }
        });
    }

    fn textured(&self, mesh: &Mesh3d, material: &MeshMaterial3d<SourceMaterial>) -> bool {
        self.meshes
            .get(&mesh.0)
            .is_some_and(|mesh| mesh.count_vertices() >= 3)
            && self
                .materials
                .get(&material.0)
                .is_some_and(|material| material.texture.is_some())
    }

    fn effect(&self, check: &mut Check, battle: &mut Match) -> Result<bool, Box<dyn Error>> {
        let game = &battle.simulation;
        let objects = |name| live_objects(game, check.first, name);
        match check.kind {
            Kind::Ambush => {
                let rebels = objects("GLAInfantryRebel");
                if rebels.is_empty() {
                    // The admitted click runs in the next ordinary logic update.
                    return Ok(false);
                }
                require(
                    rebels.len() == 16,
                    "Ambush did not create all sixteen paid rank-three Rebels",
                )?;
                let appearing = rebels.iter().all(|&id| {
                    let alpha = game.entities[id].ocl_fade_opacity(game.tick);
                    alpha > 0.15 && alpha < 1.0
                });
                Ok(appearing
                    && self.frame.visible(battle, rebels[0])?
                    && self.frame.visible(battle, rebels[4])?)
            }
            Kind::Gps => Ok(game.entities[check.scene.subject].stealthed()
                && (self
                    .particles
                    .incendiary("GPSRotisserie", battle.generation)?
                    || self.particles.incendiary(
                        "ParticleUplinkCannon_LaserBaseReadyToFire",
                        battle.generation,
                    )?)
                && self.frame.visible(battle, check.scene.subject)?),
            Kind::Sneak => self.sneak(check, battle),
            Kind::Anthrax => {
                let has_cloud = game.hazards.iter().any(|field| {
                    field.owner == 0
                        && game.catalog[game.entities[field.source].type_index].name
                            == "AnthraxBomb"
                });
                Ok(has_cloud
                    && self
                        .particles
                        .incendiary("AnthraxFieldLarge", battle.generation)?)
            }
            Kind::Radar => {
                let target = [check.scene.target[0], check.scene.target[1]];
                Ok(objects("RadarVanPing").len() == 1
                    && matches!(game.visibility_cell(0, target), Status::Clear)
                    && self.frame.visible(battle, check.scene.subject)?)
            }
            Kind::Booby => self.booby(check, battle),
        }
    }

    fn sneak(&self, check: &mut Check, battle: &mut Match) -> Result<bool, Box<dyn Error>> {
        let game = &mut battle.simulation;
        let Some(exit) = live_objects(game, check.first, "GLASneakAttackTunnelNetwork")
            .first()
            .copied()
        else {
            return Ok(false);
        };
        if !check.moved {
            require(
                game.passengers(exit).contains(&check.scene.subject),
                "built Sneak Attack tunnel lacks the paid shared passenger",
            )?;
            game.exit_container_at(check.scene.subject, exit)?;
            check.moved = true;
        }
        Ok(battle.simulation.entities[check.scene.subject]
            .contained_by()
            .is_none()
            && self.frame.visible(battle, exit)?
            && self.frame.visible(battle, check.scene.subject)?)
    }

    fn booby(&self, check: &mut Check, battle: &mut Match) -> Result<bool, Box<dyn Error>> {
        let game = &mut battle.simulation;
        let target = check.scene.subject;
        let attacker = check
            .scene
            .extra
            .ok_or("missing ordinary enemy capture infantry")?;
        if !check.moved && game.entities[target].booby_trapped() {
            if game.can_capture(attacker, target) {
                game.capture(attacker, target)?;
                check.moved = true;
            } else if !game.entities[attacker].moving()
                && !matches!(
                    game.entities[attacker].ai.order,
                    generals_sim::skirmish::Order::Move(_)
                )
            {
                // A pending or blocked Move still owns its native path recovery.
                game.move_to(attacker, game.entities[target].position)?;
            }
        }
        // The source bomb belongs to this building. Its retired object and
        // actual explosion survive the callback frame in the final evidence.
        Ok(
            booby_trap(check, game).is_some_and(|id| !game.entities[id].exists())
                && !game.entities[target].booby_trapped()
                && self
                    .particles
                    .incendiary("boobyTrapExplosion", battle.generation)?,
        )
    }
}

fn booby_trap(check: &Check, game: &Skirmish) -> Option<usize> {
    game.entities
        .iter()
        .enumerate()
        .skip(check.first)
        .find(|(_, actor)| {
            actor.owner == 0
                && actor.producer() == Some(check.scene.subject)
                && game.catalog[actor.type_index].name == "BoobyTrap"
        })
        .map(|(id, _)| id)
}

pub(super) fn timeout_details(check: &Check, game: &Skirmish) -> String {
    let Some(attacker) = check
        .scene
        .extra
        .filter(|_| matches!(check.kind, Kind::Booby))
    else {
        return "native GLA capture timed out".into();
    };
    let target = check.scene.subject;
    let trap = booby_trap(check, game).map(|id| (id, game.entities[id].exists()));
    let movement = generals_sim::skirmish::diagnostics::movement_details;
    let shroud = match game.visibility_status(1, target) {
        generals_sim::skirmish::visibility::Status::Clear => "clear",
        generals_sim::skirmish::visibility::Status::Partial => "partial",
        generals_sim::skirmish::visibility::Status::Fogged => "fogged",
        generals_sim::skirmish::visibility::Status::Shrouded => "shrouded",
    };
    format!(
        "native Booby Trap capture timed out: factory {target} trapped={} enemy_shroud={shroud}, trap(id, exists)={trap:?}, capture issued={}
Rebel: {}
Red Guard: {}",
        game.entities[target].booby_trapped(),
        check.moved,
        movement(game, check.scene.provider),
        movement(game, attacker),
    )
}

fn live_objects(game: &Skirmish, first: usize, name: &str) -> Vec<usize> {
    game.entities
        .iter()
        .enumerate()
        .skip(first)
        .filter_map(|(id, actor)| {
            (actor.owner == 0 && actor.alive() && game.catalog[actor.type_index].name == name)
                .then_some(id)
        })
        .collect()
}
