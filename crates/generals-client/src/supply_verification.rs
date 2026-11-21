//! Installed depot mesh picking followed by ordinary supply commands and updates.
use crate::{
    input::{self, Mouse, PickTarget, picking::View},
    playtest::Match,
    playtest_view::Capture,
    skirmish_verification::{construct, friendly_entity, produce_type},
    strategy_verification::template,
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
    infantry: usize,
    progress: Progress,
}

#[derive(Clone, Copy)]
struct Pickup {
    began: u64,
    cash: u32,
    boxes: u32,
    previous_boxes: u32,
    cargo: u32,
    source: Option<usize>,
}

enum Progress {
    Input,
    Pickup(Pickup),
    Deposit {
        began: u64,
        submitted_cash: u32,
        cash: u32,
        boxes: u32,
    },
    Stop(u64),
    Visibility,
    Done,
}

pub fn prepare(game: &mut Skirmish) -> Result<SupplyCapture, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    if game.players[0].faction != Faction::Gla {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    let center = construct(game, builder, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let name = match game.players[0].faction {
        Faction::Usa => "AmericaInfantryRanger",
        Faction::China => "ChinaInfantryRedguard",
        Faction::Gla => "GLAInfantryRebel",
        Faction::Civilian => return Err("supply capture requires a playable base faction".into()),
    };
    let infantry = produce_type(game, barracks, template(game, name)?, 0)?;
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
                infantry,
                progress: Progress::Input,
            });
        }
        game.step();
    }
    Err("supply capture collector failed to leave its producer".into())
}

