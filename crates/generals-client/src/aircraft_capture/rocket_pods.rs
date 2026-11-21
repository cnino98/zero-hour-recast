//! Actual original button/ground input, targeting decal and twenty-shot capture.
use super::{Check, Stage};
use crate::{
    control_bar::{CommandAction, UiData},
    input::{CommandMode, Mouse},
    particles::ParticleInspection,
    playtest::Match,
    playtest_ui::{Action, PromotionIcon},
    playtest_view::{Capture, UnitVisual, WorldCamera},
    projectile_render::ProjectileVisual,
    radius_cursor::RadiusVisual,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
    source_material::SourceMaterial,
    terrain::Terrain,
};
use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, mouse::MouseButtonInput},
    mesh::VertexAttributeValues,
    prelude::*,
    ui_render::ui_material::MaterialNode,
    window::PrimaryWindow,
};
use generals_sim::skirmish::Order;
use std::error::Error;

#[derive(Resource)]
struct Progress {
    phase: u8,
    point: Option<Vec2>,
    shots: usize,
    started: u64,
    mode_audio: bool,
    sounds: [bool; 2],
    radius_saved: bool,
    barrage_saved: bool,
}

pub(super) fn register(app: &mut App) {
    app.insert_resource(Progress {
        phase: 0,
        point: None,
        shots: 0,
        started: 0,
        mode_audio: false,
        sounds: [false; 2],
        radius_saved: false,
        barrage_saved: false,
    });
    app.add_systems(PreUpdate, audio);
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
    app.add_systems(Update, input.after(crate::radius_cursor::update));
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn audio(check: Res<Check>, mut progress: ResMut<Progress>, audio: FiringAudioInspection) {
    let check = check.into_inner();
    if !matches!(check.stage, Stage::RocketPods) {
        return;
    }
    if progress.phase < 6 {
        if progress.phase == 0 {
            // A previous ordinary attack voice is not proof of the new ground order.
            progress.sounds[0] = audio.finished(check.setup.comanche, "ComancheVoiceAttack");
        } else {
            progress.mode_audio |= audio.object_fx("ComancheVoiceModeRocketPods");
        }
    } else {
        for (heard, played) in progress.sounds.iter_mut().zip(audio.played([
            (check.setup.comanche, "ComancheVoiceAttack"),
            (check.setup.comanche, "ComancheRocketPodWeaponSound"),
        ])) {
            *heard |= played;
        }
    }
}

fn record(check: Res<Check>, mut progress: ResMut<Progress>, battle: Res<Match>) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    if matches!(check.stage, Stage::RocketPods) && progress.phase >= 6 {
        progress.shots += battle
            .simulation
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == check.setup.comanche && shot.weapon_slot == 2)
            .count();
    }
}

#[derive(SystemParam)]
struct Input<'w, 's> {
    battle: ResMut<'w, Match>,
    mouse: Res<'w, Mouse>,
    data: Res<'w, UiData>,
    terrain: Res<'w, Terrain>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static Node,
            &'static UiGlobalTransform,
            &'static Interaction,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    icons: Res<'w, Assets<PromotionIcon>>,
    window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    radius: Single<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static Visibility,
        ),
        With<RadiusVisual>,
    >,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    images: Res<'w, Assets<Image>>,
    events: MessageWriter<'w, MouseButtonInput>,
}

fn input(
    mut check: ResMut<Check>,
    mut progress: ResMut<Progress>,
    mut input: Input,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !matches!(check.stage, Stage::RocketPods)
        || progress.phase >= 7
        || !capture.ready_for_scenario()
    {
        return;
    }
    if progress.phase == 6 {
        clock.unpause();
    } else {
        clock.pause();
    }
    match input.step(&check, &mut progress, &mut capture) {
        Ok(true) => {
            progress.phase += 1;
            if progress.phase == 7 {
                progress.started = input.battle.simulation.tick;
                clock.unpause();
            }
        }
        Ok(false) => {}
        Err(error) => {
            error!("Rocket-pod native input phase {}: {error}", progress.phase);
            check.stage = Stage::Done;
            exit.write(AppExit::error());
        }
    }
}

