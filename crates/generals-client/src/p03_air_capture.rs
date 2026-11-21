//! Paid original USA air support exercised through the command bar.
mod view;

use crate::{
    control_bar::UiData,
    input::{self, CommandMode, Mouse},
    playtest::{CameraRig, Match},
    playtest_ui::{dispatch_command, selected_command},
    playtest_view::Capture,
    progression_verification::earn_combat_rank,
    skirmish_verification::{advance_until, friendly_entity, produce, require},
    strategy_verification::{research, template},
    terrain::Terrain,
    usa_research_verification::{construct_strategy_center, scout_open_target},
};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use generals_sim::skirmish::{Faction, Kind as UnitKind, Skirmish};
use std::error::Error;

#[derive(Clone, Copy)]
pub enum Kind {
    A10,
    Paradrop,
    Daisy,
    Moab,
    Leaflet,
    Spectre,
}

struct Source {
    rank: u32,
    science: &'static [&'static str],
    power: &'static str,
    aircraft: &'static str,
    payload: &'static str,
    particle: Option<&'static str>,
}

impl Kind {
    const fn source(self) -> Source {
        match self {
            Self::A10 => Source {
                rank: 3,
                science: &[
                    "SCIENCE_A10ThunderboltMissileStrike1",
                    "SCIENCE_A10ThunderboltMissileStrike2",
                    "SCIENCE_A10ThunderboltMissileStrike3",
                ],
                power: "SuperweaponA10ThunderboltMissileStrike",
                aircraft: "AmericaJetA10Thunderbolt",
                payload: "A10ThunderboltMissile",
                particle: Some("CarpetBombExplosion"),
            },
            Self::Paradrop => Source {
                rank: 3,
                science: &[
                    "SCIENCE_Paradrop1",
                    "SCIENCE_Paradrop2",
                    "SCIENCE_Paradrop3",
                ],
                power: "SuperweaponParadropAmerica",
                aircraft: "AmericaJetCargoPlane",
                payload: "AmericaParachute",
                particle: None,
            },
            Self::Daisy => Source {
                rank: 5,
                science: &["SCIENCE_DaisyCutter"],
                power: "SuperweaponDaisyCutter",
                aircraft: "AmericaJetB52",
                payload: "DaisyCutterBomb",
                particle: Some("DaisyFlameShockwave"),
            },
            Self::Moab => Source {
                rank: 5,
                science: &["SCIENCE_DaisyCutter"],
                power: "SuperweaponDaisyCutter",
                aircraft: "AmericaJetB3",
                payload: "MOAB",
                particle: Some("MOABExplosionArms"),
            },
            Self::Leaflet => Source {
                rank: 5,
                science: &["SCIENCE_LeafletDrop"],
                power: "SuperweaponLeafletDrop",
                aircraft: "AmericaJetB52",
                payload: "LeafletContainer",
                particle: Some("LeafletParticles1"),
            },
            Self::Spectre => Source {
                rank: 5,
                science: &["SCIENCE_SpectreGunshipSolo"],
                power: "SuperweaponSpectreGunship",
                aircraft: "AmericaJetSpectreGunship",
                payload: "SpectreHowitzerShell",
                particle: Some("SpectreHowitzerExplosionCloud"),
            },
        }
    }

    const fn images(self) -> [&'static str; 4] {
        match self {
            Self::A10 => [
                "p03-a10-ready.png",
                "p03-a10-approach.png",
                "p03-a10-payload.png",
                "p03-a10-effect.png",
            ],
            Self::Paradrop => [
                "p03-paradrop-ready.png",
                "p03-paradrop-approach.png",
                "p03-paradrop-payload.png",
                "p03-paradrop-effect.png",
            ],
            Self::Daisy => [
                "p03-daisy-ready.png",
                "p03-daisy-approach.png",
                "p03-daisy-payload.png",
                "p03-daisy-effect.png",
            ],
            Self::Moab => [
                "p03-moab-ready.png",
                "p03-moab-approach.png",
                "p03-moab-payload.png",
                "p03-moab-effect.png",
            ],
            Self::Leaflet => [
                "p03-leaflet-ready.png",
                "p03-leaflet-approach.png",
                "p03-leaflet-payload.png",
                "p03-leaflet-effect.png",
            ],
            Self::Spectre => [
                "p03-spectre-ready.png",
                "p03-spectre-approach.png",
                "p03-spectre-payload.png",
                "p03-spectre-effect.png",
            ],
        }
    }
}

