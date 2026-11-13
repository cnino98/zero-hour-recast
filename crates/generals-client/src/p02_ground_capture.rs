//! Native paid Burton, Bomb Truck and Combat Bike presentation checks.
use crate::{
    capture_framing::{AircraftFrame, TreadSample, focus_on},
    control_bar::{CommandAction, UiData},
    input::{self, CommandMode, Mouse, PickTarget},
    model_animation::{Animations, TerrainTracks},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{dispatch_command, selected_command},
    playtest_view::{self, Capture},
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    stealth_view::Cloaks,
    sticky_bomb_icons::Icon,
    strategy_verification::{research, template},
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*, text::TextLayoutInfo, window::PrimaryWindow};
use generals_sim::skirmish::{
    DeathType, Faction, Kind as UnitKind, Skirmish, special_ability::BurtonCharge,
};
use std::error::Error;

#[derive(Clone, Copy)]
pub enum Kind {
    Burton,
    BombTruck,
    CombatBike,
}

struct Scene {
    actor: usize,
    target: usize,
    auxiliary: usize,
    borrowed_treads: Option<[usize; 2]>,
    route: [f32; 2],
}

#[derive(Resource)]
struct Check {
    kind: Kind,
    scene: Scene,
    phase: u8,
    issued: bool,
    began: Option<f64>,
    effect_position: Option<[f32; 3]>,
    stopped_for_reveal: bool,
    bike_faded: bool,
    tread_sample: Option<TreadSample>,
    tread_goal: Option<[f32; 2]>,
}

pub fn prepare(app: &mut App, battle: &mut Match, kind: Kind) -> Result<(), Box<dyn Error>> {
    let scene = match kind {
        Kind::Burton => prepare_burton(&mut battle.simulation)?,
        Kind::BombTruck | Kind::CombatBike => prepare_gla(&mut battle.simulation, kind)?,
    };
    battle.select_objects(&[scene.actor], true);
    app.insert_resource(Check {
        kind,
        scene,
        phase: 0,
        issued: false,
        began: None,
        effect_position: None,
        stopped_for_reveal: false,
        bike_faded: false,
        tread_sample: None,
        tread_goal: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        Update,
        frame
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    if matches!(kind, Kind::BombTruck) {
        app.add_systems(Update, hover_truck.before(input::pointer));
    }
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
    Ok(())
}

fn paid_unit(
    game: &mut Skirmish,
    producer: usize,
    name: &str,
    owner: usize,
) -> Result<usize, Box<dyn Error>> {
    let kind = template(game, name)?;
    let cost = game.catalog[kind].cost;
    advance_until(game, 18_000, "income funding native capture unit", |game| {
        game.players[owner].cash >= cost
    })?;
    produce_type(game, producer, kind, owner)
}

fn prepare_burton(game: &mut Skirmish) -> Result<Scene, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, UnitKind::Builder, 0)?;
    construct(game, builder, UnitKind::PowerPlant, 0)?;
    construct(game, builder, UnitKind::SupplyCenter, 0)?;
    let barracks = construct(game, builder, UnitKind::Barracks, 0)?;
    construct(game, builder, UnitKind::Factory, 0)?;
    let strategy = template(game, "AmericaStrategyCenter")?;
    let center = construct_type(game, builder, strategy, 0)?;
    let actor = paid_unit(game, barracks, "AmericaInfantryColonelBurton", 0)?;
    research(game, center, "Upgrade_AmericaChemicalSuits")?;
    require(
        game.entities[actor].chemical_suit(),
        "Burton lacks paid Chemical Suits",
    )?;

    let enemy = friendly_entity(game, UnitKind::Builder, 1)?;
    construct(game, enemy, UnitKind::PowerPlant, 1)?;
    construct(game, enemy, UnitKind::SupplyCenter, 1)?;
    let enemy_barracks = construct(game, enemy, UnitKind::Barracks, 1)?;
    let target = paid_unit(game, enemy_barracks, "ChinaInfantryRedguard", 1)?;
    let command = friendly_entity(game, UnitKind::CommandCenter, 1)?;
    let auxiliary = paid_unit(game, command, "ChinaVehicleDozer", 1)?;
    let route = open_destination(game, actor, center)?;
    stage_move(game, actor, route, 32.0)?;
    let position = game.entities[actor].position;
    stage_move(game, auxiliary, position, 150.0)?;
    stage_move(game, target, position, 95.0)?;
    advance_until(game, 120, "Burton stationary stealth", |game| {
        game.entities[actor].stealthed()
    })?;
    Ok(Scene {
        actor,
        target,
        auxiliary,
        borrowed_treads: None,
        route: game.entities[center].position,
    })
}

