//! `HealContain::{update,doHeal}` and `ActionManager` admission, SH 2bfaea60.
use super::Skirmish;

pub struct HealRules {
    pub full_heal_frames: u32,
    pub allow_enemies: bool,
    pub allow_neutral: bool,
}

impl Skirmish {
    #[must_use]
    pub fn is_heal_container(&self, carrier: usize) -> bool {
        self.container_rules(carrier)
            .is_some_and(|rules| rules.heal.is_some())
    }

    pub(in crate::skirmish) fn heal_command_admits(&self, id: usize, carrier: usize) -> bool {
        if !self.is_heal_container(carrier) {
            return true;
        }
        let Some(rider) = self.entities.get(id) else {
            return false;
        };
        let host = &self.entities[carrier];
        if rider.health.to_bits() == rider.max_health.to_bits()
            || self.catalog[rider.type_index].transport_slots == 0
        {
            return false;
        }
        // ActionManager's manual entry restriction is stricter than the
        // OpenContain relation flags, including for allied faction structures.
        rider.owner == host.owner
            || (self.passengers(carrier).is_empty()
                && !self.catalog[host.type_index].kind_of.iter().any(|kind| {
                    matches!(
                        kind.as_str(),
                        "FS_FACTORY"
                            | "FS_BASE_DEFENSE"
                            | "FS_TECHNOLOGY"
                            | "FS_SUPPLY_DROPZONE"
                            | "FS_SUPERWEAPON"
                            | "FS_BLACK_MARKET"
                            | "FS_SUPPLY_CENTER"
                            | "FS_STRATEGY_CENTER"
                            | "FS_FAKE"
                            | "FS_INTERNET_CENTER"
                            | "FS_ADVANCED_TECH"
                            | "FS_BARRACKS"
                            | "FS_WARFACTORY"
                            | "FS_AIRFIELD"
                    )
                }))
    }

    pub(in crate::skirmish) fn update_heal_container(&mut self, carrier: usize) -> i32 {
        self.update_open_contain(carrier);
        let frames = self
            .container_rules(carrier)
            .and_then(|rules| rules.heal.as_ref())
            .expect("HealContain data")
            .full_heal_frames;
        for id in self.passengers(carrier).to_vec() {
            let actor = &self.entities[id];
            let elapsed =
                self.update_frame()
                    .wrapping_sub(crate::skirmish::ai_dispatch::native_frame(
                        actor.containment.entered_at,
                    ));
            let done = elapsed >= frames;
            let amount = if done {
                actor.max_health
            } else {
                actor.max_health / crate::skirmish::progression::integer_real(frames)
            };
            self.apply_healing(id, Some(carrier), amount);
            if done {
                // OpenContain::reserveDoorForExit always supplies DOOR_1.
                self.exit_spawned_from_container(id, carrier);
            }
        }
        1
    }
}

#[cfg(test)]
mod tests;