#[derive(Resource)]
pub struct Check {
    kind: Kind,
    source: Source,
    provider: usize,
    target: [f32; 3],
    leaflet_victim: Option<usize>,
    aircraft: usize,
    payload: usize,
    ranger: usize,
    first_created: usize,
    phase: Phase,
    framed: Option<Phase>,
    started: Option<f64>,
    issued: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Ready,
    Approach,
    Payload,
    Effect,
    Cleared,
    Finished,
}

impl Phase {
    const fn next(self) -> Self {
        match self {
            Self::Ready => Self::Approach,
            Self::Approach => Self::Payload,
            Self::Payload => Self::Effect,
            Self::Effect => Self::Cleared,
            Self::Cleared | Self::Finished => Self::Finished,
        }
    }

    const fn intermediate(self) -> Option<usize> {
        match self {
            Self::Ready => Some(0),
            Self::Approach => Some(1),
            Self::Payload => Some(2),
            Self::Effect => Some(3),
            Self::Cleared | Self::Finished => None,
        }
    }
}

pub fn prepare(app: &mut App, battle: &mut Match, kind: Kind) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let source = kind.source();
    if source.rank == 5 {
        game.set_match_options(50_000, 1);
    }
    game.reset(Faction::Usa, Faction::China)?;
    let (army, _) = earn_combat_rank(game, source.rank)?;
    for science in source.science {
        game.purchase_science(0, science)?;
    }
    if matches!(kind, Kind::Moab) {
        let strategy = construct_strategy_center(game)?;
        research(game, strategy, "Upgrade_AmericaMOAB")?;
        require(
            game.players[0].has_science("SCIENCE_MOAB"),
            "paid MOAB science missing",
        )?;
    }
    let provider = friendly_entity(game, UnitKind::CommandCenter, 0)?;
    let leaflet_victim = if matches!(kind, Kind::Leaflet) {
        let dozer = friendly_entity(game, UnitKind::Builder, 1)?;
        game.stop(dozer)?;
        Some(dozer)
    } else {
        None
    };
    let [x, y] = match kind {
        Kind::Leaflet | Kind::Spectre => {
            let subject = leaflet_victim.map_or_else(|| spectre_target(game), Ok)?;
            enemy_air_support_point(game, army[0], subject)?
        }
        _ => scout_open_target(game, provider, army[0])?,
    };
    let target = [x, y, game.ground_height([x, y])];
    let aircraft = template(game, source.aircraft)?;
    let payload = template(game, source.payload)?;
    let ranger = template(game, "AmericaInfantryRanger")?;
    advance_until(game, 10_801, "original USA air support recharge", |game| {
        game.can_use_power_at_location(provider, source.power, target)
    })?;
    let first_created = game.entities.len();
    battle.select_objects(&[provider], true);
    app.insert_resource(Check {
        kind,
        source,
        provider,
        target,
        leaflet_victim,
        aircraft,
        payload,
        ranger,
        first_created,
        phase: Phase::Ready,
        framed: None,
        started: None,
        issued: false,
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
        view::inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
    Ok(())
}

fn spectre_target(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    let friendly = friendly_entity(game, UnitKind::CommandCenter, 0)?;
    let enemy = friendly_entity(game, UnitKind::CommandCenter, 1)?;
    let center: [f32; 2] = std::array::from_fn(|axis| {
        f32::midpoint(
            game.entities[friendly].position[axis],
            game.entities[enemy].position[axis],
        )
    });
    // Use an ordinary paid ground target; the Command Center's projected bounds
    // overlap the native shell even when its entire mesh fits the camera.
    let target = produce(game, enemy, UnitKind::Builder, 1)?;
    let point = spectre_target_site(game, target, center)
        .ok_or("no reachable open Spectre target position on the original map")?;
    game.move_to(target, point)?;
    advance_until(
        game,
        10_800,
        "paid Spectre target reaching open ground",
        |game| {
            let actor = &game.entities[target];
            !actor.alive()
                || ((actor.position[0] - point[0]).hypot(actor.position[1] - point[1]) < 10.0
                    && !actor.moving()
                    && matches!(actor.ai.order, generals_sim::skirmish::Order::Idle))
        },
    )?;
    require(
        game.entities[target].alive(),
        "paid Spectre target died during staging",
    )?;
    game.stop(target)?;
    Ok(target)
}

fn spectre_target_site(game: &Skirmish, target: usize, center: [f32; 2]) -> Option<[f32; 2]> {
    let buildings: Vec<_> = game
        .entities
        .iter()
        .filter(|actor| actor.alive() && game.catalog[actor.type_index].kind.is_building())
        .map(|actor| actor.position)
        .collect();
    let extent = game
        .visibility_dimensions()
        .map(|cells| f32::from(cells) * game.visibility_cell_size());
    let axis = |limit| {
        std::iter::successors(Some(generals_sim::skirmish::CELL_SIZE * 0.5), |point| {
            Some(point + generals_sim::skirmish::CELL_SIZE)
        })
        .take_while(move |&point| point < limit)
    };
    let mut sites: Vec<_> = axis(extent[1])
        .flat_map(|y| axis(extent[0]).map(move |x| [x, y]))
        .collect();
    let distance = |point: [f32; 2]| (point[0] - center[0]).hypot(point[1] - center[1]);
    sites.sort_by(|&first, &second| distance(first).total_cmp(&distance(second)));
    sites.into_iter().find(|&point| {
        game.command_point_on_map(point)
            // Reserve the <10-unit arrival tolerance so the stopped target
            // retains the original >300-unit clearance from building centers.
            && buildings.iter().all(|position| {
                (point[0] - position[0]).hypot(point[1] - position[1]) > 310.0
            })
            // This is the native terrain-zone preview. Actual arrival
            // still requires the ordinary Move to complete at this site.
            && game.build_site_reachable(target, point)
    })
}

fn enemy_air_support_point(
    game: &mut Skirmish,
    scout: usize,
    subject: usize,
) -> Result<[f32; 2], Box<dyn Error>> {
    let point = game.entities[subject].position;
    let actor = &game.entities[scout];
    let unit = &game.catalog[actor.type_index];
    // Keep the paid Crusader within its original shroud-clearing range, but
    // beyond its shorter sight and gun range so it does not shoot the Dozer.
    let distance = f32::midpoint(actor.vision_range(unit), actor.shroud_range(unit));
    let offset = Vec2::from_array(actor.position) - Vec2::from_array(point);
    let approach = (Vec2::from_array(point) + offset.normalize_or_zero() * distance).to_array();
    game.move_to(scout, approach)?;
    advance_until(
        game,
        5400,
        "paid Crusader scouting enemy air support target",
        |game| game.entities[subject].alive() && game.visible_to(0, subject),
    )?;
    game.stop(scout)?;
    Ok(point)
}

impl Check {
    fn spawned<'a>(
        &self,
        game: &'a Skirmish,
        kind: usize,
    ) -> impl Iterator<Item = usize> + use<'a> {
        game.entities
            .iter()
            .enumerate()
            .skip(self.first_created)
            .filter(move |(_, actor)| actor.owner == 0 && actor.type_index == kind)
            .map(|(id, _)| id)
    }

    fn live_actor(&self, game: &Skirmish, kind: usize) -> Option<usize> {
        self.spawned(game, kind)
            .find(|&id| game.entities[id].exists() && game.entities[id].contained_by().is_none())
    }

    fn airborne_payload(&self, game: &Skirmish) -> Option<usize> {
        self.spawned(game, self.payload).find(|&id| {
            let actor = &game.entities[id];
            actor.alive()
                && actor.contained_by().is_none()
                && game.height_above_ground(id) > 5.0
                && (!matches!(self.kind, Kind::Paradrop) || actor.parachute_opened())
        })
    }

    fn projectile<'a>(&self, game: &'a Skirmish) -> Option<&'a generals_sim::skirmish::Projectile> {
        game.projectiles.iter().find(|projectile| {
            projectile.owner == 0
                && projectile.source >= self.first_created
                && game.entities[projectile.source].type_index == self.aircraft
                && projectile
                    .object
                    .is_some_and(|id| game.entities[id].type_index == self.payload)
        })
    }

    fn landed_ranger(&self, game: &Skirmish) -> Option<usize> {
        self.spawned(game, self.ranger).find(|&id| {
            game.entities[id].alive()
                && game.entities[id].contained_by().is_none()
                && !game.entities[id].parachute_rider()
                && game.height_above_ground(id) < 2.0
        })
    }
}

