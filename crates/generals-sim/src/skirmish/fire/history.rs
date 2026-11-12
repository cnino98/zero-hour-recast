//! Fixed `WeaponTemplate::processHistoricDamage` shared by every copy of a weapon.
pub struct Rules {
    pub frames: u32,
    pub count: usize,
    pub radius: f32,
    pub weapon: String,
}

struct Hit {
    frame: u32,
    position: [f32; 2],
    trigger: u32,
}

pub struct History {
    hits: std::collections::VecDeque<Hit>,
    trigger: u32,
}

impl History {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hits: std::collections::VecDeque::new(),
            trigger: 0,
        }
    }

    /// Returns whether this impact fires the authored historical bonus weapon.
    pub fn impact(&mut self, rules: &Rules, now: u32, position: [f32; 2]) -> bool {
        if rules.count == 0 {
            return false;
        }
        let expiration = now.wrapping_sub(rules.frames);
        while self.hits.front().is_some_and(|hit| hit.frame <= expiration) {
            self.hits.pop_front();
        }
        self.trigger = self.trigger.wrapping_add(1);
        let needed = rules.count - 1;
        if self.hits.len() >= needed {
            let squared = rules.radius * rules.radius;
            let mut count = 0;
            for hit in &mut self.hits {
                let delta = [position[0] - hit.position[0], position[1] - hit.position[1]];
                let terms = delta.map(|value| value * value);
                if terms[0] + terms[1] <= squared {
                    hit.trigger = self.trigger;
                    count += 1;
                    if count == needed {
                        self.hits.retain(|hit| hit.trigger != self.trigger);
                        return true;
                    }
                }
            }
        }
        self.hits.push_back(Hit {
            frame: now,
            position,
            trigger: 0,
        });
        false
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
