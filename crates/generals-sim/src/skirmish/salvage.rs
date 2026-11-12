//! Native `CreateCrateDie`, inactive-body crates, `SalvageCrateCollide` and deletion.
mod placement;
#[cfg(test)]
mod tests;
use super::{Entity, Kind, Skirmish, UnitType, WeaponRules, startup};

pub struct Rules {
    pub drops: Vec<Drop>,
    pub pickup: Option<Pickup>,
    pub weapons: Vec<WeaponRules>,
    pub secondary: Vec<WeaponRules>,
}
impl Rules {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            drops: Vec::new(),
            pickup: None,
            weapons: Vec::new(),
            secondary: Vec::new(),
        }
    }
}
pub struct Drop {
    pub chance: f32,
    pub killer_kinds: Vec<String>,
    pub science: Option<String>,
    pub template: String,
}
pub struct Pickup {
    pub science: String,
    pub visible_science: String,
    pub forbidden: Vec<String>,
    pub weapon_chance: f32,
    pub level_chance: f32,
    pub money: [u32; 2],
    pub lifetime: [u32; 2],
}
pub struct Event {
    pub frame: u64,
    pub collector: usize,
    pub position: [f32; 3],
    pub reward: Reward,
}
pub enum Reward {
    Weapon,
    Level,
    Money(u32),
}
pub(super) struct State {
    pub money_active: bool,
    pub level: u8,
    pub expires: Option<u64>,
}
impl State {
    pub const fn new(money_active: bool) -> Self {
        Self {
            money_active,
            level: 0,
            expires: None,
        }
    }
}
impl Entity {
    #[must_use]
    pub const fn salvage_level(&self) -> u8 {
        self.salvage.level
    }
    #[must_use]
    pub const fn crate_present(&self) -> bool {
        self.salvage.expires.is_some() || self.salvage.money_active
    }
}

pub(super) fn validate(unit: &UnitType, catalog: &[UnitType]) -> Result<(), &'static str> {
    if unit.variations.iter().any(|name| {
        catalog.iter().all(|other| {
            other.name != *name || !other.variations.is_empty() || other.initial_health <= 0.0
        })
    }) || u32::try_from(unit.variations.len()).is_err()
    {
        return Err("Invalid native build variation");
    }
    if (!unit.salvage.weapons.is_empty()
        && unit.variations.is_empty()
        && unit.salvage.weapons.len() != 2)
        || (!unit.salvage.secondary.is_empty() && unit.salvage.secondary.len() != 2)
        || (unit.kind_of.iter().any(|name| name == "WEAPON_SALVAGER")
            && unit.variations.is_empty()
            && unit.salvage.weapons.len() != 2)
    {
        return Err("Concrete salvage chassis requires both original primary weapon sets");
    }
    if unit.battle_bus.is_none() && unit.kind_of.iter().any(|name| name == "ARMOR_SALVAGER") {
        return Err("Armor salvage sets are not present in the represented catalog");
    }
    for drop in &unit.salvage.drops {
        if !(0.0..=1.0).contains(&drop.chance)
            || !catalog
                .iter()
                .any(|other| other.name == drop.template && other.salvage.pickup.is_some())
        {
            return Err("Invalid native crate creation data");
        }
    }
    if let Some(rules) = &unit.salvage.pickup
        && (unit.kind != Kind::Crate
            || unit.initial_health != 0.0
            || !(0.0..=1.0).contains(&rules.weapon_chance)
            || !(0.0..=1.0).contains(&rules.level_chance)
            || rules.money[0] > rules.money[1]
            || rules.lifetime[0] > rules.lifetime[1])
    {
        return Err("Invalid native salvage crate data");
    }
    Ok(())
}

