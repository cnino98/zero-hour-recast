//! Concrete source weapon slots, selection and lock ownership.
use super::{AreaDamage, ContinuousFire, DamageType, Entity, Skirmish, UnitType, WeaponMotion};

mod pitch;

pub struct WeaponSetRules {
    pub initial_lock: Option<usize>,
    pub share_reload: bool,
    pub replacements: Vec<WeaponTemplateSet>,
    pub templates: Vec<ConditionalWeapon>,
}

/// Native `PLAYER_UPGRADE`, `CRATEUPGRADE_ONE` and `CRATEUPGRADE_TWO` conditions.
pub struct WeaponTemplateSet {
    pub flags: u16,
    pub weapons: [Option<usize>; 3],
    pub share_lock: bool,
}

/// A source weapon instance keeps its authored set and physical slot for art binding.
pub struct ConditionalWeapon {
    pub rules: WeaponRules,
    pub flags: u16,
    pub slot: usize,
}

impl WeaponSetRules {
    /// Conditional weapons follow three ordinary slots and four salvage slots.
    #[must_use]
    pub const fn next_template_index(&self) -> usize {
        3 + 4 + self.templates.len()
    }

    #[must_use]
    pub const fn independent() -> Self {
        Self {
            initial_lock: None,
            share_reload: false,
            replacements: Vec::new(),
            templates: Vec::new(),
        }
    }
}

pub struct WeaponProjectile {
    pub calls_on_die: bool,
    pub stream: Option<String>,
    pub upgrade: Option<String>,
}

pub struct WeaponRules {
    pub projectile: WeaponProjectile,
    pub bonuses: super::containment::bonuses::BonusSet,
    pub incendiary: Option<super::fire::Weapon>,
    pub death_type: super::DeathType,
    pub reload: ClipReload,
    pub hazard: Option<super::hazards::Rules>,
    pub laser: bool,
    pub laser_lifetime: Option<[u32; 2]>,
    pub allow_attack_garrisoned: bool,
    pub name: String,
    pub damage: f32,
    pub shockwave: [f32; 3],
    pub area: Option<AreaDamage>,
    pub infantry_scatter: f32,
    pub scatter: f32,
    pub scatter_targets: Vec<[f32; 2]>,
    pub scatter_target_scalar: f32,
    pub range: f32,
    pub min_range: f32,
    pub aim_delta: f32,
    pub target_pitch: [f32; 2],
    pub delay: f32,
    pub delay_frames: Option<[u32; 2]>,
    pub pre_attack_frames: u32,
    pub pre_attack_type: Prefire,
    pub continue_attack_range: f32,
    pub leech_range: bool,
    pub auto_reload_idle_frames: u32,
    pub fire_sound_loop_frames: u32,
    pub clip_size: u32,
    pub reload_seconds: f32,
    pub speed: f32,
    pub motion: Option<WeaponMotion>,
    pub damage_type: DamageType,
    pub continuous_fire: Option<ContinuousFire>,
    pub auto_choose: u8,
    pub anti_mask: u8,
    pub request_assist_range: f32,
    pub preferred_against: Vec<String>,
}

pub enum Prefire {
    Shot,
    Attack,
    Clip,
}

/// Original `WeaponReloadType`; returning to base remains an aircraft command.
pub enum ClipReload {
    Automatic,
    None,
    ReturnToBase,
}

impl WeaponRules {
    pub(super) fn contact(&self) -> bool {
        // WeaponTemplate::isContactWeapon; RATIONALIZE_ATTACK_RANGE is enabled.
        self.range - (super::CELL_SIZE / 4.0) < super::CELL_SIZE
    }