fn prepare_gla(game: &mut Skirmish, kind: Kind) -> Result<Scene, Box<dyn Error>> {
    game.reset_sandbox(Faction::Gla)?;
    let worker = friendly_entity(game, UnitKind::Builder, 0)?;
    construct(game, worker, UnitKind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, UnitKind::Barracks, 0)?;
    let dealer = construct(game, worker, UnitKind::Factory, 0)?;
    let (actor, target, auxiliary) = match kind {
        Kind::BombTruck => {
            let palace = template(game, "GLAPalace")?;
            construct_type(game, worker, palace, 0)?;
            (
                paid_unit(game, dealer, "GLAVehicleBombTruck", 0)?,
                paid_unit(game, dealer, "GLAVehicleTechnical", 0)?,
                paid_unit(game, barracks, "GLAInfantryRebel", 0)?,
            )
        }
        Kind::CombatBike => {
            let bike = paid_unit(game, dealer, "GLAVehicleCombatBike", 0)?;
            let [rebel] = *game.passengers(bike) else {
                return Err("paid bike lacks its Rebel rider".into());
            };
            (
                bike,
                paid_unit(game, barracks, "GLAInfantryTerrorist", 0)?,
                rebel,
            )
        }
        Kind::Burton => unreachable!(),
    };
    let borrowed_treads = if matches!(kind, Kind::BombTruck) {
        Some([
            paid_unit(game, dealer, "GLATankScorpion", 0)?,
            paid_unit(game, dealer, "GLAVehicleQuadCannon", 0)?,
        ])
    } else {
        None
    };
    if let Some(donors) = borrowed_treads {
        for donor in donors {
            let destination = open_destination(game, donor, dealer)?;
            stage_move(game, donor, destination, 35.0)?;
        }
    }
    let route = open_destination(game, actor, dealer)?;
    if matches!(kind, Kind::BombTruck) {
        stage_move(game, auxiliary, route, 35.0)?;
    }
    Ok(Scene {
        actor,
        target,
        auxiliary,
        borrowed_treads,
        route,
    })
}

fn open_destination(
    game: &Skirmish,
    actor: usize,
    building: usize,
) -> Result<[f32; 2], Box<dyn Error>> {
    let origin = game.entities[actor].position;
    let footprint = game.entities[building].type_index;
    [
        [250.0, -250.0],
        [0.0, -250.0],
        [-250.0, -250.0],
        [250.0, 250.0],
        [0.0, 250.0],
        [-250.0, 250.0],
    ]
    .into_iter()
    .filter_map(|offset| {
        game.placement_near(footprint, [origin[0] + offset[0], origin[1] + offset[1]])
    })
    .find(|point| {
        game.build_site_reachable(actor, *point)
            && game.entities.iter().all(|actor| {
                let unit = &game.catalog[actor.type_index];
                !actor.alive()
                    || !unit.kind.is_building()
                    || (actor.position[0] - point[0]).hypot(actor.position[1] - point[1])
                        > 110.0 + unit.geometry_bounds.sphere_radius
            })
    })
    .ok_or_else(|| "no open ground for the paid capture units".into())
}