impl Input<'_, '_> {
    fn step(
        &mut self,
        check: &Check,
        progress: &mut Progress,
        capture: &mut Capture,
    ) -> Result<bool, Box<dyn Error>> {
        match progress.phase {
            0 | 1 => {
                if progress.phase == 0 {
                    self.battle.simulation.stop(check.setup.comanche)?;
                    if !progress.sounds[0] {
                        return Ok(false);
                    }
                }
                if !self.aim_button(check.setup.comanche)? {
                    return Ok(false);
                }
                if progress.phase == 1 {
                    self.button(ButtonState::Pressed);
                }
            }
            2 | 5 => {
                self.button(ButtonState::Released);
                if progress.phase == 5 {
                    progress.sounds = [false; 2];
                }
            }
            3 => {
                require(
                    matches!(self.mouse.pending_command, CommandMode::FireWeapon(2)),
                    "original Rocket Pods button did not enter TERTIARY target mode",
                )?;
                let game = &self.battle.simulation;
                let target = Vec2::from_array(game.entities[check.setup.target].position);
                let home = Vec2::from_array(game.entities[check.setup.airfield].position);
                // Keep the original terrain ring clear of the Command Center's roof.
                let point = target + (home - target).normalize_or_zero() * 160.0;
                let rendered = point * 0.1;
                let screen = self.camera.0.world_to_viewport(
                    self.camera.1,
                    Vec3::new(rendered.x, self.terrain.sample(rendered), -rendered.y),
                )?;
                let ray = self.camera.0.viewport_to_world(self.camera.1, screen)?;
                progress.point = self.terrain.ray_point(ray).map(|point| point * 10.0);
                self.window.1.set_cursor_position(Some(screen));
            }
            4 => {
                if !progress.mode_audio {
                    return Ok(false);
                }
                self.radius_visible(
                    check,
                    progress.point.ok_or("rocket-pod ground ray missing")?,
                )?;
                if !progress.radius_saved {
                    progress.radius_saved = true;
                    capture.intermediate("aircraft-rocketpods-radius.png");
                    return Ok(false);
                }
                self.button(ButtonState::Pressed);
            }
            6 => {
                let expected = progress.point.ok_or("rocket-pod ground ray missing")?;
                let actor = &self.battle.simulation.entities[check.setup.comanche];
                let expected_weapon = actor.weapon_index(2);
                require(
                    matches!(self.mouse.pending_command, CommandMode::Normal),
                    "ground click did not leave TERTIARY target mode",
                )?;
                if !matches!(actor.ai.order,
                    Order::FireAt { position, weapon } if weapon == expected_weapon && Vec2::from_array(position).distance(expected) < 0.001)
                {
                    return Ok(false);
                }
            }
            _ => unreachable!("bounded rocket-pod input phases"),
        }
        Ok(true)
    }

    fn button(&mut self, state: ButtonState) {
        self.events.write(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window: self.window.0,
        });
    }

    fn aim_button(&mut self, actor: usize) -> Result<bool, Box<dyn Error>> {
        let command = self
            .data
            .commands(&self.battle.simulation, actor)
            .iter()
            .find(|command| matches!(command.action, CommandAction::FireWeapon(2)))
            .ok_or("Comanche Rocket Pods command missing")?;
        require(
            command.slot == 1 && command.name == "Command_AmericaVehicleComancheFireRocketPods",
            "wrong original Rocket Pods command",
        )?;
        let (_, node, transform, interaction, material) = self.buttons.iter()
            .find(|(action, _, _, _, _)| matches!(action, Action::Slot(slot) if *slot == command.slot))
            .ok_or("Rocket Pods button missing")?;
        let expected = self.data.icon(&command.image);
        require(
            node.display != Display::None
                && self.icons.get(&material.0).is_some_and(|icon| {
                    icon.image == expected.image && icon.uv_rect == expected.uv_rect
                }),
            "original Rocket Pods command art missing",
        )?;
        let point = transform.translation / self.window.1.scale_factor();
        let authored = self.data.windows["ControlBar.wnd:ButtonCommand01"].center();
        require(
            point.distance(authored * (self.window.1.size() / Vec2::new(800.0, 600.0))) <= 1.0,
            "Rocket Pods button differs from original WND rectangle",
        )?;
        let ready = *interaction == Interaction::Hovered
            && self
                .window
                .1
                .cursor_position()
                .is_some_and(|value| value.distance(point) < 1.0);
        self.window.1.set_cursor_position(Some(point));
        Ok(ready)
    }

    fn radius_visible(&mut self, check: &Check, center: Vec2) -> Result<(), Box<dyn Error>> {
        let game = &self.battle.simulation;
        let actor = &game.entities[check.setup.comanche];
        let weapon = game.catalog[actor.type_index]
            .weapon_template(actor.weapon_index(2))
            .ok_or("Rocket Pods weapon missing")?;
        let radius = weapon.scatter + weapon.scatter_target_scalar;
        require(
            radius.to_bits() == 50.0_f32.to_bits(),
            "original Rocket Pods radius changed",
        )?;
        require(
            game.entities.iter().all(|actor| {
                let unit = &game.catalog[actor.type_index];
                !actor.alive()
                    || !unit.kind.is_building()
                    || Vec2::from_array(actor.position).distance(center)
                        > radius + unit.geometry.major.hypot(unit.geometry.minor)
            }),
            "Rocket Pods terrain radius overlaps a live building",
        )?;
        let mesh = self
            .meshes
            .get(&self.radius.0.0)
            .ok_or("Rocket Pods radius mesh missing")?;
        let material = self
            .materials
            .get(&self.radius.1.0)
            .ok_or("Rocket Pods radius material missing")?;
        let expected = self.terrain.radius_decal(center, radius, [1.0; 4]);
        require(
            *self.radius.2 != Visibility::Hidden
                && same_positions(mesh, &expected)
                && mesh.indices().is_some_and(|indices| indices.len() >= 6)
                && material
                    .texture
                    .as_ref()
                    .and_then(|handle| self.images.get(handle))
                    .is_some_and(|image| image.width() > 0)
                && !material.depth_write
                && material.parameters.vertex_diffuse == 3
                && crate::attack_cursor::context(&mut self.battle, &self.mouse) == Some(5),
            "Rocket Pods radius/cursor geometry or original material is absent",
        )
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    actors: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    meshes: Query<
        'w,
        's,
        (
            &'static crate::model_animation::AnimatedPart,
            &'static ViewVisibility,
        ),
    >,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    visible: Query<'w, 's, &'static ViewVisibility>,
    particles: ParticleInspection<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    mut progress: ResMut<Progress>,
    view: View,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !matches!(check.stage, Stage::RocketPods)
        || progress.phase != 7
        || !capture.ready_for_scenario()
    {
        return;
    }
    clock.unpause();
    let result = view.inspect(&check, &mut progress, &mut capture, &mut clock);
    match result {
        Ok(true) => {
            check.stage = Stage::Done;
            clock.pause();
            capture.finish_scenario();
            println!(
                "Original Rocket Pods: paid $800 research, actual button/ground clicks, visible 50-unit radius, advancing mode/attack/fire audio, original rocket/exhaust capture and exactly twenty TERTIARY shots with PRIMARY restored passed"
            );
        }
        Ok(false) => {}
        Err(error) => {
            error!(
                "Rocket-pod native barrage: {error}; shots={} sounds={:?}",
                progress.shots, progress.sounds
            );
            check.stage = Stage::Done;
            exit.write(AppExit::error());
        }
    }
}

