//! Paid installed Saboteurs and physical sabotage contacts, SH 2bfaea60.
//! `SabotageSupplyCenterCrateCollide::executeCrateBehavior` and
//! `SabotageMilitaryFactoryCrateCollide::executeCrateBehavior`; CRC guard is zero.
use crate::{
    input::{self, Mouse, PickTarget},
    model_animation::AnimatedPart,
    playtest::{CameraRig, MAP_NAME, Match},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::{advance_until, construct, friendly_entity, produce_type, require},
    strategy_verification::template,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{DeathType, Faction, Kind, Order, Skirmish, sabotage::Effect};
use generals_sim::terrain::TerrainMap;
use std::{error::Error, path::Path};

mod cliff;

struct Setup {
    thief: usize,
    actor: usize,
    reserve: usize,
    supply: usize,
    factory: usize,
    losses: u32,
    factory_health: u32,
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut game = crate::opponent_verification::load_game(root, MAP_NAME)?;
    let setup = prepare_scene(&mut game)?;
    steal(&mut game, &setup)?;
    queue_factory(&mut game, &setup)?;
    game.enter(setup.actor, setup.factory)?;
    let (frame, _, _) = contact(&mut game, setup.actor, setup.factory)?;
    let remaining = verify_disabled(&game, &setup)?;
    let duration = factory_duration(&game, setup.actor)?;
    for _ in 1..duration {
        game.step();
        require(
            game.entities[setup.factory].hacked(),
            "factory sabotage ended before its original duration",
        )?;
        require(
            verify_disabled(&game, &setup)? == remaining,
            "disabled factory advanced paid production",
        )?;
    }
    advance_until(&mut game, 2, "source factory sabotage expiry", |game| {
        !game.entities[setup.factory].hacked()
    })?;
    require(
        game.tick == frame + u64::from(duration),
        "factory sabotage duration differs from the original INI",
    )?;
    game.step();
    require(
        game.entities[setup.factory]
            .queue
            .front()
            .is_some_and(|queue| queue.remaining_seconds.to_bits() != remaining),
        "factory production did not resume after sabotage",
    )?;
    cliff::verify(root, &mut game, setup.reserve)?;
    println!(
        "Installed Saboteur: paid Arms Dealer prerequisite and three units, innate concealment, explicit physical supply theft, consumed actors without death losses, original factory shutdown/paused production/exact expiry passed"
    );
    verify_fake_building(&mut game)
}

fn verify_fake_building(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let mut slots = game.slots.clone();
    for (owner, slot) in slots.iter_mut().enumerate() {
        slot.faction = Some(Faction::Gla);
        slot.occupied = owner < 2;
        slot.difficulty = None;
        slot.team = None;
    }
    game.reset_slots(slots)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    construct(game, worker, Kind::Factory, 0)?;
    let saboteur = template(game, "GLAInfantrySaboteur")?;
    let actor = produce_type(game, barracks, saboteur, 0)?;

    let enemy_worker = friendly_entity(game, Kind::Builder, 1)?;
    crate::fake_building_verification::switch(game, enemy_worker, true)?;
    let fake = template(game, "FakeGLABarracks")?;
    let target = crate::fake_building_verification::isolated_fake(game, enemy_worker, fake)?;
    approach(game, actor, target, 65.0)?;
    require(
        game.catalog[fake].fake_building()
            && game.can_sabotage(actor, target)
            && game.entities[target].health.to_bits() == game.entities[target].max_health.to_bits(),
        "Saboteur lacks an eligible full-health paid fake target",
    )?;
    let destroyed = game.players[0].statistics.buildings_destroyed;
    let lost = game.players[1].statistics.buildings_lost;
    let saboteur_losses = game.players[0].statistics.units_lost;
    let enemy_kills = game.players[1].statistics.units_destroyed;
    game.enter(actor, target)?;
    let (frame, cash, module) = contact(game, actor, target)?;
    require(
        cash == 0
            && matches!(
                game.catalog[saboteur].behavior.sabotage[module].effect,
                Effect::FakeBuilding
            ),
        "Paid fake used the wrong Saboteur crate",
    )?;
    require(
        !game.entities[target].alive()
            && matches!(game.entities[target].death_type(), DeathType::Detonated)
            && game.entities[target].last_attacker() == Some(actor),
        "Fake did not take lethal DETONATED damage attributed to the paid Saboteur",
    )?;
    advance_until(game, 30, "sabotaged fake final destruction", |game| {
        !game.entities[target].exists()
    })?;
    // ScoreKeeper requires SCORE or SCORE_DESTROY. FakeGLABarracks has SCORE_CREATE.
    require(
        game.players[0].statistics.buildings_destroyed == destroyed
            && game.players[1].statistics.buildings_lost == lost
            && game.players[0].statistics.units_lost == saboteur_losses
            && game.players[1].statistics.units_destroyed == enemy_kills,
        "Fake sabotage incorrectly scored the fake or consumed Saboteur",
    )?;
    println!(
        "SABOTAGE_FAKE frame={frame} actor={actor} target={target} detonated=true destroyed=true"
    );
    Ok(())
}

fn prepare_scene(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let kind = template(game, "GLAInfantrySaboteur")?;
    let cash = game.players[0].cash;
    require(
        game.train(barracks, kind).is_err() && game.players[0].cash == cash,
        "Saboteur bypassed the original Arms Dealer prerequisite",
    )?;
    construct(game, worker, Kind::Factory, 0)?;
    let thief = produce_type(game, barracks, kind, 0)?;
    let actor = produce_type(game, barracks, kind, 0)?;
    let reserve = produce_type(game, barracks, kind, 0)?;
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy, Kind::PowerPlant, 1)?;
    let supply = construct(game, enemy, Kind::SupplyCenter, 1)?;
    let factory = construct(game, enemy, Kind::Barracks, 1)?;
    approach(game, thief, supply, 70.0)?;
    approach(game, actor, factory, 65.0)?;
    approach(game, reserve, factory, 110.0)?;
    require(
        [thief, actor, reserve]
            .iter()
            .all(|&id| game.entities[id].alive() && game.entities[id].stealthed()),
        "paid Saboteurs did not retain original innate concealment during approach",
    )?;
    require(
        !game.can_sabotage(actor, barracks)
            && game.can_sabotage(actor, factory)
            && game.can_sabotage(thief, supply),
        "original hostile sabotage target eligibility failed",
    )?;
    // Stop real collectors through their ordinary command before checking exact
    // contact-frame transfer amounts; no cash or entity state is injected.
    let collectors: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter_map(|(id, entity)| {
            (entity.complete() && game.catalog[entity.type_index].supply.capacity > 0).then_some(id)
        })
        .collect();
    for id in collectors {
        game.stop(id)?;
    }
    Ok(Setup {
        thief,
        actor,
        reserve,
        supply,
        factory,
        losses: game.players[0].statistics.units_lost,
        factory_health: game.entities[factory].health.to_bits(),
    })
}

