//! Native original Mob bodies, command icons and normal/upgraded weapons.
use crate::{
    angry_mob_verification::{self, Setup},
    capture_framing::{self, AircraftFrame},
    control_bar::UiData,
    model_animation::Animations,
    playtest::{CameraRig, Match},
    playtest_ui::selected_command,
    playtest_view::Capture,
    projectile_render::ProjectileVisual,
    skirmish_verification::{advance_until, require},
    strategy_verification::research,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    weapons: [bool; 5],
    began: f64,
    visible: usize,
    diagnostic_phase: Option<u8>,
    framing_distance: Option<f32>,
    projectiles: [bool; 3],
    resume: bool,
    locked_camera: Option<(Vec2, f32, f32)>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = angry_mob_verification::prepare(&mut battle.simulation)?;
    stage_open_area(&mut battle.simulation, &setup)?;
    battle.select_objects(&[setup.nexus], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        weapons: [false; 5],
        began: 0.0,
        visible: 0,
        diagnostic_phase: None,
        framing_distance: None,
        projectiles: [false; 3],
        resume: false,
        locked_camera: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(
        Update,
        frame
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

fn frame(
    check: Res<Check>,
    battle: Res<Match>,
    capture: Res<Capture>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    let capture = capture.into_inner();
    if !capture.ready_for_scenario() || check.phase == 255 {
        if let Some((focus, distance, yaw)) = check.locked_camera {
            camera.focus = focus;
            camera.distance = distance;
            camera.settings.yaw = yaw;
        }
        return;
    }
    let game = &battle.simulation;
    let mut center = [0.0; 3];
    for &child in game.spawned_children(check.setup.nexus) {
        let position = game.object_position(child);
        for axis in 0..3 {
            center[axis] += position[axis] / 10.0;
        }
    }
    camera.distance = check.framing_distance.unwrap_or(camera.settings.min_height);
    capture_framing::focus_on(&mut camera, terrain.into_inner(), center);
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    data: Res<'w, UiData>,
    animations: Res<'w, Animations>,
    framing: AircraftFrame<'w, 's>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    camera: Res<CameraRig>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let camera = camera.into_inner();
    if check.phase == 255 {
        return;
    }
    if check.began == 0.0 {
        check.began = view.clock.elapsed_secs_f64();
    }
    let result = require(
        view.clock.elapsed_secs_f64() - check.began < 90.0,
        &format!(
            "native Mob capture timed out phase={} visible={}/10 camera_distance={} minimum={}",
            check.phase, check.visible, camera.distance, camera.settings.min_height
        ),
    )
    .and_then(|()| view.inspect_phase(&mut check, &mut battle, camera, &mut time, &mut capture));
    if let Err(error) = result {
        check.phase = 255;
        error!("Native Angry Mob acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn inspect_phase(
        self,
        check: &mut Check,
        battle: &mut Match,
        camera: &CameraRig,
        time: &mut Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<(), Box<dyn Error>> {
        let game = &mut battle.simulation;
        let members = game.spawned_children(check.setup.nexus).to_vec();
        if !capture.ready_for_scenario() {
            return Ok(());
        }
        if check.resume {
            check.resume = false;
            time.unpause();
            return Ok(());
        }
        check.locked_camera = Some((camera.focus, camera.distance, camera.settings.yaw));
        let visible = self.visible_members(battle, &members)?;
        check.visible = visible;
        let fitted = self.fit_distance(
            battle,
            &members,
            camera,
            (check.phase == 4).then_some(check.setup.nexus),
        )?;
        check.framing_distance = Some(fitted);
        // Apply camera changes in Update, then inspect the resulting projected
        // geometry on the next frame. Never capture stale camera bounds.
        if fitted.to_bits() != camera.distance.to_bits() {
            return Ok(());
        }
        if self.capture_visibility_failure(check, battle, &members, capture)? {
            return Ok(());
        }
        if self.begin_phase(check, battle, time, capture)? {
            return Ok(());
        }
        observe_firing(check, battle);
        if visible == 10 && self.capture_projectile(check, battle, time, capture)? {
            return Ok(());
        }
        if check.phase == 2
            && check.weapons[..3].iter().all(|value| *value)
            && check.projectiles[..2].iter().all(|value| *value)
            && visible == 10
            && angry_mob_verification::weapon_names(&battle.simulation, check.setup.nexus)
                .iter()
                .any(|name| name == "GLAAngryMobPistolWeapon")
        {
            time.pause();
            capture.intermediate("angry-mob-normal-weapons.png");
            check.phase = 3;
        } else if check.phase == 4
            && check.weapons[3..].iter().all(|value| *value)
            && check.projectiles[2]
            && visible == 10
            && self.active_ak_shot(battle, check.setup.nexus)?
        {
            time.pause();
            check.phase = 255;
            capture.finish_scenario();
            println!(
                "Native original Mob: ten paid original member meshes fit visible world bounds; source command definitions and actual simulation pistol/rock/Molotov then AK47/Molotov shots passed"
            );
        }
        Ok(())
    }

    fn begin_phase(
        &self,
        check: &mut Check,
        battle: &mut Match,
        time: &mut Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<bool, Box<dyn Error>> {
        if check.phase == 0 {
            if check.visible != 10 {
                return Ok(true);
            }
            angry_mob_verification::verify_children(&battle.simulation, check.setup.nexus)?;
            for slot in [11, 13, 14] {
                require(
                    selected_command(battle, &self.data, slot).is_some(),
                    "Mob lost original attack-move, guard or stop command",
                )?;
            }
            time.pause();
            capture.intermediate("angry-mob-paid-original-bodies.png");
            check.phase = 1;
            return Ok(true);
        }
        if check.phase == 1 {
            battle
                .simulation
                .attack(check.setup.nexus, check.setup.target)?;
            check.phase = 2;
            time.unpause();
            return Ok(true);
        }
        if check.phase == 3 {
            battle.simulation.stop(check.setup.nexus)?;
            research(
                &mut battle.simulation,
                check.setup.palace,
                "Upgrade_GLAArmTheMob",
            )?;
            battle
                .simulation
                .attack(check.setup.nexus, check.setup.target)?;
            check.phase = 4;
            time.unpause();
            return Ok(true);
        }
        Ok(false)
    }

    fn capture_projectile(
        &self,
        check: &mut Check,
        battle: &Match,
        time: &mut Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<bool, Box<dyn Error>> {
        let game = &battle.simulation;
        for (visual, children) in &self.projectiles {
            if visual.generation != battle.generation {
                continue;
            }
            let Some(projectile) = game
                .projectiles
                .iter()
                .find(|projectile| projectile.id == visual.id)
            else {
                continue;
            };
            let source = &game.entities[projectile.source];
            if source.spawn_master() != Some(check.setup.nexus) {
                continue;
            }
            let Some(weapon) =
                game.catalog[source.type_index].weapon_template(projectile.weapon_index)
            else {
                continue;
            };
            let Some(index) = projectile_capture(check.phase, &weapon.name) else {
                continue;
            };
            if check.projectiles[index] {
                continue;
            }
            let Some(bounds) = self.framing.isolated_projectile(battle, children)? else {
                continue;
            };
            println!(
                "Mob live projectile capture: frame={} id={} weapon={} upgraded={} logical_ROI={bounds:?}",
                game.tick,
                visual.id,
                weapon.name,
                check.phase == 4
            );
            time.pause();
            capture.intermediate(match index {
                0 => "angry-mob-normal-rock.png",
                1 => "angry-mob-normal-molotov.png",
                _ => "angry-mob-upgraded-molotov.png",
            });
            check.projectiles[index] = true;
            check.resume = true;
            return Ok(true);
        }
        Ok(false)
    }

    fn capture_visibility_failure(
        &self,
        check: &mut Check,
        battle: &Match,
        members: &[usize],
        capture: &mut Capture,
    ) -> Result<bool, Box<dyn Error>> {
        if !matches!(check.phase, 0 | 2 | 4)
            || check.visible == 10
            || check.diagnostic_phase == Some(check.phase)
            || self.clock.elapsed_secs_f64() - check.began < 5.0
        {
            return Ok(false);
        }
        check.diagnostic_phase = Some(check.phase);
        for &id in members.iter().take(10) {
            let game = &battle.simulation;
            let actor = &game.entities[id];
            let shroud = match game.visibility_status(0, id) {
                generals_sim::skirmish::visibility::Status::Clear => "clear",
                generals_sim::skirmish::visibility::Status::Partial => "partial",
                generals_sim::skirmish::visibility::Status::Fogged => "fogged",
                generals_sim::skirmish::visibility::Status::Shrouded => "shrouded",
            };
            let body = body_mesh(&game.catalog[actor.type_index].name)
                .ok_or("Mob body mapping missing")?;
            let projected = self
                .framing
                .mesh_projection(battle, id, &self.animations, body)?;
            let viewport = self.framing.uncovered_viewport();
            let actual_body_visible =
                self.framing
                    .visible_mesh(battle, id, &self.animations, body)?;
            println!(
                "Mob visibility diagnostic: frame={} id={id} template={} owner={} world={:?} alive={} masked={} spawn_held={} logical={} shroud={shroud} flags={:?}; body={body} projected={projected:?} viewport={viewport:?} actual_body_visible={actual_body_visible}",
                game.tick,
                game.catalog[actor.type_index].name,
                actor.owner,
                game.object_position(id),
                actor.alive(),
                actor.masked(),
                actor.disabled_held(),
                game.logically_visible(0, id),
                game.model_flags(actor, [0.5, 0.1]),
            );
        }
        capture.intermediate(match check.phase {
            0 => "angry-mob-phase-0-visibility-diagnostic.png",
            2 => "angry-mob-phase-2-visibility-diagnostic.png",
            _ => "angry-mob-phase-4-visibility-diagnostic.png",
        });
        Ok(true)
    }

    fn fit_distance(
        &self,
        battle: &Match,
        members: &[usize],
        camera: &CameraRig,
        armed_nexus: Option<usize>,
    ) -> Result<f32, Box<dyn Error>> {
        let Some(viewport) = self.framing.uncovered_viewport() else {
            return Ok(camera.distance);
        };
        let mut bounds = Vec::new();
        for &id in members {
            let name = body_mesh(
                &battle.simulation.catalog[battle.simulation.entities[id].type_index].name,
            )
            .ok_or("Mob child lacks source body mapping")?;
            let Some(body) = self
                .framing
                .mesh_projection(battle, id, &self.animations, name)?
            else {
                return Ok(camera.distance);
            };
            bounds.push(body);
        }
        if let Some(nexus) = armed_nexus {
            for shot in &battle.simulation.presentation.shots {
                let actor = &battle.simulation.entities[shot.source];
                let unit = &battle.simulation.catalog[actor.type_index];
                if actor.spawn_master() != Some(nexus)
                    || unit
                        .weapon_template(shot.weapon_index)
                        .is_none_or(|weapon| weapon.name != "GLAAngryMobAK47Weapon")
                {
                    continue;
                }
                let body = body_mesh(&unit.name).ok_or("AK shooter lacks source body mapping")?;
                let prefix = body.split('.').next().ok_or("body model name missing")?;
                if let Some(gun) = self.framing.mesh_projection(
                    battle,
                    shot.source,
                    &self.animations,
                    &format!("{prefix}.AK47"),
                )? {
                    bounds.push(gun);
                }
            }
        }
        Ok(fitted_distance(
            camera.distance,
            camera.settings.min_height,
            camera.settings.max_height,
            viewport,
            &bounds,
        ))
    }

    fn active_ak_shot(&self, battle: &Match, nexus: usize) -> Result<bool, Box<dyn Error>> {
        for shot in &battle.simulation.presentation.shots {
            let actor = &battle.simulation.entities[shot.source];
            if actor.spawn_master() != Some(nexus) {
                continue;
            }
            let unit = &battle.simulation.catalog[actor.type_index];
            if unit
                .weapon_template(shot.weapon_index)
                .is_none_or(|weapon| weapon.name != "GLAAngryMobAK47Weapon")
            {
                continue;
            }
            let body = body_mesh(&unit.name).ok_or("AK shooter lacks source body mapping")?;
            let prefix = body.split('.').next().ok_or("body model name missing")?;
            if self.framing.visible_mesh(
                battle,
                shot.source,
                &self.animations,
                &format!("{prefix}.AK47"),
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn visible_members(&self, battle: &Match, members: &[usize]) -> Result<usize, Box<dyn Error>> {
        members.iter().try_fold(0, |count, &actor| {
            let name = body_mesh(
                &battle.simulation.catalog[battle.simulation.entities[actor].type_index].name,
            )
            .ok_or("Mob child lacks source body mapping")?;
            Ok(count
                + usize::from(
                    self.framing
                        .visible_mesh(battle, actor, &self.animations, name)?,
                ))
        })
    }
}

fn projectile_capture(phase: u8, weapon: &str) -> Option<usize> {
    match (phase, weapon) {
        (2, "GLAAngryMobRockProjectileWeapon") => Some(0),
        (2, "GLAAngryMobMolotovCocktailProjectileWeapon") => Some(1),
        (4, "GLAAngryMobMolotovCocktailProjectileWeapon") => Some(2),
        _ => None,
    }
}

fn observe_firing(check: &mut Check, battle: &Match) {
    let current_weapons =
        angry_mob_verification::weapon_names(&battle.simulation, check.setup.nexus);
    for name in current_weapons {
        let index = match name.as_str() {
            "GLAAngryMobPistolWeapon" => Some(0),
            "GLAAngryMobRockProjectileWeapon" => Some(1),
            "GLAAngryMobMolotovCocktailProjectileWeapon" => {
                Some(if check.phase == 2 { 2 } else { 4 })
            }
            "GLAAngryMobAK47Weapon" => Some(3),
            _ => None,
        };
        if let Some(index) = index {
            check.weapons[index] = true;
        }
    }
}

fn body_mesh(template: &str) -> Option<&'static str> {
    match template {
        "GLAInfantryAngryMobPistol01" => Some("UIMOB01_SKN.UIMOB01_SKN"),
        "GLAInfantryAngryMobRock02" | "GLAInfantryAngryMobMolotov02" => {
            Some("UIMOB02_SKN.UIMOB02_SKN")
        }
        "GLAInfantryAngryMobPistol03" => Some("UIMOB03_SKN.UIMOB03_SKIN"),
        "GLAInfantryAngryMobRock04" => Some("UIMOB04_SKN.UIMOB04_SKIN"),
        "GLAInfantryAngryMobPistol05" => Some("UIMOB05_SKN.UIMOB05_SKIN"),
        _ => None,
    }
}

fn stage_open_area(
    game: &mut generals_sim::skirmish::Skirmish,
    setup: &Setup,
) -> Result<(), Box<dyn Error>> {
    let origin = game.entities[setup.target].position;
    let footprint = game.entities[setup.palace].type_index;
    let goal = [
        [0.0, -250.0],
        [250.0, -250.0],
        [-250.0, -250.0],
        [250.0, 0.0],
        [-250.0, 0.0],
        [0.0, 250.0],
    ]
    .into_iter()
    .filter_map(|offset| {
        game.placement_near(footprint, [origin[0] + offset[0], origin[1] + offset[1]])
    })
    .find(|point| {
        game.entities.iter().all(|actor| {
            let unit = &game.catalog[actor.type_index];
            !actor.alive()
                || !unit.kind.is_building()
                || (actor.position[0] - point[0]).hypot(actor.position[1] - point[1])
                    > 150.0 + unit.geometry_bounds.sphere_radius
        })
    })
    .ok_or("no genuinely clear Mob capture movement destination")?;
    game.move_to(setup.nexus, goal)?;
    let accepted = game.entities[setup.nexus].ai_goal_position();
    let staged = advance_until(
        game,
        7200,
        "ordinary Mob movement into open capture terrain",
        |game| {
            let children = game.spawned_children(setup.nexus);
            let nexus = &game.entities[setup.nexus];
            let route = generals_sim::skirmish::diagnostics::ground_route_result(game, setup.nexus);
            nexus.alive()
                && matches!(nexus.ai.order, generals_sim::skirmish::Order::Idle)
                && !nexus.moving()
                // computePath accepts closest paths. AIInternalMoveToState's
                // 40-unit sanity check refers to the final route node, while
                // the original requested AI goal remains unchanged.
                && route.is_some_and(|route| {
                    route.requested.map(f32::to_bits) == [accepted[0], accepted[1]].map(f32::to_bits)
                        && route.endpoint.is_some_and(|end| {
                            (nexus.position[0] - end[0]).hypot(nexus.position[1] - end[1]) <= 40.0
                        })
                })
                && capture_area_clear(game, nexus.position)
                && children.len() == 10
                && children.iter().all(|&child| {
                    game.entities[child].alive() && !game.entities[child].disabled_held()
                })
        },
    );
    if staged.is_err() {
        diagnose_staging(game, setup, goal, accepted);
    }
    staged?;
    if let Some(route) = generals_sim::skirmish::diagnostics::ground_route_result(game, setup.nexus)
    {
        println!(
            "Mob staging route: requested={:?} endpoint={:?} position={:?}",
            route.requested, route.endpoint, game.entities[setup.nexus].position
        );
    }
    game.stop(setup.nexus)?;
    Ok(())
}

fn capture_area_clear(game: &generals_sim::skirmish::Skirmish, point: [f32; 2]) -> bool {
    game.entities.iter().all(|actor| {
        let unit = &game.catalog[actor.type_index];
        !actor.alive()
            || !unit.kind.is_building()
            || (actor.position[0] - point[0]).hypot(actor.position[1] - point[1])
                > 150.0 + unit.geometry_bounds.sphere_radius
    })
}

fn diagnose_staging(
    game: &generals_sim::skirmish::Skirmish,
    setup: &Setup,
    requested: [f32; 2],
    accepted: [f32; 3],
) {
    println!(
        "Mob staging failure: frame={} requested={requested:?} accepted={accepted:?} target={:?}",
        game.tick, game.entities[setup.target].position
    );
    let result = generals_sim::skirmish::diagnostics::ground_route_result(game, setup.nexus);
    println!(
        "Mob staging route result: {:?}; {}",
        result.map(|route| (route.requested, route.endpoint)),
        generals_sim::skirmish::diagnostics::movement_details(game, setup.nexus)
    );
    for id in std::iter::once(setup.nexus).chain(game.spawned_children(setup.nexus).iter().copied())
    {
        let actor = &game.entities[id];
        let order = match actor.ai.order {
            generals_sim::skirmish::Order::Idle => "idle",
            generals_sim::skirmish::Order::Move(_) => "move",
            generals_sim::skirmish::Order::Attack(_) => "attack",
            generals_sim::skirmish::Order::Exit(_) => "exit",
            generals_sim::skirmish::Order::FollowPath => "follow-path",
            _ => "other",
        };
        println!(
            "Mob staging actor: id={id} template={} position={:?} ai_goal={:?} order={order} moving={} alive={} held={} goal_distance={}",
            game.catalog[actor.type_index].name,
            actor.position,
            actor.ai_goal_position(),
            actor.moving(),
            actor.alive(),
            actor.disabled_held(),
            (actor.position[0] - accepted[0]).hypot(actor.position[1] - accepted[1])
        );
    }
}

// Camera corrections respond only to the projected original bodies. Hidden
// auxiliary meshes cannot enlarge the frame, and tiny bodies request a closer view.
fn fitted_distance(
    distance: f32,
    minimum: f32,
    maximum: f32,
    viewport: Rect,
    bodies: &[Rect],
) -> f32 {
    let tiny = bodies
        .iter()
        .any(|body| body.width() < 16.0 || body.height() < 12.0);
    let margin = Vec2::splat(12.0);
    let cropped = bodies.iter().any(|body| {
        body.min.cmplt(viewport.min + margin).any() || body.max.cmpgt(viewport.max - margin).any()
    });
    if cropped {
        (distance * 1.08).clamp(minimum, maximum)
    } else if tiny {
        (distance / 1.08).clamp(minimum, maximum)
    } else {
        distance
    }
}

#[cfg(test)]
mod tests {
    use super::{fitted_distance, projectile_capture};
    use bevy::prelude::Rect;

    #[test]
    fn upgraded_capture_cannot_reuse_normal_rock_or_an_unrelated_projectile() {
        assert_eq!(
            projectile_capture(2, "GLAAngryMobRockProjectileWeapon"),
            Some(0)
        );
        assert_eq!(
            projectile_capture(2, "GLAAngryMobMolotovCocktailProjectileWeapon"),
            Some(1)
        );
        assert_eq!(
            projectile_capture(4, "GLAAngryMobRockProjectileWeapon"),
            None
        );
        assert_eq!(
            projectile_capture(4, "GLAAngryMobMolotovCocktailProjectileWeapon"),
            Some(2)
        );
        assert_eq!(projectile_capture(4, "GLAAngryMobAK47Weapon"), None);
    }

    #[test]
    fn camera_fitting_keeps_original_body_pixel_gate_and_does_not_widen_for_culled_auxiliary_parts()
    {
        let viewport = Rect::new(0.0, 0.0, 1280.0, 532.0);
        let bodies = [
            Rect::new(600.0, 200.0, 650.0, 310.0),
            Rect::new(800.0, 250.0, 845.0, 380.0),
        ];
        assert_eq!(
            fitted_distance(12.0, 12.0, 31.0, viewport, &bodies).to_bits(),
            12.0_f32.to_bits()
        );
        let cropped = [Rect::new(600.0, 490.0, 650.0, 560.0)];
        assert!(fitted_distance(12.0, 12.0, 31.0, viewport, &cropped) > 12.0);
        let tiny = [Rect::new(600.0, 200.0, 615.0, 210.0)];
        // An undersized member must not pull a cropped member farther outside
        // the image. Settle containment before testing the final body pixel gate.
        assert!(fitted_distance(20.0, 12.0, 31.0, viewport, &[cropped[0], tiny[0]]) > 20.0);
        assert!(fitted_distance(31.0, 12.0, 31.0, viewport, &tiny) < 31.0);
        assert_eq!(
            fitted_distance(12.0, 12.0, 31.0, viewport, &tiny).to_bits(),
            12.0_f32.to_bits()
        );
        assert_eq!(
            fitted_distance(31.0, 12.0, 31.0, viewport, &cropped).to_bits(),
            31.0_f32.to_bits()
        );
    }
}
