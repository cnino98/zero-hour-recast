//! `ExperienceTracker`, `VeterancyGainCreate` and the inherited self-heal upgrade.
use super::{Entity, Skirmish, UnitType};
use num_traits::ToPrimitive;

/// Original `Object::createVeterancyLevelFX` callback at the promotion position.
pub struct PromotionEvent {
    pub frame: u64,
    pub object: usize,
    pub position: [f32; 3],
}

pub struct StartingVeterancy {
    pub science: Option<String>,
    pub level: u8,
}

pub struct VeterancyRules {
    pub trainable: bool,
    pub experience_value: [i32; 4],
    pub required: [i32; 4],
    pub health_bonus: [f32; 4],
    pub starting: Vec<StartingVeterancy>,
}

impl VeterancyRules {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            trainable: false,
            experience_value: [0; 4],
            required: [0; 4],
            health_bonus: [1.0; 4],
            starting: Vec::new(),
        }
    }
}

pub(super) fn validate(unit: &UnitType) -> Result<(), &'static str> {
    let rules = &unit.veterancy;
    let creates = unit
        .behavior_modules
        .iter()
        .filter(|module| module.family == "VeterancyGainCreate" && module.interfaces & 0x8 != 0)
        .count();
    if creates != rules.starting.len() {
        return Err("VeterancyGainCreate data differs from ordered CREATE modules");
    }
    if rules
        .health_bonus
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
        || rules.starting.iter().any(|grant| grant.level > 3)
    {
        return Err("Invalid source veterancy values");
    }
    Ok(())
}

impl Entity {
    /// `ExperienceTracker::setExperienceSink` is independent of trainability.
    pub(in crate::skirmish) const fn set_experience_sink(&mut self, sink: Option<usize>) {
        self.experience_sink = sink;
    }

    pub(super) fn weapon_damage_bonus(&self, weapon: &super::WeaponRules, extra: u32) -> f32 {
        self.weapon_factor(weapon, extra, super::containment::bonuses::DAMAGE)
    }

    pub(super) fn weapon_rate_for(&self, weapon: &super::WeaponRules, extra: u32) -> f32 {
        self.weapon_factor(weapon, extra, super::containment::bonuses::RATE)
    }

    pub(super) fn weapon_range(&self, unit: &UnitType, extra: u32) -> f32 {
        self.weapon_range_for(self.active_weapon(unit), extra)
    }

    pub(super) fn weapon_range_for(&self, weapon: &super::WeaponRules, extra: u32) -> f32 {
        weapon.range * self.weapon_factor(weapon, extra, super::containment::bonuses::RANGE)
    }
}

