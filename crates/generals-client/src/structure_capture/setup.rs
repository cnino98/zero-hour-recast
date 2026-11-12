use crate::skirmish_verification::{advance_until, construct, friendly_entity, produce, require};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub(super) struct Setup {
    pub topple: bool,
    pub target: usize,
    pub tanks: Vec<usize>,
    pub position: [f32; 3],
    pub height: f32,
}

pub(super) fn prepare(game: &mut Skirmish, topple: bool) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let name = if topple {
        "MogadishuTower04"
    } else {
        "MogadishuHighrise01"
    };
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let home = game.entities[builder].position;
    let target = structure_target(game, name, home)?;
    let original_health = game.entities[target].health;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let mut tanks = Vec::new();
    for _ in 0..3 {
        tanks.push(produce(game, factory, Kind::Tank, 0)?);
    }
    let point = game.entities[target].position;
    let unit = &game.catalog[game.entities[target].type_index];
    let height = unit.geometry.height;
    let radius = unit.geometry.major.max(unit.geometry.minor) + 100.0;
    let direction =
        bevy::prelude::Vec2::from_array([home[0] - point[0], home[1] - point[1]]).normalize();
    for (tank, lateral) in tanks.iter().zip([-45.0, 0.0, 45.0]) {
        let offset =
            direction * radius + bevy::prelude::Vec2::new(-direction.y, direction.x) * lateral;
        let destination = [point[0] + offset.x, point[1] + offset.y];
        let started = game.tick;
        let origin = game.object_position(*tank);
        game.move_to(*tank, destination)?;
        let accepted = game.entities[*tank].ai_goal_position();
        advance_until(
            game,
            18_000,
            "paid tank scouting original structure",
            |game| {
                let position = game.entities[*tank].position;
                (position[0] - accepted[0]).hypot(position[1] - accepted[1]) < 35.0
            },
        )
        .map_err(|error| {
            let route = generals_sim::skirmish::diagnostics::ground_route_result(game, *tank)
                .map(|route| format!("requested={:?} endpoint={:?} layers={:?}", route.requested, route.endpoint, route.active_layers));
            format!(
                "{error}; original_structure={name} target={target} position={:?}; tank={tank} started={started} origin={origin:?} requested={destination:?} accepted={accepted:?} current_goal={:?} route={route:?}; {}; {}",
                game.object_position(target),
                game.entities[*tank].ai_goal_position(),
                generals_sim::skirmish::diagnostics::movement_details(game, *tank),
                generals_sim::skirmish::diagnostics::weapon_details(game, *tank, target),
            )
        })?;
        game.stop(*tank)?;
    }
    require(
        game.entities[target].health.to_bits() == original_health.to_bits(),
        "staging damaged the authored structure before live capture",
    )?;
    Ok(Setup {
        topple,
        target,
        tanks,
        position: game.object_position(target),
        height,
    })
}

fn structure_target(game: &Skirmish, name: &str, home: [f32; 2]) -> Result<usize, Box<dyn Error>> {
    game.entities
        .iter()
        .enumerate()
        .filter(|(_, actor)| {
            game.players[actor.owner].faction == Faction::Civilian
                && game.catalog[actor.type_index].name == name
        })
        .min_by(|(_, left), (_, right)| {
            let distance =
                |position: [f32; 2]| (position[0] - home[0]).hypot(position[1] - home[1]);
            distance(left.position).total_cmp(&distance(right.position))
        })
        .map(|(id, _)| id)
        .ok_or_else(|| "original map lacks required civilian structure".into())
}

#[cfg(test)]
mod tests {
    use super::structure_target;
    use generals_assets::map::MapData;
    use generals_sim::skirmish::{Faction, Kind};

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_cairo_paid_tanks_scout_original_topple_tower() {
        let root = &generals_assets::game_root().unwrap();
        let fixture =
            crate::playtest::map_fixture::Fixture::load(root, super::super::map_name(true))
                .unwrap();
        let mut game = fixture.into_simulation().unwrap();
        let setup = super::prepare(&mut game, true).unwrap();
        assert_eq!(
            game.catalog[game.entities[setup.target].type_index].name,
            "MogadishuTower04"
        );
        assert_eq!(setup.tanks.len(), 3);
        for &tank in &setup.tanks {
            assert!(game.entities[tank].alive());
            assert_eq!(game.entities[tank].owner, 0);
            assert_eq!(
                game.catalog[game.entities[tank].type_index].name,
                "AmericaTankCrusader"
            );
        }
        let route = generals_sim::skirmish::diagnostics::ground_route_result(&game, setup.tanks[0])
            .unwrap();
        assert_eq!(
            route.requested.map(f32::to_bits),
            [1315.0_f32, 2455.0].map(f32::to_bits)
        );
        let raw = [1286.6445_f32, 2425.2153];
        assert!((route.requested[0] - raw[0]).hypot(route.requested[1] - raw[1]) > 35.0);
        assert_eq!(
            game.entities[setup.target].health.to_bits(),
            game.entities[setup.target].max_health.to_bits()
        );
        verify_paid_topple(&mut game, &setup);
    }

