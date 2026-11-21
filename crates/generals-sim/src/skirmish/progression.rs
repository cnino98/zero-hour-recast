//! Player ranks/sciences and the selected shared-timer general powers.
//! Pinned sources: `Player.cpp` rank, kill and purchase methods; `Science.cpp`;
//! `CashBountyPower`, `CashHackSpecialPower`, `OCLSpecialPower`, `AutoHealBehavior`
//! and `Object::attemptHealingFromSoleBenefactor`.

use super::movement::distance_squared;
use super::{Faction, Player, Skirmish};

pub struct Rank {
    pub skill_points: u32,
    pub science_points: u32,
    pub sciences: Vec<String>,
}

pub struct Science {
    pub name: String,
    pub prerequisites: Vec<String>,
    pub cost: u32,
}

pub struct FactionProgression {
    pub faction: Faction,
    pub side: String,
    pub preferred_color: u32,
    pub playable_side: bool,
    pub intrinsic_points: u32,
    pub intrinsic_sciences: Vec<String>,
}

pub struct ProgressionRules {
    pub multiplayer_colors: Vec<u32>,
    pub ranks: Vec<Rank>,
    pub sciences: Vec<Science>,
    pub factions: Vec<FactionProgression>,
    pub powers: Vec<PowerDefinition>,
}

#[derive(Default)]
pub struct UnitProgression {
    pub skill_point_value: [u32; 4],
    pub ignored_in_gui: bool,
    pub vehicle: bool,
    pub cash_hack_target: bool,
    pub powers: Vec<String>,
    pub cash_bounties: Vec<(String, f32)>,
}

pub struct PowerDefinition {
    pub name: String,
    pub required_science: String,
    pub reload_frames: u32,
    pub effect: PowerEffect,
}

pub enum PowerEffect {
    CashHack {
        amount: u32,
        upgrades: Vec<(String, u32)>,
    },
    EmergencyRepair {
        burst: RepairBurst,
        upgrades: Vec<(String, RepairBurst)>,
    },
}

pub struct RepairBurst {
    pub amount: f32,
    pub radius: f32,
    pub delay_frames: u32,
}

#[derive(Clone)]
pub enum PowerTarget {
    None,
    Construction { position: [f32; 3], angle: f32 },
    Object(usize),
    Position([f32; 2]),
    Position3D([f32; 3]),
}

pub(super) struct RepairMarker {
    owner: usize,
    position: [f32; 2],
    amount: f32,
    radius: f32,
    wake_frame: u64,
    heal_lock_frames: u32,
}

impl Player {
    #[must_use]
    pub fn has_science(&self, name: &str) -> bool {
        self.sciences.iter().any(|science| science == name)
    }
}

