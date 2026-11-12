use crate::skirmish_verification::{
    advance_until, construct, construct_type, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{CanAttackResult, Faction, Kind, Skirmish};
use std::error::Error;

pub(super) struct Setup {
    pub bridge: usize,
    pub hydrant: usize,
    pub hydrant_tank: usize,
    pub tomahawk: usize,
    pub layer: u8,
    pub target: [f32; 3],
    pub bridge_fire: [f32; 3],
}

pub(super) fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let site = prepare_site(game)?;
    move_and_wait(
        game,
        site.tomahawk,
        site.bridge_fire,
        "paid bridge scouting",
    )?;
    require(
        matches!(
            game.visibility_status(0, site.bridge),
            generals_sim::skirmish::visibility::Status::Clear
                | generals_sim::skirmish::visibility::Status::Partial
        ),
        "paid bridge scouting did not reveal the original landmark",
    )?;
    Ok(site)
}

fn prepare_site(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset_sandbox(Faction::Usa)?;
    let bridge = game
        .entities
        .iter()
        .position(|actor| {
            actor.alive() && game.catalog[actor.type_index].name == "MonumentTrainBridge"
        })
        .ok_or("original Killing Fields MonumentTrainBridge is missing")?;
    require(
        game.entities[bridge].health.to_bits() == 2000.0_f32.to_bits(),
        "original bridge health changed",
    )?;
    let center = game.object_position(bridge);
    let (layer, name, _) = game
        .radar_bridge_at([center[0], center[1]])
        .ok_or("original train bridge has no native bridge binding")?;
    require(
        name == "MonumentTrainBridge",
        "picked bridge is not original train landmark",
    )?;
    let target = game
        .pick_bridge(
            [center[0], center[1], center[2] + 1000.0],
            [center[0], center[1], center[2] - 1000.0],
        )
        .ok_or("native bridge surface pick failed")?;
    let hydrant = original_hydrant(game, center)?;
    require(
        !game
            .entities
            .iter()
            .any(|actor| actor.exists() && game.catalog[actor.type_index].name == "WaterSpray"),
        "original capture starts with unobserved WaterSpray",
    )?;
    let (tomahawk, hydrant_tank) = paid_units(game)?;
    let hydrant_point = game.entities[hydrant].position;
    let hydrant_fire = firing_site(game, hydrant_tank, hydrant_point, 70.0)?;
    move_and_wait(
        game,
        hydrant_tank,
        hydrant_fire,
        "paid Crusader hydrant staging",
    )?;
    let bridge_fire = firing_site(game, tomahawk, [center[0], center[1]], 250.0)?;
    validate_bridge_attack(game, tomahawk, bridge, target)?;
    Ok(Setup {
        bridge,
        hydrant,
        hydrant_tank,
        tomahawk,
        layer,
        target,
        bridge_fire,
    })
}

fn original_hydrant(game: &Skirmish, center: [f32; 3]) -> Result<usize, Box<dyn Error>> {
    game.entities
        .iter()
        .enumerate()
        .filter(|(_, actor)| {
            actor.alive() && game.catalog[actor.type_index].name == "FireHydrantRed"
        })
        .min_by(|(_, a), (_, b)| {
            squared(a.position, [center[0], center[1]])
                .total_cmp(&squared(b.position, [center[0], center[1]]))
        })
        .map(|(id, _)| id)
        .ok_or_else(|| "original map has no FireHydrantRed".into())
}

fn validate_bridge_attack(
    game: &mut Skirmish,
    tomahawk: usize,
    bridge: usize,
    target: [f32; 3],
) -> Result<(), Box<dyn Error>> {
    let weapon =
        game.entities[tomahawk].active_weapon(&game.catalog[game.entities[tomahawk].type_index]);
    let area = weapon
        .area
        .as_ref()
        .ok_or("original Tomahawk has no actual area damage")?;
    require(
        weapon.name == "TomahawkMissileWeapon" && area.primary_radius > 0.0,
        "paid original weapon lacks force-ground area damage",
    )?;
    let final_damage = weapon.damage
        * game.catalog[game.entities[bridge].type_index].armor[weapon.damage_type as usize];
    require(
        final_damage > 0.0,
        "source bridge armor rejects original area weapon",
    )?;
    require(
        matches!(
            game.force_attack_result(tomahawk, Some(bridge), target),
            CanAttackResult::NotPossible
        ),
        "UNATTACKABLE original bridge incorrectly admits direct target",
    )?;
    Ok(())
}