#[derive(SystemParam)]
struct ProjectileGeometry<'w, 's> {
    meshes: Res<'w, Assets<Mesh>>,
    roots: Query<
        'w,
        's,
        (
            &'static crate::projectile_render::ProjectileVisual,
            &'static Children,
        ),
    >,
    parts: Query<'w, 's, (&'static Mesh3d, &'static Transform)>,
}

impl ProjectileGeometry<'_, '_> {
    fn center(
        self,
        battle: &Match,
        projectile: &generals_sim::skirmish::Projectile,
    ) -> Option<[f32; 3]> {
        let Self {
            meshes,
            roots,
            parts,
        } = self;
        let meshes = meshes.into_inner();
        let (_, children) = roots.iter().find(|(visual, _)| {
            visual.generation == battle.generation && visual.id == projectile.id
        })?;
        // Camera framing runs before projectile_render updates this frame's root.
        let (position, axes) = projectile.render_transform(&battle.simulation.entities)?;
        let convert = |point: [f32; 3]| Vec3::new(point[0], point[2], -point[1]);
        let rotation = Quat::from_mat3(&Mat3::from_cols(
            convert(axes[0]),
            convert(axes[2]),
            -convert(axes[1]),
        ));
        let mut root = Transform {
            translation: convert(position) * 0.1,
            rotation,
            ..default()
        };
        if let Some(instance) = projectile
            .object
            .and_then(|id| battle.simulation.entities[id].drawable_instance_matrix())
        {
            root.translation += rotation * convert(instance.map(|row| row[3])) * 0.1;
        }
        let mut bounds: Option<(Vec3, Vec3)> = None;
        for child in children {
            let (mesh, local) = parts.get(*child).ok()?;
            let VertexAttributeValues::Float32x3(vertices) =
                meshes.get(&mesh.0)?.attribute(Mesh::ATTRIBUTE_POSITION)?
            else {
                return None;
            };
            for vertex in vertices {
                let point = root.transform_point(local.transform_point(Vec3::from_array(*vertex)));
                let (minimum, maximum) = bounds.get_or_insert((point, point));
                *minimum = minimum.min(point);
                *maximum = maximum.max(point);
            }
        }
        let (minimum, maximum) = bounds?;
        let center = Vec3::from_array(std::array::from_fn(|axis| {
            f32::midpoint(minimum[axis], maximum[axis])
        })) * 10.0;
        Some([center.x, -center.z, center.y])
    }
}

