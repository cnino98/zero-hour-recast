//! `AutoDepositUpdate`: per-instance deadline, capture latch and source signed income.
use crate::skirmish::Skirmish;

/// Engine-session result of the native first `UpgradeTemplate` lookup.
#[derive(Clone)]
pub enum AutoDepositUpgradeLookup {
    Unresolved,
    Missing,
    Template(String),
}

pub struct UpgradeBoost {
    /// Null is the native unresolved `UpgradeTemplate` pointer.
    pub trigger: Option<String>,
    pub amount: i32,
}

pub struct Rules {
    pub interval: u32,
    pub amount: i32,
    pub first_capture: i32,
    pub actual_money: bool,
    pub upgrades: Vec<UpgradeBoost>,
}

pub(super) struct State {
    deposit_on: u32,
    award_initial: bool,
    initialized: bool,
}

pub struct Event {
    pub object: usize,
    pub owner: usize,
    pub frame: u64,
    pub amount: i32,
    pub position: [f32; 3],
    pub radii: Option<[f32; 2]>,
    pub visible_players: u16,
}

impl Skirmish {
    /// Engine-session static lookup; match resets intentionally preserve it.
    #[must_use]
    pub fn auto_deposit_upgrade_cache(&self) -> AutoDepositUpgradeLookup {
        self.auto_deposit_upgrade.clone()
    }

    /// Carries the native static lookup when the host replaces a map simulation.
    pub fn restore_auto_deposit_upgrade_cache(&mut self, cache: AutoDepositUpgradeLookup) {
        self.auto_deposit_upgrade = cache;
    }

    pub(in crate::skirmish) fn construct_auto_deposit(&mut self, id: usize, ordinal: usize) {
        let interval = self.catalog[self.entities[id].type_index]
            .maintenance
            .deposits[ordinal]
            .interval;
        let deposit_on = self.update_frame().wrapping_add(interval);
        assert_eq!(self.entities[id].maintenance.deposits.len(), ordinal);
        self.entities[id].maintenance.deposits.push(State {
            deposit_on,
            award_initial: false,
            initialized: false,
        });
    }

    pub(in crate::skirmish) fn update_auto_deposit(&mut self, id: usize, ordinal: usize) -> i32 {
        let now = self.update_frame();
        let actor = &self.entities[id];
        if now < actor.maintenance.deposits[ordinal].deposit_on {
            return 1;
        }
        let owner = actor.owner;
        let type_index = actor.type_index;
        let rules = &self.catalog[type_index].maintenance.deposits[ordinal];
        let amount = rules.amount;
        let actual_money = rules.actual_money;
        let interval = rules.interval;
        let state = &mut self.entities[id].maintenance.deposits[ordinal];
        if !state.initialized {
            state.award_initial = true;
            state.initialized = true;
        }
        state.deposit_on = now.wrapping_add(interval);
        if owner == self.neutral_owner() || amount <= 0 {
            return 1;
        }
        if self.entities[id].construction_percent.partial_cmp(&100.0)
            != Some(std::cmp::Ordering::Equal)
        {
            return 1;
        }
        let boost = self.auto_deposit_supply_boost(id, ordinal);
        let money = amount
            .checked_add(boost)
            .expect("native signed deposit amount overflow");
        if actual_money {
            self.deposit_native_money(owner, u32::from_ne_bytes(money.to_ne_bytes()));
            let player = &mut self.players[owner];
            player.statistics.money_earned = player
                .statistics
                .money_earned
                .wrapping_add(u32::from_ne_bytes(amount.to_ne_bytes()));
        }
        if money > 0 {
            let actor = &self.entities[id];
            let unit = &self.catalog[type_index];
            let radii = unit
                .kind_of
                .iter()
                .any(|kind| kind == "STRUCTURE")
                .then(|| {
                    let geometry = self.body_geometry(id);
                    [geometry.major, geometry.minor]
                });
            let visible_players = self
                .players
                .iter()
                .enumerate()
                .fold(0, |mask, (viewer, _)| {
                    if self.logically_visible(viewer, id) {
                        mask | (1 << viewer)
                    } else {
                        mask
                    }
                });
            let mut position = self.object_position(id);
            position[2] += 10.0;
            self.presentation.deposit_events.push(Event {
                object: id,
                owner: actor.owner,
                frame: self.tick,
                amount: money,
                position,
                radii,
                visible_players,
            });
        }
        1
    }