    #[must_use]
    pub const fn unarmed() -> Self {
        Self {
            projectile: WeaponProjectile {
                calls_on_die: false,
                stream: None,
                upgrade: None,
            },
            bonuses: super::containment::bonuses::BonusSet::none(),
            incendiary: None,
            death_type: super::DeathType::Normal,
            reload: ClipReload::Automatic,
            hazard: None,
            laser: false,
            laser_lifetime: None,
            allow_attack_garrisoned: false,
            name: String::new(),
            damage: 0.0,
            shockwave: [0.0; 3],
            area: None,
            infantry_scatter: 0.0,
            scatter: 0.0,
            scatter_targets: Vec::new(),
            scatter_target_scalar: 0.0,
            range: 0.0,
            min_range: 0.0,
            aim_delta: 0.0,
            target_pitch: [-std::f32::consts::PI, std::f32::consts::PI],
            delay: 0.0,
            delay_frames: None,
            pre_attack_frames: 0,
            pre_attack_type: Prefire::Shot,
            continue_attack_range: 0.0,
            leech_range: false,
            auto_reload_idle_frames: 0,
            fire_sound_loop_frames: 0,
            clip_size: 0,
            reload_seconds: 0.0,
            // WeaponTemplate constructor: 999999 distance/frame, stored per second.
            speed: 999_999.0 * 30.0,
            motion: None,
            damage_type: DamageType::ArmorPiercing,
            continuous_fire: None,
            auto_choose: 7,
            anti_mask: 0x02,
            request_assist_range: 0.0,
            preferred_against: Vec::new(),
        }
    }
}

impl WeaponRules {
    /// `WeaponTemplate::getDelayBetweenShots`: choose the inclusive native
    /// frame range before applying the fire-time rate bonus and flooring.
    pub(in crate::skirmish) fn sample_shot_delay(&self, random: &mut [u32; 6], rate: f32) -> u32 {
        let base = self.delay_frames.map_or_else(
            || (self.delay * 30.0).ceil(),
            |[minimum, maximum]| {
                let sampled = minimum + super::startup::random_between(random, maximum - minimum);
                super::progression::integer_real(sampled)
            },
        );
        super::progression::real_unsigned((base / rate).floor())
    }
}

impl UnitType {
    #[must_use]
    pub fn weapon_template(&self, index: usize) -> Option<&WeaponRules> {
        if index >= self.power_weapon_start() {
            return self
                .behavior
                .power_weapons
                .get(index - self.power_weapon_start());
        }
        if index >= self.collision_weapon_start() {
            return self
                .behavior
                .collision_weapons
                .get(index - self.collision_weapon_start())
                .map(|rules| &rules.weapon);
        }
        if index >= self.mine_weapon_start() {
            return self
                .behavior
                .minefields
                .get(index - self.mine_weapon_start())?
                .weapon
                .as_ref();
        }
        if index < 3 {
            self.weapon_in_slot(index)
        } else if index < 5 {
            self.salvage.weapons.get(index - 3)
        } else if index < 7 {
            self.salvage.secondary.get(index - 5)
        } else if index < self.weapon_set_rules.next_template_index() {
            self.weapon_set_rules
                .templates
                .get(index - 7)
                .map(|template| &template.rules)
        } else if index < self.death_weapon_start() {
            self.behavior
                .point_defense
                .get(index - 7 - self.weapon_set_rules.templates.len())
                .map(|rules| &rules.weapon)
        } else if index < self.slow_weapon_start() {
            self.behavior
                .death_weapons
                .get(index - self.death_weapon_start())
                .and_then(|rules| rules.weapon.as_ref())
        } else if index < self.slow_weapon_end() {
            self.death_lifecycle
                .as_ref()?
                .weapons
                .get(index - self.slow_weapon_start())
        } else if index < self.damage_weapon_start() {
            self.behavior
                .fire_weapons
                .get(index - self.fire_weapon_start())?
                .weapon
                .as_ref()
        } else if index < self.instant_weapon_start() {
            let offset = index - self.damage_weapon_start();
            let rules = self.behavior.damage_weapons.get(offset / 8)?;
            if offset % 8 < 4 {
                rules.reaction[offset % 8].as_ref()
            } else {
                rules.continuous[offset % 8 - 4].as_ref()
            }
        } else if index < self.bunker_weapon_start() {
            self.behavior
                .instant_death
                .iter()
                .flat_map(|rules| &rules.weapons)
                .nth(index - self.instant_weapon_start())?
                .as_ref()
        } else if index < self.demo_weapon_start() {
            self.behavior
                .bunker_busters
                .get(index - self.bunker_weapon_start())?
                .shockwave
                .as_ref()
        } else if Some(index) == self.spectre_weapon_index() {
            self.spectre.as_ref()?.howitzer.as_ref()
        } else {
            self.demo_trap
                .get(index - self.demo_weapon_start())?
                .detonation_weapon
                .as_ref()
        }
    }

