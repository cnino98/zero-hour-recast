use super::{Action, Layer, PromotionIcon, UiData, at, faction_index, selected_command, transport};
use crate::{control_bar::Command, playtest::Match};
use bevy::{prelude::*, ui::FocusPolicy, ui_render::ui_material::MaterialNode};

type OverlayFilter = (Without<Action>, Without<super::power_shortcuts::Tray>);

#[derive(Component)]
pub struct CommandOverlay {
    slot: usize,
    button: Entity,
    rank: bool,
}

pub(super) fn setup(commands: &mut Commands, data: &UiData, icons: &mut Assets<PromotionIcon>) {
    for slot in 1..=14 {
        let area = data.windows[&format!("ControlBar.wnd:ButtonCommand{slot:02}")];
        let button = commands
            .spawn((
                Button,
                Action::Slot(slot),
                Layer::Game,
                MaterialNode(icons.add(data.icon(&data.units[0].portrait))),
                at(area),
                ZIndex(5),
            ))
            .id();
        commands.spawn((
            CommandOverlay {
                slot,
                button,
                rank: false,
            },
            Layer::Game,
            data.image("Cameo_hilited"),
            at(area),
            FocusPolicy::Pass,
            ZIndex(7),
        ));
        commands.spawn((
            CommandOverlay {
                slot,
                button,
                rank: true,
            },
            Layer::Game,
            data.image("SSChevron1L"),
            at(area),
            FocusPolicy::Pass,
            ZIndex(6),
        ));
    }
}

pub(super) fn available(battle: &mut Match, command: &Command) -> bool {
    use crate::control_bar::CommandAction;
    if selection_disabled(battle, command) {
        return false;
    }
    if let Some(upgrade) = &command.required_upgrade
        && !battle
            .selected
            .iter()
            .any(|&id| required_upgrade_met(&battle.simulation, id, upgrade))
    {
        return false;
    }
    if let CommandAction::BattlePlan(_) = command.action {
        return battle
            .selected
            .first()
            .is_some_and(|&id| battle.simulation.can_choose_battle_plan(id));
    }
    if command.name == "Command_StrategyCenter_Stop" {
        return battle.selected.first().is_some_and(|&id| {
            battle.simulation.entities[id].battle_plan()
                == Some(generals_sim::skirmish::battle_plan::Plan::Bombardment)
        });
    }
    if let Some(ready) = targeted_ready(battle, command) {
        return ready;
    }
    if matches!(command.action, CommandAction::Stop)
        && let [id] = battle.selected.as_slice()
        && battle.simulation.is_garrison(*id)
        && battle.simulation.catalog[battle.simulation.entities[*id].type_index].faction
            == generals_sim::skirmish::Faction::Civilian
        && battle.simulation.passengers(*id).is_empty()
    {
        return false;
    }
    if matches!(command.action, CommandAction::HackInternet) {
        return battle.selected.iter().any(|&id| {
            battle
                .simulation
                .entities
                .get(id)
                .is_some_and(|actor| actor.hacking_condition().is_none())
        });
    }
    if let Some(available) = transport::available(battle, command.action) {
        return available;
    }
    if let CommandAction::SwitchWeapon(slot) = command.action {
        return battle
            .selected
            .iter()
            .any(|&id| battle.simulation.weapon_slot_present(id, slot));
    }
    if let CommandAction::Research(index) = command.action {
        return battle.selected.first().is_some_and(|&id| {
            crate::input::bomb_truck::payload_available(battle, id, index)
                .unwrap_or_else(|| battle.simulation.can_research(id, index))
        });
    }
    if matches!(command.action, CommandAction::Sell) {
        return battle
            .selected
            .first()
            .is_some_and(|&id| battle.simulation.can_sell(0, id));
    }
    if matches!(command.action, CommandAction::DetonateFake) {
        return battle
            .selected
            .iter()
            .any(|&id| battle.simulation.can_detonate_fake(0, id));
    }
    if let Some(power) = command.action.power() {
        return battle
            .selected
            .iter()
            .any(|&caster| battle.simulation.can_use_power(caster, power));
    }
    let Some(index) = command.action.product() else {
        return true;
    };
    let game = &battle.simulation;
    if !game.prerequisites_met(0, index) || game.players[0].cash < game.build_cost(0, index) {
        return false;
    }
    // Construction needs no hangar. Production checks capacity only at a parking place.
    game.catalog[index].kind.is_building()
        || battle.selected.first().is_none_or(|&id| {
            game.catalog[game.entities[id].type_index].parking.is_none()
                || game.parking_available(id, index)
        })
}