fn frame(
    check: ResMut<Check>,
    capture: Res<Capture>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
    geometry: ProjectileGeometry,
) {
    let capture = capture.into_inner();
    if !capture.ready_for_scenario() {
        return;
    }
    let check = check.into_inner();
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    let game = &battle.simulation;
    let actor = match check.phase {
        Phase::Ready => Some(check.provider),
        Phase::Approach => check.live_actor(game, check.aircraft),
        Phase::Payload => check.airborne_payload(game),
        Phase::Effect if matches!(check.kind, Kind::Paradrop) => check.landed_ranger(game),
        Phase::Effect if matches!(check.kind, Kind::Leaflet) => check.leaflet_victim,
        _ => None,
    };
    let point = if matches!(check.kind, Kind::A10 | Kind::Spectre) && check.phase == Phase::Payload
    {
        check.projectile(game).and_then(|projectile| {
            if matches!(check.kind, Kind::Spectre) {
                // The original shell's hierarchy places its long mesh ahead of the logic origin.
                geometry.center(battle, projectile)
            } else {
                projectile
                    .render_transform(&game.entities)
                    .map(|(position, _)| position)
            }
        })
    } else {
        actor.map(|id| game.object_position(id))
    };
    camera.distance = match check.phase {
        Phase::Payload if matches!(check.kind, Kind::A10) => camera.settings.min_height,
        Phase::Effect if matches!(check.kind, Kind::Paradrop) => camera.settings.min_height,
        // The cargo and bomber wingspans need the stock wide view while
        // tracking their preferred flight height. Keep the smaller A-10 close.
        Phase::Approach if matches!(check.kind, Kind::A10) => {
            (camera.settings.min_height * 1.5).min(camera.settings.max_height)
        }
        _ => camera.settings.max_height,
    };
    crate::capture_framing::focus_on(&mut camera, terrain, point.unwrap_or(check.target));
    check.framed = Some(check.phase);
}