impl Skirmish {
    #[must_use]
    pub fn crate_visible_to(&self, owner: usize, id: usize) -> bool {
        let (Some(actor), Some(player)) = (self.entities.get(id), self.players.get(owner)) else {
            return false;
        };
        if actor.salvage.money_active {
            return matches!(
                self.visibility_status(owner, id),
                super::visibility::Status::Clear | super::visibility::Status::Partial
            );
        }
        let Some(rules) = &self.catalog[actor.type_index].salvage.pickup else {
            return false;
        };
        actor.crate_present()
            && (self.defeated[owner] || player.has_science(&rules.visible_science))
            && matches!(
                self.visibility_status(owner, id),
                super::visibility::Status::Clear | super::visibility::Status::Partial
            )
    }
    pub(super) fn initialize_crate(&mut self, id: usize) {
        let Some(rules) = &self.catalog[self.entities[id].type_index].salvage.pickup else {
            return;
        };
        let [low, high] = rules.lifetime;
        let delay = integer(&mut self.random, low, high).max(1);
        self.entities[id].salvage.expires = Some(self.tick + u64::from(delay));
    }
    pub(super) fn expire_crates(&mut self) {
        for id in 0..self.entities.len() {
            if self.entities[id]
                .salvage
                .expires
                .is_some_and(|frame| self.tick >= frame)
            {
                self.entities[id].salvage.expires = None;
                self.observe_ground_partition(id);
            }
        }
    }
    pub(super) fn create_salvage(&mut self, victim: usize, killer: Option<usize>) {
        if killer
            .is_some_and(|id| self.allied(self.entities[id].owner, self.entities[victim].owner))
        {
            return;
        }
        let unit = self.entities[victim].type_index;
        for index in 0..self.catalog[unit].salvage.drops.len() {
            let drop = &self.catalog[unit].salvage.drops[index];
            // This draw precedes all killer gates, even at CreationChance=1.
            if startup::random_real(&mut self.random, 0.0, 1.0) >= drop.chance {
                continue;
            }
            if (!drop.killer_kinds.is_empty() || drop.science.is_some()) && killer.is_none() {
                continue;
            }
            if let Some(id) = killer {
                let actor = &self.entities[id];
                let kinds = &self.catalog[actor.type_index].kind_of;
                if drop.killer_kinds.iter().any(|name| !kinds.contains(name))
                    || drop
                        .science
                        .as_ref()
                        .is_some_and(|science| !self.players[actor.owner].has_science(science))
                {
                    continue;
                }
            }
            // CrateTemplate's installed one-entry weighted table still consumes RNG.
            if startup::random_real(&mut self.random, 0.0, 1.0) >= 1.0 {
                continue;
            }
            let template = self
                .catalog
                .iter()
                .position(|unit| unit.name == drop.template)
                .expect("validated crate template");
            if let Some(position) = self.crate_position(victim) {
                let neutral = self.neutral_owner();
                let id = self.spawn(template, neutral, position, true);
                let angle = startup::random_angle(&mut self.random);
                self.set_object_orientation(id, angle);
                self.observe_ground_partition(id);
            }
        }
    }
    /// Original collision eligibility, beyond the mouse SALVAGER hint.
    #[must_use]
    pub fn can_salvage(&self, collector: usize, crate_id: usize) -> bool {
        let (Some(actor), Some(item)) = (self.entities.get(collector), self.entities.get(crate_id))
        else {
            return false;
        };
        let Some(rules) = &self.catalog[item.type_index].salvage.pickup else {
            return false;
        };
        let unit = &self.catalog[actor.type_index];
        item.crate_present()
            && actor.complete()
            && !actor.masked()
            && actor.owner != self.neutral_owner()
            && !unit.kind.is_building()
            && (unit.locomotor.ground.is_some() || unit.chinook.is_some())
            && unit.kind_of.iter().any(|name| name == "SALVAGER")
            && !unit
                .kind_of
                .iter()
                .any(|name| name == "PARACHUTE" || rules.forbidden.contains(name))
            && self.players[actor.owner].has_science(&rules.science)
    }
    pub(super) fn salvage_contact(&mut self, left: usize, right: usize) -> bool {
        let (item, collector) = if self.entities[left].crate_present() {
            (left, right)
        } else if self.entities[right].crate_present() {
            (right, left)
        } else {
            return false;
        };
        if self.can_salvage(collector, item) {
            self.collect_salvage(collector, item);
        }
        true
    }
    fn collect_salvage(&mut self, collector: usize, item: usize) {
        let unit = &self.catalog[self.entities[collector].type_index];
        let rules = self.catalog[self.entities[item].type_index]
            .salvage
            .pickup
            .as_ref()
            .expect("live crate");
        let actor = &self.entities[collector];
        let weapon =
            unit.kind_of.iter().any(|name| name == "WEAPON_SALVAGER") && actor.salvage.level < 2;
        let reward = if weapon && chance(&mut self.random, rules.weapon_chance) {
            self.entities[collector].salvage.level += 1;
            self.reset_salvage_weapon(collector);
            Reward::Weapon
        } else if unit.veterancy.trainable
            && actor.veterancy < 3
            && chance(&mut self.random, rules.level_chance)
        {
            let amount = unit.veterancy.required[usize::from(actor.veterancy + 1)]
                .wrapping_sub(actor.experience);
            self.add_unit_experience(collector, amount);
            Reward::Level
        } else {
            let money = integer(&mut self.random, rules.money[0], rules.money[1]);
            let player = &mut self.players[self.entities[collector].owner];
            player.cash = player.cash.wrapping_add(money);
            player.statistics.money_earned = player.statistics.money_earned.wrapping_add(money);
            Reward::Money(money)
        };
        self.presentation.salvage_events.push(Event {
            frame: self.tick,
            collector,
            position: self.object_position(item),
            reward,
        });
        // CRC=0 + PRESERVE_MULTI_CRATE_PICKUP=0: destroyed before another contact.
        self.destroy_maintenance_object(item);
        self.entities[item].salvage.expires = None;
        self.observe_ground_partition(item);
    }
    fn reset_salvage_weapon(&mut self, id: usize) {
        self.replace_weapon_set(id);
    }
}
fn chance(random: &mut [u32; 6], value: f32) -> bool {
    value.to_bits() == 1.0_f32.to_bits() || startup::random_real(random, 0.0, 1.0) < value
}
fn integer(random: &mut [u32; 6], low: u32, high: u32) -> u32 {
    if low >= high {
        high
    } else {
        low + startup::random_value(random) % (high - low + 1)
    }
}