    fn auto_deposit_supply_boost(&mut self, id: usize, ordinal: usize) -> i32 {
        let actor = &self.entities[id];
        let upgrades = &self.catalog[actor.type_index].maintenance.deposits[ordinal].upgrades;
        let Some(first) = upgrades.first() else {
            return 0;
        };
        // The native function-local static is initialized at the first executed
        // list element and retained across later calls, instances and matches.
        if matches!(
            self.auto_deposit_upgrade,
            AutoDepositUpgradeLookup::Unresolved
        ) {
            self.auto_deposit_upgrade = first.trigger.clone().map_or(
                AutoDepositUpgradeLookup::Missing,
                AutoDepositUpgradeLookup::Template,
            );
        }
        if matches!(&self.auto_deposit_upgrade, AutoDepositUpgradeLookup::Template(name) if self.players[actor.owner]
                .upgrades.iter().any(|upgrade| upgrade == name))
        {
            first.amount
        } else {
            0
        }
    }

    /// `Player::becomingTeamMember` calls the first authored `AutoDeposit` instance.
    pub(in crate::skirmish) fn award_initial_deposit(
        &mut self,
        id: usize,
        ordinal: usize,
        owner: usize,
    ) {
        let rules = &self.catalog[self.entities[id].type_index]
            .maintenance
            .deposits[ordinal];
        let interval = rules.interval;
        let amount = rules.first_capture;
        let now = self.update_frame();
        let state = &mut self.entities[id].maintenance.deposits[ordinal];
        state.deposit_on = now.wrapping_add(interval);
        if !state.award_initial || amount <= 0 {
            return;
        }
        self.deposit_native_money(owner, u32::from_ne_bytes(amount.to_ne_bytes()));
        let player = &mut self.players[owner];
        player.statistics.money_earned = player
            .statistics
            .money_earned
            .wrapping_add(u32::from_ne_bytes(amount.to_ne_bytes()));
        let mut position = self.object_position(id);
        position[2] += 10.0;
        self.presentation.deposit_events.push(Event {
            object: id,
            owner,
            frame: self.tick,
            amount,
            position,
            radii: None,
            visible_players: u16::MAX,
        });
        self.entities[id].maintenance.deposits[ordinal].award_initial = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Faction, Kind, tests};

    fn object(game: &mut Skirmish, owner: usize, actual_money: bool) -> usize {
        let mut unit = tests::fixture_type(Faction::Gla, Kind::Defense);
        unit.maintenance.deposits.push(Rules {
            interval: 3,
            amount: 100,
            first_capture: 500,
            actual_money,
            upgrades: Vec::new(),
        });
        tests::register_update(&mut unit, "AutoDepositUpdate");
        let kind = game.catalog.len();
        game.catalog.push(unit);
        game.spawn(kind, owner, [300.0, 200.0], true)
    }

    #[test]
    fn neutral_first_due_initializes_capture_bonus_and_real_capture_resets_period() {
        let mut game = tests::game();
        let neutral = game.neutral_owner();
        let id = object(&mut game, neutral, true);
        let cash = game.players[0].cash;
        game.tick = 3;
        game.run_object_updates();
        assert!(game.presentation.deposit_events.is_empty());
        game.tick = 4;
        game.transfer_maintenance_owner(id, 0);
        assert_eq!(game.players[0].cash, cash + 500);
        assert_eq!(game.presentation.deposit_events.len(), 1);
        game.tick = 6;
        game.run_object_updates();
        assert_eq!(game.players[0].cash, cash + 500);
        game.tick = 7;
        game.run_object_updates();
        assert_eq!(game.players[0].cash, cash + 600);
        assert_eq!(game.players[0].statistics.money_earned, 600);
        let other_cash = game.players[1].cash;
        game.transfer_maintenance_owner(id, 1);
        assert_eq!(game.players[1].cash, other_cash);
    }

    #[test]
    fn not_actual_money_still_emits_text_and_does_not_credit_score() {
        let mut game = tests::game();
        let id = object(&mut game, 0, false);
        let cash = game.players[0].cash;
        game.tick = 3;
        game.run_object_updates();
        assert_eq!(game.players[0].cash, cash);
        assert_eq!(game.players[0].statistics.money_earned, 0);
        assert_eq!(game.presentation.deposit_events.len(), 1);
        assert_eq!(game.presentation.deposit_events[0].object, id);
        assert_eq!(game.presentation.deposit_events[0].amount, 100);
        assert_eq!(
            game.presentation.deposit_events[0].position[2].to_bits(),
            10.0_f32.to_bits()
        );
    }