impl View<'_, '_> {
    fn inspect(
        self,
        check: &Check,
        progress: &mut Progress,
        capture: &mut Capture,
        clock: &mut Time<Virtual>,
    ) -> Result<bool, Box<dyn Error>> {
        let game = &self.battle.simulation;
        let actor = check.setup.comanche;
        require(
            game.entities[actor].alive(),
            "Comanche died before rocket-pod verification",
        )?;
        require(
            game.tick - progress.started < 1800,
            "rocket-pod barrage timed out",
        )?;
        require(progress.shots <= 20, "Rocket Pods fired more than one clip")?;
        let body_visible = self.actors.iter().any(|(visual, children)| {
            visual.index == actor
                && visual.generation == self.battle.generation
                && children.iter().any(|child| {
                    self.meshes
                        .get(child)
                        .is_ok_and(|(_, visibility)| visibility.get())
                })
        });
        let rocket_visible = self.rocket_visible(actor)?;
        if body_visible
            && rocket_visible
            && progress.sounds.iter().all(|heard| *heard)
            && !progress.barrage_saved
        {
            progress.barrage_saved = true;
            clock.pause();
            capture.intermediate("aircraft-rocketpods.png");
            return Ok(false);
        }
        if progress.shots > 0 && !matches!(game.entities[actor].ai.order, Order::FireAt { .. }) {
            require(
                progress.shots == 20,
                "Rocket Pods ended before twenty shots",
            )?;
            require(
                game.entities[actor].weapon_slot() == 0,
                "Rocket Pods did not restore PRIMARY",
            )?;
            require(
                progress.barrage_saved
                    && progress.mode_audio
                    && progress.sounds.iter().all(|heard| *heard),
                "Rocket Pods finished without visible rockets and original audio",
            )?;
            return Ok(true);
        }
        Ok(false)
    }
    fn rocket_visible(&self, actor: usize) -> Result<bool, Box<dyn Error>> {
        for (visual, children) in &self.projectiles {
            if visual.generation != self.battle.generation
                || !self.battle.simulation.projectiles.iter().any(|missile| {
                    missile.id == visual.id && missile.source == actor && missile.weapon_slot == 2
                })
                || !children.iter().any(|child| {
                    self.visible
                        .get(child)
                        .is_ok_and(|visibility| visibility.get())
                })
            {
                continue;
            }
            if self.particles.missile(visual.id, self.battle.generation)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn same_positions(actual: &Mesh, expected: &Mesh) -> bool {
    let Some(VertexAttributeValues::Float32x3(actual)) = actual.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return false;
    };
    let Some(VertexAttributeValues::Float32x3(expected)) =
        expected.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return false;
    };
    actual.len() == expected.len()
        && actual
            .iter()
            .flatten()
            .zip(expected.iter().flatten())
            .all(|(a, b)| a.to_bits() == b.to_bits())
}