    fn verify_paid_topple(game: &mut generals_sim::skirmish::Skirmish, setup: &super::Setup) {
        let target = setup.target;
        let initial_health = game.entities[target].health;
        let position = game.object_position(target);
        // The same ordinary force-attack commands as structure_capture::begin.
        // Let native attack approach/LOS handling navigate the authored House17.
        for &tank in &setup.tanks {
            game.force_attack(tank, target).unwrap();
        }
        let mut shot = false;
        let mut damaged = false;
        let mut falling = false;
        // Native capture's existing180s watchdog permits at most5400 source
        // ticks at30Hz. This headless test does not extend its gameplay budget.
        for _ in 0..5400 {
            game.step();
            shot |=
                game.presentation.shots.iter().any(|event| {
                    setup.tanks.contains(&event.source) && event.target == Some(target)
                });
            damaged |= game.entities[target].health < initial_health;
            falling |= game.entities[target]
                .structure_basis()
                .is_some_and(|basis| basis[2][2] < 0.95 && basis[2][2] > 0.2);
            if game.entities[target].post_collapse() {
                break;
            }
        }
        assert!(
            shot && damaged && falling && game.entities[target].post_collapse(),
            "ordinary paid tower attack incomplete: shot={shot} damaged={damaged} falling={falling} tick={}; {}",
            game.tick,
            setup
                .tanks
                .iter()
                .map(|&tank| generals_sim::skirmish::diagnostics::weapon_details(
                    game, tank, target
                ))
                .collect::<Vec<_>>()
                .join("; ")
        );
        assert!(game.entities[target].exists());
        assert!(game.entities[target].health <= 0.0);
        assert!(
            game.entities[target]
                .last_attacker()
                .is_some_and(|id| setup.tanks.contains(&id))
        );
        assert_eq!(
            game.object_position(target).map(f32::to_bits),
            position.map(f32::to_bits)
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_golden_oasis_capture_selects_retained_civilian_highrise() {
        let (assets, _) = crate::cleanup_verification::tests::installed();
        let bytes = assets.read(super::super::map_name(false)).unwrap().bytes;
        let map = MapData::parse(&bytes).unwrap();
        let metadata = generals_assets::scripts::map_scripts(&bytes).unwrap();
        let root = &generals_assets::game_root().unwrap();
        let settings = crate::world_data::camera_settings(&assets, root).unwrap();
        let terrain = crate::terrain::Terrain::new(
            map.heightmap,
            map.tiles,
            settings.adjust_cliffs,
            settings.three_way_blends,
        )
        .unwrap();
        let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
        let startup = crate::map_scripts::load_startup(&metadata, &data.types, &assets).unwrap();
        assert!(
            startup
                .sides
                .iter()
                .any(|side| { side.name == "PlyrCivilian" && side.faction == "FactionCivilian" })
        );
        let (mut layout, _) = crate::world_data::load(
            &assets,
            &map.objects,
            &terrain,
            &data.types,
            &mut data.models,
        )
        .unwrap();
        let authored = layout
            .neutral_objects
            .iter()
            .find(|record| record.script_name.as_deref() == Some("Player3 Garrison1"))
            .unwrap();
        assert_eq!(authored.name, "MogadishuHighrise01");
        assert_eq!(authored.original_team, "teamPlyrCivilian");
        let authored_position = [authored.position[0], authored.position[1]];
        layout.map_sides = startup.sides;
        layout.map_teams = startup.teams;
        layout.map_build_lists = startup.build_lists;
        layout.map_template_order = startup.template_order;
        let movement = terrain
            .classify_movement(&map.triggers, &map.objects)
            .unwrap();
        let mut game = data.into_simulation(movement, layout).unwrap();
        for _ in 0..2 {
            game.reset(Faction::Usa, Faction::China).unwrap();
            let builder =
                crate::skirmish_verification::friendly_entity(&game, Kind::Builder, 0).unwrap();
            let home = game.entities[builder].position;
            let target = structure_target(&game, "MogadishuHighrise01", home).unwrap();
            assert_eq!(game.named_map_object("Player3 Garrison1"), Some(target));
            let actor = &game.entities[target];
            assert_eq!(
                actor.position.map(f32::to_bits),
                authored_position.map(f32::to_bits)
            );
            assert!(actor.complete());
            assert_ne!(actor.owner, game.neutral_owner());
            assert!(game.players[actor.owner].faction == Faction::Civilian);
            game.entities[target].owner = 0;
            assert!(structure_target(&game, "MogadishuHighrise01", home).is_err());
        }
    }
}