    #[test]
    fn first_nullable_upgrade_lookup_is_shared_and_bonus_score_uses_base_amount() {
        let mut game = tests::game();
        let id = object(&mut game, 0, true);
        let kind = game.entities[id].type_index;
        game.catalog[kind].maintenance.deposits[0].upgrades = vec![
            UpgradeBoost {
                trigger: Some("First".into()),
                amount: 20,
            },
            UpgradeBoost {
                trigger: Some("Second".into()),
                amount: 90,
            },
        ];
        game.players[0].upgrades.push("Second".into());
        let cash = game.players[0].cash;
        game.tick = 3;
        game.run_object_updates();
        assert_eq!(game.players[0].cash, cash + 100);
        game.players[0].upgrades.push("First".into());
        game.tick = 6;
        game.run_object_updates();
        assert_eq!(game.players[0].cash, cash + 220);
        assert_eq!(game.players[0].statistics.money_earned, 200);
        let second = object(&mut game, 0, true);
        let second_kind = game.entities[second].type_index;
        game.catalog[second_kind].maintenance.deposits[0].upgrades = vec![UpgradeBoost {
            trigger: None,
            amount: 40,
        }];
        game.tick = 9;
        game.run_object_updates();
        assert_eq!(game.players[0].cash, cash + 480);
    }
    #[test]
    fn defeated_owner_returns_tech_building_to_neutral_without_death_or_bonus_reset() {
        let (mut game, _, oil) = crate::skirmish::maintenance::tests::capture_game();
        let oil_type = game.entities[oil].type_index;
        game.catalog[oil_type].scoring.victory = false;
        game.tick = 4;
        game.run_object_updates();
        let neutral = game.neutral_owner();
        let neutral_cash = game.players[neutral].cash;
        let first_capture_cash = game.players[0].cash;
        game.transfer_maintenance_owner(oil, 0);
        assert_eq!(game.players[0].cash, first_capture_cash + 1000);
        game.entities[oil].health = 321.0;
        let deadline = game.entities[oil].maintenance.deposits[0].deposit_on;
        game.tick = 6;
        for actor in &mut game.entities {
            if actor.owner == 0 && game.catalog[actor.type_index].scoring.victory {
                actor.health = 0.0;
            }
        }
        game.update_victory();
        assert!(game.defeated[0]);
        assert!(game.entities[oil].alive());
        assert_eq!(game.entities[oil].health.to_bits(), 321.0_f32.to_bits());
        assert_eq!(game.entities[oil].owner, neutral);
        assert_eq!(
            game.entities[oil].maintenance.deposits[0].deposit_on,
            deadline
        );
        assert_eq!(game.players[neutral].cash, neutral_cash);
        game.tick = u64::from(deadline);
        game.run_object_updates();
        assert_eq!(game.players[neutral].cash, neutral_cash);
        let recapture_cash = game.players[1].cash;
        game.transfer_maintenance_owner(oil, 1);
        assert_eq!(game.players[1].cash, recapture_cash);
        game.tick += 4;
        game.run_object_updates();
        assert_eq!(game.players[1].cash, recapture_cash + 200);
    }

    fn producer(game: &mut Skirmish, trigger: Option<String>) -> usize {
        let mut unit = tests::fixture_type(Faction::Usa, Kind::Barracks);
        unit.maintenance.deposits = vec![Rules {
            interval: 1,
            amount: 10,
            first_capture: 0,
            actual_money: true,
            upgrades: vec![UpgradeBoost {
                trigger,
                amount: 100,
            }],
        }];
        tests::register_update(&mut unit, "AutoDepositUpdate");
        let kind = game.catalog.len();
        game.catalog.push(unit);
        game.spawn(kind, 0, [300.0; 2], true)
    }

    #[test]
    fn engine_session_resolved_null_is_not_looked_up_again_after_match_reset() {
        let mut first = tests::game();
        assert!(matches!(
            first.auto_deposit_upgrade_cache(),
            AutoDepositUpgradeLookup::Unresolved
        ));
        producer(&mut first, None);
        first.tick = 1;
        first.run_object_updates();
        assert!(matches!(
            first.auto_deposit_upgrade_cache(),
            AutoDepositUpgradeLookup::Missing
        ));
        let cached = first.auto_deposit_upgrade_cache();
        let mut next = tests::game();
        next.players[0]
            .upgrades
            .push("DifferentFirstUpgrade".into());
        next.restore_auto_deposit_upgrade_cache(cached);
        producer(&mut next, Some("DifferentFirstUpgrade".into()));
        let money = next.players[0].cash;
        next.tick = 1;
        next.run_object_updates();
        assert!(matches!(
            next.auto_deposit_upgrade_cache(),
            AutoDepositUpgradeLookup::Missing
        ));
        assert_eq!(next.players[0].cash, money + 10);
    }
}
