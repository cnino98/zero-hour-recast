//! Native `Player::doBountyForKill` and its localized cash feedback.
use super::{
    Skirmish,
    progression::{integer_real, real_unsigned},
};

pub struct Event {
    pub frame: u64,
    pub amount: u32,
    pub position: [f32; 3],
}

impl Skirmish {
    pub(super) fn award_cash_bounty(&mut self, owner: usize, victim: usize, killer: Option<usize>) {
        let victim = &self.entities[victim];
        let Some(killer) = killer else {
            return;
        };
        let cost = self.build_cost(victim.owner, victim.type_index);
        // CRC0 uses ceil with WWMath's epsilon at exact integer boundaries.
        let bounty = integer_real(cost) * self.players[owner].cash_bounty;
        let amount = real_unsigned((bounty - 0.0001).ceil().max(0.0));
        if amount == 0 {
            return;
        }
        self.deposit_native_money(owner, amount);
        self.players[owner].statistics.money_earned = self.players[owner]
            .statistics
            .money_earned
            .wrapping_add(amount);
        let mut position = self.object_position(killer);
        position[2] += 10.0;
        self.presentation.cash_bounties.push(Event {
            frame: self.tick,
            amount,
            position,
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::tests;

    #[test]
    fn bounty_deposits_exact_native_amount_and_floats_above_the_killer() {
        let mut game = tests::game();
        let killer = game.spawn(6, 0, [300.0, 300.0], true);
        let victim = game.spawn(5, 1, [400.0, 300.0], true);
        game.catalog[5].cost = 301;
        game.players[0].cash_bounty = 0.05;
        let cash = game.players[0].cash;
        let earned = game.players[0].statistics.money_earned;
        game.award_cash_bounty(0, victim, None);
        assert_eq!(game.players[0].cash, cash);
        assert!(game.presentation.cash_bounties.is_empty());
        game.award_cash_bounty(0, victim, Some(killer));
        assert_eq!(game.players[0].cash, cash + 16);
        assert_eq!(game.players[0].statistics.money_earned, earned + 16);
        assert_eq!(
            game.presentation.money_deposit_events.last().unwrap().owner,
            0
        );
        let event = game.presentation.cash_bounties.last().unwrap();
        assert_eq!(event.amount, 16);
        let [x, y, z] = game.object_position(killer);
        assert_eq!(
            event.position.map(f32::to_bits),
            [x, y, z + 10.0].map(f32::to_bits)
        );
    }
}
