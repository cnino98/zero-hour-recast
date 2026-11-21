use super::super::{Production, ProductionItem, Skirmish};

pub struct Research {
    pub name: String,
    pub cost: u32,
    pub frames: u32,
    pub player: bool,
    pub sciences: Vec<String>,
    pub ignores_underpowered: bool,
}

pub struct ResearchEvent {
    pub frame: u64,
    pub producer: usize,
    pub type_index: usize,
    pub upgrade: usize,
    pub owner: usize,
    pub position: [f32; 3],
}

impl Skirmish {
    /// `Object::giveUpgrade` inserts the object mask before `updateUpgradeModules`.
    pub(in crate::skirmish) fn grant_native_object_upgrade(&mut self, id: usize, name: &str) {
        if !self.entities[id]
            .maintenance
            .upgrades
            .iter()
            .any(|upgrade| upgrade == name)
        {
            self.entities[id].maintenance.upgrades.push(name.into());
        }
        self.activate_upgrade_modules(id);
    }

    /// `Object::removeUpgrade` clears only the object mask, then visits authored UPGRADE interfaces.
    pub(in crate::skirmish) fn remove_native_object_upgrade(&mut self, id: usize, name: &str) {
        self.entities[id]
            .maintenance
            .upgrades
            .retain(|upgrade| upgrade != name);
        let level = match name {
            "Upgrade_Veterancy_VETERAN" => Some(1),
            "Upgrade_Veterancy_ELITE" => Some(2),
            "Upgrade_Veterancy_HEROIC" => Some(3),
            _ => None,
        };
        if let Some(level) = level {
            self.entities[id].veterancy_upgrades &= !(1 << level);
        }
        let type_index = self.entities[id].type_index;
        for index in 0..self.update_objects[self.entities[id].update_owner].constructed_behaviors {
            let modules = &self.catalog[type_index].behavior_modules;
            let module = &modules[index];
            // UpgradeMux::resetUpgrade is a proven no-op unless activation masks overlap.
            if module.interfaces & 0x80 == 0
                || !module
                    .upgrade
                    .triggers
                    .iter()
                    .any(|trigger| trigger == name)
            {
                continue;
            }
            let ordinal = modules[..index]
                .iter()
                .filter(|prior| prior.family == module.family)
                .count();
            let family = module.family.clone();
            let object = self.entities[id].update_owner;
            if !self.update_objects[object].upgrade_executed[index] {
                continue;
            }
            self.update_objects[object].upgrade_executed[index] = false;
            match family.as_str() {
                "FireWeaponWhenDamagedBehavior" => self.reset_damage_weapon_upgrade(id, ordinal),
                "FireWeaponWhenDeadBehavior" => {
                    self.entities[id].behaviors.death_weapons[ordinal] = false;
                }
                "FXListDie" => self.entities[id].behaviors.death_fx[ordinal] = false,
                "CountermeasuresBehavior" => self.reset_countermeasure_upgrade(id, ordinal),
                "SpyVisionUpdate" => self.reset_spy_vision_upgrade(id, ordinal),
                "RadarUpgrade" => self.reset_radar_upgrade(id, ordinal),
                "AutoHealBehavior" => {
                    self.entities[id].maintenance.auto_heal[ordinal].active = false;
                }
                _ => {} // UpgradeMux::resetUpgrade never undoes the applied effect.
            }
        }
    }

    #[must_use]
    pub fn has_upgrade(&self, owner: usize, name: &str) -> bool {
        self.players
            .get(owner)
            .is_some_and(|player| player.upgrades.iter().any(|upgrade| upgrade == name))
    }

