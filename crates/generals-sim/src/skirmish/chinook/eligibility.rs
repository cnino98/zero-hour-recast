//! `ActionManager::canEnterObject` modes used by Chinook's native virtual commands.
use super::Skirmish;
impl Skirmish {
    pub(in crate::skirmish) fn action_enter_admits(
        &self,
        id: usize,
        target: usize,
        source: i32,
        combat_drop: bool,
    ) -> bool {
        let (Some(actor), Some(host)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        if id == target
            || !host.exists()
            || host.is_effectively_dead()
            || (!self.computer_player(actor.owner)
                && source != 1
                && !self.maintenance_target_visible(actor.owner, target))
            || actor.under_construction()
            || host.under_construction()
            || host.selling()
            || self.attack_kind(id, "IGNORED_IN_GUI")
            || self.attack_kind(id, "MOB_NEXUS")
            || self.attack_kind(target, "IGNORED_IN_GUI")
            || host.subdued()
            || self.attack_kind(id, "STRUCTURE")
            || self.attack_kind(id, "IMMOBILE")
        {
            return false;
        }
        if self.attack_kind(id, "INFANTRY")
            && host.unmanned()
            && !self.attack_kind(id, "REJECT_UNMANNED")
        {
            return true;
        }
        if self.attack_kind(id, "AIRCRAFT") && self.attack_kind(target, "FS_AIRFIELD") {
            let deck = actor
                .producer
                .and_then(|producer| self.entities.get(producer))
                .filter(|producer| producer.exists())
                .and_then(|producer| self.catalog[producer.type_index].parking.as_ref())
                .map_or(0.0, |rules| rules.deck_height);
            if actor.object_status & (1 << 29) != 0 && deck >= self.object_position(id)[2]
                || self.height_above_ground(id) <= 0.0
            {
                return false;
            }
            return actor.owner == host.owner
                && host.parking.as_ref().is_some_and(|parking| {
                    parking.space_index(id).is_some()
                        || !self.attack_kind(id, "PRODUCED_AT_HELIPAD")
                            && self.parking_available(target, actor.type_index)
                });
        }
        if self.car_bomb_accepts(id, target)
            || self.hijacker_accepts(id, target)
            || self.can_sabotage(id, target)
        {
            return true;
        }
        if self.catalog[host.type_index].contain_family().is_none() {
            return false;
        }
        if self.is_heal_container(target)
            && actor.health.partial_cmp(&actor.max_health) == Some(std::cmp::Ordering::Equal)
        {
            return false;
        }
        let faction_structure = self.catalog[host.type_index].kind_of.iter().any(|kind| {
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
        });
        if combat_drop {
            return !faction_structure;
        }
        if host.owner != actor.owner
            && (faction_structure
                || self
                    .passengers(target)
                    .iter()
                    .any(|&rider| !self.entities[rider].stealthed()))
        {
            return false;
        }
        self.native_container_admits(id, target, false)
    }
}
