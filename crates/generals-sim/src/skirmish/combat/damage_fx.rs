//! SH 2bfaea60 `ActiveBody::doDamageFX`, `DamageFX` and `ArmorSet` `SparseMatchFinder`.
use crate::skirmish::{DAMAGE_TYPE_COUNT, DamageType, Entity, Skirmish};

pub struct Entry {
    pub major_threshold: f32,
    pub major: String,
    pub minor: String,
    pub throttle: u32,
}

pub struct Rules {
    pub entries: [[Entry; 4]; DAMAGE_TYPE_COUNT],
}

impl Rules {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: std::array::from_fn(|_| {
                std::array::from_fn(|_| Entry {
                    major_threshold: 0.0,
                    major: String::new(),
                    minor: String::new(),
                    throttle: 0,
                })
            }),
        }
    }
}

pub struct ArmorSet {
    /// `ArmorSetType` ordinal bits; rank and crate flags are mutually exclusive.
    pub conditions: u8,
    pub effects: Option<Rules>,
}

pub(in crate::skirmish) struct State {
    last: Option<DamageType>,
    next: u32,
}

impl State {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            last: None,
            next: 0,
        }
    }
}

impl Entity {
    fn damage_fx_armor_flags(&self) -> u8 {
        let rank = match self.veterancy {
            1 => 1,
            2 => 2,
            3 => 4,
            _ => 0,
        };
        let crate_level = match self.salvage_level() {
            1 => 1 << 6,
            2 => 1 << 7,
            _ => 0,
        };
        // WEAK_VERSUS_BASEDEFENSES has no producer in pinned Core + GeneralsMD.
        rank | crate_level
            | (u8::from(self.armor_upgrade_active()) << 3)
            | (u8::from(self.battle_bus_second_life()) << 5)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn body_damage_fx_set(&self, id: usize) -> Option<usize> {
        let actor = &self.entities[id];
        let flags = actor.damage_fx_armor_flags();
        let sets = &self.catalog[actor.type_index].damage_fx;
        let mut result = None;
        let mut best_matches = 0;
        let mut best_extra = u32::MAX;
        for (index, set) in sets.iter().enumerate() {
            let matches = (flags & set.conditions).count_ones();
            let extra = (!flags & set.conditions).count_ones();
            if matches > best_matches || (matches == best_matches && extra < best_extra) {
                result = Some(index);
                best_matches = matches;
                best_extra = extra;
            }
        }
        result
    }

    pub(in crate::skirmish) fn do_body_damage_fx(
        &mut self,
        id: usize,
        set: Option<usize>,
        actual_damage: f32,
        damage_type: DamageType,
        source: Option<usize>,
    ) {
        let actor = &self.entities[id];
        let Some(rules) = set.and_then(|index| {
            self.catalog[actor.type_index].damage_fx[index]
                .effects
                .as_ref()
        }) else {
            return;
        };
        let now = self.update_frame();
        let state = &actor.damage_state.damage_fx;
        if state
            .last
            .is_some_and(|last| last as usize == damage_type as usize)
            && state.next > now
        {
            return;
        }
        let source = source.filter(|&other| self.entities.get(other).is_some_and(Entity::exists));
        let level = source.map_or(0, |other| usize::from(self.entities[other].veterancy));
        let entry = &rules.entries[damage_type as usize][level];
        let throttle = entry.throttle;
        let effect = if actual_damage == 0.0 {
            String::new()
        } else if actual_damage >= entry.major_threshold {
            entry.major.clone()
        } else {
            entry.minor.clone()
        };
        let state = &mut self.entities[id].damage_state.damage_fx;
        state.last = Some(damage_type);
        state.next = now.wrapping_add(throttle);
        if effect.is_empty() {
            return;
        }
        let actor = &self.entities[id];
        let basis = actor
            .projectile_transform()
            .map(|(_, axes)| {
                std::array::from_fn(|row| std::array::from_fn(|column| axes[column][row]))
            })
            .or_else(|| actor.physical_basis())
            .unwrap_or_else(|| crate::skirmish::physical_ocl::transform::yaw(actor.facing));
        let mut event = self.snapshot_object_fx(id, effect, self.object_position(id), basis);
        event.secondary_position = source.map(|other| self.object_position(other));
        self.presentation
            .weapon_fx
            .push(crate::skirmish::WeaponFxEvent::ObjectFx(
                self.presentation.object_fx_events.len(),
            ));
        self.presentation.object_fx_events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{
        DeathType,
        tests::{death_fx, game, register_behavior},
    };

    fn effects() -> Rules {
        let mut rules = Rules::empty();
        for entry in rules.entries.iter_mut().flatten() {
            entry.major_threshold = 10.0;
            entry.major = "MajorHit".into();
            entry.minor = "MinorHit".into();
            entry.throttle = 3;
        }
        rules.entries[DamageType::Crush as usize][2].major = "EliteCrush".into();
        rules
    }

    #[test]
    fn zero_damage_throttles_same_type_but_other_type_and_exact_expiry_emit() {
        let mut game = game();
        game.catalog[5].damage_fx.push(ArmorSet {
            conditions: 0,
            effects: Some(effects()),
        });
        let victim = game.spawn(5, 0, [300.0, 300.0], true);
        let source = game.spawn(6, 1, [400.0, 300.0], true);
        game.entities[source].veterancy = 2;
        game.apply_damage(
            victim,
            Some(source),
            0.0,
            DamageType::Crush,
            DeathType::Crushed,
        );
        game.apply_damage(
            victim,
            Some(source),
            10.0,
            DamageType::Crush,
            DeathType::Crushed,
        );
        assert!(game.presentation.object_fx_events.is_empty());
        game.apply_damage(
            victim,
            Some(source),
            1.0,
            DamageType::SmallArms,
            DeathType::Normal,
        );
        assert_eq!(game.presentation.object_fx_events[0].effect, "MinorHit");
        game.apply_damage(
            victim,
            Some(source),
            10.0,
            DamageType::Crush,
            DeathType::Crushed,
        );
        assert_eq!(game.presentation.object_fx_events[1].effect, "EliteCrush");
        assert_eq!(
            game.presentation.object_fx_events[1]
                .secondary_position
                .map(|p| p.map(f32::to_bits)),
            Some(game.object_position(source).map(f32::to_bits))
        );
        game.tick += 3;
        game.apply_damage(
            victim,
            Some(source),
            10.0,
            DamageType::Crush,
            DeathType::Crushed,
        );
        assert_eq!(game.presentation.object_fx_events.len(), 3);
    }

    #[test]
    fn damage_effect_follows_die_callbacks_and_snapshots_victim_pose() {
        let mut game = game();
        let unit = &mut game.catalog[5];
        unit.damage_fx.push(ArmorSet {
            conditions: 0,
            effects: Some(effects()),
        });
        unit.behavior.death_fx.push(death_fx("DeathEffect"));
        register_behavior(unit, "FXListDie", 0x82);
        let victim = game.spawn(5, 0, [300.0, 300.0], true);
        game.set_object_orientation(victim, std::f32::consts::FRAC_PI_2);
        game.apply_damage(victim, None, 1_000.0, DamageType::Crush, DeathType::Crushed);
        assert_eq!(
            game.presentation
                .object_fx_events
                .iter()
                .map(|event| event.effect.as_str())
                .collect::<Vec<_>>(),
            ["DeathEffect", "MajorHit"]
        );
        assert_eq!(
            game.presentation.object_fx_events[1].basis[1][0].to_bits(),
            1.0_f32.to_bits()
        );
        assert!(
            game.presentation.object_fx_events[1]
                .secondary_position
                .is_none()
        );
    }

    #[test]
    fn armor_sparse_match_prefers_more_matches_then_fewer_extras_then_first_tie() {
        let mut game = game();
        game.catalog[5].damage_fx = [0, 4 | 8, 4, 4]
            .into_iter()
            .map(|conditions| ArmorSet {
                conditions,
                effects: None,
            })
            .collect();
        let victim = game.spawn(5, 0, [300.0, 300.0], true);
        assert_eq!(game.body_damage_fx_set(victim), Some(0));
        game.entities[victim].veterancy = 3;
        assert_eq!(game.body_damage_fx_set(victim), Some(2));
    }
    #[test]
    fn full_health_healing_uses_unclipped_amount_and_zero_healing_throttles() {
        let mut game = game();
        game.catalog[5].damage_fx.push(ArmorSet {
            conditions: 0,
            effects: Some(effects()),
        });
        let victim = game.spawn(5, 0, [300.0; 2], true);
        let health = game.entities[victim].health;
        game.apply_healing(victim, None, 0.0);
        game.apply_healing(victim, None, 20.0);
        assert!(game.presentation.object_fx_events.is_empty());
        game.tick += 3;
        game.apply_healing(victim, None, 20.0);
        assert_eq!(game.entities[victim].health.to_bits(), health.to_bits());
        assert_eq!(game.presentation.object_fx_events[0].effect, "MajorHit");
    }

    #[test]
    fn handled_subdual_and_status_damage_emit_without_health_loss_or_damage_scalar() {
        let mut game = game();
        game.catalog[5].damage_fx.push(ArmorSet {
            conditions: 0,
            effects: Some(effects()),
        });
        game.catalog[5].maintenance.subdual = Some(crate::skirmish::microwave::SubdualRules {
            cap: 150.0,
            heal_rate: 3,
            heal_amount: 10.0,
        });
        let victim = game.spawn(5, 0, [300.0; 2], true);
        let health = game.entities[victim].health;
        game.entities[victim].battle_bonuses.armor = 0.1;
        game.apply_weapon_hit(
            victim,
            2,
            20.0,
            DamageType::SubdualVehicle,
            DeathType::Normal,
        );
        game.apply_weapon_hit(victim, 2, 20.0, DamageType::Status, DeathType::Normal);
        assert_eq!(game.entities[victim].health.to_bits(), health.to_bits());
        assert_eq!(
            game.entities[victim].subdual_damage().to_bits(),
            20.0_f32.to_bits()
        );
        assert_eq!(
            game.presentation
                .object_fx_events
                .iter()
                .map(|event| event.effect.as_str())
                .collect::<Vec<_>>(),
            ["MajorHit", "MajorHit"]
        );
    }

    #[test]
    fn hazard_hit_effect_follows_death_and_indestructible_bodies_do_not_emit() {
        let mut game = game();
        let unit = &mut game.catalog[5];
        unit.damage_fx.push(ArmorSet {
            conditions: 0,
            effects: Some(effects()),
        });
        unit.behavior.death_fx.push(death_fx("DeathEffect"));
        register_behavior(unit, "FXListDie", 0x82);
        let victim = game.spawn(5, 0, [300.0; 2], true);
        game.apply_hazard_hit(victim, None, 1000.0, DamageType::Flame, DeathType::Burned);
        assert_eq!(
            game.presentation
                .object_fx_events
                .iter()
                .map(|event| event.effect.as_str())
                .collect::<Vec<_>>(),
            ["DeathEffect", "MajorHit"]
        );
        let other = game.spawn(5, 0, [400.0; 2], true);
        game.set_map_body_controls(other, true, true);
        game.apply_hazard_hit(other, None, 1000.0, DamageType::Flame, DeathType::Burned);
        assert_eq!(game.presentation.object_fx_events.len(), 2);
    }
}