    #[must_use]
    pub fn object_has_upgrade(&self, id: usize, name: &str) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            let level = match name {
                "Upgrade_Veterancy_VETERAN" => Some(1),
                "Upgrade_Veterancy_ELITE" => Some(2),
                "Upgrade_Veterancy_HEROIC" => Some(3),
                _ => None,
            };
            level.is_some_and(|level| actor.veterancy_upgrades & (1 << level) != 0)
                || actor
                    .maintenance
                    .upgrades
                    .iter()
                    .any(|upgrade| upgrade == name)
        })
    }

    #[must_use]
    pub fn can_research(&self, producer: usize, index: usize) -> bool {
        let Some(actor) = self.entities.get(producer) else {
            return false;
        };
        let Some(upgrade) = self.catalog[actor.type_index]
            .maintenance
            .research
            .get(index)
        else {
            return false;
        };
        let disabled = self.object_disabled_mask(producer);
        actor.complete()
            && upgrade.sciences.iter().all(|science| self.players[actor.owner].has_science(science))
            // ControlBar::getCommandAvailability allows the authored exception
            // only when energy's UNDERPOWERED bit is the sole reason.
            && (disabled == 0 || upgrade.ignores_underpowered && disabled == 1 << 6)
            && self.production_queue_available(producer)
            && self.attachment_research_available(producer, &upgrade.name)
            && self.drone_research_available(producer, &upgrade.name)
            && self.players[actor.owner].cash >= upgrade.cost
            && !(if upgrade.player {
                self.has_upgrade(actor.owner, &upgrade.name)
            } else {
                self.object_has_upgrade(producer, &upgrade.name)
            })
            && (!upgrade.player
                || !self.players[actor.owner]
                    .upgrades_in_progress
                    .contains(&upgrade.name))
            && !actor.queue.iter().any(|entry| match entry.item {
                ProductionItem::Upgrade(index) => {
                    self.catalog[actor.type_index].maintenance.research[index].name == upgrade.name
                }
                ProductionItem::Unit(_) => false,
            })
    }

    /// # Errors
    /// Rejects unsupported, completed, duplicate or unaffordable research.
    pub fn queue_upgrade(&mut self, producer: usize, index: usize) -> Result<(), &'static str> {
        self.ready_actor(producer)?;
        if !self.can_research(producer, index) {
            return Err("Upgrade cannot be researched");
        }
        let actor = &mut self.entities[producer];
        let upgrade = &self.catalog[actor.type_index].maintenance.research[index];
        self.players[actor.owner].cash -= upgrade.cost;
        let seconds = super::super::progression::integer_real(upgrade.frames) / 30.0;
        actor.queue.push_back(Production {
            quantity: 1,
            produced: 0,
            parking_door: None,
            id: -1,
            item: ProductionItem::Upgrade(index),
            elapsed_frames: 0.0,
            remaining_seconds: seconds,
            total_seconds: seconds,
        });
        actor.production_added();
        // queueUpgrade calls Player::addUpgrade(IN_PRODUCTION) for both types.
        let progress = &mut self.players[actor.owner].upgrades_in_progress;
        if !progress.contains(&upgrade.name) {
            progress.push(upgrade.name.clone());
        }
        Ok(())
    }

    pub(in crate::skirmish) fn finish_research(&mut self, producer: usize, index: usize) {
        self.presentation.research_events.push(ResearchEvent {
            frame: self.tick,
            producer,
            type_index: self.entities[producer].type_index,
            upgrade: index,
            owner: self.entities[producer].owner,
            position: self.object_position(producer),
        });
        let actor = &mut self.entities[producer];
        let upgrade = &self.catalog[actor.type_index].maintenance.research[index];
        // ProductionUpdate books research spending only on successful completion.
        self.players[actor.owner].statistics.money_spent += upgrade.cost;
        let player_upgrade = upgrade.player;
        let name = upgrade.name.clone();
        let owner = actor.owner;
        if player_upgrade {
            self.complete_player_upgrade(owner, &name);
        } else {
            self.grant_native_object_upgrade(producer, &name);
        }
        self.update_power();
    }

    /// # Errors
    /// Rejects a stale upgrade entry or a foreign producer.
    pub fn cancel_upgrade(
        &mut self,
        owner: usize,
        producer: usize,
        index: usize,
    ) -> Result<(), &'static str> {
        if self.end_game_at.is_some() {
            return Err("Match has ended");
        }
        let actor = self
            .entities
            .get(producer)
            .filter(|actor| actor.alive() && actor.owner == owner)
            .ok_or("Invalid upgrade producer")?;
        let entry = actor
            .queue
            .iter()
            .position(
                |entry| matches!(entry.item, ProductionItem::Upgrade(value) if value == index),
            )
            .ok_or("Upgrade is no longer in production")?;
        self.cancel_upgrade_entry(producer, 0, entry)
    }

    pub(in crate::skirmish) fn cancel_upgrade_entry(
        &mut self,
        producer: usize,
        ordinal: usize,
        entry: usize,
    ) -> Result<(), &'static str> {
        let actor = &self.entities[producer];
        let ProductionItem::Upgrade(index) = actor.production_queue_at(ordinal)[entry].item else {
            unreachable!("upgrade cancellation requires upgrade production");
        };
        let owner = actor.owner;
        let upgrade = &self.catalog[actor.type_index].maintenance.research[index];
        if upgrade.player
            && !self.players[owner]
                .upgrades_in_progress
                .contains(&upgrade.name)
        {
            return Err("Player upgrade is not in production");
        }
        let name = upgrade.player.then(|| upgrade.name.clone());
        self.players[owner].cash = self.players[owner].cash.wrapping_add(upgrade.cost);
        self.remove_production_entry(producer, ordinal, entry);
        // cancelUpgrade removes Player status only for player upgrades, after
        // removing the queue entry. Object upgrades retain its native mask.
        if let Some(name) = name {
            self.players[owner]
                .upgrades_in_progress
                .retain(|value| value != &name);
            self.players[owner].upgrades.retain(|value| value != &name);
        }
        Ok(())
    }

    /// `Player::addUpgrade(COMPLETE)`, including repeated completion callbacks.
    pub(in crate::skirmish) fn complete_player_upgrade(&mut self, owner: usize, name: &str) {
        self.players[owner]
            .upgrades_in_progress
            .retain(|value| value != name);
        if !self.players[owner]
            .upgrades
            .iter()
            .any(|value| value == name)
        {
            self.players[owner].upgrades.push(name.into());
        }
        // Player::onUpgradeCompleted visits team prototypes, team instances,
        // and their members; dead Objects still have upgrade callbacks.
        for id in self.team_membership.player_objects(owner) {
            self.activate_upgrade_modules(id);
        }
    }

    pub(in crate::skirmish) fn production_cost(&self, producer: usize, entry: &Production) -> u32 {
        match entry.item {
            ProductionItem::Unit(index) => self.build_cost(self.entities[producer].owner, index),
            ProductionItem::Upgrade(index) => {
                self.catalog[self.entities[producer].type_index]
                    .maintenance
                    .research[index]
                    .cost
            }
        }
    }
}

#[cfg(test)]
mod status_tests;
