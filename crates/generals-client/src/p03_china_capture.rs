//! Original China promotion buttons, delivered payloads and paid cannon warheads.
use crate::{
    capture_framing::{AircraftFrame, focus_on},
    china_power_verification::{cluster_target_point, prepare_cannon, scout_target_point},
    control_bar::{CommandAction, UiData},
    input::{self, CommandMode, Mouse, PickTarget},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{dispatch_command, selected_command},
    playtest_view::Capture,
    progression_verification::{earn_combat_rank, target_supply_center},
    projectile_render::ProjectileVisual,
    skirmish_verification::{friendly_entity, require},
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::{Faction, Kind as UnitKind, Skirmish};
use std::error::Error;

#[derive(Clone, Copy)]
pub enum Kind {
    Artillery,
    Cluster,
    Carpet,
    Emp,
    Frenzy,
    Cannon,
}

impl Kind {
    const fn promotion(self) -> Option<(u32, &'static str, &'static str)> {
        match self {
            Self::Artillery => Some((
                3,
                "SCIENCE_ArtilleryBarrage1",
                "SuperweaponArtilleryBarrage",
            )),
            Self::Cluster => Some((3, "SCIENCE_ClusterMines", "SuperweaponClusterMines")),
            Self::Carpet => Some((
                3,
                "Early_SCIENCE_ChinaCarpetBomb",
                "Early_SuperweaponChinaCarpetBomb",
            )),
            Self::Emp => Some((5, "SCIENCE_EMPPulse", "SuperweaponEMPPulse")),
            Self::Frenzy => Some((3, "SCIENCE_Frenzy1", "SuperweaponFrenzy")),
            Self::Cannon => None,
        }
    }

    const fn visual(self, phase: u8) -> &'static str {
        match self {
            Self::Artillery => "ChinaArtilleryBarrageShell",
            Self::Cluster => "ChinaClusterMine",
            Self::Carpet => "ChinaCarpetBomb",
            Self::Emp => "EMPPulseEffectSpheroid",
            Self::Frenzy => "FrenzyCloud",
            Self::Cannon if phase == 0 => "NukeCannonShell",
            Self::Cannon => "NeutronCannonShell",
        }
    }
}

#[derive(Resource)]
struct Check {
    kind: Kind,
    provider: usize,
    target: usize,
    point: [f32; 2],
    power: Option<&'static str>,
    phase: u8,
    issued: bool,
    started: Option<f64>,
    first_shot_at: Option<u64>,
    framed: Option<[f32; 3]>,
}

pub fn prepare(app: &mut App, battle: &mut Match, kind: Kind) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let (provider, target, point, power) = if let Some((rank, science, power)) = kind.promotion() {
        game.set_match_options(50_000, 1);
        game.reset(Faction::China, Faction::China)?;
        let (army, _) = earn_combat_rank(game, rank)?;
        game.purchase_science(0, science)?;
        let provider = friendly_entity(game, UnitKind::CommandCenter, 0)?;
        let (target, point) = if matches!(kind, Kind::Frenzy) {
            (army[0], game.entities[army[0]].position)
        } else {
            let target = target_supply_center(game)?;
            let point = if matches!(kind, Kind::Cluster) {
                cluster_target_point(game, target)?
            } else {
                game.entities[target].position
            };
            let scout = friendly_entity(game, UnitKind::Builder, 0)?;
            scout_target_point(game, scout, point)?;
            (target, point)
        };
        (provider, target, point, Some(power))
    } else {
        let (provider, target) = prepare_cannon(game)?;
        (provider, target, game.entities[target].position, None)
    };
    battle.select_objects(&[provider], true);
    app.insert_resource(Check {
        kind,
        provider,
        target,
        point,
        power,
        phase: 0,
        issued: false,
        started: None,
        first_shot_at: None,
        framed: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
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
    Ok(())
}

fn named_actors<'a>(
    game: &'a Skirmish,
    name: &'a str,
) -> impl DoubleEndedIterator<Item = usize> + 'a {
    game.entities
        .iter()
        .enumerate()
        .filter(move |(_, actor)| {
            actor.exists()
                && !actor.disabled_held()
                && !actor.drawable_effectively_hidden()
                && game.catalog[actor.type_index].name == name
        })
        .map(|(id, _)| id)
}