impl Skirmish {
    pub(super) fn created_veterancy_gain(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].veterancy;
        let grant = &rules.starting[ordinal];
        let level = grant.level;
        if rules.trainable
            && level > actor.veterancy
            && grant
                .science
                .as_ref()
                .is_none_or(|science| self.players[actor.owner].has_science(science))
        {
            self.entities[id].experience = rules.required[usize::from(level)];
            // VeterancyGainCreate passes provideFeedback=false to setMinVeterancyLevel.
            self.change_veterancy(id, level, false);
        }
    }

    pub(super) fn add_unit_experience(&mut self, id: usize, amount: i32) {
        self.add_experience_points(id, amount, true);
    }

    // ExperienceTracker's canScaleForBonus defaults to true at ordinary callers.
    pub(in crate::skirmish) fn add_experience_points(
        &mut self,
        id: usize,
        amount: i32,
        can_scale: bool,
    ) {
        let entity = &self.entities[id];
        // Every sink edge scales and truncates before the next tracker sees
        // the amount. An untrainable child still transfers to its live sink.
        if let Some(sink) = entity
            .experience_sink
            .filter(|&sink| self.entities[sink].exists())
        {
            let scaled = (amount.to_f32().expect("native experience Real")
                * entity.experience_scalar())
            .to_i32()
            .expect("native scaled experience Int");
            self.add_experience_points(sink, scaled, can_scale);
            return;
        }
        let entity = &mut self.entities[id];
        let rules = &self.catalog[entity.type_index].veterancy;
        if !entity
            .combat_bike
            .rider_trainable
            .unwrap_or(rules.trainable)
        {
            return;
        }
        let scaled = if can_scale {
            (amount.to_f32().expect("native experience Real") * entity.experience_scalar())
                .to_i32()
                .expect("native scaled experience Int")
        } else {
            amount
        };
        entity.experience = entity.experience.wrapping_add(scaled);
        let mut level = 0_u8;
        while level < 3 && entity.experience >= rules.required[usize::from(level + 1)] {
            level += 1;
        }
        self.change_veterancy(id, level, true);
    }

    pub(in crate::skirmish) fn gain_experience_levels(
        &mut self,
        id: usize,
        levels: u8,
        can_scale: bool,
    ) {
        let actor = &self.entities[id];
        let next = actor.veterancy.saturating_add(levels).min(3);
        if next > actor.veterancy {
            let needed = self.catalog[actor.type_index].veterancy.required[usize::from(next)]
                .wrapping_sub(actor.experience);
            self.add_experience_points(id, needed, can_scale);
        }
    }

    pub(super) fn set_veterancy(&mut self, id: usize, level: u8) {
        // ExperienceTracker::setVeterancyLevel explicitly permits untrainable cars.
        if self.entities[id].veterancy != level {
            self.entities[id].experience = self.catalog[self.entities[id].type_index]
                .veterancy
                .required[usize::from(level)];
            self.change_veterancy(id, level, true);
        }
    }

    pub(super) fn set_hijacker_veterancy(&mut self, id: usize, level: u8, feedback: bool) {
        if self.entities[id].veterancy != level {
            self.entities[id].experience = self.catalog[self.entities[id].type_index]
                .veterancy
                .required[usize::from(level)];
            self.change_veterancy(id, level, feedback);
        }
    }

    pub(super) fn clear_pilot_veterancy(&mut self, id: usize) {
        let rules = &self.catalog[self.entities[id].type_index].veterancy;
        if self.entities[id]
            .combat_bike
            .rider_trainable
            .unwrap_or(rules.trainable)
        {
            self.entities[id].experience = 0;
            let mut level = 0;
            while level < 3 && rules.required[usize::from(level + 1)] == 0 {
                level += 1;
            }
            self.change_veterancy(id, level, false);
        }
    }

    fn change_veterancy(&mut self, id: usize, level: u8, feedback: bool) {
        let extra = self.passenger_bonus_flags(id);
        let entity = &mut self.entities[id];
        let old = entity.veterancy;
        if old == level {
            return;
        }
        entity.veterancy = level;
        // Fixed-CRC Object::onVeterancyLevelChanged retains the tracker change,
        // but refuses health, upgrade and weapon bonuses for effectively dead units.
        if entity.is_effectively_dead() {
            return;
        }
        let unit = &self.catalog[entity.type_index];
        let rules = &unit.veterancy;
        entity.veterancy_upgrades |= 1 << level;
        let multiplier =
            rules.health_bonus[usize::from(level)] / rules.health_bonus[usize::from(old)];
        let maximum = entity.max_health * multiplier;
        let ratio = entity.health / entity.max_health;
        let desired = maximum * ratio;
        let delta = desired - entity.health;
        entity.max_health = maximum;
        entity.health = (entity.health + delta).clamp(0.0, maximum);
        if unit.body_kind.is_active() {
            entity.refresh_body_health();
        }
        self.activate_upgrade_modules(id);
        super::overlord::health_changed(&mut self.entities, &self.catalog, id);
        let desired = if level == 0 { 0 } else { 1 << (level - 1) };
        let entity = &mut self.entities[id];
        for bit in [1, 2, 4] {
            if (entity.firing.veterancy_flags ^ desired) & bit != 0 {
                entity.firing.veterancy_flags ^= bit;
                entity.weapon_bonus_changed(&self.catalog[entity.type_index], self.tick, extra);
            }
        }
        if feedback
            && level > old
            && !self.catalog[self.entities[id].type_index]
                .progression
                .ignored_in_gui
        {
            // isLogicallyVisible adds stealth/disguise gates once those systems
            // are ported. It does not reject contained units or ordinary fog.
            self.presentation.promotion_events.push(PromotionEvent {
                frame: self.tick,
                object: id,
                position: self.object_position(id),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Faction, FactionProgression, Kind, ProgressionRules, Rank, Science};

    fn redguards() -> (Skirmish, usize) {
        let mut game = super::super::tests::game();
        for unit in &mut game.catalog {
            unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
        }
        let index = game.type_for(0, Kind::Infantry).unwrap();
        let unit = &mut game.catalog[index];
        unit.max_health = 120.0;
        unit.initial_health = 120.0;
        unit.veterancy = VeterancyRules {
            trainable: true,
            experience_value: [5, 5, 10, 20],
            required: [0, 20, 40, 80],
            health_bonus: [1.0, 1.2, 1.3, 1.5],
            starting: vec![StartingVeterancy {
                science: Some("SCIENCE_RedGuardTraining".into()),
                level: 1,
            }],
        };
        crate::skirmish::tests::register_behavior(unit, "VeterancyGainCreate", 0x8);
        unit.maintenance
            .auto_heal
            .push(crate::skirmish::tests::self_heal(
                10.0,
                30,
                150,
                &["Upgrade_Veterancy_ELITE", "Upgrade_Veterancy_HEROIC"],
            ));
        crate::skirmish::tests::register_update(unit, "AutoHealBehavior");
        unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec![
            "Upgrade_Veterancy_ELITE".into(),
            "Upgrade_Veterancy_HEROIC".into(),
        ];
        for (rank, (damage, rate)) in [(1.1, 1.2), (1.2, 1.4), (1.3, 1.6)].into_iter().enumerate() {
            unit.weapon.bonuses.global[9 + rank][0] = damage;
            unit.weapon.bonuses.global[9 + rank][3] = rate;
        }
        (game, index)
    }

    #[test]
    fn kill_value_uses_victim_rank_and_promotion_preserves_damage_ratio() {
        let (mut game, infantry) = redguards();
        let attacker = game.spawn(infantry, 0, [350.0, 200.0], true);
        let victim = game.spawn(infantry, 1, [400.0, 200.0], true);
        game.entities[attacker].health = 60.0;
        game.add_unit_experience(attacker, 10);
        game.add_unit_experience(victim, 40);
        assert_eq!(game.entities[victim].veterancy, 2);
        game.apply_hit(victim, attacker, 1000.0);
        assert_eq!(game.entities[attacker].experience, 20);
        assert_eq!(game.entities[attacker].veterancy, 1);
        assert_eq!(
            game.entities[attacker].max_health.to_bits(),
            144.0_f32.to_bits()
        );
        assert_eq!(game.entities[attacker].health.to_bits(), 72.0_f32.to_bits());
        assert_eq!(game.entities[attacker].veterancy_upgrades, 0b0010);
    }

    #[test]
    fn science_purchase_changes_only_future_created_units() {
        let (mut game, infantry) = redguards();
        game.configure_progression(ProgressionRules {
            multiplayer_colors: crate::skirmish::tests::original_multiplayer_colors(),
            ranks: vec![Rank {
                skill_points: 0,
                science_points: 1,
                sciences: Vec::new(),
            }],
            sciences: vec![Science {
                name: "SCIENCE_RedGuardTraining".into(),
                prerequisites: Vec::new(),
                cost: 1,
            }],
            factions: vec![FactionProgression {
                side: "GLA".into(),
                preferred_color: 0xff00_ff00,
                playable_side: true,
                faction: Faction::Gla,
                intrinsic_points: 0,
                intrinsic_sciences: Vec::new(),
            }],
            powers: Vec::new(),
        })
        .unwrap();
        let existing = game.spawn(infantry, 0, [350.0, 200.0], true);
        game.purchase_science(0, "SCIENCE_RedGuardTraining")
            .unwrap();
        let trained = game.spawn(infantry, 0, [350.0, 250.0], true);
        let opponent = game.spawn(infantry, 1, [450.0, 250.0], true);
        assert_eq!(game.players[0].science_points, 0);
        assert_eq!(game.entities[existing].veterancy, 0);
        assert_eq!(game.entities[opponent].veterancy, 0);
        assert_eq!(game.entities[trained].veterancy, 1);
        assert_eq!(game.entities[trained].experience, 20);
        assert_eq!(game.entities[trained].health.to_bits(), 144.0_f32.to_bits());
        assert!(game.presentation.promotion_events.is_empty());
        assert!(!game.can_purchase_science(0, "SCIENCE_RedGuardTraining"));
    }

    #[test]
    fn skipped_rank_grants_only_its_upgrade_and_damage_restarts_self_heal_delay() {
        let (mut game, infantry) = redguards();
        let id = game.spawn(infantry, 0, [350.0, 200.0], true);
        game.entities[id].health = 60.0;
        game.add_unit_experience(id, 80);
        assert_eq!(game.entities[id].veterancy_upgrades, 0b1000);
        assert_eq!(game.entities[id].max_health.to_bits(), 180.0_f32.to_bits());
        assert_eq!(game.entities[id].health.to_bits(), 90.0_f32.to_bits());
        game.step();
        assert_eq!(game.entities[id].health.to_bits(), 100.0_f32.to_bits());
        game.apply_hit(id, 3, 10.0);
        for _ in 0..149 {
            game.step();
        }
        assert_eq!(game.entities[id].health.to_bits(), 90.0_f32.to_bits());
        game.step();
        assert_eq!(game.entities[id].health.to_bits(), 100.0_f32.to_bits());
        for _ in 0..29 {
            game.step();
        }
        assert_eq!(game.entities[id].health.to_bits(), 100.0_f32.to_bits());
        game.step();
        assert_eq!(game.entities[id].health.to_bits(), 110.0_f32.to_bits());
    }

    #[test]
    fn dead_launcher_gets_tracker_xp_without_resurrection_or_upgrade_callbacks() {
        let (mut game, infantry) = redguards();
        let id = game.spawn(infantry, 0, [350.0, 200.0], true);
        game.entities[id].health = 0.0;
        game.entities[id].set_effectively_dead(true);
        game.add_unit_experience(id, 80);
        assert_eq!(game.entities[id].veterancy, 3);
        assert_eq!(game.entities[id].experience, 80);
        assert_eq!(game.entities[id].max_health.to_bits(), 120.0_f32.to_bits());
        assert_eq!(game.entities[id].health.to_bits(), 0.0_f32.to_bits());
        assert_eq!(game.entities[id].veterancy_upgrades, 0);
        assert!(!game.entities[id].maintenance.auto_heal[0].active);
        assert!(game.presentation.promotion_events.is_empty());
    }

    #[test]
    fn promotion_feedback_requires_a_crossed_level_and_retains_the_promotion_position() {
        let (mut game, infantry) = redguards();
        let id = game.spawn(infantry, 0, [350.0, 200.0], true);
        game.add_unit_experience(id, 10);
        assert!(game.presentation.promotion_events.is_empty());
        let position = game.object_position(id);
        game.add_unit_experience(id, 70);
        assert_eq!(game.presentation.promotion_events.len(), 1);
        assert_eq!(game.presentation.promotion_events[0].object, id);
        assert_eq!(
            game.presentation.promotion_events[0]
                .position
                .map(f32::to_bits),
            position.map(f32::to_bits)
        );
        game.entities[id].position[0] += 10.0;
        assert_eq!(
            game.presentation.promotion_events[0]
                .position
                .map(f32::to_bits),
            position.map(f32::to_bits)
        );
        game.add_unit_experience(id, 100);
        assert_eq!(game.presentation.promotion_events.len(), 1);
        game.reset_sandbox(crate::skirmish::Faction::Gla).unwrap();
        assert!(game.presentation.promotion_events.is_empty());
    }

    #[test]
    fn gui_ignored_object_gets_bonuses_without_promotion_feedback() {
        let (mut game, infantry) = redguards();
        game.catalog[infantry].progression.ignored_in_gui = true;
        let id = game.spawn(infantry, 0, [350.0, 200.0], true);
        game.add_unit_experience(id, 20);
        assert_eq!(game.entities[id].veterancy, 1);
        assert!(game.presentation.promotion_events.is_empty());
    }

    #[test]
    fn veteran_rate_adds_to_weapon_spin_up_and_next_shot_damage_changes() {
        let (mut game, infantry) = redguards();
        game.catalog[infantry].weapon.continuous_fire =
            Some(super::super::firing::ContinuousFire {
                mean_shots: 1,
                fast_shots: 4,
                coast_frames: 30.0,
            });
        game.catalog[infantry].weapon.bonuses.template[2][3] = 2.0;
        game.catalog[infantry].weapon.bonuses.template[3][3] = 3.0;
        let id = game.spawn(infantry, 0, [350.0, 200.0], true);
        game.add_unit_experience(id, 20);
        let target = game.spawn(infantry, 1, [400.0, 200.0], true);
        game.entities[id].weapon_set.weapons[0].possible_next_shot = game.tick + 30;
        game.notify_firing_tracker_shot(id, 0, Some(target));
        game.notify_firing_tracker_shot(id, 0, Some(target));
        let unit = &game.catalog[infantry];
        assert!(
            (game.entities[id].weapon_rate_for(game.entities[id].active_weapon(unit), 0) - 2.2)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (game.entities[id].weapon_damage_bonus(game.entities[id].active_weapon(unit), 0) - 1.1)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn promotion_bonus_is_captured_before_an_in_flight_shot_arrives() {
        let (mut game, infantry) = redguards();
        game.catalog[infantry].weapon.speed = 30.0;
        game.catalog[infantry].weapon.damage = 15.0;
        game.catalog[infantry].weapon.motion = Some(super::super::projectiles::WeaponMotion {
            turret_mounts: None,
            projectile_type: None,
            collide_mask: 0,
            shell_speed_scale: None,
            launches: std::array::from_fn(|_| {
                vec![super::super::projectiles::LaunchTransform::IDENTITY]
            }),
            damage_thresholds: [0.5, 0.25],
            shots_per_barrel: 1,
        });
        let attacker = game.spawn(infantry, 0, [350.0, 200.0], true);
        let victim = game.spawn(infantry, 1, [380.0, 200.0], true);
        game.attack(attacker, victim).unwrap();
        for _ in 0..120 {
            game.step();
            if !game.projectiles.is_empty() {
                break;
            }
        }
        assert_eq!(game.projectiles.len(), 1);
        game.stop(attacker).unwrap();
        game.add_unit_experience(attacker, 20);
        for _ in 0..40 {
            game.step();
        }
        assert_eq!(game.entities[victim].health.to_bits(), 105.0_f32.to_bits());
    }
    #[test]
    fn negative_experience_reverts_rank_and_preserves_the_damage_ratio() {
        let (mut game, infantry) = redguards();
        let id = game.spawn(infantry, 0, [350.0, 200.0], true);
        game.entities[id].health = 60.0;
        game.add_unit_experience(id, 40);
        assert_eq!(game.entities[id].veterancy, 2);
        assert_eq!(game.presentation.promotion_events.len(), 1);
        game.add_unit_experience(id, -50);
        assert_eq!(game.entities[id].experience, -10);
        assert_eq!(game.entities[id].veterancy, 0);
        assert_eq!(game.entities[id].max_health.to_bits(), 120.0_f32.to_bits());
        assert_eq!(game.entities[id].health.to_bits(), 60.0_f32.to_bits());
        assert_eq!(game.presentation.promotion_events.len(), 1);
    }

    fn scalar_upgrade(unit: &mut UnitType, scalar: f32) {
        unit.maintenance
            .experience_upgrades
            .push(crate::skirmish::ExperienceUpgrade {
                trigger: "Training".into(),
                scalar,
            });
        crate::skirmish::tests::register_behavior(unit, "ExperienceScalarUpgrade", 0x80);
        unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Training".into()];
    }

    #[test]
    fn untrainable_sink_chain_scales_and_truncates_at_each_tracker() {
        let (mut game, infantry) = redguards();
        let tank = game.type_for(0, crate::skirmish::Kind::Tank).unwrap();
        game.catalog[tank].veterancy.trainable = false;
        scalar_upgrade(&mut game.catalog[tank], 0.5);
        scalar_upgrade(&mut game.catalog[infantry], 1.0);
        let parent = game.spawn(infantry, 0, [350.0, 200.0], true);
        let middle = game.spawn(tank, 0, [375.0, 200.0], true);
        let child = game.spawn(tank, 0, [400.0, 200.0], true);
        for id in [parent, middle, child] {
            game.grant_native_object_upgrade(id, "Training");
        }
        game.entities[middle].set_experience_sink(Some(parent));
        game.entities[child].set_experience_sink(Some(middle));
        game.add_unit_experience(child, 3);
        assert_eq!(game.entities[parent].experience, 12);
        assert_eq!(game.entities[middle].experience, 0);
        assert_eq!(game.entities[child].experience, 0);
    }

    #[test]
    fn removed_sink_falls_back_to_the_own_trainable_tracker() {
        let (mut game, infantry) = redguards();
        let parent = game.spawn(infantry, 0, [350.0, 200.0], true);
        let child = game.spawn(infantry, 0, [375.0, 200.0], true);
        game.entities[child].set_experience_sink(Some(parent));
        game.destroy_maintenance_object(parent);
        game.process_update_destruction();
        assert!(!game.entities[parent].exists());
        game.add_unit_experience(child, 20);
        assert_eq!(game.entities[parent].experience, 0);
        assert_eq!(game.entities[child].experience, 20);
        assert_eq!(game.entities[child].veterancy, 1);
    }

    #[test]
    fn native_int_accumulation_wraps_before_recalculating_the_rank() {
        let (mut game, infantry) = redguards();
        game.catalog[infantry].veterancy.required[3] = i32::MAX - 5;
        let id = game.spawn(infantry, 0, [350.0, 200.0], true);
        game.set_veterancy(id, 3);
        game.add_unit_experience(id, 16);
        assert_eq!(
            game.entities[id].experience,
            (i32::MAX - 5).wrapping_add(16)
        );
        assert_eq!(game.entities[id].veterancy, 0);
        assert_eq!(game.presentation.promotion_events.len(), 1);
    }
}