fn required_upgrade_met(
    game: &generals_sim::skirmish::Skirmish,
    id: usize,
    upgrade: &crate::control_bar::RequiredUpgrade,
) -> bool {
    match upgrade {
        crate::control_bar::RequiredUpgrade::Player(name) => {
            game.has_upgrade(game.entities[id].owner, name)
        }
        crate::control_bar::RequiredUpgrade::Object(name) => game.object_has_upgrade(id, name),
    }
}

fn targeted_ready(battle: &mut Match, command: &Command) -> Option<bool> {
    use crate::control_bar::CommandAction;
    if !matches!(
        command.action,
        CommandAction::Capture
            | CommandAction::HelixBomb
            | CommandAction::Hack(_)
            | CommandAction::DisableBuilding
            | CommandAction::FireWeapon(_)
            | CommandAction::ClearMines(_)
            | CommandAction::SnipeVehicle(_)
            | CommandAction::BurtonKnife
            | CommandAction::BurtonCharge(_)
            | CommandAction::BurtonDetonate
            | CommandAction::BombTruckDisguise
            | CommandAction::BombTruckDetonate
            | CommandAction::Cleanup
    ) {
        return None;
    }
    Some((0..battle.selected.len()).any(|index| {
        let id = battle.selected[index];
        if command
            .required_upgrade
            .as_ref()
            .is_some_and(|upgrade| !required_upgrade_met(&battle.simulation, id, upgrade))
        {
            return false;
        }
        match command.action {
            CommandAction::Capture => {
                battle.simulation.capture_ready(id) && !finite_in_use(battle, id, 0)
            }
            CommandAction::HelixBomb => battle.simulation.helix_bomb_ready(id),
            CommandAction::Hack(ability) => {
                battle.simulation.finite_ability_ready(id, ability)
                    && !finite_in_use(battle, id, ability)
            }
            CommandAction::DisableBuilding => battle.simulation.disable_building_ready(id),
            CommandAction::FireWeapon(slot) => battle.simulation.location_weapon_ready(id, slot),
            CommandAction::ClearMines(slot) => battle.simulation.mine_clearing_ready(id, slot),
            CommandAction::SnipeVehicle(slot) => battle.simulation.pilot_weapon_ready(id, slot),
            CommandAction::BurtonKnife => battle.simulation.burton_knife_ready(id),
            CommandAction::BurtonCharge(charge) => {
                battle.simulation.burton_charge_ready(id, charge)
            }
            CommandAction::BurtonDetonate => battle.simulation.burton_can_detonate(id),
            CommandAction::BombTruckDisguise => battle.simulation.bomb_truck_disguise_ready(id),
            CommandAction::BombTruckDetonate => battle.simulation.can_detonate_bomb_truck(0, id),
            CommandAction::Cleanup => battle.simulation.cleanup_ready(id),
            _ => unreachable!("targeted command"),
        }
    }))
}

fn finite_in_use(battle: &Match, id: usize, ability: usize) -> bool {
    battle.simulation.capture_in_use(id)
        && battle.simulation.entities[id].finite_ability_index() == ability
}

fn selection_disabled(battle: &Match, command: &Command) -> bool {
    use crate::control_bar::CommandAction;
    !matches!(
        command.action,
        CommandAction::Sell
            | CommandAction::Evacuate
            | CommandAction::ExitContainer(_)
            | CommandAction::Stop
            | CommandAction::Rally
            | CommandAction::SwitchWeapon(_)
            | CommandAction::Research(_)
    ) && !battle.selected.is_empty()
        && battle.selected.iter().all(|&id| {
            let actor = &battle.simulation.entities[id];
            if command.action.power().is_some() {
                battle.simulation.object_disabled(id)
            } else {
                actor.hacked() || actor.paralyzed() || actor.subdued()
            }
        })
}

pub(super) fn visible(battle: &Match, command: &Command) -> bool {
    match command.action {
        crate::control_bar::CommandAction::ExitContainer(slot) => transport::visible(battle, slot),
        _ => true,
    }
}

