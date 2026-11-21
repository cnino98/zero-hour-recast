//! `PropagandaTowerBehavior` scan/list membership, per-frame sole-benefactor healing.
use crate::skirmish::{Skirmish, movement::distance_squared};
#[cfg(test)]
mod tests;

pub struct Propaganda {
    pub radius: f32,
    pub delay: u32,
    pub heal: [f32; 2],
    pub upgrade: String,
    pub pulse: [String; 2],
    pub affects_self: bool,
}

pub struct Pulse {
    pub source: usize,
    pub upgraded: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn update_propaganda_module(&mut self, source: usize) -> i32 {
        let actor = &self.entities[source];
        if actor.under_construction() {
            return 1;
        }
        if actor.selling() {
            self.remove_propaganda(source);
            return i32::from_ne_bytes(crate::skirmish::module_updates::FOREVER.to_ne_bytes());
        }
        if actor.is_effectively_dead() {
            return i32::from_ne_bytes(crate::skirmish::module_updates::FOREVER.to_ne_bytes());
        }
        let mut child = source;
        let mut enclosed = false;
        while let Some(parent) = self.entities[child].contained_by() {
            let rider = &self.entities[child];
            if rider.attachment_parent().is_none() && !rider.parachute_rider() && !rider.stationed()
            {
                enclosed = true;
                break;
            }
            child = parent;
        }
        if self.object_disabled_mask(source) & !(1 << 3) != 0 || enclosed {
            self.remove_propaganda(source);
            return 1;
        }
        let rules = self.catalog[actor.type_index]
            .behavior
            .propaganda
            .as_ref()
            .expect("bound PropagandaTowerBehavior");
        let upgraded = self.has_upgrade(actor.owner, &rules.upgrade);
        let now = self.update_frame();
        let scanned = u32::try_from(actor.attachment.scanned).expect("native scan frame");
        if now.wrapping_sub(scanned) >= rules.delay {
            self.scan_propaganda(source, upgraded);
        }
        let rules = self.catalog[self.entities[source].type_index]
            .behavior
            .propaganda
            .as_ref()
            .expect("propaganda data");
        let heal = rules.heal[usize::from(upgraded)];
        let deadline = u64::from(now.wrapping_add(rules.delay));
        let influence = std::mem::take(&mut self.entities[source].attachment.influence);
        for target in influence {
            let actor = &self.entities[target];
            let unit = &self.catalog[actor.type_index];
            if !actor.exists()
                || !(unit.scoring.create || unit.scoring.destroy || unit.scoring.victory)
            {
                continue;
            }
            self.entities[source].attachment.influence.push(target);
            let unit = &self.catalog[self.entities[target].type_index];
            if unit.weapon.damage > 0.0
                || unit
                    .secondary_weapon
                    .as_ref()
                    .is_some_and(|weapon| weapon.damage > 0.0)
            {
                self.propaganda_bonus(target, 1, true);
                if upgraded {
                    self.propaganda_bonus(target, 2, true);
                }
            }
            let actor = &mut self.entities[target];
            if u64::from(now) > actor.maintenance.healing_benefactor_until
                || actor.maintenance.healing_benefactor == Some(source)
            {
                let amount = heal / 30.0 * actor.max_health;
                actor.maintenance.healing_benefactor = Some(source);
                actor.maintenance.healing_benefactor_until = deadline;
                self.apply_healing(target, Some(source), amount);
            }
        }
        1
    }

    pub(in crate::skirmish) fn capture_propaganda(&mut self, id: usize) {
        if self.catalog[self.entities[id].type_index]
            .behavior
            .propaganda
            .is_none()
        {
            return;
        }
        let delay = if self.entities[id].owner == self.neutral_owner() {
            self.remove_propaganda(id);
            crate::skirmish::module_updates::FOREVER
        } else {
            1
        };
        self.wake_family_update(id, "PropagandaTowerBehavior", 0, delay);
    }

    fn propaganda_bonus(&mut self, target: usize, bit: u8, enabled: bool) {
        let extra = self.passenger_bonus_flags(target);
        let actor = &mut self.entities[target];
        if (actor.attachment.bonuses & bit != 0) == enabled {
            return;
        }
        if enabled {
            actor.attachment.bonuses |= bit;
        } else {
            actor.attachment.bonuses &= !bit;
        }
        actor.weapon_bonus_changed(
            &self.catalog[actor.type_index],
            self.tick,
            extra,
            &mut self.random,
        );
    }

    fn scan_propaganda(&mut self, source: usize, upgraded: bool) {
        let actor = &self.entities[source];
        let rules = self.catalog[actor.type_index]
            .behavior
            .propaganda
            .as_ref()
            .expect("propaganda module");
        let mut inside = Vec::new();
        for id in self
            .partition
            .fastest_range_objects(actor.position, rules.radius)
        {
            let candidate = &self.entities[id];
            if !candidate.is_effectively_dead()
                && self.cell_at(actor.position).is_some()
                    == self.cell_at(candidate.position).is_some()
                && self.allied(actor.owner, candidate.owner)
                && !self.catalog[candidate.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "STRUCTURE")
                && (id != source || rules.affects_self)
                && distance_squared(actor.position, candidate.position)
                    < rules.radius * rules.radius
            {
                inside.insert(0, id);
            }
        }
        for id in std::mem::replace(
            &mut self.entities[source].attachment.influence,
            inside.clone(),
        ) {
            if !inside.contains(&id) {
                self.propaganda_bonus(id, 1, false);
                self.propaganda_bonus(id, 2, false);
            }
        }
        self.entities[source].attachment.scanned = u64::from(self.update_frame());
        self.presentation
            .propaganda_events
            .push(Pulse { source, upgraded });
    }

    pub(in crate::skirmish) fn remove_propaganda(&mut self, source: usize) {
        for id in std::mem::take(&mut self.entities[source].attachment.influence) {
            self.propaganda_bonus(id, 1, false);
            self.propaganda_bonus(id, 2, false);
        }
    }
}
