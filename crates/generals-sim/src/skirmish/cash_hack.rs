//! Native `CashHackSpecialPower` after the base targeted power callback.
use super::{Skirmish, special_ability::FeedbackEffect};

pub struct Rules {
    pub default_amount: i32,
    pub upgrades: Vec<(String, i32)>,
}

impl Skirmish {
    pub(in crate::skirmish) fn execute_cash_hack(
        &mut self,
        source: usize,
        target: usize,
        ordinal: usize,
    ) {
        let caster = &self.entities[source];
        let rules = &self.catalog[caster.type_index].behavior.cash_hack_powers[ordinal];
        let owner = caster.owner;
        let enemy = self.entities[target].owner;
        let amount = rules
            .upgrades
            .iter()
            .find(|(science, _)| self.players[owner].has_science(science))
            .map_or(rules.default_amount, |(_, amount)| *amount);
        let amount = u32::from_ne_bytes(amount.to_ne_bytes());
        let cash = self.players[enemy].cash.min(amount);
        if cash > 0 {
            self.players[enemy].cash -= cash;
            self.deposit_native_money(owner, cash);
            self.players[owner].statistics.money_earned = self.players[owner]
                .statistics
                .money_earned
                .wrapping_add(cash);
            self.native_ability_feedback(source, target, FeedbackEffect::PowerCashStolen(cash));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Kind, tests::game};

    #[test]
    fn matched_upgrade_takes_remaining_balance_and_preserves_callback_pose_and_income() {
        let mut game = game();
        let kind = game.type_for(0, Kind::CommandCenter).unwrap();
        game.catalog[kind].behavior.cash_hack_powers.push(Rules {
            default_amount: 1000,
            upgrades: vec![("III".into(), 4000), ("II".into(), 2000)],
        });
        let source = game.spawn(kind, 0, [200.0, 150.0], true);
        let target = game.spawn(kind, 1, [400.0, 350.0], true);
        game.players[0].sciences.extend(["II".into(), "III".into()]);
        game.players[0].cash = 7000;
        game.players[1].cash = 3000;
        game.execute_cash_hack(source, target, 0);
        assert_eq!((game.players[0].cash, game.players[1].cash), (10000, 0));
        assert_eq!(game.players[0].statistics.money_earned, 3000);
        assert_eq!(game.presentation.money_deposit_events.len(), 1);
        let event = game.presentation.ability_feedback.first().unwrap();
        assert!(matches!(
            event.effect,
            FeedbackEffect::PowerCashStolen(3000)
        ));
        assert_eq!(
            event.target_position.map(f32::to_bits),
            game.object_position(target).map(f32::to_bits)
        );
        game.execute_cash_hack(source, target, 0);
        assert_eq!(game.presentation.ability_feedback.len(), 1);
        assert_eq!(game.presentation.money_deposit_events.len(), 1);
    }
}