fn approach(
    game: &mut Skirmish,
    actor: usize,
    target: usize,
    gap: f32,
) -> Result<(), Box<dyn Error>> {
    let origin = game.entities[actor].position;
    let center = game.entities[target].position;
    let delta = [origin[0] - center[0], origin[1] - center[1]];
    let length = delta[0].hypot(delta[1]);
    let radius = game.catalog[game.entities[target].type_index]
        .geometry_bounds
        .sphere_radius
        + gap;
    let point = [
        (delta[0] / length).mul_add(radius, center[0]),
        (delta[1] / length).mul_add(radius, center[1]),
    ];
    game.move_to(actor, point)?;
    advance_until(
        game,
        18_000,
        "paid Saboteur approaching enemy building",
        |game| {
            let current = game.entities[actor].position;
            (current[0] - point[0]).hypot(current[1] - point[1]) < 15.0
                && game.visible_to(0, target)
        },
    )?;
    game.stop(actor)?;
    require(
        game.entities[actor].alive(),
        "uncommanded building proximity consumed Saboteur",
    )
}

fn contact(
    game: &mut Skirmish,
    actor: usize,
    target: usize,
) -> Result<(u64, u32, usize), Box<dyn Error>> {
    require(
        matches!(game.entities[actor].ai.order, Order::Enter(id) if id == target),
        "Saboteur command did not enter its explicit goal building",
    )?;
    for _ in 0..3000 {
        let cash = [game.players[0].cash, game.players[1].cash];
        let earned = game.players[0].statistics.money_earned;
        game.step();
        if let Some(event) = game
            .presentation
            .sabotage_events
            .iter()
            .find(|event| event.source == actor && event.target == target)
        {
            if let Effect::SupplyCenter { amount } = game.catalog[game.entities[actor].type_index]
                .behavior
                .sabotage[event.module]
                .effect
            {
                require(
                    event.cash == amount.min(cash[1]) && event.cash > 0,
                    "supply sabotage transferred the wrong original bounded amount",
                )?;
                require(
                    game.players[0].cash == cash[0] + event.cash
                        && game.players[1].cash + event.cash == cash[1]
                        && game.players[0].statistics.money_earned == earned + event.cash,
                    "supply sabotage cash or earned-income accounting differs",
                )?;
            }
            require(
                game.entities[actor].masked() && !game.entities[actor].corpse_present(),
                "successful crate did not destroy Saboteur without a corpse",
            )?;
            return Ok((event.frame, event.cash, event.module));
        }
        require(
            game.entities[actor].alive(),
            "Saboteur died before explicit sabotage contact",
        )?;
    }
    Err("Saboteur did not reach its physical sabotage contact".into())
}

