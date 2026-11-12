//! Installed depot mesh picking followed by ordinary supply commands and updates.
use crate::{
    input::{self, Mouse, PickTarget, picking::View},
    playtest::Match,
    playtest_view::Capture,
    skirmish_verification::{construct, friendly_entity},
    supply_presentation::{self, SupplyPart},
};
use bevy::{app::AppExit, prelude::*};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

#[derive(Resource)]
pub struct SupplyCapture {
    pub collector: usize,
    pub source: usize,
    center: usize,
    checked: bool,
    visibility_checked: bool,
}

pub fn prepare(game: &mut Skirmish) -> Result<SupplyCapture, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    if game.players[0].faction != Faction::Gla {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    let center = construct(game, builder, Kind::SupplyCenter, 0)?;
    for _ in 0..1800 {
        if let Some((collector, _)) = game.entities.iter().enumerate().find(|(id, unit)| {
            *id != builder
                && unit.owner == 0
                && unit.complete()
                && game.catalog[unit.type_index].supply.capacity > 0
                && !matches!(unit.ai.order, Order::Exit(_))
        }) {
            let position = game.entities[center].position;
            let source = game
                .supplies
                .iter()
                .enumerate()
                .filter(|(_, depot)| depot.boxes > 0)
                .min_by(|(_, a), (_, b)| {
                    Vec2::from_array(game.entities[a.object].position)
                        .distance_squared(Vec2::from_array(position))
                        .total_cmp(
                            &Vec2::from_array(game.entities[b.object].position)
                                .distance_squared(Vec2::from_array(position)),
                        )
                })
                .map(|(id, _)| id)
                .ok_or("no stocked supply source")?;
            game.stop(collector)?;
            return Ok(SupplyCapture {
                collector,
                source,
                center,
                checked: false,
                visibility_checked: false,
            });
        }
        game.step();
    }
    Err("supply capture collector failed to leave its producer".into())
}

pub fn update(
    mut scenario: ResMut<SupplyCapture>,
    capture: Res<Capture>,
    view: View,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut exit: MessageWriter<AppExit>,
    parts: Query<(&SupplyPart, &Visibility, &InheritedVisibility)>,
) {
    let capture = capture.into_inner();
    if scenario.visibility_checked || !capture.ready_for_feedback() {
        return;
    }
    if scenario.checked {
        // The prior frame applied supply visibility and PostUpdate propagated it.
        scenario.visibility_checked = true;
        let boxes = battle.simulation.supplies[scenario.source].boxes;
        match supply_presentation::verify_visibility(scenario.source, boxes, &parts) {
            Ok(count) => eprintln!(
                "Visible depot contents: supply {}, {count} numbered crate meshes",
                scenario.source
            ),
            Err(error) => {
                error!("Supply rendered depletion diagnostic failed: {error}");
                exit.write(AppExit::error());
            }
        }
        return;
    }
    scenario.checked = true;
    let result = view.verify_supply(&scenario, &mut battle, &mut mouse);
    if let Err(error) = result {
        error!("Supply render/input diagnostic failed: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn verify_supply(
        self,
        scenario: &SupplyCapture,
        battle: &mut Match,
        mouse: &mut Mouse,
    ) -> Result<(), Box<dyn Error>> {
        let screen = self
            .supply_screen_point(battle, scenario.source)
            .ok_or("stocked depot has no visible pickable source mesh")?;
        let picked = self.pick(battle, screen);
        if !matches!(picked, Some(PickTarget::Supply(id)) if id == scenario.source) {
            return Err("rendered depot did not resolve to its supply object".into());
        }
        let rally = battle.simulation.entities[scenario.center]
            .rally_point
            .map(|point| point.map(f32::to_bits));
        for selection in [
            vec![scenario.center],
            vec![scenario.collector, scenario.center],
        ] {
            battle.select_objects(&selection, true);
            if input::context_order(battle, self.terrain_point(screen), picked, mouse) {
                return Err("ineligible depot click issued a movement or rally order".into());
            }
        }
        if battle.simulation.entities[scenario.center]
            .rally_point
            .map(|point| point.map(f32::to_bits))
            != rally
        {
            return Err("depot click changed a selected producer's rally point".into());
        }
        battle.select_objects(&[scenario.collector], true);
        let before = battle.simulation.players[0].cash;
        let boxes = battle.simulation.supplies[scenario.source].boxes;
        if !input::context_order(battle, self.terrain_point(screen), picked, mouse) {
            return Err("depot click failed to issue a context order".into());
        }
        for _ in 0..10_800 {
            battle.simulation.step();
            if battle.simulation.players[0].cash > before {
                if battle.simulation.supplies[scenario.source].boxes >= boxes {
                    return Err("income was not withdrawn from the clicked supply source".into());
                }
                eprintln!(
                    "Visible depot click: supply {}, cash {} -> {}, boxes {} -> {}",
                    scenario.source,
                    before,
                    battle.simulation.players[0].cash,
                    boxes,
                    battle.simulation.supplies[scenario.source].boxes
                );
                battle.simulation.stop(scenario.collector)?;
                return Ok(());
            }
        }
        Err("collector could not gather and deposit from the clicked source".into())
    }
}
