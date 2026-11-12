//! Real W3D render skeletons retain the controls of the last client draw.
use super::*;
use crate::skirmish_verification::{construct, friendly_entity, produce_type};
use generals_sim::skirmish::{Faction, Kind};

pub fn cached_gattling() -> (Match, Animations, Players, usize) {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let game = &mut battle.simulation;
    game.reset(Faction::China, Faction::Gla).unwrap();
    let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
    construct(game, builder, Kind::PowerPlant, 0).unwrap();
    construct(game, builder, Kind::SupplyCenter, 0).unwrap();
    let factory = construct(game, builder, Kind::Factory, 0).unwrap();
    let index = crate::strategy_verification::template(game, "ChinaTankGattling").unwrap();
    let actor = produce_type(game, factory, index, 0).unwrap();
    let mut models = Vec::new();
    let mut animations = load(&assets, &mut models, &game.catalog).unwrap();
    let mut players = Players(Vec::new());
    for draw in &animations.draws[index] {
        let mut player = Player::new(game.entities[actor].position, game.tick);
        player.state = draw.select(&BTreeSet::new());
        players.0.push(player);
    }
    animations.retain_models(models, 0);
    for (module, player) in players.0.iter_mut().enumerate() {
        animations
            .pose(player, &game.entities[actor], module, game.tick, None)
            .expect("original Gattling rendered pose");
    }
    (battle, animations, players, actor)
}

#[ignore = "requires original game installation"]
#[test]
fn laser_snapshot_uses_actual_rendered_skeleton_until_next_client_draw() {
    let (mut battle, animations, mut players, actor) = cached_gattling();
    let before = players
        .current_bone("Muzzle01")
        .expect("original Gattling muzzle bone")
        .0
        .translation;
    battle.simulation.entities[actor].ai.turret.angle += 0.8;
    battle.simulation.entities[actor].ai.turret.pitch -= 0.2;
    assert_eq!(
        players
            .current_bone("Muzzle01")
            .unwrap()
            .0
            .translation
            .map(f32::to_bits),
        before.map(f32::to_bits),
    );
    for (module, player) in players.0.iter_mut().enumerate() {
        animations
            .pose(
                player,
                &battle.simulation.entities[actor],
                module,
                battle.simulation.tick,
                None,
            )
            .expect("changed original Gattling rendered pose");
    }
    assert_ne!(
        players
            .current_bone("Muzzle01")
            .unwrap()
            .0
            .translation
            .map(f32::to_bits),
        before.map(f32::to_bits),
    );
}
