//! SH `WeaponBonusUpdate` pulse and contained unit callback.
use super::Skirmish;

pub struct Rules {
    pub required: Vec<String>,
    pub forbidden: Vec<String>,
    pub duration: u32,
    pub repeat: u32,
    pub range: f32,
    pub bonus: u8,
}

impl Skirmish {
    pub(in crate::skirmish) fn update_weapon_bonus(
        &mut self,
        source: usize,
        ordinal: usize,
    ) -> i32 {
        let actor = &self.entities[source];
        let kind = actor.type_index;
        let rules = &self.catalog[kind].behavior.weapon_bonuses[ordinal];
        let position = actor.position;
        let range_squared = rules.range * rules.range;
        let duration = rules.duration;
        let repeat = rules.repeat;
        let bonus = rules.bonus;
        let targets: Vec<_> = self
            .partition
            .fastest_range_objects(position, rules.range)
            .into_iter()
            .filter(|&id| {
                let target = &self.entities[id];
                let dx = target.position[0] - position[0];
                let dy = target.position[1] - position[1];
                let xx = dx * dx;
                let yy = dy * dy;
                !target.is_effectively_dead()
                    && target.is_off_map() == actor.is_off_map()
                    && !actor.helpers.defection.undetected
                    && !target.helpers.defection.undetected
                    && self.object_relationship(source, id) == 2
                    && xx + yy < range_squared
            })
            .collect();
        for target in targets {
            if self.matches_weapon_bonus_mask(target, kind, ordinal) {
                self.do_temp_weapon_bonus(target, bonus, duration);
            }
            // Only the host receives the partition-level relationship, life and
            // map predicates. OpenContain visits the actual held list forward.
            for passenger in self.passengers(target).to_vec() {
                if self.matches_weapon_bonus_mask(passenger, kind, ordinal) {
                    self.do_temp_weapon_bonus(passenger, bonus, duration);
                }
            }
        }
        i32::from_ne_bytes(repeat.to_ne_bytes())
    }

    fn matches_weapon_bonus_mask(&self, target: usize, kind: usize, ordinal: usize) -> bool {
        let rules = &self.catalog[kind].behavior.weapon_bonuses[ordinal];
        let flags = &self.catalog[self.entities[target].type_index].kind_of;
        rules.required.iter().all(|kind| flags.contains(kind))
            && rules.forbidden.iter().all(|kind| !flags.contains(kind))
    }
}
