//! Installed-data setup actions, followed by the original player-slot dropdown.
use super::{Action, Dropdown, Menu, OpponentSlot};
use crate::playtest::Match;
use bevy::prelude::*;
use generals_sim::skirmish::{Faction, OpponentDifficulty};

pub fn enable(app: &mut App) {
    app.add_systems(First, (verify, super::name_verification::verify).chain());
}

fn verify(
    mut done: Local<bool>,
    mut menu: ResMut<Menu>,
    mut battle: ResMut<Match>,
    mut exit: MessageWriter<AppExit>,
) {
    if *done {
        return;
    }
    *done = true;
    if let Err(error) = scenario(&mut menu, &mut battle) {
        eprintln!("Original skirmish setup verification failed: {error}");
        exit.write(AppExit::error());
        return;
    }
    eprintln!(
        "Original skirmish setup: four-player teams, random factions, colors, starts and full logic RNG retained across restart; Medium/Hard and enemy faction retained; Open/Closed sandbox remains unoccupied without victory for 900 ticks each"
    );
}

fn match_options(menu: &mut Menu, battle: &mut Match) -> Result<(), &'static str> {
    let original = (
        menu.starting_cash,
        menu.superweapon_restricted,
        menu.game_speed,
    );
    apply(&Action::ChooseStartingCash(20_000), menu, battle);
    let restricted = menu.superweapon_restricted;
    apply(&Action::ToggleSuperweaponLimit, menu, battle);
    if menu.starting_cash != 20_000 || menu.superweapon_restricted == restricted {
        return Err("starting cash and superweapon selection interfered");
    }
    menu.game_speed = 15;
    apply(&Action::Start, menu, battle);
    if battle.simulation.players[0].cash != 20_000 || battle.game_speed != 15 {
        return Err("match options did not reach the running match");
    }
    apply(&Action::ConfirmRestart, menu, battle);
    if battle.simulation.players[0].cash != 20_000 || battle.game_speed != 15 {
        return Err("restart lost starting money or game speed");
    }
    apply(&Action::ConfirmMenu, menu, battle);
    (
        menu.starting_cash,
        menu.superweapon_restricted,
        menu.game_speed,
    ) = original;
    Ok(())
}

fn scenario(menu: &mut Menu, battle: &mut Match) -> Result<(), &'static str> {
    menu.factions.fill(Some(Faction::Usa));
    menu.opponents.fill(OpponentSlot::Open);
    menu.starts.fill(None);
    menu.colors.fill(None);
    menu.teams.fill(None);
    apply(&Action::Skirmish, menu, battle);
    for (slot, faction) in [
        (OpponentSlot::Medium, Faction::Gla),
        (OpponentSlot::Hard, Faction::Usa),
    ] {
        apply(
            &Action::ChooseOpponent {
                slot: 1,
                opponent: slot,
            },
            menu,
            battle,
        );
        apply(
            &Action::ChooseFaction {
                slot: 1,
                faction: Some(faction),
            },
            menu,
            battle,
        );
        apply(&Action::Start, menu, battle);
        configured(battle, slot, faction)?;
        apply(&Action::ConfirmRestart, menu, battle);
        configured(battle, slot, faction)?;
        apply(&Action::ConfirmMenu, menu, battle);
    }
    for slot in [OpponentSlot::Open, OpponentSlot::Closed] {
        apply(
            &Action::ChooseOpponent {
                slot: 1,
                opponent: slot,
            },
            menu,
            battle,
        );
        apply(&Action::ToggleDropdown(Dropdown::Faction(1)), menu, battle);
        if menu.open.is_some() {
            return Err("unoccupied faction combo opened");
        }
        apply(&Action::Start, menu, battle);
        for _ in 0..900 {
            battle.simulation.step();
        }
        if battle.opponent_difficulty.is_some()
            || battle
                .simulation
                .entities
                .iter()
                .any(|unit| unit.owner == 1)
            || battle.simulation.ended_at.is_some()
        {
            return Err("empty slot created an opponent or ended the sandbox");
        }
        apply(&Action::ConfirmRestart, menu, battle);
        if battle.opponent_difficulty.is_some()
            || battle
                .simulation
                .entities
                .iter()
                .any(|unit| unit.owner == 1)
        {
            return Err("sandbox restart created an opponent");
        }
        apply(&Action::ConfirmMenu, menu, battle);
    }
    match_options(menu, battle)?;
    four_player_teams(menu, battle)?;
    random_factions(menu, battle)?;
    apply(&Action::Skirmish, menu, battle);
    apply(
        &Action::ChooseOpponent {
            slot: 1,
            opponent: OpponentSlot::Hard,
        },
        menu,
        battle,
    );
    apply(
        &Action::ChooseFaction {
            slot: 1,
            faction: Some(Faction::Gla),
        },
        menu,
        battle,
    );
    Ok(())
}