impl Skirmish {
    /// Installs source player templates, palette and progression before the first logic frame.
    ///
    /// # Errors
    /// Rejects an already-started game or invalid rank, science and player identity references.
    pub fn configure_progression(&mut self, rules: ProgressionRules) -> Result<(), &'static str> {
        if self.tick != 0 {
            return Err("Progression must be configured before the match starts");
        }
        validate_rules(&rules)?;
        for unit in &self.catalog {
            // ThingTemplate BuildCost is an unsigned short. Bound XP to exact f32
            // integers while its scripting modifiers and signed deltas are absent.
            if u16::try_from(unit.cost).is_err()
                || unit
                    .progression
                    .skill_point_value
                    .iter()
                    .any(|value| *value > 16_777_216)
            {
                return Err("Progression source values exceed the supported exact numeric range");
            }
            if unit
                .progression
                .powers
                .iter()
                .any(|name| !rules.powers.iter().any(|power| power.name == *name))
                || unit.veterancy.starting.iter().any(|grant| {
                    grant.science.as_ref().is_some_and(|name| {
                        !rules.sciences.iter().any(|science| science.name == *name)
                    })
                })
                || unit
                    .progression
                    .cash_bounties
                    .iter()
                    .any(|(name, percent)| {
                        !percent.is_finite()
                            || *percent < 0.0
                            || *percent > 1.0
                            || !rules.sciences.iter().any(|science| science.name == *name)
                    })
            {
                return Err("Invalid unit progression references");
            }
        }
        self.validate_player_identities(&rules)?;
        self.progression = Some(rules);
        self.reset_player_identities()?;
        self.reset_progression();
        Ok(())
    }

    pub(super) fn reset_progression(&mut self) {
        self.repair_markers.clear();
        let Some(rules) = &self.progression else {
            return;
        };
        for player in &mut self.players {
            player.rank = 1;
            player.skill_points = 0;
            player.science_points = rules.ranks[0].science_points;
            player.sciences.clear();
            player.acquired_sciences.clear();
            player.cash_bounty = 0.0;
            player.power_ready_at = vec![None; rules.powers.len()];
            if let Some(faction) = rules
                .factions
                .iter()
                .find(|faction| faction.faction == player.faction)
            {
                player.science_points += faction.intrinsic_points;
                player.sciences.clone_from(&faction.intrinsic_sciences);
            }
            for name in &rules.ranks[0].sciences {
                add_science(player, name);
            }
        }
        for owner in 0..self.players.len() {
            for science in self.players[owner].sciences.clone() {
                self.special_power_science_acquired(owner, &science);
                self.players[owner].acquired_sciences.push(science);
            }
        }
        for id in 0..self.entities.len() {
            self.initialize_progression_modules(id);
        }
    }

    #[must_use]
    pub fn rank_progress(&self, owner: usize) -> f32 {
        let Some(player) = self.players.get(owner) else {
            return 0.0;
        };
        let Some(rules) = &self.progression else {
            return 0.0;
        };
        let Some(index) = player
            .rank
            .checked_sub(1)
            .and_then(|rank| usize::try_from(rank).ok())
        else {
            return 0.0;
        };
        let Some(next) = rules.ranks.get(index + 1) else {
            return 1.0;
        };
        let current = rules.ranks[index].skill_points;
        integer_real(player.skill_points - current) / integer_real(next.skill_points - current)
    }

    fn science_supported(&self, name: &str) -> bool {
        self.catalog.iter().any(|unit| {
            unit.special_powers
                .iter()
                .any(|power| power.required_science == name)
                || unit
                    .behavior
                    .cash_hack_powers
                    .iter()
                    .any(|power| power.upgrades.iter().any(|(science, _)| science == name))
                || unit
                    .behavior
                    .ocl_powers
                    .iter()
                    .any(|power| power.upgrades.iter().any(|(science, _)| science == name))
                || unit
                    .spectre_deployments
                    .iter()
                    .any(|deployment| deployment.required_science == name)
                || unit
                    .build_rules
                    .sciences
                    .iter()
                    .any(|science| science == name)
                || unit
                    .veterancy
                    .starting
                    .iter()
                    .any(|grant| unit.veterancy.trainable && grant.science.as_deref() == Some(name))
                || unit
                    .progression
                    .cash_bounties
                    .iter()
                    .any(|(science, _)| science == name)
        }) || self.progression.as_ref().is_some_and(|rules| {
            rules.powers.iter().any(|power| {
                power.required_science == name
                    || match &power.effect {
                        PowerEffect::CashHack { upgrades, .. } => {
                            upgrades.iter().any(|(science, _)| science == name)
                        }
                        PowerEffect::EmergencyRepair { upgrades, .. } => {
                            upgrades.iter().any(|(science, _)| science == name)
                        }
                    }
            })
        })
    }

    #[must_use]
    pub fn can_purchase_science(&self, owner: usize, name: &str) -> bool {
        let Some(player) = self.players.get(owner) else {
            return false;
        };
        let Some(science) = self
            .progression
            .as_ref()
            .and_then(|rules| rules.sciences.iter().find(|science| science.name == name))
        else {
            return false;
        };
        self.end_game_at.is_none()
            && self.science_supported(name)
            && !player.has_science(name)
            && science.cost > 0
            && science.cost <= player.science_points
            && science
                .prerequisites
                .iter()
                .all(|required| player.has_science(required))
    }

    // Player::grantScience receives a resolved, grantable ScienceStore entry.
    pub(in crate::skirmish) fn grant_native_science(&mut self, owner: usize, name: &str) {
        if self.players[owner].has_science(name) {
            return;
        }
        add_science(&mut self.players[owner], name);
        self.notify_acquired_science(owner, name);
    }

    fn notify_acquired_science(&mut self, owner: usize, name: &str) {
        for power_index in 0..self
            .progression
            .as_ref()
            .map_or(0, |rules| rules.powers.len())
        {
            let power = &self
                .progression
                .as_ref()
                .expect("configured progression")
                .powers[power_index];
            if power.required_science == name
                && self.entities.iter().any(|entity| {
                    entity.owner == owner
                        && entity.alive()
                        && self.catalog[entity.type_index]
                            .progression
                            .powers
                            .contains(&power.name)
                })
            {
                self.players[owner].power_ready_at[power_index] = Some(self.tick);
            }
        }
        self.special_power_science_acquired(owner, name);
        for id in 0..self.entities.len() {
            if self.entities[id].owner == owner && self.entities[id].alive() {
                self.initialize_progression_modules(id);
            }
        }
        self.players[owner].acquired_sciences.push(name.into());
    }

    /// Purchases a supported science, charging its source cost exactly once.
    ///
    /// # Errors
    /// Rejects missing prerequisites, unsupported/owned sciences or insufficient points.
    pub fn purchase_science(&mut self, owner: usize, name: &str) -> Result<(), &'static str> {
        if !self.can_purchase_science(owner, name) {
            return Err("Science is unavailable, already owned, or needs more promotion points");
        }
        let rules = self
            .progression
            .as_ref()
            .ok_or("Progression is not configured")?;
        let science = rules
            .sciences
            .iter()
            .find(|science| science.name == name)
            .ok_or("Science is not defined")?;
        let player = &mut self.players[owner];
        player.science_points -= science.cost;
        add_science(player, name);
        self.notify_acquired_science(owner, name);
        Ok(())
    }

    pub(super) fn initialize_progression_modules(&mut self, id: usize) {
        let Some(rules) = &self.progression else {
            return;
        };
        let entity = &self.entities[id];
        let unit = &self.catalog[entity.type_index];
        let player = &mut self.players[entity.owner];
        for (science, percent) in &unit.progression.cash_bounties {
            if player.has_science(science) {
                player.cash_bounty = player.cash_bounty.max(*percent);
            }
        }
        for (index, power) in rules.powers.iter().enumerate() {
            if unit.progression.powers.contains(&power.name)
                && player.has_science(&power.required_science)
                && player.power_ready_at[index].is_none()
            {
                // Player::getOrStartSpecialPowerReadyFrame creates a missing shared timer at now.
                player.power_ready_at[index] = Some(self.tick);
            }
        }
    }

    pub(super) fn score_kill(&mut self, killer: usize, victim: usize) {
        self.record_kill(killer, victim);
        let Some(attacker) = self.entities.get(killer) else {
            return;
        };
        // The native tracker follows its sink and applies each object's scalar.
        self.score_player_kill(attacker.owner, victim, Some(killer));
    }

    pub(super) fn score_player_kill(&mut self, owner: usize, victim: usize, sink: Option<usize>) {
        let victim_id = victim;
        let victim = &self.entities[victim];
        let unit = &self.catalog[victim.type_index];
        if !self.enemies(victim.owner, owner)
            || victim.under_construction()
            || unit.progression.ignored_in_gui
        {
            return;
        }
        let level = usize::from(victim.veterancy);
        let experience = unit.veterancy.experience_value[level];
        let skill_points = unit.progression.skill_point_value[level];
        self.add_capture_skill_points(owner, skill_points);
        self.award_cash_bounty(owner, victim_id, sink);
        if let Some(sink) = sink {
            self.add_unit_experience(sink, experience);
        }
    }

    pub(super) fn add_capture_skill_points(&mut self, owner: usize, amount: u32) {
        let Some(rules) = &self.progression else {
            return;
        };
        let player = &mut self.players[owner];
        let old_sciences = player.sciences.len();
        let cap = rules.ranks.last().expect("Validated ranks").skill_points;
        player.skill_points = player.skill_points.saturating_add(amount).min(cap);
        while let Some(rank) = rules
            .ranks
            .get(usize::try_from(player.rank).expect("Validated rank"))
        {
            if player.skill_points < rank.skill_points {
                break;
            }
            player.rank += 1;
            player.science_points += rank.science_points;
            for science in &rank.sciences {
                add_science(player, science);
            }
        }
        let acquired = player.sciences[old_sciences..].to_vec();
        for science in acquired {
            self.special_power_science_acquired(owner, &science);
            self.players[owner].acquired_sciences.push(science);
        }
    }

    fn power_index(&self, caster: usize, name: &str) -> Option<usize> {
        let actor = self.entities.get(caster)?;
        if !actor.complete()
            || !self.catalog[actor.type_index]
                .progression
                .powers
                .iter()
                .any(|power| power == name)
        {
            return None;
        }
        let rules = self.progression.as_ref()?;
        let index = rules.powers.iter().position(|power| power.name == name)?;
        self.players[actor.owner]
            .has_science(&rules.powers[index].required_science)
            .then_some(index)
    }

    #[must_use]
    pub fn can_use_power(&mut self, caster: usize, name: &str) -> bool {
        if self.entities.get(caster).is_none() || self.object_disabled(caster) {
            return false;
        }
        if self.native_power_available(caster, name) {
            return self.end_game_at.is_none() && self.special_power_percent(caster, name) >= 1.0;
        }
        self.end_game_at.is_none()
            && self.power_index(caster, name).is_some_and(|index| {
                self.players[self.entities[caster].owner].power_ready_at[index]
                    .is_some_and(|ready| self.tick >= ready)
            })
    }

    #[must_use]
    pub fn power_cooldown_remaining(&mut self, caster: usize, name: &str) -> u32 {
        if self.native_power_available(caster, name) {
            return self.special_power_remaining(caster, name);
        }
        let Some(index) = self.power_index(caster, name) else {
            return 0;
        };
        let owner = self.entities[caster].owner;
        self.players[owner].power_ready_at[index].map_or(0, |ready| {
            u32::try_from(ready.saturating_sub(self.tick)).unwrap_or(u32::MAX)
        })
    }

    /// Source shared-timer fraction used by the command button's recharge clock.
    #[must_use]
    pub fn power_progress(&mut self, caster: usize, name: &str) -> f32 {
        if self.native_power_available(caster, name) {
            return self.special_power_percent(caster, name);
        }
        let Some(index) = self.power_index(caster, name) else {
            return 0.0;
        };
        let Some(rules) = &self.progression else {
            return 0.0;
        };
        let reload_frames = rules.powers[index].reload_frames;
        1.0 - integer_real(self.power_cooldown_remaining(caster, name))
            / integer_real(reload_frames)
    }

    /// `ActionManager::canDoSpecialPowerAtLocation`, including the pinned
    /// Sneak Attack placement fix and the original reconnaissance exception.
    pub fn can_use_power_at_location(
        &mut self,
        caster: usize,
        name: &str,
        position: [f32; 3],
    ) -> bool {
        if !position.into_iter().all(f32::is_finite)
            || !self.native_power_available(caster, name)
            || !self.can_use_power(caster, name)
        {
            return false;
        }
        let kind = self.entities[caster].type_index;
        let Some(rules) = self.catalog[kind]
            .special_powers
            .iter()
            .find(|power| power.name == name)
        else {
            return false;
        };
        let point = [position[0], position[1]];
        match rules.power_type.as_str() {
            "SPECIAL_SPY_SATELLITE"
            | "SPECIAL_RADAR_VAN_SCAN"
            | "SPECIAL_SPY_DRONE"
            | "SPECIAL_HELIX_NAPALM_BOMB" => point.iter().enumerate().all(|(axis, value)| {
                self.playable_extent[0][axis] < *value && *value < self.playable_extent[1][axis]
            }),
            "SPECIAL_SNEAK_ATTACK" => self.catalog[kind]
                .behavior
                .ocl_powers
                .iter()
                .find(|power| power.power == name)
                .is_some_and(|power| {
                    self.sneak_attack_location_valid(caster, &power.reference, position)
                }),
            "SPECIAL_PARADROP_AMERICA"
            | "INFA_SPECIAL_PARADROP_AMERICA"
            | "SPECIAL_CRATE_DROP"
            | "SPECIAL_TANK_PARADROP" => {
                let ground = self.ground_height(point);
                !self
                    .movement_terrain
                    .parachute_water_height(point, ground)
                    .is_some_and(|water| water > ground)
                    && !matches!(
                        self.visibility_cell(self.entities[caster].owner, point),
                        super::visibility::Status::Shrouded
                    )
            }
            "SPECIAL_DAISY_CUTTER"
            | "AIRF_SPECIAL_DAISY_CUTTER"
            | "SPECIAL_CARPET_BOMB"
            | "SPECIAL_CHINA_CARPET_BOMB"
            | "SPECIAL_LEAFLET_DROP"
            | "EARLY_SPECIAL_LEAFLET_DROP"
            | "EARLY_SPECIAL_CHINA_CARPET_BOMB"
            | "AIRF_SPECIAL_CARPET_BOMB"
            | "SUPR_SPECIAL_CRUISE_MISSILE"
            | "SPECIAL_CLUSTER_MINES"
            | "NUKE_SPECIAL_CLUSTER_MINES"
            | "SPECIAL_EMP_PULSE"
            | "SPECIAL_NAPALM_STRIKE"
            | "SPECIAL_BLACK_MARKET_NUKE"
            | "SPECIAL_ANTHRAX_BOMB"
            | "SPECIAL_TERROR_CELL"
            | "SPECIAL_AMBUSH"
            | "SPECIAL_NEUTRON_MISSILE"
            | "NUKE_SPECIAL_NEUTRON_MISSILE"
            | "SUPW_SPECIAL_NEUTRON_MISSILE"
            | "SPECIAL_SCUD_STORM"
            | "SPECIAL_A10_THUNDERBOLT_STRIKE"
            | "AIRF_SPECIAL_A10_THUNDERBOLT_STRIKE"
            | "SPECIAL_SPECTRE_GUNSHIP"
            | "AIRF_SPECIAL_SPECTRE_GUNSHIP"
            | "SPECIAL_REPAIR_VEHICLES"
            | "EARLY_SPECIAL_REPAIR_VEHICLES"
            | "SPECIAL_GPS_SCRAMBLER"
            | "SLTH_SPECIAL_GPS_SCRAMBLER"
            | "SPECIAL_ARTILLERY_BARRAGE"
            | "SPECIAL_FRENZY"
            | "EARLY_SPECIAL_FRENZY"
            | "SPECIAL_PARTICLE_UPLINK_CANNON"
            | "SUPW_SPECIAL_PARTICLE_UPLINK_CANNON"
            | "LAZR_SPECIAL_PARTICLE_UPLINK_CANNON"
            | "SPECIAL_CLEANUP_AREA"
            | "SPECIAL_BATTLESHIP_BOMBARDMENT" => !matches!(
                self.visibility_cell(self.entities[caster].owner, point),
                super::visibility::Status::Shrouded
            ),
            "SPECIAL_LAUNCH_BAIKONUR_ROCKET" => true,
            _ => false,
        }
    }

    pub fn can_use_power_at_object(&mut self, caster: usize, name: &str, target: usize) -> bool {
        if !self.can_use_power(caster, name) || self.entities.get(target).is_none() {
            return false;
        }
        let Some(ordinal) = self.special_power_ordinal(caster, name) else {
            return self.power_index(caster, name).is_some()
                && self.entities[target].complete()
                && self.enemies(self.entities[target].owner, self.entities[caster].owner)
                && self.catalog[self.entities[target].type_index]
                    .progression
                    .cash_hack_target;
        };
        let kind = self.entities[caster].type_index;
        let Some(power_type) = super::special_ability::PowerType::parse(
            &self.catalog[kind].special_powers[ordinal].power_type,
        ) else {
            return false;
        };
        self.can_special_power_at_object_from(
            caster,
            name,
            power_type,
            target,
            super::ai_goals::COMMAND_FROM_PLAYER,
        )
    }

    /// Executes the selected source power through a living completed owner module.
    ///
    /// # Errors
    /// Rejects unavailable powers, invalid targets, and powers still recharging.
    pub fn cast_power(
        &mut self,
        caster: usize,
        name: &str,
        target: PowerTarget,
    ) -> Result<(), &'static str> {
        if !self.can_use_power(caster, name) {
            return Err("Power is unavailable or still recharging");
        }
        if let Some(ordinal) = self.special_power_ordinal(caster, name) {
            return self.cast_native_power(caster, name, ordinal, &target);
        }
        let target = match target {
            PowerTarget::Position3D([x, y, _]) => PowerTarget::Position([x, y]),
            target => target,
        };
        let index = self
            .power_index(caster, name)
            .ok_or("Power is not defined")?;
        let owner = self.entities[caster].owner;
        let power = &self
            .progression
            .as_ref()
            .ok_or("Progression is not configured")?
            .powers[index];
        match (&power.effect, target) {
            (PowerEffect::CashHack { amount, upgrades }, PowerTarget::Object(target)) => {
                let victim = self.entities.get(target).ok_or("Target does not exist")?;
                if !victim.complete()
                    || !self.enemies(victim.owner, owner)
                    || !self.catalog[victim.type_index].progression.cash_hack_target
                {
                    return Err("Cash Hack requires a completed enemy capturable cash generator");
                }
                let desired = upgrades
                    .iter()
                    .find(|(science, _)| self.players[owner].has_science(science))
                    .map_or(*amount, |(_, amount)| *amount);
                let cash = self.players[victim.owner].cash.min(desired);
                self.players[victim.owner].cash -= cash;
                self.players[owner].cash = self.players[owner].cash.saturating_add(cash);
                self.players[owner].statistics.money_earned += cash;
            }
            (PowerEffect::EmergencyRepair { burst, upgrades }, PowerTarget::Position(position)) => {
                if self.cell_at(position).is_none() {
                    return Err("Repair location is outside the map");
                }
                let burst = upgrades
                    .iter()
                    .find(|(science, _)| self.players[owner].has_science(science))
                    .map_or(burst, |(_, burst)| burst);
                // The installed SingleBurst marker wakes one frame later, then its
                // zero-lifetime DeletionUpdate destroys it after the heal update.
                self.repair_markers.push(RepairMarker {
                    owner,
                    position,
                    amount: burst.amount,
                    radius: burst.radius,
                    wake_frame: self.tick + u64::from(burst.delay_frames),
                    heal_lock_frames: burst.delay_frames,
                });
            }
            _ => return Err("Wrong target type for this power"),
        }
        self.players[owner].power_ready_at[index] =
            Some(self.tick + u64::from(power.reload_frames));
        Ok(())
    }

    fn cast_native_power(
        &mut self,
        caster: usize,
        name: &str,
        ordinal: usize,
        target: &PowerTarget,
    ) -> Result<(), &'static str> {
        let accepted = match *target {
            PowerTarget::None => {
                let kind = self.entities[caster].type_index;
                let power_type = self.catalog[kind].special_powers[ordinal]
                    .power_type
                    .as_str();
                if !matches!(
                    power_type,
                    "SPECIAL_CIA_INTELLIGENCE"
                        | "SPECIAL_COMMUNICATIONS_DOWNLOAD"
                        | "SPECIAL_REMOTE_CHARGES"
                        | "SPECIAL_DETONATE_DIRTY_NUKE"
                        | "SPECIAL_CHANGE_BATTLE_PLANS"
                        | "SPECIAL_LAUNCH_BAIKONUR_ROCKET"
                ) {
                    return Err("Power requires a target");
                }
                self.native_special_power(caster, name, None, None, false)
            }
            PowerTarget::Construction { position, angle } => {
                let kind = self.entities[caster].type_index;
                if self.catalog[kind].special_powers[ordinal].power_type != "SPECIAL_SNEAK_ATTACK" {
                    return Err("Power does not support construction targeting");
                }
                if !self.can_use_power_at_location(caster, name, position) || !angle.is_finite() {
                    return Err("Invalid special power construction location");
                }
                let kind = self.entities[caster].type_index;
                let Some(ocl) = self.catalog[kind]
                    .behavior
                    .ocl_powers
                    .iter()
                    .position(|power| power.power == name)
                else {
                    return Err("Power has no construction list");
                };
                if self.object_disabled(caster) {
                    return Err("Power provider is disabled");
                }
                self.execute_power_ocl(caster, ocl, position, Some(angle));
                true
            }
            PowerTarget::Object(target) => {
                if self.entities.get(target).is_none() {
                    return Err("Target does not exist");
                }
                let kind = self.entities[caster].type_index;
                let power_type = &self.catalog[kind].special_powers[ordinal].power_type;
                let Some(power_type) = super::special_ability::PowerType::parse(power_type) else {
                    return Err("Power requires a location");
                };
                if !self.can_special_power_at_object_from(
                    caster,
                    name,
                    power_type,
                    target,
                    super::ai_goals::COMMAND_FROM_PLAYER,
                ) {
                    return Err("Invalid special power target");
                }
                self.special_power_at_object(caster, name, target)
            }
            PowerTarget::Position(_) | PowerTarget::Position3D(_) => {
                let position = match *target {
                    PowerTarget::Position([x, y]) if x.is_finite() && y.is_finite() => {
                        [x, y, self.ground_height([x, y])]
                    }
                    PowerTarget::Position(_) => return Err("Invalid special power location"),
                    PowerTarget::Position3D(position) => position,
                    _ => unreachable!("location target"),
                };
                if !self.can_use_power_at_location(caster, name, position) {
                    return Err("Invalid special power location");
                }
                self.special_power_at_location(caster, name, position, false)
            }
        };
        accepted
            .then_some(())
            .ok_or("Special power module rejected the command")
    }

    pub(super) fn update_repair_markers(&mut self) {
        for marker_index in 0..self.repair_markers.len() {
            let marker = &self.repair_markers[marker_index];
            if marker.wake_frame > self.tick {
                continue;
            }
            let owner = marker.owner;
            let position = marker.position;
            let amount = marker.amount;
            let radius = marker.radius;
            let heal_lock_frames = marker.heal_lock_frames;
            for id in 0..self.entities.len() {
                let entity = &mut self.entities[id];
                let unit = &self.catalog[entity.type_index];
                if super::setup::allied(&self.slots, entity.owner, owner)
                    && entity.alive()
                    && unit.progression.vehicle
                    && entity.health < entity.max_health
                    && distance_squared(entity.position, position) <= radius * radius
                    && self.tick > entity.maintenance.healing_benefactor_until
                {
                    entity.health = (entity.health + amount).min(entity.max_health);
                    if unit.body_kind.is_active() {
                        entity.refresh_body_health();
                    }
                    entity.maintenance.healing_benefactor = None;
                    entity.maintenance.healing_benefactor_until =
                        self.tick + u64::from(heal_lock_frames);
                    self.clear_poison(id);
                    super::overlord::health_changed(&mut self.entities, &self.catalog, id);
                }
            }
        }
        self.repair_markers
            .retain(|marker| marker.wake_frame > self.tick);
    }
}