    #[must_use]
    pub fn weapon_template_count(&self) -> usize {
        self.power_weapon_start() + self.behavior.power_weapons.len()
    }

    pub(in crate::skirmish) fn power_weapon_start(&self) -> usize {
        self.collision_weapon_start() + self.behavior.collision_weapons.len()
    }

    pub(in crate::skirmish) fn collision_weapon_start(&self) -> usize {
        self.mine_weapon_start() + self.behavior.minefields.len()
    }

    pub(in crate::skirmish) fn mine_weapon_start(&self) -> usize {
        self.demo_weapon_start()
            + self.demo_trap.len()
            + usize::from(
                self.spectre
                    .as_ref()
                    .is_some_and(|rules| rules.howitzer.is_some()),
            )
    }

    pub(in crate::skirmish) fn spectre_weapon_index(&self) -> Option<usize> {
        self.spectre.as_ref()?.howitzer.as_ref()?;
        Some(self.demo_weapon_start() + self.demo_trap.len())
    }

    pub(in crate::skirmish) fn demo_weapon_start(&self) -> usize {
        self.bunker_weapon_start() + self.behavior.bunker_busters.len()
    }

    pub(in crate::skirmish) fn bunker_weapon_start(&self) -> usize {
        self.instant_weapon_start()
            + self
                .behavior
                .instant_death
                .iter()
                .map(|rules| rules.weapons.len())
                .sum::<usize>()
    }

    pub(in crate::skirmish) const fn instant_weapon_start(&self) -> usize {
        self.damage_weapon_start() + self.behavior.damage_weapons.len() * 8
    }

    pub(in crate::skirmish) const fn damage_weapon_start(&self) -> usize {
        self.fire_weapon_start() + self.behavior.fire_weapons.len()
    }

    pub(in crate::skirmish) const fn fire_weapon_start(&self) -> usize {
        self.slow_weapon_end()
    }

    /// Index of the actual Weapon owned by an authored `FireWeaponUpdate` ordinal.
    #[must_use]
    pub fn fire_weapon_index(&self, ordinal: usize) -> Option<usize> {
        self.behavior.fire_weapons.get(ordinal)?.weapon.as_ref()?;
        Some(self.fire_weapon_start() + ordinal)
    }

    pub(super) const fn slow_weapon_end(&self) -> usize {
        self.slow_weapon_start()
            + match &self.death_lifecycle {
                Some(rules) => rules.weapons.len(),
                None => 0,
            }
    }

    pub(super) const fn death_weapon_start(&self) -> usize {
        self.weapon_set_rules.next_template_index() + self.behavior.point_defense.len()
    }

    pub(super) const fn slow_weapon_start(&self) -> usize {
        self.death_weapon_start() + self.behavior.death_weapons.len()
    }