pub fn update(
    battle: ResMut<Match>,
    data: Res<UiData>,
    mut icons: ResMut<Assets<PromotionIcon>>,
    mut buttons: Query<
        (&Action, &MaterialNode<PromotionIcon>, &mut Node),
        Without<super::power_shortcuts::Tray>,
    >,
    interactions: Query<&Interaction, With<Action>>,
    mut overlays: Query<(&CommandOverlay, &mut ImageNode, &mut Node), OverlayFilter>,
    powers: (
        Query<(&super::power_shortcuts::Tray, &mut Node), Without<Action>>,
        super::superweapon_timers::View,
    ),
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let (mut trays, countdown) = powers;
    countdown.update(battle, data);
    for (tray, mut node) in &mut trays {
        super::power_shortcuts::update_tray(battle, data, tray, &mut node);
    }
    for (action, handle, mut node) in &mut buttons {
        if let Action::PowerShortcut { faction, slot } = *action {
            super::power_shortcuts::update_button(
                battle, data, faction, slot, &mut icons, handle, &mut node,
            );
            continue;
        }
        let Action::Slot(slot) = *action else {
            continue;
        };
        let command = selected_command(battle, data, slot);
        node.reborrow()
            .map_unchanged(|node| &mut node.display)
            .set_if_neq(if battle.started && command.is_some() {
                Display::Flex
            } else {
                Display::None
            });
        let Some(command) = command else { continue };
        if icons.get(&handle.0).is_none() {
            continue;
        }
        let mut icon = data.icon(command_image(battle, data, command));
        update_state(&mut icon, battle, data, command);
        super::promotion_icon::update(&mut icons, &handle.0, icon);
    }
    for (overlay, mut image, mut node) in &mut overlays {
        let command = selected_command(battle, data, overlay.slot);
        if overlay.rank {
            let rank = command.and_then(|command| match command.action {
                crate::control_bar::CommandAction::ExitContainer(slot) => {
                    let rider = transport::passenger(battle, slot)?;
                    match battle.simulation.entities[rider].veterancy {
                        1 => Some("SSChevron1L"),
                        2 => Some("SSChevron2L"),
                        3 => Some("SSChevron3L"),
                        _ => None,
                    }
                }
                _ => None,
            });
            node.reborrow()
                .map_unchanged(|node| &mut node.display)
                .set_if_neq(if rank.is_some() {
                    Display::Flex
                } else {
                    Display::None
                });
            if let Some(rank) = rank {
                super::set_image(&mut image, data.image(rank));
            }
            continue;
        }
        let interaction = interactions
            .get(overlay.button)
            .copied()
            .unwrap_or(Interaction::None);
        let checked = command.is_some_and(|command| match command.action {
            crate::control_bar::CommandAction::SwitchWeapon(slot) => {
                !battle.selected.is_empty()
                    && battle
                        .selected
                        .iter()
                        .all(|&id| battle.simulation.entities[id].weapon_slot() == slot)
            }
            crate::control_bar::CommandAction::BattlePlan(plan) => {
                battle.selected.first().is_some_and(|&id| {
                    battle.simulation.entities[id].battle_plan_desired() == Some(plan)
                })
            }
            _ => false,
        });
        let visible = command.is_some_and(|command| available(battle, command))
            && (checked || interaction != Interaction::None);
        node.reborrow()
            .map_unchanged(|node| &mut node.display)
            .set_if_neq(if visible {
                Display::Flex
            } else {
                Display::None
            });
        super::set_image(
            &mut image,
            data.image(if checked || interaction == Interaction::Pressed {
                "Cameo_push"
            } else {
                "Cameo_hilited"
            }),
        );
    }
}

