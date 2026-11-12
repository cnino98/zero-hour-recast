//! `ProductionUpdate::updateDoors/setHoldDoorOpen`, including strict frame edges.

fn native_frame(now: u64) -> u32 {
    u32::try_from(now & u64::from(u32::MAX)).expect("masked native frame")
}

pub struct Rules {
    pub count: i32,
    pub opening: u32,
    pub waiting: u32,
    pub closing: u32,
    pub complete: u32,
}

struct Door {
    opened: u32,
    waiting: u32,
    closed: u32,
    clear_flags: u8,
    set_flags: u8,
    held: bool,
}

impl Door {
    pub(super) const fn new() -> Self {
        Self {
            opened: 0,
            waiting: 0,
            closed: 0,
            clear_flags: 0,
            set_flags: 0,
            held: false,
        }
    }

    const fn hold_open(&mut self, now: u32) {
        if self.opened == 0 && self.waiting == 0 && self.closed == 0 {
            self.opened = now;
            self.set_flags |= 1;
        }
    }

    const fn update(&mut self, now: u32, rules: &Rules, hold: bool) {
        if self.opened != 0 {
            if now.wrapping_sub(self.opened) > rules.opening {
                self.opened = 0;
                self.waiting = now;
                self.clear_flags |= 1;
                self.set_flags = (self.set_flags & !1) | 2;
            }
        } else if self.waiting != 0 {
            if now.wrapping_sub(self.waiting) > rules.waiting && !hold {
                self.waiting = 0;
                self.closed = now;
                self.clear_flags |= 2;
                self.set_flags = (self.set_flags & !2) | 4;
            }
        } else if self.closed != 0 && !hold && now.wrapping_sub(self.closed) > rules.closing {
            self.closed = 0;
            self.clear_flags |= 4;
            self.set_flags &= !4;
        }
    }

    const fn exit_ready(&mut self, now: u32) -> bool {
        if self.opened == 0 && self.waiting == 0 && self.closed == 0 {
            self.opened = now;
            self.set_flags |= 1;
        } else if self.waiting != 0 {
            self.waiting = now;
        } else if self.closed != 0 {
            self.waiting = now;
            self.clear_flags |= 5;
            self.set_flags = (self.set_flags & !5) | 2;
        }
        self.waiting != 0
    }
}

pub(in crate::skirmish) struct State {
    doors: [Door; 4],
    complete_at: u32,
    complete_pending: Option<bool>,
}

impl State {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            doors: [const { Door::new() }; 4],
            complete_at: 0,
            complete_pending: None,
        }
    }

    pub(in crate::skirmish) fn set_hold(&mut self, index: usize, hold: bool, now: u64) {
        self.doors[index].held = hold;
        if hold {
            self.doors[index].hold_open(native_frame(now));
        }
    }

    pub(in crate::skirmish) fn update_doors(&mut self, rules: &Rules, now: u64) -> (u16, u16) {
        let mut clear = 0;
        let mut set = 0;
        let now = native_frame(now);
        if rules.count > 0 {
            for door in &mut self.doors {
                door.update(now, rules, door.held);
            }
        }
        // Dirty model flags are delivered even when door animation timers are disabled.
        for (index, door) in self.doors.iter_mut().enumerate() {
            clear |= u16::from(door.clear_flags) << (index * 3);
            set |= u16::from(door.set_flags) << (index * 3);
            door.clear_flags = 0;
            door.set_flags = 0;
        }
        if self.complete_at != 0 && now.wrapping_sub(self.complete_at) > rules.complete {
            self.complete_at = 0;
            self.complete_pending = Some(false);
        }
        if let Some(complete) = self.complete_pending.take() {
            if complete {
                set |= 1 << 12;
            } else {
                clear |= 1 << 12;
            }
        }
        (clear, set)
    }

    pub(in crate::skirmish) fn exit_ready(
        &mut self,
        door: Option<usize>,
        rules: &Rules,
        now: u64,
    ) -> bool {
        let now = native_frame(now);
        if self.complete_at == 0 {
            self.complete_at = now;
            self.complete_pending = Some(true);
        }
        if rules.count <= 0 {
            return true;
        }
        door.is_none_or(|index| self.doors[index].exit_ready(now))
    }
}

/// Shared Drawable flags are changed only by an instance's delivered dirty bits.
pub(in crate::skirmish) fn condition_names(bits: u16) -> Vec<&'static str> {
    const NAMES: [&str; 13] = [
        "DOOR_1_OPENING",
        "DOOR_1_WAITING_OPEN",
        "DOOR_1_CLOSING",
        "DOOR_2_OPENING",
        "DOOR_2_WAITING_OPEN",
        "DOOR_2_CLOSING",
        "DOOR_3_OPENING",
        "DOOR_3_WAITING_OPEN",
        "DOOR_3_CLOSING",
        "DOOR_4_OPENING",
        "DOOR_4_WAITING_OPEN",
        "DOOR_4_CLOSING",
        "CONSTRUCTION_COMPLETE",
    ];
    NAMES
        .into_iter()
        .enumerate()
        .filter_map(|(index, name)| (bits & (1 << index) != 0).then_some(name))
        .collect()
}