    pub fn weapon_template_mut(&mut self, index: usize) -> Option<&mut WeaponRules> {
        let start = self.power_weapon_start();
        if index >= start {
            return self.behavior.power_weapons.get_mut(index - start);
        }
        let collide_start = self.collision_weapon_start();
        if index >= collide_start {
            return self
                .behavior
                .collision_weapons
                .get_mut(index - collide_start)
                .map(|rules| &mut rules.weapon);
        }
        let mine_start = self.mine_weapon_start();
        if index >= mine_start {
            return self
                .behavior
                .minefields
                .get_mut(index - mine_start)?
                .weapon
                .as_mut();
        }
        match index {
            0 => Some(&mut self.weapon),
            1 => self.secondary_weapon.as_mut(),
            2 => self.tertiary_weapon.as_mut(),
            3 | 4 => self.salvage.weapons.get_mut(index - 3),
            5 | 6 => self.salvage.secondary.get_mut(index - 5),
            _ if index < self.weapon_set_rules.next_template_index() => self
                .weapon_set_rules
                .templates
                .get_mut(index - 7)
                .map(|template| &mut template.rules),
            _ if index < self.death_weapon_start() => self
                .behavior
                .point_defense
                .get_mut(index - 7 - self.weapon_set_rules.templates.len())
                .map(|rules| &mut rules.weapon),
            _ if index < self.slow_weapon_start() => {
                let start = self.death_weapon_start();
                self.behavior
                    .death_weapons
                    .get_mut(index - start)
                    .and_then(|rules| rules.weapon.as_mut())
            }
            _ if index < self.slow_weapon_end() => {
                let start = self.slow_weapon_start();
                self.death_lifecycle
                    .as_mut()?
                    .weapons
                    .get_mut(index - start)
            }
            _ if index < self.damage_weapon_start() => {
                let start = self.fire_weapon_start();
                self.behavior
                    .fire_weapons
                    .get_mut(index - start)?
                    .weapon
                    .as_mut()
            }
            _ if index < self.instant_weapon_start() => {
                let offset = index - self.damage_weapon_start();
                let rules = self.behavior.damage_weapons.get_mut(offset / 8)?;
                if offset % 8 < 4 {
                    rules.reaction[offset % 8].as_mut()
                } else {
                    rules.continuous[offset % 8 - 4].as_mut()
                }
            }
            _ if index < self.bunker_weapon_start() => {
                let offset = index - self.instant_weapon_start();
                self.behavior
                    .instant_death
                    .iter_mut()
                    .flat_map(|rules| &mut rules.weapons)
                    .nth(offset)?
                    .as_mut()
            }
            _ if index < self.demo_weapon_start() => {
                let offset = index - self.bunker_weapon_start();
                self.behavior
                    .bunker_busters
                    .get_mut(offset)?
                    .shockwave
                    .as_mut()
            }
            _ if Some(index) == self.spectre_weapon_index() => {
                self.spectre.as_mut()?.howitzer.as_mut()
            }
            _ => {
                let offset = index - self.demo_weapon_start();
                self.demo_trap.get_mut(offset)?.detonation_weapon.as_mut()
            }
        }
    }

    #[must_use]
    pub fn weapon_context(&self, index: usize) -> (usize, u16) {
        match index {
            _ if index >= self.death_weapon_start() => (0, 0),
            0..=2 => (index, 0),
            3..=6 => ((index - 3) / 2, if index % 2 == 1 { 2 } else { 4 }),
            _ => self
                .weapon_set_rules
                .templates
                .get(index - 7)
                .map_or((2, 0), |template| (template.slot, template.flags)),
        }
    }