fn update_state(icon: &mut PromotionIcon, battle: &mut Match, data: &UiData, command: &Command) {
    if command.required_upgrade.as_ref().is_some_and(|upgrade| {
        !battle
            .selected
            .iter()
            .any(|&id| required_upgrade_met(&battle.simulation, id, upgrade))
    }) {
        icon.state.x = 1.0;
        return;
    }
    weapon_reload_clock(icon, battle, data, command.action);
    if let Some(power) = command.action.power() {
        let (gray, clock) = capture_clock(battle.selected.iter().copied().map(|id| {
            let required = command
                .required_upgrade
                .as_ref()
                .is_none_or(|upgrade| required_upgrade_met(&battle.simulation, id, upgrade));
            let restricted = battle.simulation.object_disabled(id);
            let progress = required.then(|| battle.simulation.power_progress(id, power));
            (progress, restricted)
        }));
        icon.state.x = if gray { 1.0 } else { 0.0 };
        if let Some(percent) = clock {
            icon.state.z = percent;
            icon.clock_color =
                data.schemes[faction_index(battle.simulation.players[0].faction)].clock_color;
        }
    } else if let Some(ability) = match command.action {
        crate::control_bar::CommandAction::Capture => Some(0),
        crate::control_bar::CommandAction::Hack(ability) => Some(ability),
        _ => None,
    } {
        let mut clocks = Vec::new();
        for index in 0..battle.selected.len() {
            let id = battle.selected[index];
            let actor = &battle.simulation.entities[id];
            let available = battle.simulation.catalog[actor.type_index].behavior.special_abilities.iter()
                .filter_map(|rules| rules.power.as_ref()).any(|power| matches!((ability, power.kind),
                    (0, generals_sim::skirmish::special_ability::PowerType::SpecialInfantryCaptureBuilding
                        | generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding)
                    | (1, generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusDisableVehicleHack)
                    | (2, generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusStealCashHack)));
            if !available {
                continue;
            }
            let required = command
                .required_upgrade
                .as_ref()
                .is_none_or(|upgrade| required_upgrade_met(&battle.simulation, id, upgrade));
            let disabled = actor.hacked() || actor.paralyzed() || actor.subdued();
            let progress = battle.simulation.finite_ability_progress(id, ability);
            clocks.push((
                required.then_some(progress),
                disabled || (progress >= 1.0 && finite_in_use(battle, id, ability)),
            ));
        }
        let (gray, clock) = capture_clock(clocks.into_iter());
        icon.state.x = if gray { 1.0 } else { 0.0 };
        if let Some(percent) = clock {
            icon.state.z = percent;
            icon.clock_color =
                data.schemes[faction_index(battle.simulation.players[0].faction)].clock_color;
        }
    } else if matches!(command.action, crate::control_bar::CommandAction::HelixBomb) {
        let selected = battle.selected.clone();
        let clocks: Vec<_> = selected
            .into_iter()
            .filter_map(|id| {
                let progress = battle.simulation.helix_bomb_percent(id)?;
                let actor = &battle.simulation.entities[id];
                Some((
                    command
                        .required_upgrade
                        .as_ref()
                        .is_none_or(|upgrade| required_upgrade_met(&battle.simulation, id, upgrade))
                        .then_some(progress),
                    actor.hacked()
                        || actor.paralyzed()
                        || actor.subdued()
                        || (progress >= 1.0 && !battle.simulation.helix_bomb_ready(id)),
                ))
            })
            .collect();
        let (gray, clock) = capture_clock(clocks.into_iter());
        icon.state.x = if gray { 1.0 } else { 0.0 };
        if let Some(percent) = clock {
            icon.state.z = percent;
            icon.clock_color =
                data.schemes[faction_index(battle.simulation.players[0].faction)].clock_color;
        }
    } else if !available(battle, command) && command.name != "Command_StructureExit" {
        // Source DOZER_CONSTRUCT and UNIT_BUILD both return COMMAND_RESTRICTED
        // for missing prerequisites or money, selecting disabled grayscale.
        icon.state.x = 1.0;
    }
}

fn weapon_reload_clock(
    icon: &mut PromotionIcon,
    battle: &mut Match,
    data: &UiData,
    action: crate::control_bar::CommandAction,
) {
    let reload_slot = match action {
        crate::control_bar::CommandAction::SnipeVehicle(slot)
        | crate::control_bar::CommandAction::ClearMines(slot) => Some(slot),
        crate::control_bar::CommandAction::BurtonKnife => Some(1),
        _ => None,
    };
    if let Some(slot) = reload_slot
        && let Some(progress) = {
            let simulation = &mut battle.simulation;
            battle
                .selected
                .iter()
                .filter_map(|&id| simulation.weapon_button_reload_progress(id, slot))
                .max_by(f32::total_cmp)
        }
    {
        icon.state.z = (progress * 100.0).trunc();
        icon.clock_color =
            data.schemes[faction_index(battle.simulation.players[0].faction)].clock_color;
    }
}

// ControlBarMultiSelect evaluates in selection order. Its clock is the final
// not-ready evaluation, while any available object enables the shared button.
fn capture_clock(entries: impl Iterator<Item = (Option<f32>, bool)>) -> (bool, Option<f32>) {
    let mut available = false;
    let mut not_ready = false;
    let mut clock = None;
    for (progress, restricted) in entries {
        not_ready = false;
        if let Some(progress) = progress {
            if progress < 1.0 {
                clock = Some((progress * 100.0).trunc());
                not_ready = !restricted;
            } else if !restricted {
                available = true;
            }
        }
    }
    (!available && !not_ready, clock)
}

