//! Original `DynamicGeometryInfoUpdate` delay, inclusive endpoints and reversal.
use num_traits::ToPrimitive;

pub struct Rules {
    pub delay: u32,
    /// Height, major radius, minor radius, in source order.
    pub initial: [f32; 3],
    pub final_size: [f32; 3],
    pub transition: u32,
    pub reverse: bool,
}

pub struct State {
    delay: u32,
    elapsed: u32,
    finished: bool,
    reversed: bool,
}

impl State {
    pub(super) const fn new(rules: &Rules) -> Self {
        Self {
            delay: if rules.delay > 0 { rules.delay } else { 1 },
            elapsed: 0,
            finished: false,
            reversed: false,
        }
    }

    /// Updates the native geometry, returning `None` while delayed or finished.
    pub(super) fn update(&mut self, rules: &Rules) -> Option<[f32; 3]> {
        if self.finished {
            return None;
        }
        if self.delay > 0 {
            self.delay -= 1;
            if self.delay > 0 {
                return None;
            }
        }
        let (initial, end) = if self.reversed {
            (rules.final_size, rules.initial)
        } else {
            (rules.initial, rules.final_size)
        };
        let ratio = self.elapsed.to_f32().expect("native unsigned frame")
            / rules.transition.to_f32().expect("native unsigned duration");
        let geometry = std::array::from_fn(|axis| {
            let delta = ratio * (end[axis] - initial[axis]);
            initial[axis] + delta
        });
        self.elapsed = self.elapsed.wrapping_add(1);
        if self.elapsed > rules.transition {
            if rules.reverse && !self.reversed {
                self.reversed = true;
                self.elapsed = 0;
            } else {
                self.finished = true;
            }
        }
        Some(geometry)
    }

    pub(super) const fn started(&self) -> bool {
        self.delay == 0
    }

    pub(super) const fn reversed(&self) -> bool {
        self.reversed
    }
}

#[cfg(test)]
mod tests;