impl Rules {
    pub const NONE: Self = Self {
        count: 0,
        opening: 0,
        waiting: 0,
        closing: 0,
        complete: 0,
    };
}

impl crate::skirmish::Skirmish {
    pub(in crate::skirmish) fn update_production_doors(&mut self, producer: usize, ordinal: usize) {
        let actor = &mut self.entities[producer];
        let rules = self.catalog[actor.type_index].production_doors_at(ordinal);
        let (clear, set) = actor
            .production_doors_at_mut(ordinal)
            .update_doors(rules, self.tick);
        actor.production_door_flags = (actor.production_door_flags & !clear) | set;
    }

    pub(in crate::skirmish) fn production_exit_ready(
        &mut self,
        producer: usize,
        ordinal: usize,
    ) -> bool {
        let actor = &self.entities[producer];
        let product = match actor
            .production_queue_at(ordinal)
            .front()
            .expect("production queue")
            .item
        {
            crate::skirmish::ProductionItem::Unit(index) => index,
            crate::skirmish::ProductionItem::Upgrade(_) => {
                unreachable!("research does not reserve exit doors")
            }
        };
        if actor.parking.is_some()
            && actor.production_queue_at(ordinal)[0].parking_door.is_none()
            && !self.catalog[product]
                .kind_of
                .iter()
                .any(|flag| flag == "PRODUCED_AT_HELIPAD")
        {
            let Ok(door) = self.queue_parking_door(producer, product) else {
                return false;
            };
            self.entities[producer].production_queue_at_mut(ordinal)[0].parking_door = door;
        }
        let actor = &mut self.entities[producer];
        let rules = &self.catalog[actor.type_index];
        let door = if actor.parking.is_some() {
            actor
                .production_queue_at(ordinal)
                .front()
                .expect("production queue")
                .parking_door
        } else {
            // Default/Queue/SupplyCenter exits reserve DOOR_1; Queue's busy
            // exit must not animate a door before it becomes available.
            if rules
                .behavior
                .production_exit
                .as_ref()
                .is_some_and(|exit| exit.queue.is_some())
                && !actor.queue_exit.free()
            {
                return false;
            }
            Some(0)
        };
        actor.production_doors_at_mut(ordinal).exit_ready(
            door,
            rules.production_doors_at(ordinal),
            self.tick,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Rules, State, condition_names};

    #[test]
    fn zero_animation_count_delivers_explicit_hold_flags_without_stepping_timers() {
        let mut state = State::new();
        state.set_hold(0, true, 1);
        let (clear, set) = state.update_doors(&Rules::NONE, 2);
        assert_eq!(clear, 0);
        assert_eq!(condition_names(set), ["DOOR_1_OPENING"]);
        state.set_hold(0, false, 3);
        assert_eq!(state.update_doors(&Rules::NONE, 100), (0, 0));
    }

    #[test]
    fn door_and_completion_expiry_use_native_unsigned_frame_wrap() {
        let mut state = State::new();
        let rules = Rules {
            count: 1,
            opening: 2,
            waiting: 1,
            closing: 1,
            complete: 2,
        };
        let start = u64::from(u32::MAX) - 1;
        assert!(!state.exit_ready(Some(0), &rules, start));
        let (_, mut flags) = state.update_doors(&rules, start + 2);
        assert!(condition_names(flags).contains(&"DOOR_1_OPENING"));
        assert!(condition_names(flags).contains(&"CONSTRUCTION_COMPLETE"));
        let (clear, set) = state.update_doors(&rules, start + 3);
        flags = (flags & !clear) | set;
        assert_eq!(condition_names(flags), ["DOOR_1_WAITING_OPEN"]);
    }

    #[test]
    fn opening_one_module_does_not_clear_another_modules_closing_flag() {
        let mut state = State::new();
        state.set_hold(0, true, 1);
        let (clear, set) = state.update_doors(&Rules::NONE, 2);
        let flags = (4 & !clear) | set;
        assert_eq!(condition_names(flags), ["DOOR_1_OPENING", "DOOR_1_CLOSING"]);
    }

    #[test]
    fn expired_pending_completion_still_clears_another_instances_shared_flag() {
        let mut state = State::new();
        assert!(state.exit_ready(None, &Rules::NONE, 1));
        let (clear, set) = state.update_doors(&Rules::NONE, 2);
        assert_eq!(((1 << 12) & !clear) | set, 0);
    }
}