pub(super) fn command_image<'a>(battle: &Match, data: &'a UiData, command: &'a Command) -> &'a str {
    if matches!(
        command.action,
        crate::control_bar::CommandAction::ExitContainer(_)
    ) {
        return transport::image(battle, data, command);
    }
    winning_science(battle, data, command).map_or(&command.image, |button| &button.image)
}

fn winning_science<'a>(
    battle: &Match,
    data: &'a UiData,
    command: &Command,
) -> Option<&'a crate::control_bar::ScienceButton> {
    let player = &battle.simulation.players[0];
    // populateContextCommand stops at the first unowned science in the ordered list.
    let best = command
        .sciences
        .iter()
        .take_while(|science| player.has_science(science))
        .last();
    best.and_then(|science| {
        data.promotions.buttons[faction_index(player.faction)]
            .iter()
            .find(|button| &button.science == science)
    })
}

pub(super) fn power_text<'a>(
    battle: &Match,
    data: &'a UiData,
    command: &'a Command,
) -> (&'a str, &'a str) {
    winning_science(battle, data, command)
        .map_or((&command.label, &command.description), |button| {
            (&button.label, &button.description)
        })
}

#[cfg(test)]
mod capture_tests {
    use super::capture_clock;

    #[test]
    fn native_selection_order_retains_last_clock_even_when_another_unit_is_ready() {
        let (gray, clock) = capture_clock([(Some(0.8), false), (Some(0.2), false)].into_iter());
        assert!(!gray);
        assert_eq!(clock.map(f32::to_bits), Some(20.0_f32.to_bits()));
        let (gray, clock) = capture_clock([(Some(0.2), false), (Some(1.0), false)].into_iter());
        assert!(!gray);
        assert_eq!(clock.map(f32::to_bits), Some(20.0_f32.to_bits()));
        let (gray, clock) = capture_clock([(Some(0.2), false), (Some(1.0), true)].into_iter());
        assert!(gray);
        assert_eq!(clock.map(f32::to_bits), Some(20.0_f32.to_bits()));
        let (gray, clock) = capture_clock([(None, false)].into_iter());
        assert!(gray);
        assert!(clock.is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_verification::advance_until;

    #[ignore = "requires original game installation"]
    #[test]
    fn ordinary_build_buttons_do_not_require_aircraft_parking() {
        use crate::skirmish_verification::friendly_entity;
        use generals_sim::skirmish::Kind;

        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let builder = friendly_entity(&battle.simulation, Kind::Builder, 0).unwrap();
        let center = friendly_entity(&battle.simulation, Kind::CommandCenter, 0).unwrap();
        let power = battle.simulation.type_for(0, Kind::PowerPlant).unwrap();
        let factory = battle.simulation.type_for(0, Kind::Factory).unwrap();
        let builder_type = battle.simulation.entities[builder].type_index;
        let power_command = data
            .commands(&battle.simulation, builder)
            .iter()
            .find(|command| command.action.product() == Some(power))
            .unwrap();
        let builder_command = data
            .commands(&battle.simulation, center)
            .iter()
            .find(|command| command.action.product() == Some(builder_type))
            .unwrap();
        assert!(battle.simulation.catalog[builder_type].parking.is_none());
        assert!(
            battle.simulation.catalog[battle.simulation.entities[center].type_index]
                .parking
                .is_none()
        );
        assert!(battle.simulation.prerequisites_met(0, power));
        assert!(
            battle
                .simulation
                .construction_admission(builder, power)
                .is_ok()
        );
        battle.select_objects(&[builder], true);
        assert!(available(&mut battle, power_command));
        let factory_command = data
            .commands(&battle.simulation, builder)
            .iter()
            .find(|command| command.action.product() == Some(factory))
            .unwrap();
        assert!(!battle.simulation.prerequisites_met(0, factory));
        assert!(!available(&mut battle, factory_command));
        let cash = battle.simulation.players[0].cash;
        battle.simulation.players[0].cash = battle.simulation.build_cost(0, power) - 1;
        assert!(!available(&mut battle, power_command));
        battle.simulation.players[0].cash = cash;
        assert!(available(&mut battle, power_command));
        battle.select_objects(&[center], true);
        assert!(available(&mut battle, builder_command));
        battle.simulation.train(center, builder_type).unwrap();
        assert_eq!(
            battle.simulation.players[0].cash,
            cash - battle.simulation.build_cost(0, builder_type)
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn china_dozer_and_gla_worker_offer_their_starting_buildings() {
        use generals_sim::skirmish::{Faction, Kind};

        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        for (faction, name) in [
            (Faction::China, "ChinaPowerPlant"),
            (Faction::Gla, "GLABarracks"),
        ] {
            battle.simulation.reset_sandbox(faction).unwrap();
            let builder =
                crate::skirmish_verification::friendly_entity(&battle.simulation, Kind::Builder, 0)
                    .unwrap();
            let product = crate::strategy_verification::template(&battle.simulation, name).unwrap();
            let command = data
                .commands(&battle.simulation, builder)
                .iter()
                .find(|button| button.action.product() == Some(product))
                .unwrap();
            battle.select_objects(&[builder], true);
            assert!(available(&mut battle, command), "{name}");
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_airfield_buttons_track_queued_hangar_reservations_and_cancellation() {
        use crate::skirmish_verification::{construct, construct_type, friendly_entity};
        use crate::strategy_verification::template;
        use generals_sim::skirmish::{Faction, Kind};

        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        // Use the supported skirmish cash option to keep affordability independent
        // of the four paid aircraft reservations throughout this test.
        battle.simulation.set_match_options(50_000, 0);
        battle
            .simulation
            .reset(Faction::Usa, Faction::China)
            .unwrap();
        let builder = friendly_entity(&battle.simulation, Kind::Builder, 0).unwrap();
        construct(&mut battle.simulation, builder, Kind::PowerPlant, 0).unwrap();
        construct(&mut battle.simulation, builder, Kind::SupplyCenter, 0).unwrap();
        let airfield_type = template(&battle.simulation, "AmericaAirfield").unwrap();
        let airfield = construct_type(&mut battle.simulation, builder, airfield_type, 0).unwrap();
        let raptor = template(&battle.simulation, "AmericaJetRaptor").unwrap();
        advance_until(
            &mut battle.simulation,
            30,
            "completed Airfield parking initialization",
            |game| game.parking_available(airfield, raptor),
        )
        .unwrap();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let raptor_command = data
            .commands(&battle.simulation, airfield)
            .iter()
            .find(|command| command.name == "Command_ConstructAmericaJetRaptor")
            .unwrap();
        let comanche_command = data
            .commands(&battle.simulation, airfield)
            .iter()
            .find(|command| command.name == "Command_ConstructAmericaVehicleComanche")
            .unwrap();
        assert_eq!(raptor_command.action.product(), Some(raptor));
        battle.select_objects(&[airfield], true);
        let cash = battle.simulation.players[0].cash;
        let cost = battle.simulation.build_cost(0, raptor);
        for queued in 0..4 {
            assert!(available(&mut battle, raptor_command));
            battle.simulation.train(airfield, raptor).unwrap();
            assert_eq!(battle.simulation.entities[airfield].queue.len(), queued + 1);
        }
        assert_eq!(battle.simulation.players[0].cash, cash - 4 * cost);
        assert!(battle.simulation.players[0].cash >= cost);
        assert!(battle.simulation.prerequisites_met(0, raptor));
        assert!(!available(&mut battle, raptor_command));
        assert!(available(&mut battle, comanche_command));
        assert!(battle.simulation.train(airfield, raptor).is_err());
        assert_eq!(battle.simulation.players[0].cash, cash - 4 * cost);
        // No logic frame runs between train and cancel: no aircraft has exited,
        // so CRC0's partial-batch cancellation restriction does not apply.
        let cancelled = battle.simulation.entities[airfield].queue[1].id;
        battle
            .simulation
            .cancel_production(0, airfield, cancelled)
            .unwrap();
        assert_eq!(battle.simulation.entities[airfield].queue.len(), 3);
        assert!(
            battle.simulation.entities[airfield]
                .queue
                .iter()
                .all(|entry| entry.id != cancelled)
        );
        assert_eq!(battle.simulation.players[0].cash, cash - 3 * cost);
        assert!(available(&mut battle, raptor_command));
        battle.simulation.train(airfield, raptor).unwrap();
        assert_eq!(battle.simulation.entities[airfield].queue.len(), 4);
        assert_eq!(battle.simulation.players[0].cash, cash - 4 * cost);
        assert!(!available(&mut battle, raptor_command));
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn required_upgrade_scope_uses_completed_player_or_selected_object_research() {
        use crate::control_bar::{CommandAction, RequiredUpgrade};
        use generals_sim::skirmish::{Kind, Research};
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let producer = crate::skirmish_verification::friendly_entity(
            &battle.simulation,
            Kind::CommandCenter,
            0,
        )
        .unwrap();
        let sibling =
            crate::skirmish_verification::friendly_entity(&battle.simulation, Kind::Builder, 0)
                .unwrap();
        let enemy =
            crate::skirmish_verification::friendly_entity(&battle.simulation, Kind::Builder, 1)
                .unwrap();
        let kind = battle.simulation.entities[producer].type_index;
        let first = battle.simulation.catalog[kind].maintenance.research.len();
        for (name, player) in [
            ("Upgrade_UIPlayerRequirement", true),
            ("Upgrade_UIObjectRequirement", false),
        ] {
            battle.simulation.catalog[kind]
                .maintenance
                .research
                .push(Research {
                    name: name.into(),
                    cost: 10,
                    frames: 1,
                    player,
                    sciences: Vec::new(),
                    ignores_underpowered: false,
                });
        }
        let mut command = Command {
            name: "Command_RequirementFixture".into(),
            slot: 1,
            action: CommandAction::Stop,
            accelerator: None,
            multi_select: true,
            image: String::new(),
            label: String::new(),
            description: String::new(),
            sciences: Vec::new(),
            required_upgrade: Some(RequiredUpgrade::Player(
                "Upgrade_UIPlayerRequirement".into(),
            )),
        };
        battle.select_objects(&[sibling], true);
        assert!(!available(&mut battle, &command));
        battle.simulation.queue_upgrade(producer, first).unwrap();
        assert!(!available(&mut battle, &command));
        advance_until(
            &mut battle.simulation,
            120,
            "player requirement research",
            |game| game.has_upgrade(0, "Upgrade_UIPlayerRequirement"),
        )
        .unwrap();
        assert!(available(&mut battle, &command));
        battle.selected = vec![enemy];
        assert!(!available(&mut battle, &command));
        command.required_upgrade = Some(RequiredUpgrade::Object(
            "Upgrade_UIObjectRequirement".into(),
        ));
        battle.select_objects(&[producer], true);
        assert!(!available(&mut battle, &command));
        battle
            .simulation
            .queue_upgrade(producer, first + 1)
            .unwrap();
        advance_until(
            &mut battle.simulation,
            120,
            "object requirement research",
            |game| game.object_has_upgrade(producer, "Upgrade_UIObjectRequirement"),
        )
        .unwrap();
        assert!(available(&mut battle, &command));
        battle.select_objects(&[sibling], true);
        assert!(!available(&mut battle, &command));
        // GameLogic::selectObject rejects a structure added after the dozer.
        // Its completed object research must not enable the dozer's button.
        battle.select_objects(&[sibling, producer], true);
        assert_eq!(battle.selected, vec![sibling]);
        assert_eq!(battle.logic_selection, vec![sibling]);
        assert!(!available(&mut battle, &command));
        battle.select_objects(&[producer, sibling], true);
        assert_eq!(battle.selected.len(), 2);
        assert!(battle.selected.contains(&producer) && battle.selected.contains(&sibling));
        assert!(available(&mut battle, &command));
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn targeted_weapon_requires_the_same_objects_completed_upgrade() {
        use crate::control_bar::{CommandAction, RequiredUpgrade};
        use crate::skirmish_verification::{construct, friendly_entity, produce};
        use generals_sim::skirmish::{Kind, Research};
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
        construct(game, builder, Kind::PowerPlant, 0).unwrap();
        let barracks = construct(game, builder, Kind::Barracks, 0).unwrap();
        let ranger_type = game
            .catalog
            .iter()
            .position(|unit| unit.name == "AmericaInfantryRanger")
            .unwrap();
        game.catalog[ranger_type].location_weapons |= 1;
        let ranger = produce(game, barracks, Kind::Infantry, 0).unwrap();
        let kind = game.entities[barracks].type_index;
        let research = game.catalog[kind].maintenance.research.len();
        game.catalog[kind].maintenance.research.push(Research {
            name: "Upgrade_TargetButtonPlayer".into(),
            cost: 10,
            frames: 1,
            player: true,
            sciences: Vec::new(),
            ignores_underpowered: false,
        });
        game.catalog[kind].maintenance.research.push(Research {
            name: "Upgrade_TargetButtonObject".into(),
            cost: 10,
            frames: 1,
            player: false,
            sciences: Vec::new(),
            ignores_underpowered: false,
        });
        assert!(game.location_weapon_ready(ranger, 0));
        let mut command = Command {
            name: "Command_RequiredGroundWeapon".into(),
            slot: 1,
            action: CommandAction::FireWeapon(0),
            accelerator: None,
            multi_select: true,
            image: String::new(),
            label: String::new(),
            description: String::new(),
            sciences: Vec::new(),
            required_upgrade: Some(RequiredUpgrade::Player("Upgrade_TargetButtonPlayer".into())),
        };
        battle.select_objects(&[ranger], true);
        assert!(!available(&mut battle, &command));
        battle.simulation.queue_upgrade(barracks, research).unwrap();
        assert!(!available(&mut battle, &command));
        advance_until(
            &mut battle.simulation,
            120,
            "targeted player requirement",
            |game| game.has_upgrade(0, "Upgrade_TargetButtonPlayer"),
        )
        .unwrap();
        assert!(available(&mut battle, &command));
        command.required_upgrade =
            Some(RequiredUpgrade::Object("Upgrade_TargetButtonObject".into()));
        battle
            .simulation
            .queue_upgrade(barracks, research + 1)
            .unwrap();
        advance_until(
            &mut battle.simulation,
            120,
            "targeted object requirement",
            |game| game.object_has_upgrade(barracks, "Upgrade_TargetButtonObject"),
        )
        .unwrap();
        battle.select_objects(&[ranger, barracks], true);
        // The ready Ranger cannot borrow the Barracks' object upgrade.
        assert!(!available(&mut battle, &command));
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn shared_primary_reload_does_not_draw_a_zero_reload_pilot_button_clock() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let kind =
            crate::strategy_verification::template(&battle.simulation, "GLAInfantryJarmenKell")
                .unwrap();
        // Allocate a paid custom-map variant before either weapon is used.
        let unit = &mut battle.simulation.catalog[kind];
        unit.weapon.clip_size = 1;
        unit.weapon.reload_seconds = 4.0;
        unit.secondary_weapon.as_mut().unwrap().reload_seconds = 0.0;
        unit.weapon_set_rules.share_reload = true;
        let setup = crate::jarmen_verification::prepare(&mut battle.simulation).unwrap();
        let point = battle.simulation.entities[setup.sniper].position;
        battle.simulation.move_to(setup.infantry, point).unwrap();
        advance_until(
            &mut battle.simulation,
            18_000,
            "infantry entering rifle range",
            |game| {
                let target = game.entities[setup.infantry].position;
                (target[0] - point[0]).hypot(target[1] - point[1]) < 275.0
            },
        )
        .unwrap();
        battle.simulation.stop(setup.infantry).unwrap();
        let previous_shots = battle.simulation.presentation.shots.len();
        battle
            .simulation
            .attack(setup.sniper, setup.infantry)
            .unwrap();
        advance_until(
            &mut battle.simulation,
            1500,
            "shared rifle reload",
            |game| {
                game.presentation
                    .shots
                    .iter()
                    .skip(previous_shots)
                    .any(|shot| shot.source == setup.sniper && shot.weapon_slot == 0)
            },
        )
        .unwrap();
        battle.simulation.stop(setup.sniper).unwrap();
        battle.select_objects(&[setup.sniper], true);
        // The generic Weapon percent query must retain the shared reload interval.
        assert!(
            battle
                .simulation
                .weapon_reload_progress(setup.sniper, 1)
                .is_some()
        );
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let mut data =
            crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let mut images = Assets::<Image>::default();
        for (name, image) in std::mem::take(&mut data.textures) {
            data.handles.insert(name, images.add(image));
        }
        let command = selected_command(&battle, &data, 1).unwrap();
        assert!(matches!(
            command.action,
            crate::control_bar::CommandAction::SnipeVehicle(1)
        ));
        let mut icon = data.icon(command_image(&battle, &data, command));
        assert_eq!(icon.state.z.to_bits(), (-1.0_f32).to_bits());
        update_state(&mut icon, &mut battle, &data, command);
        assert!(available(&mut battle, command));
        assert_eq!(icon.state.x.to_bits(), 0.0_f32.to_bits());
        assert_eq!(icon.state.z.to_bits(), (-1.0_f32).to_bits());
        assert!(
            battle
                .simulation
                .weapon_reload_progress(setup.sniper, 1)
                .is_some()
        );
    }
}