fn scene_actor(check: &Check, game: &Skirmish) -> Option<usize> {
    let mut actors = named_actors(game, check.kind.visual(check.phase));
    if matches!(check.kind, Kind::Carpet) {
        // Follow the first released bomb as it falls clear of the carrier.
        actors.find(|&id| {
            game.entities[id].owner == 0
                && game.entities[id].alive()
                && game.height_above_ground(id) > 0.0
        })
    } else {
        actors.next_back()
    }
}

fn scene_position(check: &Check, game: &Skirmish, subject: usize) -> [f32; 3] {
    let position = game.object_position(subject);
    if matches!(check.kind, Kind::Carpet)
        && let Some(carrier) = named_actors(game, "ChinaJetCarpetBomber")
            .find(|&id| game.entities[id].owner == 0 && game.entities[id].alive())
    {
        let carrier = game.object_position(carrier);
        return std::array::from_fn(|axis| f32::midpoint(position[axis], carrier[axis]));
    }
    position
}

fn frame(
    check: Res<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let game = &battle.into_inner().simulation;
    let center = check.framed.unwrap_or_else(|| {
        scene_actor(check, game).map_or_else(
            || {
                [
                    check.point[0],
                    check.point[1],
                    game.ground_height(check.point),
                ]
            },
            |subject| scene_position(check, game, subject),
        )
    });
    camera.distance = if matches!(check.kind, Kind::Artillery | Kind::Cannon) {
        camera.settings.min_height
    } else if matches!(check.kind, Kind::Carpet) {
        // Fit the narrow bomb without cropping its much larger original carrier.
        (camera.settings.max_height - camera.settings.min_height)
            .mul_add(-0.25, camera.settings.max_height)
    } else {
        camera.settings.max_height
    };
    focus_on(&mut camera, terrain.into_inner(), center);
}

#[derive(SystemParam)]
struct View<'w, 's> {
    frame: AircraftFrame<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    ui: Res<'w, UiData>,
    clock: Res<'w, Time<Real>>,
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
    check.framed = None;
    let now = view.clock.elapsed_secs_f64();
    let started = *check.started.get_or_insert(now);
    let result = require(
        now - started < 140.0,
        "native China power capture timed out",
    )
    .and_then(|()| observe(&mut check, view, &mut battle, &mut mouse, &mut time));
    match result {
        Ok(Some(position)) => {
            time.pause();
            check.framed = Some(position);
            check.started = None;
            println!(
                "Native P03 China {} at frame {}",
                check.kind.visual(check.phase),
                battle.simulation.tick
            );
            if matches!(check.kind, Kind::Cannon) && check.phase == 0 {
                check.phase = 1;
                check.issued = false;
                capture.intermediate("p03-china-cannon-nuclear.png");
            } else {
                check.phase = u8::MAX;
                capture.finish_scenario();
            }
        }
        Ok(None) => {}
        Err(error) => {
            error!(
                "Native P03 China {}: {error}",
                check.kind.visual(check.phase)
            );
            check.phase = u8::MAX;
            exit.write(AppExit::error());
        }
    }
}

