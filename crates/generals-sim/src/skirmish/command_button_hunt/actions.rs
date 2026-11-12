//! SH ActionManager.cpp96,1002–1309,1623–1913: source-aware object power admission.
use crate::skirmish::{
    Skirmish, ai_goals::COMMAND_FROM_SCRIPT, special_ability::PowerType, visibility::Status,
};

impl Skirmish {
    pub(in crate::skirmish) fn action_target_shrouded(
        &self,
        id: usize,
        target: usize,
        source: i32,
    ) -> bool {
        let owner = self.entities[id].owner;
        !self.computer_player(owner)
            && source != COMMAND_FROM_SCRIPT
            && matches!(
                self.visibility_status(owner, target),
                Status::Fogged | Status::Shrouded
            )
    }

    fn native_has_power_type(&self, id: usize, kind: PowerType) -> bool {
        self.catalog[self.entities[id].type_index]
            .special_powers
            .iter()
            .any(|power| PowerType::parse(&power.power_type) == Some(kind))
    }

    fn native_power_of_type(&self, id: usize, kind: PowerType) -> Option<&str> {
        self.catalog[self.entities[id].type_index]
            .special_powers
            .iter()
            .find(|power| PowerType::parse(&power.power_type) == Some(kind))
            .map(|power| power.name.as_str())
    }

    fn action_appears_friendly(&self, id: usize, target: usize) -> bool {
        self.container_apparent_owner(target, self.entities[id].owner)
            .is_some_and(|owner| self.object_player_relationship(id, owner) != 0)
    }

    fn action_undetected(&self, target: usize) -> bool {
        let actor = &self.entities[target];
        actor.stealthed() && !actor.detected() && !actor.has_native_disguise()
    }

    pub(in crate::skirmish) fn can_special_power_at_object_from(
        &mut self,
        id: usize,
        name: &str,
        kind: PowerType,
        target: usize,
        source: i32,
    ) -> bool {
        if !self.native_has_power_type(id, kind)
            || self.entities[target].is_effectively_dead()
            || self.special_power_ordinal(id, name).is_none()
            || self.special_power_percent(id, name) < 1.0
            || self.action_target_shrouded(id, target, source)
        {
            return false;
        }
        let enemy = self.object_relationship(id, target) == 0;
        match kind {
            PowerType::SpecialBattleshipBombardment => self.object_relationship(id, target) != 2,
            PowerType::SpecialTankhunterTntAttack => {
                self.attack_kind(target, "STRUCTURE")
                    || self.attack_kind(target, "VEHICLE") && !self.attack_kind(target, "AIRCRAFT")
            }
            PowerType::SpecialBoobyTrap => self.attack_kind(target, "STRUCTURE") && !enemy,
            PowerType::SpecialMissileDefenderLaserGuidedMissiles => {
                self.attack_kind(target, "VEHICLE") && enemy
            }
            PowerType::SpecialHackerDisableBuilding => {
                self.attack_kind(target, "STRUCTURE")
                    && enemy
                    && self.attack_kind(target, "CAPTURABLE")
                    && !self.attack_kind(target, "REBUILD_HOLE")
            }
            PowerType::SpecialInfantryCaptureBuilding
            | PowerType::SpecialBlacklotusCaptureBuilding => {
                self.action_capture_building(id, target)
            }
            PowerType::SpecialBlacklotusDisableVehicleHack => {
                let actor = &self.entities[target];
                let airborne = self.height_above_ground(target)
                    > actor
                        .active_locomotor(&self.catalog[actor.type_index])
                        .airborne_targeting_height;
                !self.attack_kind(target, "AIRCRAFT")
                    && !airborne
                    && enemy
                    && self.attack_kind(target, "VEHICLE")
                    && !self.action_undetected(target)
                    && !self.action_appears_friendly(id, target)
            }
            PowerType::SpecialBlacklotusStealCashHack => self.action_steal_cash(id, target),
            PowerType::SpecialCashHack => {
                self.attack_kind(target, "STRUCTURE")
                    && enemy
                    && self.attack_kind(target, "CAPTURABLE")
                    && !self.attack_kind(target, "REBUILD_HOLE")
                    && !self.entities[target].under_construction()
                    && self.attack_kind(target, "CASH_GENERATOR")
            }
            PowerType::SpecialDisguiseAsVehicle => {
                self.attack_kind(target, "VEHICLE")
                    && !self.attack_kind(target, "AIRCRAFT")
                    && !self.attack_kind(target, "BOAT")
                    && !self.attack_kind(target, "CLIFF_JUMPER")
                    && !self.catalog[self.entities[target].type_index]
                        .behavior_modules
                        .iter()
                        .any(|module| module.family == "RailroadBehavior")
            }
            PowerType::SpecialDefector => !self.attack_kind(target, "STRUCTURE") && enemy,
            PowerType::SpecialRemoteCharges
            | PowerType::SpecialTimedCharges
            | PowerType::SpecialHelixNapalmBomb => {
                !self.attack_kind(target, "BRIDGE")
                    && !self.attack_kind(target, "BRIDGE_TOWER")
                    && (self.attack_kind(target, "STRUCTURE")
                        || self.attack_kind(target, "VEHICLE"))
                    && self.native_charge_target_available(id, kind, target)
            }
            _ => false,
        }
    }

    fn action_capture_building(&mut self, id: usize, target: usize) -> bool {
        if self.attack_kind(target, "IMMUNE_TO_CAPTURE") {
            return false;
        }
        let name = self
            .native_power_of_type(id, PowerType::SpecialInfantryCaptureBuilding)
            .or_else(|| self.native_power_of_type(id, PowerType::SpecialBlacklotusCaptureBuilding))
            .map(str::to_owned);
        let Some(name) = name else {
            return false;
        };
        if self.special_power_percent(id, &name) < 1.0 {
            return false;
        }
        let victim = &self.entities[target];
        let relation = self.object_relationship(id, target);
        if !self.attack_kind(target, "STRUCTURE")
            || victim.under_construction()
            || victim.sold_model()
            || !(relation == 0 || self.attack_kind(target, "CAPTURABLE") && relation != 2)
            || self.action_undetected(target)
        {
            return false;
        }
        if self.is_garrison(target)
            && self
                .passengers(target)
                .iter()
                .any(|&rider| !self.attack_kind(rider, "STEALTH_GARRISON"))
        {
            return false;
        }
        !self.action_appears_friendly(id, target)
    }

    fn action_steal_cash(&mut self, id: usize, target: usize) -> bool {
        let Some(name) = self
            .native_power_of_type(id, PowerType::SpecialBlacklotusStealCashHack)
            .map(str::to_owned)
        else {
            return false;
        };
        self.special_power_percent(id, &name) >= 1.0
            && !self.entities[target].under_construction()
            && self.object_relationship(id, target) == 0
            && self.attack_kind(target, "CASH_GENERATOR")
            && self.attack_kind(target, "CAPTURABLE")
            && !self.attack_kind(target, "REBUILD_HOLE")
            && !self.action_undetected(target)
            && !self.action_appears_friendly(id, target)
    }
}