fn steal(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let health = game.entities[setup.supply].health.to_bits();
    game.enter(setup.thief, setup.supply)?;
    let (frame, cash, _) = contact(game, setup.thief, setup.supply)?;
    require(
        game.entities[setup.supply].health.to_bits() == health
            && game.players[0].statistics.units_lost == setup.losses,
        "supply theft caused invented target damage or Saboteur death loss",
    )?;
    println!(
        "SABOTAGE_CASH frame={frame} actor={} target={} stolen={cash} consumed=true",
        setup.thief, setup.supply
    );
    Ok(())
}

fn queue_factory(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let kind = template(game, "AmericaInfantryRanger")?;
    for _ in 0..3 {
        let cash = game.players[1].cash;
        game.train(setup.factory, kind)?;
        require(
            game.players[1].cash + game.catalog[kind].cost == cash,
            "victim barracks queue did not pay its original Ranger cost",
        )?;
    }
    Ok(())
}

fn factory_duration(game: &Skirmish, actor: usize) -> Result<u32, Box<dyn Error>> {
    game.catalog[game.entities[actor].type_index]
        .behavior
        .sabotage
        .iter()
        .find_map(|rules| {
            if let Effect::MilitaryFactory { frames } = rules.effect {
                Some(frames)
            } else {
                None
            }
        })
        .ok_or_else(|| "installed Saboteur lacks military-factory duration".into())
}

fn verify_disabled(game: &Skirmish, setup: &Setup) -> Result<u32, Box<dyn Error>> {
    require(
        game.entities[setup.factory].hacked()
            && game.entities[setup.factory].health.to_bits() == setup.factory_health,
        "factory sabotage failed its original disable without damage",
    )?;
    require(
        game.entities[setup.actor].masked()
            && !game.entities[setup.actor].corpse_present()
            && game.entities[setup.reserve].alive()
            && game.players[0].statistics.units_lost == setup.losses,
        "factory sabotage consumed the wrong unit or invoked death accounting",
    )?;
    game.entities[setup.factory]
        .queue
        .front()
        .map(|queue| queue.remaining_seconds.to_bits())
        .ok_or_else(|| "victim factory lost its paid production queue".into())
}

#[derive(Resource)]
struct Check {
    setup: Setup,
    cliff: CliffCapture,
    stage: u8,
    began: Option<f64>,
    command: Option<u64>,
    contact: Option<u64>,
    remaining: Option<u32>,
    shutdown_sound: String,
    heard: bool,
    diagnostic_saved: bool,
}

#[derive(Debug)]
enum CliffStage {
    Ready,
    Ascending,
    OnCliff,
    Saving,
    Finishing,
    Done,
}