fn paid_units(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let strategy = kind(game, "AmericaStrategyCenter")?;
    construct_type(game, builder, strategy, 0)?;
    let crusader = kind(game, "AmericaTankCrusader")?;
    let tomahawk = kind(game, "AmericaVehicleTomahawk")?;
    let tomahawk = produce_type(game, factory, tomahawk, 0)?;
    let hydrant_tank = produce_type(game, factory, crusader, 0)?;
    require(game.has_radar(0), "paid USA base has no native radar")?;
    Ok((tomahawk, hydrant_tank))
}

fn kind(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    game.catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or_else(|| format!("missing original {name}").into())
}

pub(super) fn floor(game: &Skirmish, point: [f32; 2]) -> [f32; 3] {
    [point[0], point[1], game.ground_height(point)]
}

fn firing_site(
    game: &mut Skirmish,
    object: usize,
    target: [f32; 2],
    distance: f32,
) -> Result<[f32; 3], Box<dyn Error>> {
    game.materialize_selection_group(game.entities[object].owner, &[object]);
    for offset in [
        [distance, 0.0],
        [-distance, 0.0],
        [0.0, distance],
        [0.0, -distance],
    ] {
        let point = floor(game, [target[0] + offset[0], target[1] + offset[1]]);
        if game.move_to_world(object, point).is_ok() && exact_goal(game, object, point) {
            game.stop(object)?;
            return Ok(point);
        }
    }
    Err("original map has no unadjusted paid unit firing site".into())
}

fn move_and_wait(
    game: &mut Skirmish,
    object: usize,
    point: [f32; 3],
    label: &str,
) -> Result<(), Box<dyn Error>> {
    game.materialize_selection_group(game.entities[object].owner, &[object]);
    game.move_to_world(object, point)?;
    if !exact_goal(game, object, point) {
        return Err(format!(
            "{label}: ordinary staging goal {point:?} became {:?}; {}",
            game.entities[object].ai_goal_position(),
            generals_sim::skirmish::diagnostics::movement_details(game, object),
        )
        .into());
    }
    advance_until(game, 18000, label, |game| {
        squared(game.entities[object].position, [point[0], point[1]]) < 25.0 * 25.0
    })?;
    game.stop(object)?;
    Ok(())
}

pub(super) fn exact_goal(game: &Skirmish, object: usize, point: [f32; 3]) -> bool {
    game.entities[object].ai_goal_position().map(f32::to_bits) == point.map(f32::to_bits)
}

pub(super) fn squared(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let xx = dx * dx;
    let yy = dy * dy;
    xx + yy
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_sim::skirmish::{Order, visibility::Status};

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_bridge_scouting_reveals_before_the_intact_capture() {
        let root = &generals_assets::game_root().unwrap();
        let mut game = crate::playtest::map_fixture::Fixture::load(root, super::super::map_name())
            .unwrap()
            .into_simulation()
            .unwrap();
        // Reproduce the former real paid preparation before its missing scout.
        let baseline = prepare_site(&mut game).unwrap();
        assert!(matches!(
            game.visibility_status(0, baseline.bridge),
            Status::Fogged
        ));
        assert_eq!(
            game.entities[baseline.bridge].health.to_bits(),
            2000.0_f32.to_bits()
        );
        move_and_wait(
            &mut game,
            baseline.tomahawk,
            baseline.bridge_fire,
            "ordinary paid bridge scouting regression",
        )
        .unwrap();
        assert!(matches!(
            game.visibility_status(0, baseline.bridge),
            Status::Clear | Status::Partial
        ));
        // The actual capture entrypoint must establish that prerequisite itself.
        let setup = prepare(&mut game).unwrap();
        assert!(matches!(
            game.visibility_status(0, setup.bridge),
            Status::Clear | Status::Partial
        ));
        assert_eq!(
            game.entities[setup.bridge].health.to_bits(),
            2000.0_f32.to_bits()
        );
        assert!(game.entities[setup.tomahawk].alive());
        assert_eq!(
            game.catalog[game.entities[setup.tomahawk].type_index].name,
            "AmericaVehicleTomahawk"
        );
        // Stop clears the current AI goal; retain the completed source request.
        assert!(matches!(
            game.entities[setup.tomahawk].ai.order,
            Order::Idle
        ));
        let route = generals_sim::skirmish::diagnostics::ground_route_result(&game, setup.tomahawk)
            .unwrap();
        assert_eq!(
            route.requested.map(f32::to_bits),
            [setup.bridge_fire[0], setup.bridge_fire[1]].map(f32::to_bits)
        );
        assert!(
            squared(
                game.entities[setup.tomahawk].position,
                [setup.bridge_fire[0], setup.bridge_fire[1]]
            ) < 25.0 * 25.0
        );
    }
}