fn issue(
    check: &mut Check,
    battle: &mut Match,
    ui: &UiData,
    mouse: &mut Mouse,
) -> Result<(), Box<dyn Error>> {
    let action = (1..=14)
        .filter_map(|slot| selected_command(battle, ui, slot))
        .find(|command| command.action.power() == Some(check.source.power))
        .ok_or("original USA air support button missing")?
        .action;
    dispatch_command(battle, mouse, action);
    require(
        matches!(mouse.pending_command, CommandMode::Power { caster, name, .. }
            if caster == check.provider && name == check.source.power),
        "original USA air support button did not enter targeting mode",
    )?;
    input::targeted_order(
        battle,
        mouse,
        Vec2::from_array([check.target[0], check.target[1]]),
        None,
    );
    require(
        matches!(mouse.pending_command, CommandMode::Normal),
        "original USA air support target was rejected",
    )?;
    check.issued = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_air_support_camera_initializes_without_resource_conflicts() {
        use bevy::{ecs::system::System, prelude::*};
        let mut frame = IntoSystem::into_system(super::frame);
        frame.initialize(&mut World::new());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn earned_rank_spectre_target_reaches_clear_original_map_ground_as_paid_enemy_dozer() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        game.set_match_options(50_000, 1);
        game.reset(Faction::Usa, Faction::China).unwrap();
        earn_combat_rank(game, 5).unwrap();
        let before = game.entities.len();
        let target = spectre_target(game).unwrap();
        let actor = &game.entities[target];
        assert!(target >= before);
        assert_eq!(game.catalog[actor.type_index].name, "ChinaVehicleDozer");
        assert_eq!(actor.owner, 1);
        assert!(actor.alive() && actor.complete());
        assert!(!actor.moving());
        assert!(matches!(
            actor.ai.order,
            generals_sim::skirmish::Order::Idle
        ));
        assert!(game.command_point_on_map(actor.position));
        assert!(game.entities.iter().all(|building| {
            !building.alive()
                || !game.catalog[building.type_index].kind.is_building()
                || (actor.position[0] - building.position[0])
                    .hypot(actor.position[1] - building.position[1])
                    > 300.0
        }));
    }
}
