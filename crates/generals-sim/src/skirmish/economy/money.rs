//! Native `Money::deposit`, income buckets and `AcademyStats::recordIncome`.
use crate::skirmish::Skirmish;

pub(in crate::skirmish) struct State {
    buckets: [u32; 60],
    current: usize,
    per_minute: u32,
    last_income: u32,
    max_between_income: u32,
}

impl State {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            buckets: [0; 60],
            current: 0,
            per_minute: 0,
            last_income: 0,
            max_between_income: 0,
        }
    }
}

pub struct Event {
    pub owner: usize,
    pub frame: u64,
}

impl Skirmish {
    pub(in crate::skirmish) fn deposit_native_money(&mut self, owner: usize, amount: u32) {
        self.deposit_money(owner, amount, true);
    }

    pub(in crate::skirmish) fn deposit_sale_refund(&mut self, owner: usize, amount: u32) {
        self.deposit_money(owner, amount, false);
    }

    fn deposit_money(&mut self, owner: usize, amount: u32, track_income: bool) {
        if amount == 0 {
            return;
        }
        self.presentation.money_deposit_events.push(Event {
            owner,
            frame: self.tick,
        });
        let now = self.update_frame();
        let player = &mut self.players[owner];
        let money = &mut player.native_money;
        if track_income {
            money.buckets[money.current] = money.buckets[money.current].wrapping_add(amount);
            money.per_minute = money.per_minute.wrapping_add(amount);
        }
        player.cash = player.cash.wrapping_add(amount);
        // Native recordIncome subtracts now from the previous unsigned frame.
        let delta = money.last_income.wrapping_sub(now);
        if delta > money.max_between_income {
            money.max_between_income = delta;
        }
        money.last_income = now;
    }

    pub(in crate::skirmish) fn update_native_money_buckets(&mut self) {
        let next = usize::try_from((self.update_frame() / 30) % 60).expect("native income bucket");
        for player in &mut self.players {
            let money = &mut player.native_money;
            if next != money.current {
                money.per_minute = money.per_minute.wrapping_sub(money.buckets[next]);
                money.current = next;
                money.buckets[next] = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::tests;

    #[test]
    fn sale_refund_sounds_and_records_academy_time_without_counting_as_income() {
        let mut game = tests::game();
        let cash = game.players[0].cash;
        game.tick = 179;
        game.deposit_sale_refund(0, 500);
        assert_eq!(game.players[0].cash, cash + 500);
        assert_eq!(game.players[0].statistics.money_earned, 0);
        assert_eq!(game.players[0].native_money.per_minute, 0);
        assert_eq!(game.players[0].native_money.last_income, 179);
        assert_eq!(game.presentation.money_deposit_events.len(), 1);
        assert_eq!(game.presentation.money_deposit_events[0].owner, 0);
    }

    #[test]
    fn native_deposit_zero_wrap_income_window_and_academy_unsigned_gap() {
        let mut game = tests::game();
        game.players[0].cash = u32::MAX - 2;
        game.tick = 15;
        game.deposit_native_money(0, 0);
        assert!(game.presentation.money_deposit_events.is_empty());
        game.deposit_native_money(0, 5);
        assert_eq!(game.players[0].cash, 2);
        assert_eq!(game.players[0].statistics.money_earned, 0);
        assert_eq!(game.players[0].native_money.per_minute, 5);
        assert_eq!(game.players[0].native_money.buckets[0], 5);
        assert_eq!(game.players[0].native_money.last_income, 15);
        assert_eq!(
            game.players[0].native_money.max_between_income,
            0_u32.wrapping_sub(15)
        );
        assert_eq!(game.presentation.money_deposit_events.len(), 1);
        assert_eq!(game.presentation.money_deposit_events[0].owner, 0);
        game.tick = 30;
        game.update_native_money_buckets();
        assert_eq!(game.players[0].native_money.current, 1);
        assert_eq!(game.players[0].native_money.per_minute, 5);
        game.tick = 1800;
        game.update_native_money_buckets();
        assert_eq!(game.players[0].native_money.current, 0);
        assert_eq!(game.players[0].native_money.per_minute, 0);
        assert_eq!(game.players[0].cash, 2);
    }
}
