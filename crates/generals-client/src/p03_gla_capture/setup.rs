use super::{Kind, Scene};
use crate::{
    p03_capture::gla::gla_technology,
    progression_verification::earn_combat_rank,
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    },
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{Kind as ObjectKind, Skirmish, visibility::Status};
use std::error::Error;

const TRAP_UPGRADE: &str = "Upgrade_GLAInfantryRebelBoobyTrapAttack";
const RADAR_UPGRADE: &str = "Upgrade_GLARadarVanScan";

pub(super) fn prepare(game: &mut Skirmish, kind: Kind) -> Result<Scene, Box<dyn Error>> {
    let scene = match kind {
        Kind::Radar => radar(game)?,
        Kind::Booby => booby(game)?,
        _ => earned(game, kind)?,
    };
    advance_until(game, 10_801, "GLA power recharging", |game| {
        game.can_use_power(scene.provider, kind.power())
    })?;
    Ok(scene)
}

fn earned(game: &mut Skirmish, kind: Kind) -> Result<Scene, Box<dyn Error>> {
    let (rank, sciences): (_, &[_]) = match kind {
        Kind::Ambush => (
            3,
            &[
                "SCIENCE_RebelAmbush1",
                "SCIENCE_RebelAmbush2",
                "SCIENCE_RebelAmbush3",
            ],
        ),
        Kind::Gps => (5, &["SCIENCE_GPSScrambler"]),
        Kind::Sneak => (5, &["SCIENCE_SneakAttack"]),
        Kind::Anthrax => (5, &["SCIENCE_AnthraxBomb"]),
        Kind::Radar | Kind::Booby => unreachable!("paid research uses its own provider"),
    };
    let (army, _) = earn_combat_rank(game, rank)?;
    for science in sciences {
        game.purchase_science(0, science)?;
    }
    let provider = friendly_entity(game, ObjectKind::CommandCenter, 0)?;
    let subject = army[0];
    let mut target = game.object_position(subject);
    match kind {
        Kind::Sneak => {
            let worker = friendly_entity(game, ObjectKind::Builder, 0)?;
            construct(game, worker, ObjectKind::Barracks, 0)?;
            let tunnel = construct_type(game, worker, template(game, "GLATunnelNetwork")?, 0)?;
            game.enter(subject, tunnel)?;
            advance_until(game, 6000, "tank enters its paid tunnel", |game| {
                game.entities[subject].contained_by() == Some(tunnel)
            })?;
            target = sneak_site(game, provider, worker, tunnel, 0.0)?;
        }
        Kind::Anthrax => {
            let home = game.entities[provider].position;
            let point = [home[0] + 135.0, home[1] + 75.0];
            target = [point[0], point[1], game.ground_height(point)];
        }
        _ => {}
    }
    if matches!(
        game.visibility_cell(0, [target[0], target[1]]),
        Status::Shrouded
    ) {
        let worker = friendly_entity(game, ObjectKind::Builder, 0)?;
        let point = [target[0], target[1]];
        game.move_to(worker, point)?;
        advance_until(
            game,
            7200,
            "Worker scouts the original power target",
            |game| !matches!(game.visibility_cell(0, point), Status::Shrouded),
        )?;
        game.move_to(worker, game.entities[provider].position)?;
    }
    Ok(Scene {
        provider,
        target,
        subject,
        extra: None,
    })
}

