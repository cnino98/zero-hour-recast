//! SH `CrateCollide`/`MoneyCrateCollide`; CRC=0, `PRESERVE_MULTI_CRATE_PICKUP=0`.
use super::Skirmish;

pub struct Boost {
    pub upgrade: String,
    pub amount: i32,
}

pub struct Rules {
    pub money: u32,
    pub forbidden: Vec<String>,
    pub building_pickup: bool,
    pub boosts: Vec<Boost>,
    pub animation: String,
    pub animation_seconds: f32,
    pub animation_rise: f32,
}

pub struct Event {
    pub frame: u64,
    pub item: usize,
    pub collector: usize,
    pub position: [f32; 3],
    pub amount: u32,
}

impl Skirmish {
    /// `CrateCollide::isValidToExecute` for the admitted original money crates.
    #[must_use]
    pub fn can_collect_money_crate(&self, collector: usize, item: usize) -> bool {
        let (Some(actor), Some(crate_actor)) =
            (self.entities.get(collector), self.entities.get(item))
        else {
            return false;
        };
        let Some(rules) = &self.catalog[crate_actor.type_index].behavior.money_crate else {
            return false;
        };
        let unit = &self.catalog[actor.type_index];
        let building = rules.building_pickup && unit.kind_of.iter().any(|flag| flag == "STRUCTURE");
        crate_actor.salvage.money_active
            && actor.owner != self.neutral_owner()
            && (!matches!(unit.ai.update, super::UnitAiUpdate::Absent) || building)
            && !unit
                .kind_of
                .iter()
                .any(|flag| flag == "PARACHUTE" || rules.forbidden.contains(flag))
            && actor.alive()
            && !actor.masked()
            && (building || self.height_above_ground(item) <= 0.0)
    }

    pub(in crate::skirmish) fn money_crate_contact(&mut self, left: usize, right: usize) -> bool {
        let (item, collector) = if self.entities[left].salvage.money_active {
            (left, right)
        } else if self.entities[right].salvage.money_active {
            (right, left)
        } else {
            return false;
        };
        if !self.can_collect_money_crate(collector, item) {
            return true;
        }
        let actor = &self.entities[collector];
        let rules = self.catalog[self.entities[item].type_index]
            .behavior
            .money_crate
            .as_ref()
            .expect("money crate module");
        let boost = rules
            .boosts
            .iter()
            .find(|boost| self.has_upgrade(actor.owner, &boost.upgrade))
            .map_or(0, |boost| boost.amount);
        let money = rules.money.wrapping_add_signed(boost);
        let owner = actor.owner;
        let position = self.object_position(item);
        self.deposit_native_money(owner, money);
        self.players[owner].statistics.money_earned = self.players[owner]
            .statistics
            .money_earned
            .wrapping_add(money);
        self.presentation.money_crate_events.push(Event {
            frame: self.tick,
            item,
            collector,
            position,
            amount: money,
        });
        self.destroy_maintenance_object(item);
        self.entities[item].salvage.money_active = false;
        self.observe_ground_partition(item);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{
        Faction, Kind,
        tests::{fixture_type, game},
    };

    fn scenario() -> (Skirmish, usize, usize) {
        let mut game = game();
        let mut crate_type = fixture_type(Faction::Civilian, Kind::Crate);
        crate_type.initial_health = 0.0;
        crate_type.max_health = 0.0;
        crate_type.behavior.money_crate = Some(Rules {
            money: 1500,
            forbidden: vec!["PROJECTILE".into()],
            building_pickup: false,
            boosts: Vec::new(),
            animation: "MoneyPickUp".into(),
            animation_seconds: 4.0,
            animation_rise: 15.0,
        });
        crate::skirmish::tests::register_behavior(&mut crate_type, "MoneyCrateCollide", 0x10);
        let kind = game.catalog.len();
        game.catalog.push(crate_type);
        let item = game.spawn(kind, game.neutral_owner(), [200.0, 200.0], true);
        let unit = game.type_for(0, Kind::Builder).unwrap();
        game.catalog[unit].ai.update = crate::skirmish::UnitAiUpdate::Ordinary;
        let collector = game.spawn(unit, 0, [200.0, 200.0], true);
        (game, item, collector)
    }

    #[test]
    fn money_pickup_credits_collector_without_salvage_science_or_rng_and_consumes_once() {
        let (mut game, item, collector) = scenario();
        let cash = game.players[0].cash;
        let earned = game.players[0].statistics.money_earned;
        let random = game.random;
        assert!(game.can_collect_money_crate(collector, item));
        assert!(game.money_crate_contact(item, collector));
        assert_eq!(game.players[0].cash, cash + 1500);
        assert_eq!(game.players[0].statistics.money_earned, earned + 1500);
        assert_eq!(game.random, random);
        assert!(!game.entities[item].crate_present());
        assert!(!game.money_crate_contact(collector, item));
        assert_eq!(game.players[0].cash, cash + 1500);
        assert_eq!(game.presentation.money_crate_events.len(), 1);
        assert_eq!(game.presentation.money_deposit_events.len(), 1);
        assert_eq!(game.presentation.money_deposit_events[0].owner, 0);
    }

    #[test]
    fn money_pickup_rejects_neutral_projectile_dead_and_airborne_crate_contacts() {
        let (mut game, item, collector) = scenario();
        game.entities[collector].owner = game.neutral_owner();
        assert!(!game.can_collect_money_crate(collector, item));
        game.entities[collector].owner = 0;
        let kind = game.entities[collector].type_index;
        game.catalog[kind].kind_of.push("PROJECTILE".into());
        assert!(!game.can_collect_money_crate(collector, item));
        game.catalog[kind].kind_of.pop();
        game.entities[collector].health = 0.0;
        assert!(!game.can_collect_money_crate(collector, item));
        game.entities[collector].health = game.entities[collector].max_health;
        game.entities[item].placed_height_offset = Some(1.0);
        assert!(!game.can_collect_money_crate(collector, item));
        game.entities[item].placed_height_offset = Some(0.0);
        assert!(game.can_collect_money_crate(collector, item));
    }
}