    #[must_use]
    pub const fn weapon_in_slot(&self, slot: usize) -> Option<&WeaponRules> {
        match slot {
            0 => Some(&self.weapon),
            1 => self.secondary_weapon.as_ref(),
            2 => self.tertiary_weapon.as_ref(),
            _ => None,
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn weapon_slot(&self) -> usize {
        self.weapon_set.current
    }

    #[must_use]
    pub const fn weapon_index(&self, slot: usize) -> usize {
        if slot >= 3 {
            return usize::MAX;
        }
        if let Some(indices) = self.weapon_set.indices {
            match indices[slot] {
                Some(index) => index,
                _ => usize::MAX,
            }
        } else if slot < 2 && self.salvage.level != 0 {
            2 + slot * 2 + self.salvage.level as usize
        } else {
            slot
        }
    }

    #[must_use]
    pub const fn weapon_set_upgraded(&self) -> bool {
        self.weapon_set.upgraded
    }

    /// Drawable ammunition sampled by the FINAL weapon-status helper. Reading
    /// presentation state never advances the authoritative weapon clock.
    #[must_use]
    pub fn weapon_rounds_remaining(&self, slot: usize) -> u32 {
        self.weapon_set
            .status
            .clip_status
            .get(slot)
            .map_or(0, |&(ammo, _)| ammo)
    }

    /// Missing native weapons expose zero statistics; allocation queries still
    /// distinguish an empty slot from a real weapon (notably working builders).
    #[must_use]
    pub fn active_weapon<'a>(&self, unit: &'a UnitType) -> &'a WeaponRules {
        static UNARMED: WeaponRules = WeaponRules::unarmed();
        unit.weapon_template(self.weapon_index(self.weapon_set.current))
            .unwrap_or(&UNARMED)
    }
}

pub(super) struct State {
    pub status: super::firing::status::State,
    pub weapons: [super::firing::WeaponState; 3],
    pub allocation_generation: [u64; 3],
    pub current: usize,
    pub lock: Lock,
    pub upgraded: bool,
    pub mine_clearing: bool,
    pub forced_veteran: bool,
    pub indices: Option<[Option<usize>; 3]>,
    pub template_set: Option<usize>,
}

impl State {
    pub fn new(unit: &UnitType) -> Self {
        let template_set = best_set(&unit.weapon_set_rules.replacements, 0);
        let indices = template_set.map(|index| unit.weapon_set_rules.replacements[index].weapons);
        Self {
            status: super::firing::status::State::new(),
            weapons: std::array::from_fn(|slot| {
                let index = indices.map_or(Some(slot), |indices| indices[slot]);
                super::firing::WeaponState::new(
                    index
                        .and_then(|index| unit.weapon_template(index))
                        .map_or(0, |weapon| weapon.clip_size),
                )
            }),
            allocation_generation: [0; 3],
            current: unit.weapon_set_rules.initial_lock.unwrap_or(0),
            lock: if unit.weapon_set_rules.initial_lock.is_some() {
                Lock::Permanent
            } else {
                Lock::None
            },
            upgraded: false,
            mine_clearing: false,
            forced_veteran: false,
            indices,
            template_set,
        }
    }
}

pub(super) enum Lock {
    None,
    Temporary,
    Permanent,
}

impl Skirmish {
    pub(super) fn victim_anti_mask(&self, target: usize) -> u8 {
        let actor = &self.entities[target];
        let unit = &self.catalog[actor.type_index];
        let has_kind = |name| unit.kind_of.iter().any(|kind| kind == name);
        if has_kind("SMALL_MISSILE") {
            0x08
        } else if has_kind("BALLISTIC_MISSILE") {
            0x40
        } else if has_kind("PROJECTILE") {
            0x04
        } else if has_kind("MINE") || has_kind("DEMOTRAP") {
            0x10 | 0x02
        } else if self.height_above_ground(target)
            > actor.active_locomotor(unit).airborne_targeting_height
        {
            if has_kind("VEHICLE") {
                0x01
            } else if has_kind("INFANTRY") {
                0x20
            } else if has_kind("PARACHUTE") {
                0x80
            } else {
                0
            }
        } else {
            0x02
        }
    }

    /// The allocated `WeaponSet`'s combined anti-mask gate, before slot locks/selection.
    #[must_use]
    pub fn weapon_can_target(&self, id: usize, target: usize) -> bool {
        let forced = self
            .entities
            .get(id)
            .is_some_and(|actor| actor.ai.attack_context.forced);
        self.weapon_can_target_mode(id, target, forced)
    }