pub fn sneak_site(
    game: &mut Skirmish,
    provider: usize,
    scout: usize,
    tunnel: usize,
    rotation: f32,
) -> Result<[f32; 3], Box<dyn Error>> {
    use generals_sim::skirmish::PlacementFailure;

    let kind = template(game, "GLASneakAttackTunnelNetwork")?;
    let default = game.catalog[kind].placement_angle;
    let selected = default + rotation;
    let origin = game.entities[tunnel].position;
    let distance = |[x, y]: [f32; 2]| (x - origin[0]).hypot(y - origin[1]);
    let size = game.visibility_cell_size();
    let [width, height] = game.visibility_dimensions();
    let coordinate = |axis| f32::from(axis).mul_add(size, size * 0.5);
    let mut candidates: Vec<_> = (0..height)
        .flat_map(|y| (0..width).map(move |x| [coordinate(x), coordinate(y)]))
        .filter(|&point| (95.0..=500.0).contains(&distance(point)))
        .collect();
    candidates.sort_by(|&left, &right| distance(left).total_cmp(&distance(right)));
    let mut failures = [0; 7];
    let mut scouted = 0;
    for [x, y] in candidates {
        let world = [x, y, game.ground_height([x, y])];
        let failure = |game: &Skirmish| {
            game.sneak_attack_placement_failure(provider, kind, world, selected, true)
                .or_else(|| {
                    game.sneak_attack_placement_failure(provider, kind, world, default, false)
                })
        };
        match failure(game) {
            None => return Ok(world),
            Some(PlacementFailure::Shroud)
                if scouted < 8 && game.build_site_reachable(scout, [x, y]) =>
            {
                scouted += 1;
                game.move_to(scout, [x, y])?;
                if advance_until(
                    game,
                    900,
                    "Worker reveals the Sneak Attack footprint",
                    |game| !matches!(failure(game), Some(PlacementFailure::Shroud)),
                )
                .is_ok()
                    && failure(game).is_none()
                {
                    game.stop(scout)?;
                    return Ok(world);
                }
            }
            Some(failure) => {
                failures[match failure {
                    PlacementFailure::Generic => 0,
                    PlacementFailure::RestrictedTerrain => 1,
                    PlacementFailure::NotFlatEnough => 2,
                    PlacementFailure::ObjectsInTheWay => 3,
                    PlacementFailure::TooCloseToSupplies => 4,
                    PlacementFailure::NoClearPath => 5,
                    PlacementFailure::Shroud => 6,
                }] += 1;
            }
        }
    }
    Err(format!("no clear reachable Sneak Attack exit near paid Tunnel {origin:?}; generic/restricted/slope/objects/supply/path/shroud={failures:?}, scout attempts={scouted}").into())
}

pub fn paid_radar(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let (worker, _, factory, _) = gla_technology(game)?;
    let market = construct_type(game, worker, template(game, "GLABlackMarket")?, 0)?;
    let van_type = template(game, "GLAVehicleRadarVan")?;
    let van = produce_type(game, factory, van_type, 0)?;
    assert!(!game.can_use_power(van, Kind::Radar.power()));
    research(game, market, RADAR_UPGRADE)?;
    let future = produce_type(game, factory, van_type, 0)?;
    Ok((van, future))
}

fn radar(game: &mut Skirmish) -> Result<Scene, Box<dyn Error>> {
    let (provider, _) = paid_radar(game)?;
    let subject = friendly_entity(game, ObjectKind::CommandCenter, 1)?;
    let target = game.object_position(subject);
    Ok(Scene {
        provider,
        target,
        subject,
        extra: None,
    })
}

fn booby(game: &mut Skirmish) -> Result<Scene, Box<dyn Error>> {
    let (_, barracks, factory, _) = gla_technology(game)?;
    let rebel = template(game, "GLAInfantryRebel")?;
    let provider = produce_type(game, barracks, rebel, 0)?;
    research(game, barracks, TRAP_UPGRADE)?;

    let dozer = friendly_entity(game, ObjectKind::Builder, 1)?;
    construct(game, dozer, ObjectKind::PowerPlant, 1)?;
    let enemy_barracks = construct(game, dozer, ObjectKind::Barracks, 1)?;
    research(game, enemy_barracks, "Upgrade_InfantryCaptureBuilding")?;
    let red_guard = template(game, "ChinaInfantryRedguard")?;
    let attacker = produce_type(game, enemy_barracks, red_guard, 1)?;
    let target = game.object_position(factory);
    Ok(Scene {
        provider,
        target,
        subject: factory,
        extra: Some(attacker),
    })
}