// Use the paid Arms Dealer's building footprint for the route search.
fn move_borrowed_truck(
    game: &mut Skirmish,
    actor: usize,
    issued: &mut Option<[f32; 2]>,
) -> Result<[f32; 2], Box<dyn Error>> {
    if let Some(goal) = *issued {
        return Ok(goal);
    }
    let dealer = friendly_entity(game, UnitKind::Factory, game.entities[actor].owner)?;
    let goal = open_destination(game, actor, dealer)?;
    game.move_to(actor, goal)?;
    // MOVING becomes true only after the movement state advances. Reissuing
    // while it is false restarts that state on every display callback.
    *issued = Some(goal);
    Ok(goal)
}

fn stage_move(
    game: &mut Skirmish,
    actor: usize,
    destination: [f32; 2],
    radius: f32,
) -> Result<(), Box<dyn Error>> {
    game.move_to(actor, destination)?;
    advance_until(
        game,
        7_200,
        "ground capture unit approaching its staging point",
        |game| {
            Vec2::from_array(game.entities[actor].position).distance(Vec2::from_array(destination))
                < radius
        },
    )?;
    game.stop(actor)?;
    Ok(())
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
    // Keep the composed view while the renderer reads back an intermediate image.
    if check.phase != 0 && !capture.ready_for_scenario() {
        return;
    }
    let game = &battle.simulation;
    let scene = &check.scene;
    let point = check
        .effect_position
        .unwrap_or_else(|| match (check.kind, check.phase) {
            (Kind::Burton, 0) => midpoint(game, scene.actor, scene.target),
            (Kind::Burton, 1 | 2) => game.object_position(scene.auxiliary),
            _ => game.object_position(scene.actor),
        });
    camera.distance = camera.settings.min_height;
    focus_on(&mut camera, terrain.into_inner(), point);
}

fn midpoint(game: &Skirmish, first: usize, second: usize) -> [f32; 3] {
    let first = game.object_position(first);
    let second = game.object_position(second);
    std::array::from_fn(|axis| f32::midpoint(first[axis], second[axis]))
}