fn random_factions(menu: &mut Menu, battle: &mut Match) -> Result<(), &'static str> {
    apply(&Action::Skirmish, menu, battle);
    menu.starts.fill(None);
    menu.colors.fill(None);
    for slot in 0..menu.factions.len() {
        if slot != 0 {
            apply(
                &Action::ChooseOpponent {
                    slot,
                    opponent: OpponentSlot::Easy,
                },
                menu,
                battle,
            );
        }
        apply(
            &Action::ChooseFaction {
                slot,
                faction: None,
            },
            menu,
            battle,
        );
    }
    apply(&Action::Start, menu, battle);
    let original: Vec<_> = battle
        .simulation
        .players
        .iter()
        .map(|player| player.faction)
        .collect();
    let starts = battle.simulation.start_positions.clone();
    let colors = battle.simulation.player_colors.clone();
    let random = generals_sim::skirmish::diagnostics::record(
        &battle.simulation,
        &mut std::hash::DefaultHasher::new(),
    );
    for _ in 0..120 {
        battle.simulation.step();
    }
    apply(&Action::ConfirmRestart, menu, battle);
    if battle
        .simulation
        .slots
        .iter()
        .any(|slot| slot.faction.is_some())
        || battle
            .simulation
            .players
            .iter()
            .map(|player| player.faction)
            .collect::<Vec<_>>()
            != original
        || battle.simulation.start_positions != starts
        || battle.simulation.player_colors != colors
        || generals_sim::skirmish::diagnostics::record(
            &battle.simulation,
            &mut std::hash::DefaultHasher::new(),
        ) != random
    {
        return Err("random faction/start/color or original logic RNG changed across restart");
    }
    apply(&Action::ConfirmMenu, menu, battle);
    Ok(())
}

fn configured(battle: &Match, slot: OpponentSlot, faction: Faction) -> Result<(), &'static str> {
    let difficulty = matches!(
        (slot, &battle.opponent_difficulty),
        (OpponentSlot::Medium, Some(OpponentDifficulty::Normal))
            | (OpponentSlot::Hard, Some(OpponentDifficulty::Hard))
    );
    if !battle.started
        || !difficulty
        || battle.simulation.players[1].faction != faction
        || !battle
            .simulation
            .entities
            .iter()
            .any(|unit| unit.owner == 1)
    {
        return Err("selected AI/faction or restart configuration was not applied");
    }
    Ok(())
}

fn apply(action: &Action, menu: &mut Menu, battle: &mut Match) {
    assert!(super::super::activate(action, menu, battle));
}

fn four_player_teams(menu: &mut Menu, battle: &mut Match) -> Result<(), &'static str> {
    for (slot, opponent) in [
        (1, OpponentSlot::Easy),
        (2, OpponentSlot::Medium),
        (3, OpponentSlot::Hard),
    ] {
        apply(&Action::ChooseOpponent { slot, opponent }, menu, battle);
    }
    for slot in [0, 2] {
        apply(
            &Action::ChooseTeam {
                slot,
                team: Some(1),
            },
            menu,
            battle,
        );
    }
    apply(
        &Action::ChooseColor {
            slot: 0,
            color: Some(5),
        },
        menu,
        battle,
    );
    apply(
        &Action::ChooseColor {
            slot: 1,
            color: Some(5),
        },
        menu,
        battle,
    );
    if menu.colors[1] == Some(5) {
        return Err("duplicate color admitted");
    }
    apply(&Action::ChooseStart(3), menu, battle);
    apply(&Action::ChooseStart(1), menu, battle);
    apply(&Action::Start, menu, battle);
    if battle.simulation.start_positions[0] != Some(3)
        || battle.simulation.start_positions[1] != Some(1)
        || battle.simulation.player_colors[0] != Some(5)
    {
        return Err("manual starts or player color were not applied");
    }
    let colors = battle.simulation.player_colors.clone();
    let starts = battle.simulation.start_positions.clone();
    for _ in 0..2 {
        if battle
            .simulation
            .slots
            .iter()
            .filter(|slot| slot.occupied)
            .count()
            != 4
            || !battle.simulation.allied(0, 2)
            || !battle.simulation.enemies(0, 1)
            || !battle.simulation.enemies(0, 3)
            || battle.simulation.start_positions != starts
            || battle.simulation.player_colors != colors
        {
            return Err("four-player menu configuration or retained restart changed");
        }
        apply(&Action::ConfirmRestart, menu, battle);
    }
    apply(&Action::ConfirmMenu, menu, battle);
    for slot in [2, 3] {
        apply(
            &Action::ChooseOpponent {
                slot,
                opponent: OpponentSlot::Open,
            },
            menu,
            battle,
        );
    }
    for slot in [0, 2] {
        apply(&Action::ChooseTeam { slot, team: None }, menu, battle);
    }
    Ok(())
}