    pub(super) fn weapon_can_target_mode(&self, id: usize, target: usize, forced: bool) -> bool {
        let (Some(actor), Some(victim)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        if self.catalog[self.entities[target].type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "UNATTACKABLE")
        {
            return false;
        }
        let force_ignores_stealth = forced
            && (actor.owner == victim.owner
                || (self.attack_kind(target, "DISGUISER") && victim.has_native_disguise()));
        if !force_ignores_stealth && !self.stealth_targetable(id, target) {
            return false;
        }
        if self.uses_spawn_weapons(id) {
            return self.spawned_children(id).iter().any(|&child| {
                self.entities[child].alive() && self.weapon_can_target_mode(child, target, forced)
            });
        }
        let mask = self.victim_anti_mask(target);
        let unit = &self.catalog[actor.type_index];
        (0..3).any(|slot| {
            self.weapon_available(id, slot)
                && unit
                    .weapon_template(actor.weapon_index(slot))
                    .is_some_and(|weapon| weapon.anti_mask & mask != 0)
        })
    }

    /// Native immobile/contained range admission tests the current slot before selection.
    #[must_use]
    pub fn weapon_target_reachable(&self, id: usize, target: usize) -> bool {
        let forced = self
            .entities
            .get(id)
            .is_some_and(|actor| actor.ai.attack_context.forced);
        self.weapon_target_reachable_mode(id, target, forced)
    }

    pub(super) fn weapon_target_reachable_mode(
        &self,
        id: usize,
        target: usize,
        forced: bool,
    ) -> bool {
        if self.uses_spawn_weapons(id) {
            return self.spawned_children(id).iter().any(|&child| {
                self.entities[child].alive()
                    && self.weapon_target_reachable_mode(child, target, forced)
            });
        }
        if !self.weapon_can_target_mode(id, target, forced)
            || !self.hero_pilot_targetable(id, target)
        {
            return false;
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        // WeaponSet::getAbleToUseWeaponAgainstTarget uses the closest window
        // as the range origin for an enclosed garrison occupant.
        let can_reach = if actor.contained_by().is_some() {
            self.container_attack_in_range(id, target)
        } else {
            !(unit.kind_of.iter().any(|kind| kind == "IMMOBILE") || actor.disabled_held())
                || self.within_attack_range(id, target)
        };
        self.any_weapon_within_target_pitch(id, target) && can_reach
    }

    /// Read-only normal player weapon admission, independent of an active force command.
    #[must_use]
    pub fn player_attack_target_reachable(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(victim)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        id != target
            && actor.alive()
            && victim.alive()
            && !victim.masked()
            && self.player_attack_relationship(id, target)
            && self.attack_target_reachable_mode(id, target, false)
    }

    /// Read-only source force-object weapon admission for the player cursor.
    #[must_use]
    pub fn force_attack_target_reachable(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(victim)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        if id == target || !actor.alive() || !victim.alive() || victim.masked() {
            return false;
        }
        self.attack_target_reachable_mode(id, target, true)
    }

    pub(super) fn attack_target_reachable_mode(
        &self,
        id: usize,
        target: usize,
        forced: bool,
    ) -> bool {
        self.weapon_target_reachable_mode(id, target, forced)
            || self.passengers(id).iter().any(|&rider| {
                self.entities[rider].alive()
                    && self.passenger_can_fire(rider)
                    && self.weapon_target_reachable_mode(rider, target, forced)
            })
            || self.entities[id].attachment_child().is_some_and(|rider| {
                self.entities[rider].alive()
                    && self.weapon_target_reachable_mode(rider, target, forced)
            })
    }

    #[must_use]
    pub fn weapon_available(&self, id: usize, slot: usize) -> bool {
        if self.uses_spawn_weapons(id) {
            return self
                .spawned_children(id)
                .iter()
                .any(|&child| self.entities[child].alive() && self.weapon_available(child, slot));
        }
        self.allocated_weapon(id, slot)
            .is_some_and(|weapon| weapon.damage > 0.0)
    }

    pub(super) fn allocated_weapon(&self, id: usize, slot: usize) -> Option<&WeaponRules> {
        if slot >= 3 {
            return None;
        }
        let actor = self.entities.get(id)?;
        let unit = &self.catalog[actor.type_index];
        if actor.weapon_set.indices.is_none()
            && slot != 0
            && !unit.weapon_upgrade_triggers.is_empty()
            && !actor.weapon_set.upgraded
        {
            return None;
        }
        unit.weapon_template(actor.weapon_index(slot))
    }

    /// # Errors
    /// Rejects a weapon slot absent from the object's current source weapon set.
    pub fn switch_weapon(&mut self, id: usize, slot: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.weapon_slot_present(id, slot) {
            return Err("Weapon is unavailable in the current set");
        }
        self.entities[id].weapon_set.current = slot;
        self.entities[id].weapon_set.lock = Lock::Permanent;
        Ok(())
    }

    /// A source weapon can select a mode without dealing damage (Demo Trap).
    #[must_use]
    pub fn weapon_slot_present(&self, id: usize, slot: usize) -> bool {
        self.allocated_weapon(id, slot)
            .is_some_and(|weapon| weapon.damage > 0.0 || !weapon.name.is_empty())
    }

    pub(in crate::skirmish) fn apply_weapon_set_upgrade(&mut self, id: usize) {
        self.entities[id].weapon_set.upgraded = true;
        self.replace_weapon_set(id);
    }

    pub(super) fn replace_weapon_set(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let flags = if actor.weapon_set.forced_veteran {
            32
        } else {
            0
        } | u16::from(actor.weapon_set.upgraded)
            | if actor.weapon_set.mine_clearing {
                16
            } else {
                0
            }
            | if actor.car_bomb.is_some() { 8 } else { 0 }
            | match actor.salvage.level {
                1 => 2,
                2 => 4,
                _ => 0,
            }
            | actor.combat_bike.rider_weapon;
        let replacement = best_set(&unit.weapon_set_rules.replacements, flags);
        if actor.weapon_set.template_set == replacement {
            return;
        }
        if let Some(index) = replacement {
            let set = &unit.weapon_set_rules.replacements[index];
            if !set.share_lock {
                actor.weapon_set.current = 0;
                actor.weapon_set.lock = Lock::None;
            }
            actor.weapon_set.template_set = Some(index);
            actor.weapon_set.indices = Some(set.weapons);
        } else {
            actor.weapon_set.current = 0;
            actor.weapon_set.lock = Lock::None;
            actor.weapon_set.template_set = None;
            actor.weapon_set.indices = None;
        }
        // updateWeaponSet deletes and reallocates every slot when its template set changes.
        for generation in &mut actor.weapon_set.allocation_generation {
            *generation = generation
                .checked_add(1)
                .expect("weapon allocation identity overflow");
        }
        actor.weapon_set.weapons = std::array::from_fn(|slot| {
            super::firing::WeaponState::new(
                unit.weapon_template(actor.weapon_index(slot))
                    .map_or(0, |weapon| weapon.clip_size),
            )
        });
    }

    pub(super) fn choose_weapon(&mut self, id: usize, target: usize) -> bool {
        let source_mask = self.entities[id].ai.attack_context.source.mask();
        self.choose_weapon_from(id, target, source_mask)
    }

    pub(super) fn choose_weapon_from(&mut self, id: usize, target: usize, source_mask: u8) -> bool {
        if !matches!(self.entities[id].weapon_set.lock, Lock::None) {
            return true;
        }
        let target_mask = self.victim_anti_mask(target);
        let type_index = self.entities[id].type_index;
        let now = super::ai_dispatch::native_frame(self.tick);
        let mut ready = None;
        let mut backup = None;
        let mut damage_ready = 0.0;
        let mut damage_backup = 0.0;
        // PREFER_MOST_DAMAGE: descending slots and >= preserve primary tie priority.
        for slot in (0..3).rev() {
            let Some(weapon) = self.allocated_weapon(id, slot) else {
                continue;
            };
            if weapon.auto_choose & (source_mask | 4) == 0 {
                continue;
            }
            let automatic = matches!(weapon.reload, ClipReload::Automatic);
            let status = self.entities[id].weapon_status(slot, &self.catalog[type_index], now);
            if matches!(status, Some(super::ai_dispatch::WeaponStatus::OutOfAmmo)) && !automatic {
                continue;
            }
            let weapon = self.allocated_weapon(id, slot).expect("allocated weapon");
            if weapon.anti_mask & target_mask == 0 || !self.within_target_pitch(id, target, weapon)
            {
                continue;
            }
            let mut damage = self.estimate_native_weapon_damage(id, slot, target);
            let status = self.entities[id].weapon_status(slot, &self.catalog[type_index], now);
            let mut weapon_ready = matches!(status, Some(super::ai_dispatch::WeaponStatus::Ready))
                && !self.weapon_turret_aims_at(id, slot, target);
            let weapon = self.allocated_weapon(id, slot).expect("allocated weapon");
            if damage <= 0.0 && !matches!(weapon.damage_type, DamageType::Unresistable) {
                continue;
            }
            let victim = &self.catalog[self.entities[target].type_index];
            if !weapon.preferred_against.is_empty()
                && weapon
                    .preferred_against
                    .iter()
                    .all(|kind| victim.kind_of.contains(kind))
            {
                damage = 1.0e10;
                weapon_ready = !matches!(status, Some(super::ai_dispatch::WeaponStatus::OutOfAmmo));
            }
            if weapon_ready {
                if damage >= damage_ready {
                    damage_ready = damage;
                    ready = Some(slot);
                }
            } else if damage >= damage_backup {
                damage_backup = damage;
                backup = Some(slot);
            }
        }
        let chosen = ready.or(backup);
        self.entities[id].weapon_set.current = chosen.unwrap_or(0);
        chosen.is_some()
    }
}

// SparseMatchFinder::findBestInfoSlow: most matching flags, fewest extras, first tie.
fn best_set(sets: &[WeaponTemplateSet], flags: u16) -> Option<usize> {
    let mut result = None;
    let mut best_yes = 0;
    let mut best_extra = u32::MAX;
    for (index, set) in sets.iter().enumerate() {
        let yes = (set.flags & flags).count_ones();
        let extra = (set.flags & !flags).count_ones();
        if yes > best_yes || (yes >= best_yes && extra < best_extra) {
            result = Some(index);
            best_yes = yes;
            best_extra = extra;
        }
    }
    result
}

pub(super) fn validate(unit: &UnitType) -> Result<(), &'static str> {
    for weapon in (0..unit.weapon_template_count()).filter_map(|index| unit.weapon_template(index))
    {
        let values = [
            weapon.damage,
            weapon.infantry_scatter,
            weapon.scatter,
            weapon.range,
            weapon.min_range,
            weapon.delay,
            weapon.reload_seconds,
            weapon.speed,
            weapon.request_assist_range,
            weapon.continue_attack_range,
        ];
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || !weapon.aim_delta.is_finite()
            || weapon.target_pitch.iter().any(|value| !value.is_finite())
            || weapon.damage > 1_000_000.0
            || weapon
                .delay_frames
                .is_some_and(|[minimum, maximum]| minimum > maximum)
        {
            return Err("Invalid weapon source values");
        }
        if weapon
            .bonuses
            .global
            .iter()
            .chain(&weapon.bonuses.template)
            .any(|row| row.iter().any(|value| !value.is_finite() || *value < 0.0) || row[3] <= 0.0)
        {
            return Err("Invalid weapon bonus source values");
        }
        if weapon
            .continuous_fire
            .as_ref()
            .is_some_and(|fire| !fire.coast_frames.is_finite() || fire.coast_frames < 0.0)
        {
            return Err("Invalid continuous-fire source values");
        }
        super::damage::validate(weapon)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