fn add_science(player: &mut Player, name: &str) {
    if !player.has_science(name) {
        player.sciences.push(name.to_owned());
    }
}

pub(super) fn integer_real(value: u32) -> f32 {
    let bytes = value.to_le_bytes();
    f32::from(u16::from_le_bytes([bytes[2], bytes[3]])).mul_add(
        65_536.0,
        f32::from(u16::from_le_bytes([bytes[0], bytes[1]])),
    )
}

pub(super) fn real_unsigned(value: f32) -> u32 {
    // Positive integral IEEE-754 conversion after source ceil; avoids saturating
    // casts hiding invalid input. Bounty is bounded by validated source build cost.
    if value < 1.0 {
        return 0;
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 23) & 0xff) - 127;
    let mantissa = (bits & 0x7f_ffff) | 0x80_0000;
    if exponent < 23 {
        mantissa >> (23 - exponent)
    } else {
        mantissa.checked_shl(exponent - 23).unwrap_or(u32::MAX)
    }
}

fn validate_rules(rules: &ProgressionRules) -> Result<(), &'static str> {
    if rules.ranks.is_empty()
        || rules.ranks[0].skill_points != 0
        || rules
            .ranks
            .windows(2)
            .any(|ranks| ranks[0].skill_points >= ranks[1].skill_points)
        || rules
            .ranks
            .iter()
            .any(|rank| rank.skill_points > i32::MAX.unsigned_abs())
    {
        return Err("Invalid source rank thresholds");
    }
    let total_points = rules
        .ranks
        .iter()
        .try_fold(0_u32, |sum, rank| sum.checked_add(rank.science_points))
        .ok_or("Promotion point total overflows")?;
    for (index, faction) in rules.factions.iter().enumerate() {
        if total_points.checked_add(faction.intrinsic_points).is_none()
            || rules.factions[..index]
                .iter()
                .any(|other| other.faction == faction.faction)
        {
            return Err("Invalid faction promotion points");
        }
    }
    let exists = |name: &str| rules.sciences.iter().any(|science| science.name == name);
    for (index, science) in rules.sciences.iter().enumerate() {
        if science.name.is_empty()
            || rules.sciences[..index]
                .iter()
                .any(|other| other.name == science.name)
            || science.prerequisites.iter().any(|name| !exists(name))
        {
            return Err("Invalid science reference");
        }
    }
    if rules
        .ranks
        .iter()
        .any(|rank| rank.sciences.iter().any(|name| !exists(name)))
        || rules
            .factions
            .iter()
            .any(|faction| faction.intrinsic_sciences.iter().any(|name| !exists(name)))
    {
        return Err("Invalid granted science reference");
    }
    for (index, power) in rules.powers.iter().enumerate() {
        if power.name.is_empty()
            || !exists(&power.required_science)
            || power.reload_frames == 0
            || rules.powers[..index]
                .iter()
                .any(|other| other.name == power.name)
        {
            return Err("Invalid special power definition");
        }
        let valid = match &power.effect {
            PowerEffect::CashHack { upgrades, .. } => upgrades.iter().all(|(name, _)| exists(name)),
            PowerEffect::EmergencyRepair { burst, upgrades } => {
                valid_burst(burst)
                    && upgrades
                        .iter()
                        .all(|(name, burst)| exists(name) && valid_burst(burst))
            }
        };
        if !valid {
            return Err("Invalid special power upgrade");
        }
    }
    Ok(())
}

fn valid_burst(burst: &RepairBurst) -> bool {
    burst.amount.is_finite()
        && burst.amount > 0.0
        && burst.radius.is_finite()
        && burst.radius > 0.0
        && burst.delay_frames == 1
}