fn hover_truck(
    check: Res<Check>,
    battle: Res<Match>,
    mut view: ParamSet<(
        input::picking::View,
        Single<&mut Window, With<PrimaryWindow>>,
    )>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    let truck = check.scene.actor;
    let game = &battle.simulation;
    let Some(appearance) = game.entities[truck]
        .disguised_appearance()
        .filter(|_| check.phase == 1)
    else {
        return;
    };
    let view_picker = view.p0();
    let base = playtest_view::unit_position(game, truck);
    let height = game.catalog[appearance.type_index]
        .geometry_bounds
        .max_height;
    let point = [0.5, 0.25, 0.75].into_iter().find_map(|fraction| {
        let point = view_picker
            .camera
            .0
            .world_to_viewport(
                view_picker.camera.1,
                base + Vec3::Y * (height * fraction * 0.1),
            )
            .ok()?;
        (point.cmpge(Vec2::ZERO).all()
            && point.cmplt(view_picker.window.size()).all()
            && !crate::playtest_ui::captures_pointer(
                battle,
                &view_picker.window,
                point,
                &view_picker.ui,
                &view_picker.promotions,
            )
            && matches!(view_picker.pick(battle, point), Some(PickTarget::Unit(id)) if id == truck))
        .then_some(point)
    });
    if let Some(point) = point {
        // Exercise the ordinary window pick and world-tooltip delay without
        // relying on a physical macOS pointer event in the native runner.
        view.p1().set_cursor_position(Some(point));
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    data: Res<'w, UiData>,
    framing: AircraftFrame<'w, 's>,
    animations: Res<'w, Animations>,
    cloaks: Res<'w, Cloaks>,
    particles: ParticleInspection<'w, 's>,
    tracks: Option<Res<'w, TerrainTracks>>,
    charge_icons: Query<
        'w,
        's,
        (
            &'static Icon,
            &'static ComputedNode,
            &'static InheritedVisibility,
            &'static ImageNode,
        ),
    >,
    hover_labels: Query<
        'w,
        's,
        (
            &'static Text,
            &'static TextLayoutInfo,
            &'static InheritedVisibility,
            &'static ChildOf,
        ),
        With<crate::world_hover::Label>,
    >,
    hover_clips: Query<'w, 's, (&'static crate::world_hover::Clip, &'static ComputedNode)>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == u8::MAX || !capture.ready_for_scenario() {
        return;
    }
    let elapsed = view.clock.elapsed_secs_f64();
    let began = *check.began.get_or_insert(elapsed);
    let result = require(
        elapsed - began < 150.0,
        &format!(
            "ground capture phase {} timed out at simulation tick {}",
            check.phase, battle.simulation.tick
        ),
    )
    .and_then(|()| view.inspect(&mut check, &mut battle, &mut mouse, &mut time));
    match result {
        Ok(Some(file)) => {
            println!(
                "Native ground capture: {file}, frame {}",
                battle.simulation.tick
            );
            time.pause();
            check.began = None;
            check.issued = false;
            check.tread_sample = None;
            check.tread_goal = None;
            let final_phase = match check.kind {
                Kind::Burton => 2,
                Kind::BombTruck => 4,
                Kind::CombatBike => 3,
            };
            if check.phase == final_phase {
                check.phase = u8::MAX;
                capture.finish_scenario();
            } else {
                check.phase += 1;
                capture.intermediate(file);
            }
        }
        Ok(None) => {}
        Err(error) => {
            check.phase = u8::MAX;
            error!("Native ground acceptance: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl View<'_, '_> {
    fn inspect(
        self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
    ) -> Result<Option<&'static str>, Box<dyn Error>> {
        match check.kind {
            Kind::Burton => self.burton(check, battle, mouse, time),
            Kind::BombTruck => self.truck(check, battle, mouse, time),
            Kind::CombatBike => self.bike(check, battle, mouse, time),
        }
    }

    fn command(
        &self,
        battle: &mut Match,
        mouse: &mut Mouse,
        slot: usize,
        accepts: impl FnOnce(CommandAction) -> bool,
        target: Option<usize>,
    ) -> Result<(), Box<dyn Error>> {
        let action = selected_command(battle, &self.data, slot)
            .ok_or("missing original HUD action")?
            .action;
        require(accepts(action), "unexpected command in original HUD slot")?;
        dispatch_command(battle, mouse, action);
        if let Some(target) = target {
            let point = battle.simulation.entities[target].position.into();
            input::targeted_order(battle, mouse, point, Some(target));
            if !matches!(mouse.pending_command, CommandMode::Normal) {
                let target = &battle.simulation.entities[target];
                return Err(format!(
                    "original targeted HUD slot {slot} rejected {} owned by {} (health {}, masked {}, last attacker {:?})",
                    battle.simulation.catalog[target.type_index].name, target.owner, target.health,
                    target.masked(), target.last_attacker(),
                ).into());
            }
        }
        Ok(())
    }

    fn burton(
        &self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
    ) -> Result<Option<&'static str>, Box<dyn Error>> {
        let Scene {
            actor,
            target,
            auxiliary,
            route,
            ..
        } = check.scene;
        match check.phase {
            0 => {
                if !check.issued {
                    self.command(
                        battle,
                        mouse,
                        1,
                        |action| matches!(action, CommandAction::BurtonKnife),
                        Some(target),
                    )?;
                    time.unpause();
                    check.issued = true;
                    return Ok(None);
                }
                let game = &battle.simulation;
                require(
                    game.entities[target].alive(),
                    "knife windup ended before native capture",
                )?;
                if model_has(game, actor, "PREATTACK_B")
                    && self.framing.visible(battle, actor)?
                    && self.framing.visible(battle, target)?
                {
                    return Ok(Some("burton-knife.png"));
                }
            }
            1 => {
                if !check.issued {
                    time.unpause();
                    if battle.simulation.entities[target].alive() {
                        return Ok(None);
                    }
                    require(
                        battle.simulation.entities[target].last_attacker() == Some(actor),
                        "Burton did not deliver the original knife strike",
                    )?;
                    self.command(
                        battle,
                        mouse,
                        4,
                        |action| {
                            matches!(action, CommandAction::BurtonCharge(BurtonCharge::Remote))
                        },
                        Some(auxiliary),
                    )?;
                    check.issued = true;
                }
                let game = &battle.simulation;
                if let Some(charge) = child(game, auxiliary, "RemoteC4Charge")
                    && self.charge_icon_visible(charge, battle.generation)
                    && self.framing.visible(battle, auxiliary)?
                {
                    return Ok(Some("burton-c4.png"));
                }
            }
            2 => {
                if !check.issued {
                    battle.simulation.move_to(actor, route)?;
                    time.unpause();
                    check.issued = true;
                }
                let game = &battle.simulation;
                let victim = &game.entities[auxiliary];
                if victim.alive()
                    && check.effect_position.is_none()
                    && Vec2::from_array(victim.position)
                        .distance(Vec2::from_array(game.entities[actor].position))
                        > 125.0
                {
                    check.effect_position = Some(game.object_position(auxiliary));
                    battle.simulation.stop(actor)?;
                    self.command(
                        battle,
                        mouse,
                        6,
                        |action| matches!(action, CommandAction::BurtonDetonate),
                        None,
                    )?;
                }
                if !battle.simulation.entities[auxiliary].alive()
                    && battle.simulation.entities[actor].alive()
                    && self
                        .particles
                        .incendiary("ExplosionLarge01", battle.generation)?
                {
                    return Ok(Some("burton-c4-detonation.png"));
                }
            }
            _ => unreachable!(),
        }
        Ok(None)
    }

    fn charge_icon_visible(&self, charge: usize, generation: u32) -> bool {
        self.charge_icons
            .iter()
            .any(|(icon, node, visibility, image)| {
                icon.bomb == charge
                    && icon.generation == generation
                    && visibility.get()
                    && node.size().cmpge(Vec2::splat(4.0)).all()
                    && image.rect.is_some()
            })
    }

    fn truck(
        &self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
    ) -> Result<Option<&'static str>, Box<dyn Error>> {
        let Scene {
            actor,
            target,
            route,
            ..
        } = check.scene;
        match check.phase {
            0 => {
                if !check.issued {
                    let game = &mut battle.simulation;
                    research(game, actor, "Upgrade_GLABombTruckBioBomb")?;
                    research(game, actor, "Upgrade_GLABombTruckHighExplosiveBomb")?;
                    game.move_to(actor, route)?;
                    time.unpause();
                    check.issued = true;
                }
                let game = &battle.simulation;
                let vehicle = &game.entities[actor];
                if vehicle.moving()
                    && self.wheel_effects(actor, "RocketBuggyDust", battle.generation)?
                    && self.framing.visible(battle, actor)?
                {
                    require(
                        vehicle
                            .sub_object_overrides()
                            .iter()
                            .any(|part| part.name == "Bombload04" && part.visible),
                        "combined paid truck payload missing",
                    )?;
                    if self.framing.visible_mesh(
                        battle,
                        actor,
                        &self.animations,
                        "UVBMBTRUK.BOMBLOAD04",
                    )? {
                        return Ok(Some("bomb-truck-payload.png"));
                    }
                }
            }
            1 => {
                if !check.issued {
                    battle.simulation.stop(actor)?;
                    self.command(
                        battle,
                        mouse,
                        1,
                        |action| matches!(action, CommandAction::BombTruckDisguise),
                        Some(target),
                    )?;
                    time.unpause();
                    check.issued = true;
                }
                let game = &battle.simulation;
                let disguised =
                    game.entities[actor]
                        .disguised_appearance()
                        .is_some_and(|appearance| {
                            appearance.type_index == game.entities[target].type_index
                        });
                let fade_finished = self
                    .cloaks
                    .base_opacity(actor)
                    .is_some_and(|opacity| opacity.to_bits() == 1.0_f32.to_bits());
                if disguised && fade_finished && self.framing.visible(battle, actor)? {
                    time.pause();
                    let name = &self.data.units[game.entities[actor].type_index].name;
                    if mouse.hovered == Some(actor) && self.hover_name_visible(name) {
                        return Ok(Some("bomb-truck-disguise.png"));
                    }
                }
            }
            2 | 3 => return self.borrowed_treads(check, battle, mouse, time),
            4 => {
                return self.reveal_and_detonate(check, battle, mouse, time);
            }
            _ => unreachable!(),
        }
        Ok(None)
    }

    fn borrowed_treads(
        &self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
    ) -> Result<Option<&'static str>, Box<dyn Error>> {
        let actor = check.scene.actor;
        let targets = check
            .scene
            .borrowed_treads
            .ok_or("missing paid tread donor vehicles")?;
        let donor = targets[usize::from(check.phase - 2)];
        time.unpause();
        if !check.issued {
            if !battle.simulation.bomb_truck_disguise_target(actor, donor) {
                return Ok(None);
            }
            battle.simulation.stop(actor)?;
            self.command(
                battle,
                mouse,
                1,
                |action| matches!(action, CommandAction::BombTruckDisguise),
                Some(donor),
            )?;
            check.issued = true;
        }
        let game = &battle.simulation;
        let borrowed = game.entities[actor]
            .disguised_appearance()
            .is_some_and(|appearance| appearance.type_index == game.entities[donor].type_index);
        let opaque = self
            .cloaks
            .base_opacity(actor)
            .is_some_and(|opacity| opacity.to_bits() == 1.0_f32.to_bits());
        if borrowed && opaque {
            move_borrowed_truck(&mut battle.simulation, actor, &mut check.tread_goal)?;
            if battle.simulation.entities[actor].moving()
                && self.framing.visible(battle, actor)?
                && let Some(sample) = self.framing.tread_offset(battle, actor, &self.animations)?
            {
                // A retained nonzero U is not movement: the same visible original
                // tread mesh must advance on a later display frame.
                let advanced = check.tread_sample.as_ref().is_some_and(|prior| {
                    prior.model == sample.model
                        && prior.mesh == sample.mesh
                        && prior.offset != sample.offset
                });
                check.tread_sample = Some(sample);
                if advanced {
                    return Ok(Some(if check.phase == 2 {
                        "bomb-truck-tank-treads.png"
                    } else {
                        "bomb-truck-halftrack-treads.png"
                    }));
                }
            }
        }
        Ok(None)
    }

    fn hover_name_visible(&self, name: &str) -> bool {
        self.hover_labels
            .iter()
            .any(|(text, glyphs, visible, parent)| {
                text.0 == name
                    && visible.get()
                    && !glyphs.glyphs.is_empty()
                    && self
                        .hover_clips
                        .get(parent.parent())
                        .is_ok_and(|(clip, node)| {
                            matches!(clip, crate::world_hover::Clip::Text { highlight: false })
                                && node.size().x >= glyphs.size.x * glyphs.scale_factor
                        })
            })
    }

    fn reveal_and_detonate(
        &self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
    ) -> Result<Option<&'static str>, Box<dyn Error>> {
        let Scene {
            actor, auxiliary, ..
        } = check.scene;
        if !check.issued {
            battle.simulation.force_attack(actor, auxiliary)?;
            time.unpause();
            check.issued = true;
        }
        let game = &battle.simulation;
        if !check.stopped_for_reveal
            && Vec2::from_array(game.entities[actor].position)
                .distance(Vec2::from_array(game.entities[auxiliary].position))
                <= 90.0
        {
            // StealthUpdate sees the approach in the fixed tick first.
            battle.simulation.stop(actor)?;
            check.stopped_for_reveal = true;
        }
        let game = &battle.simulation;
        if game.entities[actor].alive()
            && game.entities[actor].disguised_appearance().is_none()
            && check.effect_position.is_none()
        {
            check.effect_position = Some(game.object_position(actor));
            self.command(
                battle,
                mouse,
                3,
                |action| matches!(action, CommandAction::BombTruckDetonate),
                None,
            )?;
        }
        let game = &battle.simulation;
        if matches!(game.entities[actor].death_type(), DeathType::Suicided)
            && let Some(hulk) = child(game, actor, "DeadBombTruckHulk")
            && self.framing.visible(battle, hulk)?
            && self
                .particles
                .incendiary("BombTruckHEBioExplosionArms", battle.generation)?
        {
            return Ok(Some("bomb-truck-detonation.png"));
        }
        Ok(None)
    }

    fn bike(
        &self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
    ) -> Result<Option<&'static str>, Box<dyn Error>> {
        let Scene {
            actor,
            target,
            auxiliary,
            route,
            ..
        } = check.scene;
        match check.phase {
            0 => {
                if !check.issued {
                    battle.simulation.move_to(actor, route)?;
                    time.unpause();
                    check.issued = true;
                }
                let game = &battle.simulation;
                if game.passengers(actor) == [auxiliary]
                    && game.entities[actor].moving()
                    && self.track_edges(actor)? >= 8
                    && self.wheel_effects(actor, "CombatBikeDust", battle.generation)?
                    && model_has(game, actor, "RIDER2")
                    && self.framing.visible(battle, actor)?
                {
                    return Ok(Some("combat-bike-rebel.png"));
                }
            }
            1 => {
                if !check.issued {
                    battle.simulation.stop(actor)?;
                    time.unpause();
                    check.issued = true;
                }
                if !battle.simulation.entities[actor].moving()
                    && self.track_edges(actor)? >= 7
                    && self.wheel_dust_cleared(actor, battle.generation)?
                    && self.framing.visible(battle, actor)?
                {
                    return Ok(Some("combat-bike-tracks.png"));
                }
            }
            2 => {
                if !check.issued && !self.resume_bike(check, battle, mouse, time)? {
                    return Ok(None);
                }
                let game = &battle.simulation;
                if game.passengers(actor) == [target]
                    && model_has(game, actor, "RIDER5")
                    && self.framing.visible(battle, actor)?
                {
                    require(
                        game.entities[auxiliary].alive()
                            && game.entities[auxiliary].contained_by().is_none(),
                        "old rider did not survive the replacement",
                    )?;
                    require(
                        battle.selected.contains(&actor),
                        "new rider selection did not transfer to the bike",
                    )?;
                    return Ok(Some("combat-bike-terrorist.png"));
                }
            }
            3 => {
                if !check.issued {
                    if !battle.simulation.can_scuttle_combat_bike(actor) {
                        time.unpause();
                        return Ok(None);
                    }
                    self.command(
                        battle,
                        mouse,
                        1,
                        |action| matches!(action, CommandAction::Evacuate),
                        None,
                    )?;
                    time.unpause();
                    check.issued = true;
                }
                let game = &battle.simulation;
                if matches!(game.entities[actor].death_type(), DeathType::Toppled)
                    && let Some(hulk) = child(game, actor, "GLACombatBikeToppledHulk")
                    && self.framing.visible(battle, hulk)?
                {
                    require(
                        game.entities[target].alive()
                            && game.entities[target].contained_by().is_none(),
                        "scuttle did not release the rider alive",
                    )?;
                    return Ok(Some("combat-bike-hulk.png"));
                }
            }
            _ => unreachable!(),
        }
        Ok(None)
    }

    fn resume_bike(
        &self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
    ) -> Result<bool, Box<dyn Error>> {
        let Scene {
            actor,
            target,
            route,
            ..
        } = check.scene;
        time.unpause();
        if !check.bike_faded {
            let tracks = self.terrain_tracks()?;
            if tracks.visible_edges(actor) == 0 && !tracks.has_track_mesh(actor) {
                println!(
                    "Native Combat Bike original tracks faded at frame {}",
                    battle.simulation.tick
                );
                battle.simulation.move_to(actor, route)?;
                check.bike_faded = true;
            }
            return Ok(false);
        }
        if !battle.simulation.entities[actor].moving()
            || !self.wheel_effects(actor, "CombatBikeDust", battle.generation)?
            || !self.terrain_tracks()?.has_track_mesh(actor)
        {
            return Ok(false);
        }
        println!(
            "Native Combat Bike emitted original tracks again at frame {}",
            battle.simulation.tick
        );
        battle.simulation.stop(actor)?;
        battle.select_objects(&[target], true);
        let point = battle.simulation.entities[actor].position.into();
        require(
            input::context_order(battle, Some(point), Some(PickTarget::Unit(actor)), mouse),
            "Combat Bike boarding click was rejected",
        )?;
        check.issued = true;
        Ok(true)
    }

    fn wheel_dust_cleared(&self, actor: usize, generation: u32) -> Result<bool, Box<dyn Error>> {
        for name in [
            "CombatBikeDust",
            "CombatBikeDirtSpray",
            "CombatBikeDirtPowerSlide",
        ] {
            if self.particles.object_attached(actor, name, generation)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn track_edges(&self, actor: usize) -> Result<usize, Box<dyn Error>> {
        Ok(self.terrain_tracks()?.visible_edges(actor))
    }

    fn terrain_tracks(&self) -> Result<&TerrainTracks, Box<dyn Error>> {
        self.tracks
            .as_deref()
            .ok_or_else(|| "terrain track renderer missing from vehicle capture".into())
    }

    fn wheel_effects(
        &self,
        actor: usize,
        dust: &str,
        generation: u32,
    ) -> Result<bool, Box<dyn Error>> {
        Ok(self.track_edges(actor)? >= 2
            && self.particles.object_attached(actor, dust, generation)?)
    }
}

fn model_has(game: &Skirmish, actor: usize, condition: &str) -> bool {
    let actor = &game.entities[actor];
    game.model_flags(actor, game.catalog[actor.type_index].body_damage_thresholds)
        .contains(condition)
}

fn child(game: &Skirmish, producer: usize, name: &str) -> Option<usize> {
    game.entities.iter().position(|actor| {
        actor.exists()
            && actor.producer() == Some(producer)
            && game.catalog[actor.type_index].name == name
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_bomb_truck_moves_after_scorpion_and_quad_disguises() {
        let (_, mut battle, mut animations) =
            crate::cleanup_verification::tests::installed_with_animations();
        let game = &mut battle.simulation;
        animations.bind_logical_bones(&mut game.catalog);
        let scene = prepare_gla(game, Kind::BombTruck).unwrap();
        let actor = scene.actor;
        let donors = scene.borrowed_treads.unwrap();
        assert_eq!(
            donors.map(|id| game.catalog[game.entities[id].type_index].name.as_str()),
            ["GLATankScorpion", "GLAVehicleQuadCannon"]
        );
        stage_move(game, actor, scene.route, 35.0).unwrap();
        disguise(game, actor, scene.target);
        for donor in donors {
            disguise(game, actor, donor);
            let start = Vec2::from_array(game.entities[actor].position);
            let mut issued = None;
            let goal = move_borrowed_truck(game, actor, &mut issued).unwrap();
            assert!(start.distance(Vec2::from_array(goal)) > 50.0);
            advance_until(
                game,
                600,
                "borrowed truck moves under its disguise",
                |game| {
                    // Two display callbacks per logic step must not continually
                    // restart the move before the source MOVING flag is set.
                    for _ in 0..2 {
                        let retained = move_borrowed_truck(game, actor, &mut issued).unwrap();
                        assert_eq!(retained.map(f32::to_bits), goal.map(f32::to_bits));
                    }
                    let truck = &game.entities[actor];
                    truck.moving() && start.distance(Vec2::from_array(truck.position)) > 20.0
                },
            )
            .unwrap();
            assert_eq!(
                game.entities[actor]
                    .disguised_appearance()
                    .unwrap()
                    .type_index,
                game.entities[donor].type_index
            );
            game.stop(actor).unwrap();
        }
    }

    fn disguise(game: &mut Skirmish, actor: usize, donor: usize) {
        advance_until(
            game,
            600,
            "paid donor admitted by original disguise",
            |game| game.bomb_truck_disguise_target(actor, donor),
        )
        .unwrap();
        game.bomb_truck_order_disguise(actor, donor).unwrap();
        let expected = game.entities[donor].type_index;
        advance_until(
            game,
            600,
            "Bomb Truck acquires paid donor appearance",
            |game| {
                game.entities[actor]
                    .disguised_appearance()
                    .is_some_and(|appearance| appearance.type_index == expected)
            },
        )
        .unwrap();
    }
}