pub fn update(
    mut scenario: ResMut<SupplyCapture>,
    mut capture: ResMut<Capture>,
    view: View,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut exit: MessageWriter<AppExit>,
    parts: Query<(&SupplyPart, &Visibility, &InheritedVisibility)>,
) {
    let result = (|| {
        match scenario.progress {
            Progress::Input => {
                if !capture.ready_for_feedback() {
                    return Ok(());
                }
                scenario.progress = view.verify_supply(&scenario, &mut battle, &mut mouse)?;
                capture.wait_for_scenario();
            }
            Progress::Pickup(pickup) => {
                scenario.progress = observe_pickup(&scenario, &battle.simulation, pickup)?;
            }
            Progress::Deposit {
                began,
                submitted_cash,
                cash,
                boxes,
            } => {
                let game = &battle.simulation;
                if observe_deposit(&scenario, game, began, submitted_cash, cash, boxes)? {
                    eprintln!(
                        "Visible depot click: supply {}, cash {} -> {}, boxes {} -> {}",
                        scenario.source,
                        cash,
                        game.players[0].cash,
                        boxes,
                        game.supplies[scenario.source].boxes
                    );
                    let frame = game.tick;
                    crate::playtest_ui::dispatch_command(
                        &mut battle,
                        &mut mouse,
                        crate::control_bar::CommandAction::Stop,
                    );
                    scenario.progress = Progress::Stop(frame);
                }
            }
            Progress::Stop(frame) => {
                if battle.simulation.tick > frame {
                    scenario.progress = Progress::Visibility;
                }
            }
            Progress::Visibility => {
                // The prior frame applied supply visibility and PostUpdate propagated it.
                let boxes = battle.simulation.supplies[scenario.source].boxes;
                let count = supply_presentation::verify_visibility(scenario.source, boxes, &parts)?;
                eprintln!(
                    "Visible depot contents: supply {}, {count} numbered crate meshes",
                    scenario.source
                );
                scenario.progress = Progress::Done;
                capture.finish_scenario();
            }
            Progress::Done => {}
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        scenario.progress = Progress::Done;
        error!("Supply render/input diagnostic failed: {error}");
        exit.write(AppExit::error());
    }
}

fn validate_trip(scenario: &SupplyCapture, game: &Skirmish) -> Result<(), Box<dyn Error>> {
    for id in [scenario.collector, scenario.center] {
        if !game.entities[id].complete() || game.entities[id].owner != 0 {
            return Err(
                "supply collector or intended center died, was captured or became unavailable"
                    .into(),
            );
        }
    }
    Ok(())
}

fn observe_pickup(
    scenario: &SupplyCapture,
    game: &Skirmish,
    mut pickup: Pickup,
) -> Result<Progress, Box<dyn Error>> {
    validate_trip(scenario, game)?;
    let elapsed = game.tick - pickup.began;
    if elapsed > 10_800 {
        return Err("collector could not withdraw fresh cargo from the clicked source".into());
    }
    let actor = &game.entities[scenario.collector];
    let source = match actor.ai.order {
        Order::Harvest(source) => Some(source),
        _ => None,
    };
    let boxes = game.supplies[scenario.source].boxes;
    if elapsed > 0
        && actor.cargo > pickup.cargo
        && boxes < pickup.previous_boxes
        && source.or(pickup.source) == Some(scenario.source)
    {
        return Ok(Progress::Deposit {
            began: pickup.began,
            submitted_cash: pickup.cash,
            cash: game.players[0].cash,
            boxes: pickup.boxes,
        });
    }
    if elapsed == 10_800 {
        return Err("collector could not withdraw fresh cargo from the clicked source".into());
    }
    pickup.previous_boxes = boxes;
    pickup.cargo = actor.cargo;
    pickup.source = source;
    Ok(Progress::Pickup(pickup))
}

fn observe_deposit(
    scenario: &SupplyCapture,
    game: &Skirmish,
    began: u64,
    submitted_cash: u32,
    cash: u32,
    boxes: u32,
) -> Result<bool, Box<dyn Error>> {
    validate_trip(scenario, game)?;
    let elapsed = game.tick - began;
    if elapsed > 10_800 {
        return Err("collector could not gather and deposit from the clicked source".into());
    }
    let actor = &game.entities[scenario.collector];
    if actor.cargo == 0
        && matches!(actor.ai.order, Order::ReturnSupply)
        && game.players[0].cash > cash
    {
        if game.players[0].cash <= submitted_cash {
            return Err("completed supply trip did not increase the original player cash".into());
        }
        if game.supplies[scenario.source].boxes >= boxes {
            return Err("income was not withdrawn from the clicked supply source".into());
        }
        return Ok(true);
    }
    if elapsed == 10_800 {
        return Err("collector could not gather and deposit from the clicked source".into());
    }
    Ok(false)
}

impl View<'_, '_> {
    fn verify_supply(
        self,
        scenario: &SupplyCapture,
        battle: &mut Match,
        mouse: &mut Mouse,
    ) -> Result<Progress, Box<dyn Error>> {
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
            vec![scenario.collector, scenario.infantry],
        ] {
            battle.select_objects(&selection, true);
            if battle.selected != selection {
                return Err("supply rejection fixture lost its selected objects".into());
            }
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
        Ok(Progress::Pickup(Pickup {
            began: battle.simulation.tick,
            cash: before,
            boxes,
            previous_boxes: boxes,
            cargo: battle.simulation.entities[scenario.collector].cargo,
            source: match battle.simulation.entities[scenario.collector].ai.order {
                Order::Harvest(source) => Some(source),
                _ => None,
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_verification::advance_until;

    fn installed() -> (
        generals_assets::virtual_assets::VirtualAssets,
        Match,
        SupplyCapture,
    ) {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        battle
            .simulation
            .reset(Faction::China, Faction::China)
            .unwrap();
        let scenario = prepare(&mut battle.simulation).unwrap();
        (assets, battle, scenario)
    }

    fn snapshot(game: &Skirmish, scenario: &SupplyCapture) -> Pickup {
        let actor = &game.entities[scenario.collector];
        let boxes = game.supplies[scenario.source].boxes;
        Pickup {
            began: game.tick,
            cash: game.players[0].cash,
            boxes,
            previous_boxes: boxes,
            cargo: actor.cargo,
            source: match actor.ai.order {
                Order::Harvest(source) => Some(source),
                _ => None,
            },
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn preloaded_paid_truck_must_make_a_fresh_clicked_trip_before_payment_is_accepted()
    -> Result<(), Box<dyn Error>> {
        let (assets, mut battle, scenario) = installed();
        let collector = scenario.collector;
        let game = &mut battle.simulation;
        let capacity = game.catalog[game.entities[collector].type_index]
            .supply
            .capacity;
        game.harvest(collector, scenario.source)?;
        advance_until(
            game,
            10_800,
            "original truck fills its pre-existing cargo",
            |game| game.entities[collector].cargo == capacity,
        )?;
        game.stop(collector)?;
        let initial = snapshot(game, &scenario);
        let mut progress = Progress::Pickup(initial);
        let point = game.supply_position(scenario.source);
        let mut mouse = input::load(&assets, std::path::Path::new(""), &game.catalog)?;
        battle.select_objects(&[collector], true);
        assert!(input::context_order(
            &mut battle,
            Some(point.into()),
            Some(PickTarget::Supply(scenario.source)),
            &mut mouse,
        ));
        let mut old_payment_seen = false;
        for _ in 0..10_800 {
            input::commands::step(&mut battle, &mut mouse);
            let game = &battle.simulation;
            progress = match progress {
                Progress::Pickup(pickup) => {
                    let next = observe_pickup(&scenario, game, pickup)?;
                    if game.players[0].cash > initial.cash
                        && game.entities[collector].cargo == 0
                        && game.supplies[scenario.source].boxes == initial.boxes
                    {
                        old_payment_seen = true;
                        assert!(matches!(next, Progress::Pickup(_)));
                    }
                    next
                }
                Progress::Deposit {
                    began,
                    submitted_cash,
                    cash,
                    boxes,
                } => {
                    assert!(old_payment_seen, "old cargo must pay before the fresh trip");
                    assert_eq!(
                        began, initial.began,
                        "phase change must retain the original deadline"
                    );
                    if observe_deposit(&scenario, game, began, submitted_cash, cash, boxes)? {
                        assert!(game.supplies[scenario.source].boxes < initial.boxes);
                        assert!(game.players[0].cash > initial.cash);
                        return Ok(());
                    }
                    Progress::Deposit {
                        began,
                        submitted_cash,
                        cash,
                        boxes,
                    }
                }
                _ => panic!("unexpected supply trip phase"),
            };
        }
        panic!("real fresh supply trip did not complete within its original deadline");
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn unrelated_payment_and_stale_stock_do_not_validate_a_wrong_source_or_lost_collector()
    -> Result<(), Box<dyn Error>> {
        let (_, mut battle, scenario) = installed();
        let game = &mut battle.simulation;
        game.harvest(scenario.collector, scenario.source)?;
        let initial = snapshot(game, &scenario);
        // Consecutive observations: another load removes stock and pays, while
        // this collector has not acquired any fresh cargo.
        game.tick += 1;
        game.supplies[scenario.source].boxes -= 1;
        game.players[0].cash += 75;
        let Progress::Pickup(pickup) = observe_pickup(&scenario, game, initial)? else {
            panic!("another collector's withdrawal/payment was accepted");
        };
        let other = game
            .supplies
            .iter()
            .enumerate()
            .find(|(source, stock)| *source != scenario.source && stock.boxes > 0)
            .map(|(source, _)| source)
            .unwrap();
        game.harvest(scenario.collector, other)?;
        game.tick += 1;
        game.entities[scenario.collector].cargo += 1;
        assert!(
            matches!(
                observe_pickup(&scenario, game, pickup)?,
                Progress::Pickup(_)
            ),
            "stale clicked-stock depletion cannot validate a later different pickup"
        );
        game.tick += 1;
        game.supplies[scenario.source].boxes -= 1;
        game.entities[scenario.collector].cargo += 1;
        assert!(
            matches!(
                observe_pickup(&scenario, game, pickup)?,
                Progress::Pickup(_)
            ),
            "an explicit different Harvest target overrides the preceding source observation"
        );
        for id in [scenario.collector, scenario.center] {
            game.entities[id].owner = 1;
            assert!(observe_pickup(&scenario, game, pickup).is_err());
            assert!(
                observe_deposit(
                    &scenario,
                    game,
                    initial.began,
                    initial.cash,
                    initial.cash,
                    initial.boxes
                )
                .is_err()
            );
            game.entities[id].owner = 0;
            let health = game.entities[id].health;
            game.entities[id].health = 0.0;
            assert!(observe_pickup(&scenario, game, pickup).is_err());
            assert!(
                observe_deposit(
                    &scenario,
                    game,
                    initial.began,
                    initial.cash,
                    initial.cash,
                    initial.boxes
                )
                .is_err()
            );
            game.entities[id].health = health;
        }
        Ok(())
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn pickup_and_deposit_share_one_deadline_with_exact_boundary_success()
    -> Result<(), Box<dyn Error>> {
        let (_, mut battle, scenario) = installed();
        let game = &mut battle.simulation;
        game.harvest(scenario.collector, scenario.source)?;
        let initial = snapshot(game, &scenario);
        game.tick = initial.began + 10_800;
        game.entities[scenario.collector].cargo += 1;
        game.supplies[scenario.source].boxes -= 1;
        let Progress::Deposit {
            began,
            submitted_cash,
            cash,
            boxes,
        } = observe_pickup(&scenario, game, initial)?
        else {
            panic!("fresh pickup on the last allowed frame was rejected");
        };
        assert_eq!(began, initial.began);
        game.deliver_supplies(scenario.collector, scenario.center)?;
        game.entities[scenario.collector].cargo = 0;
        game.players[0].cash += 75;
        assert!(observe_deposit(
            &scenario,
            game,
            began,
            submitted_cash,
            cash,
            boxes
        )?);
        game.tick += 1;
        assert!(observe_deposit(&scenario, game, began, submitted_cash, cash, boxes).is_err());
        assert!(observe_pickup(&scenario, game, initial).is_err());
        Ok(())
    }
}