struct CliffCapture {
    actor: usize,
    terrain: TerrainMap,
    foot: f32,
    stage: CliffStage,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = prepare_scene(&mut battle.simulation)?;
    let factory = friendly_entity(&battle.simulation, Kind::Factory, 0)?;
    let radar_van = template(&battle.simulation, "GLAVehicleRadarVan")?;
    produce_type(&mut battle.simulation, factory, radar_van, 0)?;
    require(
        battle.simulation.has_radar(0),
        "native radar provider did not execute original RadarUpgrade",
    )?;
    steal(&mut battle.simulation, &setup)?;
    // The headless theft is verified separately. Its retained callback would
    // otherwise first reach the client after switching to the factory's victim.
    advance_until(
        &mut battle.simulation,
        3,
        "retire preflight theft feedback",
        |game| game.presentation.sabotage_events.is_empty(),
    )?;
    let root = generals_assets::game_root()?;
    let terrain = cliff::terrain(&root)?;
    let barracks = friendly_entity(&battle.simulation, Kind::Barracks, 0)?;
    let saboteur = battle.simulation.entities[setup.reserve].type_index;
    let climber = produce_type(&mut battle.simulation, barracks, saboteur, 0)?;
    cliff::reach_foot(&mut battle.simulation, climber, &terrain)?;
    let foot = battle.simulation.object_position(climber)[2];
    battle.select_objects(&[climber], true);
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let files = [content::scan(assets.read("data/ini/miscaudio.ini")?)?];
    let declaration = crate::skirmish_data::definition(&files, "MiscAudio", "")?;
    let field = crate::skirmish_data::field(&declaration.lines, 0, "SabotageShutDownBuilding")
        .ok_or("missing native sabotage shutdown sound")?;
    let shutdown_sound = crate::skirmish_data::scalar(field)?.to_owned();
    app.insert_resource(Check {
        setup,
        cliff: CliffCapture {
            actor: climber,
            terrain,
            foot,
            stage: CliffStage::Ready,
        },
        stage: 0,
        began: None,
        command: None,
        contact: None,
        remaining: None,
        shutdown_sound,
        heard: false,
        diagnostic_saved: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let battle = app.world().resource::<Match>();
    let generation = battle.generation;
    let mut rig = app.world_mut().resource_mut::<CameraRig>();
    rig.focus = (Vec2::from_array(cliff::LOW) + Vec2::from_array(cliff::HIGH)) * 0.05;
    rig.generation = generation;
    app.add_systems(FixedUpdate, observe.after(crate::particles::step));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

fn observe(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let battle = battle.into_inner();
    if matches!(check.cliff.stage, CliffStage::Ascending) {
        let game = &battle.simulation;
        let id = check.cliff.actor;
        let actor = &game.entities[id];
        if actor.moving()
            && cliff::on_cliff(&check.cliff.terrain, actor.position)
            && game.object_position(id)[2] > check.cliff.foot + 5.0
        {
            time.pause();
            check.cliff.stage = CliffStage::OnCliff;
        }
    }
    if check.stage != 2 {
        return;
    }
    let game = &battle.simulation;
    if let Some(event) = game
        .presentation
        .sabotage_events
        .iter()
        .find(|event| event.source == check.setup.actor && event.target == check.setup.factory)
    {
        check.contact = Some(event.frame);
    }
    if check.contact.is_some() {
        if check.contact.is_some_and(|frame| game.tick >= frame + 12) {
            time.pause();
        }
        match verify_disabled(game, &check.setup) {
            Ok(remaining) if check.remaining.is_none_or(|previous| previous == remaining) => {
                check.remaining = Some(remaining);
            }
            Ok(_) => {
                error!("Saboteur native: hacked barracks advanced production");
                check.stage = 3;
                exit.write(AppExit::error());
            }
            Err(error) => {
                error!("Saboteur native: {error}");
                check.stage = 3;
                exit.write(AppExit::error());
            }
        }
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    rig: ResMut<'w, CameraRig>,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    meshes: Query<'w, 's, (&'static AnimatedPart, &'static ViewVisibility)>,
    audio: crate::source_audio::FiringAudioInspection<'w, 's>,
    radar: Query<
        'w,
        's,
        (
            &'static Visibility,
            Option<&'static CalculatedClip>,
            &'static ChildOf,
        ),
        With<crate::radar::EventEdge>,
    >,
    nodes: Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform)>,
    map: Single<
        'w,
        's,
        (&'static ComputedNode, &'static UiGlobalTransform),
        With<crate::radar::Display>,
    >,
    messages: Query<'w, 's, (&'static Text, &'static Node), With<crate::notifications::Row>>,
    ui: Res<'w, crate::control_bar::UiData>,
}

fn inspect_cliff(
    check: &mut Check,
    battle: &mut Match,
    time: &mut Time<Virtual>,
    capture: &mut Capture,
    view: &mut View,
) -> Result<(), Box<dyn Error>> {
    let id = check.cliff.actor;
    require(
        battle.simulation.entities[id].alive(),
        "paid Saboteur died during native cliff traversal",
    )?;
    match check.cliff.stage {
        CliffStage::Ready if capture.ready_for_scenario() && visible(view, battle, id) => {
            battle.simulation.move_to(id, cliff::HIGH)?;
            time.unpause();
            check.cliff.stage = CliffStage::Ascending;
        }
        CliffStage::OnCliff if capture.ready_for_scenario() && visible(view, battle, id) => {
            let position = battle.simulation.object_position(id);
            capture.intermediate("sabotage-cliff.png");
            check.cliff.stage = CliffStage::Saving;
            println!(
                "SABOTAGE_CLIFF_RENDERED actor={id} position={position:?} gained_height={} authored_cliff=true original_mesh_visible=true",
                position[2] - check.cliff.foot
            );
        }
        CliffStage::Saving if capture.ready_for_scenario() => {
            check.cliff.stage = CliffStage::Finishing;
            time.unpause();
        }
        CliffStage::Finishing
            if cliff::arrived(&battle.simulation, id, cliff::HIGH, &check.cliff.terrain) =>
        {
            time.pause();
            require(
                battle.simulation.object_position(id)[2] - check.cliff.foot > 10.0,
                "native Saboteur stopped before gaining the original cliff elevation",
            )?;
            let actor = Vec2::from_array(battle.simulation.entities[check.setup.actor].position);
            let target = Vec2::from_array(battle.simulation.entities[check.setup.factory].position);
            view.rig.focus = (actor + target) * 0.05;
            battle.select_objects(&[check.setup.actor], true);
            check.cliff.stage = CliffStage::Done;
            check.began = None;
        }
        _ => {}
    }
    Ok(())
}

fn visible(view: &View, battle: &Match, actor: usize) -> bool {
    view.visuals.iter().any(|(visual, children)| {
        visual.index == actor
            && visual.generation == battle.generation
            && children.iter().any(|child| {
                view.meshes
                    .get(child)
                    .is_ok_and(|(_, visible)| visible.get())
            })
    })
}

fn feedback_visible(view: &View) -> bool {
    let bounds = Rect::from_center_size(view.map.1.translation, view.map.0.size());
    let visible: Vec<_> = view
        .radar
        .iter()
        .filter(|(visibility, _, _)| **visibility == Visibility::Visible)
        .collect();
    !visible.is_empty()
        && visible
            .iter()
            .all(|(_, clip, _)| clip.is_some_and(|clip| clip.clip == bounds))
        && view
            .messages
            .iter()
            .filter(|(text, node)| {
                text.0 == view.ui.label("RADAR:Infiltration") && node.display != Display::None
            })
            .count()
            == 1
}

fn diagnose(view: &View, check: &Check, battle: &Match, capture: &Capture) {
    let bounds = Rect::from_center_size(view.map.1.translation, view.map.0.size());
    let edges: Vec<_> = view
        .radar
        .iter()
        .filter(|(visibility, _, _)| **visibility == Visibility::Visible)
        .collect();
    let matching = edges
        .iter()
        .filter(|(_, clip, _)| clip.is_some_and(|clip| clip.clip == bounds))
        .count();
    let clips: Vec<_> = edges.iter().take(6).map(|(_, clip, _)| *clip).collect();
    let parent = edges.first().and_then(|(_, _, parent)| {
        view.nodes
            .get(parent.parent())
            .ok()
            .map(|(node, transform)| Rect::from_center_size(transform.translation, node.size()))
    });
    let warnings = view
        .messages
        .iter()
        .filter(|(text, node)| {
            text.0 == view.ui.label("RADAR:Infiltration") && node.display != Display::None
        })
        .count();
    error!(
        "SABOTAGE_FEEDBACK_DIAGNOSTIC stage={} frame={} contact={:?} target_visible={} shutdown_heard={} capture_ready={} map_bounds={bounds:?} parent_bounds={parent:?} visible_edges={} matching_clips={matching} first_clips={clips:?} warning_count={warnings}",
        check.stage,
        battle.simulation.tick,
        check.contact,
        visible(view, battle, check.setup.factory),
        check.heard,
        capture.ready_for_scenario(),
        edges.len(),
    );
}

fn issue_context_command(
    check: &mut Check,
    battle: &mut Match,
    mouse: &mut Mouse,
) -> Result<(), Box<dyn Error>> {
    queue_factory(&mut battle.simulation, &check.setup)?;
    let target = check.setup.factory;
    require(
        input::context_order(battle, None, Some(PickTarget::Unit(target)), mouse),
        "native hostile-building context command rejected Saboteur",
    )?;
    require(
        matches!(battle.simulation.entities[check.setup.actor].ai.order, Order::Enter(id) if id == target),
        "native context command did not route through original enter goal",
    )?;
    check.command = Some(battle.simulation.tick);
    battle.observer_player = Some(1);
    check.stage = 2;
    Ok(())
}

fn inspect(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut view: View,
    mut exit: MessageWriter<AppExit>,
) {
    if check.stage == 3 {
        return;
    }
    let now = view.clock.elapsed_secs_f64();
    let began = *check.began.get_or_insert(now);
    if now - began >= 60.0 {
        diagnose(&view, &check, &battle, &capture);
        error!(
            "Saboteur native acceptance timed out at cliff stage {:?}, factory stage {}, frame {}, contact {:?}",
            check.cliff.stage, check.stage, battle.simulation.tick, check.contact
        );
        check.stage = 3;
        exit.write(AppExit::error());
        return;
    }
    if !matches!(check.cliff.stage, CliffStage::Done) {
        if let Err(error) =
            inspect_cliff(&mut check, &mut battle, &mut time, &mut capture, &mut view)
        {
            error!("Saboteur native cliff: {error}");
            check.stage = 3;
            exit.write(AppExit::error());
        }
        return;
    }
    let actor_visible = visible(&view, &battle, check.setup.actor);
    let reserve_visible = visible(&view, &battle, check.setup.reserve);
    let target_visible = visible(&view, &battle, check.setup.factory);
    let feedback_visible = feedback_visible(&view);
    if check.stage == 2
        && now - began >= 30.0
        && !check.diagnostic_saved
        && !feedback_visible
        && capture.ready_for_scenario()
    {
        diagnose(&view, &check, &battle, &capture);
        check.diagnostic_saved = true;
        // An intermediate readback cannot mark acceptance successful. Keep the
        // original guard and timeout while preserving the failed live frame.
        capture.intermediate("sabotage-feedback-diagnostic.png");
    }
    check.heard |= view.audio.object_fx(&check.shutdown_sound);
    if !capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        match check.stage {
            0 if actor_visible && reserve_visible && target_visible => {
                check.stage = 1;
                capture.intermediate("sabotage-before-command.png");
                println!(
                    "SABOTAGE_VISIBLE actor={} reserve={} target={} original_meshes=true",
                    check.setup.actor, check.setup.reserve, check.setup.factory
                );
            }
            1 => {
                issue_context_command(&mut check, &mut battle, &mut mouse)?;
                time.unpause();
            }
            2 => {
                if let Some(frame) = check.contact {
                    if battle.simulation.tick >= frame + 12
                        && target_visible
                        && check.heard
                        && feedback_visible
                    {
                        verify_disabled(&battle.simulation, &check.setup)?;
                        time.pause();
                        check.stage = 3;
                        battle.clear_drawable_selection();
                        capture.finish_scenario();
                        println!(
                            "SABOTAGE_RENDERED frame={frame} actor_consumed=true reserve_alive=true target_visible=true victim_view=true factory_disabled=true production_paused=true original_shutdown_audio_advancing=true radar_edges={} warning_visible=true",
                            view.radar
                                .iter()
                                .filter(|(visibility, _, _)| **visibility == Visibility::Visible)
                                .count()
                        );
                    }
                } else {
                    require(
                        battle.simulation.tick - check.command.unwrap_or(0) < 3000,
                        "native Saboteur contact failed to complete",
                    )?;
                }
            }
            _ => {}
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Saboteur rendered acceptance: {error}");
        check.stage = 3;
        exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_saboteur_contacts_preserve_theft_and_factory_duration() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_saboteur_detonates_fake_with_source_attribution() {
        let root = &generals_assets::game_root().unwrap();
        let mut game = crate::opponent_verification::load_game(root, super::MAP_NAME).unwrap();
        super::verify_fake_building(&mut game).unwrap();
    }
}