fn observe(
    check: &mut Check,
    view: View,
    battle: &mut Match,
    mouse: &mut Mouse,
    time: &mut Time<Virtual>,
) -> Result<Option<[f32; 3]>, Box<dyn Error>> {
    let View {
        frame,
        particles,
        projectiles,
        ui,
        ..
    } = view;
    if !check.issued {
        issue(check, battle, &ui, mouse)?;
        check.issued = true;
        time.unpause();
        return Ok(None);
    }
    if let Some(power) = check.power {
        if battle
            .simulation
            .power_cooldown_remaining(check.provider, power)
            == 0
        {
            return Ok(None);
        }
    } else if battle.simulation.entities[check.provider].weapon_slot() != usize::from(check.phase) {
        return Ok(None);
    }
    let game = &battle.simulation;
    let visual = check.kind.visual(check.phase);
    if matches!(check.kind, Kind::Cannon)
        && check.first_shot_at.is_none()
        && game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == check.provider && shot.weapon_slot == 0)
    {
        check.first_shot_at = Some(game.tick);
    }
    if matches!(check.kind, Kind::Frenzy) {
        let visible = game.entities[check.target].frenzy_tint()
            && frame.visible(battle, check.target)?
            && particles.incendiary(visual, battle.generation)?;
        return Ok(visible.then(|| game.object_position(check.target)));
    }
    if matches!(check.kind, Kind::Artillery | Kind::Cannon) {
        for projectile in &game.projectiles {
            let Some(subject) = projectile.object else {
                continue;
            };
            if game.catalog[game.entities[subject].type_index].name != visual {
                continue;
            }
            if let Some((_, children)) = projectiles.iter().find(|(visual, _)| {
                visual.generation == battle.generation && visual.id == projectile.id
            }) && frame.isolated_projectile(battle, children)?.is_some()
            {
                if check.phase == 1 && matches!(check.kind, Kind::Cannon) {
                    require(
                        game.tick - check.first_shot_at.expect("first nuclear shot") >= 300,
                        "neutron shot bypassed the original shared reload",
                    )?;
                }
                return Ok(Some(game.object_position(subject)));
            }
        }
        return Ok(None);
    }
    if matches!(check.kind, Kind::Emp) && !game.entities[check.target].drawable_disabled_tint() {
        return Ok(None);
    }
    let Some(subject) = scene_actor(check, game) else {
        return Ok(None);
    };
    let position = scene_position(check, game, subject);
    let visible = if matches!(check.kind, Kind::Cluster) {
        // Friendly mines have zero body opacity. The original drop effect
        // remains visible while the deposited mines arm.
        particles.incendiary("ClusterMineDrop", battle.generation)?
    } else if matches!(check.kind, Kind::Carpet) {
        let Some(carrier) = named_actors(game, "ChinaJetCarpetBomber")
            .find(|&id| game.entities[id].owner == 0 && game.entities[id].alive())
        else {
            return Ok(None);
        };
        frame.visible(battle, carrier)? && frame.separated_actor(battle, carrier, subject)?
    } else {
        frame.visible(battle, subject)?
    };
    Ok(visible.then_some(position))
}

fn issue(
    check: &Check,
    battle: &mut Match,
    ui: &UiData,
    mouse: &mut Mouse,
) -> Result<(), Box<dyn Error>> {
    let point = Vec2::from_array(check.point);
    let mut commands = (1..=14).filter_map(|slot| selected_command(battle, ui, slot));
    if let Some(power) = check.power {
        let command = commands
            .find(|command| command.action.power() == Some(power))
            .ok_or("original promotion power button missing")?;
        dispatch_command(battle, mouse, command.action);
        require(
            matches!(mouse.pending_command, CommandMode::Power { caster, name, .. }
                if caster == check.provider && name == power),
            "original promotion power button did not enter targeting mode",
        )?;
        input::targeted_order(battle, mouse, point, None);
        return require(
            matches!(mouse.pending_command, CommandMode::Normal),
            "original promotion power target was rejected",
        );
    }
    let slot = usize::from(check.phase);
    let command = commands
        .find(|command| matches!(command.action, CommandAction::SwitchWeapon(mode) if mode == slot))
        .ok_or("original Nuke Cannon shell selection missing")?;
    dispatch_command(battle, mouse, command.action);
    require(
        input::context_order(
            battle,
            Some(point),
            Some(PickTarget::Unit(check.target)),
            mouse,
        ),
        "ordinary Nuke Cannon target was rejected",
    )
}
