//! Paid Patriot combat and independently framed original `MiG` wreck geometry.
use crate::{
    capture_framing::{AircraftFrame, focus_on},
    china_aircraft_verification::{self, crash},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::{
        advance_until, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::{Kind, Order, Skirmish, visibility::Status};
use std::error::Error;

#[derive(Resource)]
struct Check {
    aircraft: usize,
    defenders: [usize; 2],
    spotters: [usize; 2],
    began: Option<f64>,
    phase: Phase,
    engaged: bool,
    impact: Option<u64>,
    hulk: Option<usize>,
}

#[derive(PartialEq, Eq)]
enum Phase {
    Waiting,
    Attack,
    Falling,
    Impact,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let setup = china_aircraft_verification::prepare(game)?;
    let defenders = crash::prepare(game, &setup)?;
    let spotters = stage_spotters(game, defenders)?;
    let aircraft = setup.migs[0];
    battle.select_objects(&[aircraft], true);
    app.insert_resource(Check {
        aircraft,
        defenders,
        spotters,
        began: None,
        phase: Phase::Waiting,
        engaged: false,
        impact: None,
        hulk: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

fn stage_spotters(
    game: &mut Skirmish,
    defenders: [usize; 2],
) -> Result<[usize; 2], Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let factory = template(game, "ChinaWarFactory")?;
    let outpost = template(game, "ChinaVehicleListeningOutpost")?;
    let cost = game.catalog[factory].cost + 2 * game.catalog[outpost].cost;
    advance_until(
        game,
        18_000,
        "ordinary supply income funds MiG spotter",
        |game| game.players[0].cash >= cost,
    )?;
    let factory = construct_type(game, builder, factory, 0)?;
    let spotter = produce_type(game, factory, outpost, 0)?;
    let battery = Vec2::from_array(game.entities[defenders[0]].position);
    let threat_zones = defenders.map(|id| {
        let defender = &game.entities[id];
        let position = Vec2::from_array(defender.position);
        let range = game.catalog[defender.type_index].weapon.range;
        (position, range + 50.0)
    });

    // The ridge scout clears the battery. The MiG drifts back toward home
    // before its wreck settles, beyond that scout's original sight radius.
    let ridge = Vec2::new(1755.0, 1465.0);
    park_scout(game, spotter, ridge, threat_zones)?;
    require(
        matches!(game.visibility_cell(0, battery.to_array()), Status::Clear),
        "paid ridge scout did not clear the original Patriot",
    )?;
    let recovery = produce_type(game, factory, outpost, 0)?;
    park_scout(game, recovery, Vec2::new(1255.0, 1155.0), threat_zones)?;
    Ok([spotter, recovery])
}

fn park_scout(
    game: &mut Skirmish,
    spotter: usize,
    goal: Vec2,
    threat_zones: [(Vec2, f32); 2],
) -> Result<(), Box<dyn Error>> {
    game.move_to(spotter, goal.to_array())?;
    let mut clearance = f32::INFINITY;
    advance_until(
        game,
        3600,
        "paid MiG crash spotter reaches staging",
        |game| {
            let actor = &game.entities[spotter];
            let position = Vec2::from_array(actor.position);
            for (battery, safe_range) in threat_zones {
                clearance = clearance.min(battery.distance(position) - safe_range);
            }
            !actor.alive() || clearance <= 0.0 || matches!(actor.ai.order, Order::Idle)
        },
    )?;
    let actor = &game.entities[spotter];
    let position = Vec2::from_array(actor.position);
    require(
        actor.alive() && clearance > 0.0 && position.distance(goal) < 25.0,
        &format!(
            "paid MiG scout {spotter} failed standoff: goal={goal:?}, position={position:?}, health={}, attacker={:?}, minimum clearance={clearance}, Patriots={threat_zones:?}",
            actor.health,
            actor.last_attacker()
        ),
    )?;
    println!(
        "Native MiG spotter {spotter} reached {position:?} with minimum clearance {clearance}"
    );
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(
        Update,
        frame
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(FixedFirst, attack);
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
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
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let game = &battle.into_inner().simulation;
    camera.distance = if check.hulk.is_some() {
        camera.settings.min_height
    } else {
        camera.settings.max_height
    };
    focus_on(
        &mut camera,
        terrain.into_inner(),
        game.object_position(check.hulk.unwrap_or(check.aircraft)),
    );
}

fn attack(mut check: ResMut<Check>, mut battle: ResMut<Match>, mut exit: MessageWriter<AppExit>) {
    if check.phase == Phase::Attack
        && let Err(error) = crash::engage_visible(
            &mut battle.simulation,
            check.aircraft,
            check.defenders[0],
            &mut check.engaged,
        )
    {
        check.phase = Phase::Done;
        error!("Native MiG counterattack on visible Patriot: {error}");
        exit.write(AppExit::error());
    }
}

fn record(mut check: ResMut<Check>, battle: Res<Match>) {
    let game = &battle.into_inner().simulation;
    let id = check.aircraft;
    if let Some(event) = game
        .presentation
        .object_fx_events
        .iter()
        .find(|event| event.object == id && event.effect == "FX_JetDeathHitGround")
    {
        check.impact = Some(event.frame);
    }
    if check.impact.is_some() && check.hulk.is_none() {
        check.hulk = game.entities.iter().position(|actor| {
            actor.producer() == Some(id)
                && actor.physical_present()
                && game.catalog[actor.type_index].name == "ChinaJetMIGHulk"
        });
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    framing: AircraftFrame<'w, 's>,
    particles: ParticleInspection<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut capture: ResMut<Capture>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == Phase::Done || !capture.ready_for_scenario() {
        return;
    }
    if let Err(error) = view.advance(&mut check, &mut battle, &mut capture, &mut time) {
        check.phase = Phase::Done;
        error!("Native MiG crash: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn advance(
        self,
        check: &mut Check,
        battle: &mut Match,
        capture: &mut Capture,
        time: &mut Time<Virtual>,
    ) -> Result<(), Box<dyn Error>> {
        let now = self.clock.elapsed_secs_f64();
        let start = *check.began.get_or_insert(now);
        let game = &battle.simulation;
        let id = check.aircraft;
        let actor = &game.entities[id];
        require(
            now - start < 120.0,
            &format!(
                "MiG trial timed out: tick={} phase={:?} health={} attacker={:?} impact={:?} hulk={:?}; {}",
                game.tick,
                actor.aircraft_phase(),
                actor.health,
                actor.last_attacker(),
                check.impact,
                check.hulk,
                self.framing
                    .visibility_diagnostic(battle, check.hulk.unwrap_or(id))?
            ),
        )?;
        match check.phase {
            Phase::Waiting => {
                let target = game.entities[check.defenders[0]].position;
                battle.simulation.move_to(id, target)?;
                check.phase = Phase::Attack;
                time.unpause();
            }
            Phase::Attack if !actor.alive() => {
                require(
                    actor.jet_corpse()
                        && actor
                            .last_attacker()
                            .is_some_and(|attacker| check.defenders.contains(&attacker)),
                    "paid Patriot did not cause native airborne MiG death",
                )?;
                if check.impact.is_none()
                    && self.framing.visible(battle, id)?
                    && self.framing.physical_pose(battle, id)?
                    && self.particles.incendiary("JetBurning", battle.generation)?
                {
                    check.phase = Phase::Falling;
                    time.pause();
                    capture.intermediate("mig-crash-falling.png");
                }
            }
            Phase::Falling => {
                check.phase = Phase::Impact;
                time.unpause();
            }
            Phase::Impact => {
                if let (Some(hulk), Some(impact)) = (check.hulk, check.impact)
                    && game.tick.saturating_sub(impact) >= 67
                    && check.spotters.iter().all(|&id| game.entities[id].alive())
                    && matches!(game.visibility_status(0, hulk), Status::Clear)
                    && self.framing.visible(battle, hulk)?
                    && self.framing.physical_pose(battle, hulk)?
                {
                    require(
                        actor.masked() && game.entities[hulk].physical_present(),
                        "MiG hull did not come from final original physical death",
                    )?;
                    println!(
                        "Native MiG: real Patriot AA, separately framed falling body and physical hull {hulk}"
                    );
                    check.phase = Phase::Done;
                    time.pause();
                    capture.finish_scenario();
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_mig_capture_keeps_original_hulk_clear_at_capture_age() {
        let (_, mut battle, mut animations) =
            crate::cleanup_verification::tests::installed_with_animations();
        let game = &mut battle.simulation;
        animations.bind_logical_bones(&mut game.catalog);
        let setup = china_aircraft_verification::prepare(game).unwrap();
        let defenders = crash::prepare(game, &setup).unwrap();
        let spotters = stage_spotters(game, defenders).unwrap();
        assert!(spotters.iter().all(|&id| {
            game.catalog[game.entities[id].type_index].name == "ChinaVehicleListeningOutpost"
        }));
        let aircraft = setup.migs[0];
        let goal = game.entities[defenders[0]].position;
        game.move_to(aircraft, goal).unwrap();
        let mut engaged = false;
        advance_until(game, 9000, "paid MiG ground impact", |game| {
            if game.entities[aircraft].alive() {
                crash::engage_visible(game, aircraft, defenders[0], &mut engaged).unwrap();
            }
            game.presentation
                .object_fx_events
                .iter()
                .any(|event| event.object == aircraft && event.effect == "FX_JetDeathHitGround")
        })
        .unwrap();
        assert!(engaged);
        assert!(
            game.entities[aircraft]
                .last_attacker()
                .is_some_and(|attacker| defenders.contains(&attacker))
        );
        for _ in 0..67 {
            game.step();
        }
        let (hulk, wreck) = game
            .entities
            .iter()
            .enumerate()
            .find(|(_, actor)| {
                actor.producer() == Some(aircraft)
                    && game.catalog[actor.type_index].name == "ChinaJetMIGHulk"
            })
            .unwrap();
        assert!(wreck.exists() && wreck.physical_present() && wreck.has_drawable());
        assert!(spotters.iter().all(|&id| game.entities[id].alive()));
        assert!(
            matches!(game.visibility_status(0, hulk), Status::Clear),
            "original hulk at {:?} is obscured from both paid scouts at {:?}",
            wreck.position,
            spotters.map(|id| game.entities[id].position)
        );
    }
}
