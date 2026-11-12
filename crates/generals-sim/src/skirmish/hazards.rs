//! Native poison-field `FireWeaponUpdate`/`LifetimeUpdate` and `PoisonedBehavior`.
use super::{DeathType, Entity, Skirmish, UnitType};

#[derive(Clone, Copy)]
pub enum Origin {
    Weapon(usize),
    Cooldown(usize),
}

impl UnitType {
    #[must_use]
    pub fn hazard_rules(&self, origin: Origin) -> Option<&Rules> {
        match origin {
            Origin::Weapon(index) => self.weapon_template(index)?.hazard.as_ref(),
            Origin::Cooldown(index) => self
                .behavior
                .contamination
                .get(index)
                .map(|rules| &rules.field),
        }
    }

    pub fn hazard_origins(&self) -> impl Iterator<Item = Origin> {
        (0..self.weapon_template_count())
            .map(Origin::Weapon)
            .chain((0..self.behavior.contamination.len()).map(Origin::Cooldown))
    }
}

pub struct PoisonRules {
    pub interval: u32,
    pub duration: u32,
}

pub struct Rules {
    pub name: String,
    pub damage: f32,
    pub radius: f32,
    pub interval: u32,
    pub airborne_height: f32,
    pub lifetime: u32,
    pub health: f32,
    pub geometry_radius: f32,
    pub geometry: crate::collision_geometry::Geometry,
    pub core_radius: f32,
    pub core_damage: f32,
    pub death_type: DeathType,
    pub fire_fx: String,
    pub death_fx: String,
    pub ambient: String,
}

pub struct Field {
    pub id: u64,
    pub source: usize,
    pub origin: Origin,
    pub template_type: usize,
    pub owner: usize,
    pub position: [f32; 3],
    pub created: u64,
    pub expires: u64,
    pub object: usize,
}

impl Field {
    #[must_use]
    pub fn health(&self, entities: &[Entity]) -> f32 {
        entities[self.object].health
    }
}

pub(in crate::skirmish) enum PoisonSource {
    Invalid,
    Object(usize),
}

/// Poison timers are per module; the Drawable tint is common to their Object.
pub(super) struct PoisonState {
    pub(super) instances: Vec<Option<Poison>>,
    pub(super) tint: bool,
}

impl PoisonState {
    pub(super) const fn new() -> Self {
        Self {
            instances: Vec::new(),
            tint: false,
        }
    }
}

pub(super) struct Poison {
    damage: f32,
    source: PoisonSource,
    next: u32,
    until: u32,
    death_type: DeathType,
}

impl Entity {
    #[must_use]
    pub const fn poisoned(&self) -> bool {
        self.poison.tint
    }

    pub(super) fn clear_poison(&mut self) {
        self.poison.instances.fill_with(|| None);
        if self.has_drawable() {
            self.poison.tint = false;
        }
    }
}

impl Skirmish {
    pub(super) fn create_weapon_hazard(
        &mut self,
        source: usize,
        weapon: usize,
        owner: usize,
        position: [f32; 3],
        first_update: u64,
    ) {
        self.create_hazard(
            source,
            Origin::Weapon(weapon),
            owner,
            position,
            first_update,
            None,
        );
    }

    pub(super) fn create_hazard(
        &mut self,
        source: usize,
        origin: Origin,
        owner: usize,
        position: [f32; 3],
        _first_update: u64,
        lifetime: Option<u32>,
    ) {
        let template_type = self.entities[source].type_index;
        self.create_hazard_from_template(source, template_type, origin, owner, position, lifetime);
    }

    pub(in crate::skirmish) fn create_bound_weapon_hazard(
        &mut self,
        source: usize,
        binding: super::WeaponBinding,
        owner: usize,
        position: [f32; 3],
    ) {
        self.create_hazard_from_template(
            source,
            binding.type_index,
            Origin::Weapon(binding.weapon_index),
            owner,
            position,
            None,
        );
    }

    fn create_hazard_from_template(
        &mut self,
        source: usize,
        template_type: usize,
        origin: Origin,
        owner: usize,
        position: [f32; 3],
        lifetime: Option<u32>,
    ) {
        let Some(rules) = self.catalog[template_type].hazard_rules(origin) else {
            return;
        };
        let type_index = self
            .catalog
            .iter()
            .position(|unit| unit.name == rules.name)
            .expect("admitted actual hazard Object template");
        self.hazard_serial = self.hazard_serial.wrapping_add(1);
        let id = self.hazard_serial;
        let position = [
            position[0],
            position[1],
            self.ground_height([position[0], position[1]]),
        ];
        let object = self.spawn_native_hazard(type_index, owner, position, id);
        if let Some(frames) = lifetime.filter(|&frames| frames != 0) {
            self.set_native_lifetime(object, frames);
        }
        let expires = self.native_lifetime_deadline(object);
        self.hazard_indices.insert(id, self.hazards.len());
        self.hazards.push(Field {
            id,
            object,
            source,
            origin,
            template_type,
            owner,
            position,
            created: self.tick,
            expires,
        });
    }

    pub(super) fn retire_hazard(&mut self, index: usize) {
        let field = self.hazards.remove(index);
        self.hazard_indices.remove(&field.id);
        for (index, field) in self.hazards.iter().enumerate().skip(index) {
            self.hazard_indices.insert(field.id, index);
        }
    }

    pub(in crate::skirmish) fn hazard_index(&self, id: u64) -> Option<usize> {
        self.hazard_indices.get(&id).copied()
    }

    pub(in crate::skirmish) fn poison_damage_received(
        &mut self,
        id: usize,
        ordinal: usize,
        source: PoisonSource,
        actual_damage: f32,
        death_type: DeathType,
    ) {
        let now = self.update_frame();
        let rules = &self.catalog[self.entities[id].type_index].behavior.poisoned[ordinal];
        let next = now.wrapping_add(rules.interval);
        let previous = self.entities[id].poison.instances[ordinal].as_ref();
        let next = previous
            .filter(|poison| poison.next != 0)
            .map_or(next, |poison| poison.next.min(next));
        self.entities[id].poison.instances[ordinal] = Some(Poison {
            damage: actual_damage,
            source,
            next,
            until: now.wrapping_add(rules.duration),
            death_type,
        });
        if self.entities[id].has_drawable() {
            self.entities[id].poison.tint = true;
        }
        let sleep = self.poison_sleep(id, ordinal);
        self.wake_family_update(id, "PoisonedBehavior", ordinal, sleep);
    }

    pub(in crate::skirmish) fn construct_poison_module(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].poison.instances.len(), ordinal);
        self.entities[id].poison.instances.push(None);
    }

    pub(in crate::skirmish) fn poison_module_healed(&mut self, id: usize, ordinal: usize) {
        self.entities[id].poison.instances[ordinal] = None;
        if self.entities[id].has_drawable() {
            self.entities[id].poison.tint = false;
        }
        self.wake_family_update(
            id,
            "PoisonedBehavior",
            ordinal,
            crate::skirmish::module_updates::FOREVER,
        );
    }

    pub(in crate::skirmish) fn clear_poison(&mut self, id: usize) {
        self.entities[id].clear_poison();
        for ordinal in 0..self.catalog[self.entities[id].type_index]
            .behavior
            .poisoned
            .len()
        {
            self.wake_family_update(
                id,
                "PoisonedBehavior",
                ordinal,
                crate::skirmish::module_updates::FOREVER,
            );
        }
    }

    fn poison_sleep(&self, id: usize, ordinal: usize) -> u32 {
        let Some(poison) = &self.entities[id].poison.instances[ordinal] else {
            return crate::skirmish::module_updates::FOREVER;
        };
        let now = self.update_frame();
        if poison.until == 0 || poison.until == now {
            return crate::skirmish::module_updates::FOREVER;
        }
        let next = poison.next.min(poison.until);
        if next > now { next - now } else { 1 }
    }

    pub(in crate::skirmish) fn apply_hazard_hit(
        &mut self,
        id: usize,
        owner: Option<usize>,
        amount: f32,
        damage_type: super::DamageType,
        death_type: DeathType,
    ) {
        if self.entities[id].indestructible() || self.entities[id].is_effectively_dead() {
            return;
        }
        if matches!(
            self.catalog[self.entities[id].type_index].body_kind,
            super::BodyKind::Inactive
        ) {
            self.apply_damage(id, None, amount, damage_type, death_type);
            return;
        }
        let fx_set = self.body_damage_fx_set(id);
        if amount.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            self.do_body_damage_fx(id, fx_set, 0.0, damage_type, None);
            self.repulse_after_damage(id);
            return;
        }
        self.damage_body(id, None, amount, damage_type, death_type);
        if !self.entities[id].is_effectively_dead() {
            self.do_body_damage_fx(id, fx_set, amount, damage_type, None);
            self.repulse_after_damage(id);
            return;
        }
        self.entities[id].damage_state.death_type = death_type;
        // This profile preserves NO_XP_FROM_OCL_KILLS: the inert field retains
        // player credit, but has no launcher experience sink or salvage eligibility.
        // ActiveBody scores before onDie can neutralize a captured tech building.
        if let Some(owner) = owner {
            self.record_player_kill(owner, id);
            self.score_player_kill(owner, id, None);
        }
        self.finish_death(id, None);
        self.do_body_damage_fx(id, fx_set, amount, damage_type, None);
        self.repulse_after_damage(id);
    }

    pub(in crate::skirmish) fn update_poison_module(&mut self, id: usize, ordinal: usize) -> i32 {
        let now = self.update_frame();
        let Some(poison) = self.entities[id].poison.instances[ordinal].as_ref() else {
            return i32::try_from(crate::skirmish::module_updates::FOREVER).expect("native sleep");
        };
        if poison.until == 0 {
            return i32::try_from(crate::skirmish::module_updates::FOREVER).expect("native sleep");
        }
        if poison.next != 0 && now >= poison.next {
            let amount = poison.damage;
            let death_type = poison.death_type;
            match poison.source {
                PoisonSource::Invalid => self.apply_hazard_hit(
                    id,
                    None,
                    amount,
                    super::DamageType::Unresistable,
                    death_type,
                ),
                PoisonSource::Object(source) => {
                    self.apply_typed_hit(id, source, amount, death_type);
                }
            }
            let interval =
                self.catalog[self.entities[id].type_index].behavior.poisoned[ordinal].interval;
            if let Some(poison) = &mut self.entities[id].poison.instances[ordinal] {
                poison.next = now.wrapping_add(interval);
            }
        }
        if self.entities[id].poison.instances[ordinal]
            .as_ref()
            .is_some_and(|poison| poison.until != 0 && now >= poison.until)
            && !self.entities[id].is_effectively_dead()
        {
            self.entities[id].poison.instances[ordinal] = None;
            if self.entities[id].has_drawable() {
                self.entities[id].poison.tint = false;
            }
        }
        i32::try_from(self.poison_sleep(id, ordinal)).expect("native sleep")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collision_geometry::{Geometry, Shape},
        skirmish::tests::game,
    };

    fn poison_registration(game: &mut Skirmish, type_index: usize) {
        crate::skirmish::tests::poison_damage_fixture(game, type_index);
    }

    fn direct_poison_source(game: &mut Skirmish) -> usize {
        let mut unit = crate::skirmish::tests::fixture_type(
            crate::skirmish::Faction::Civilian,
            crate::skirmish::Kind::Effect,
        );
        unit.name = "DirectPoisonSourceField".into();
        unit.kind_of = vec![
            "IMMOBILE".into(),
            "CLEANUP_HAZARD".into(),
            "INERT".into(),
            "NO_COLLIDE".into(),
        ];
        unit.kind_of.push("NO_COLLIDE".into());
        // This damage-state regression authors a source field with no periodic weapon;
        // the poison damage event is invoked explicitly through real ActiveBody handling.
        let index = game.catalog.len();
        game.catalog.push(unit);
        game.spawn(index, 0, [300.0, 300.0], true)
    }

    #[test]
    fn hazard_death_emits_one_native_snapshot_and_direct_delete_emits_none() {
        use crate::skirmish::{WeaponFxEvent, physical_ocl::transform};
        for oriented in [true, false] {
            let mut game = game();
            let mut rules = field();
            rules.death_fx = "FieldDeath".into();
            game.catalog[6].weapon.hazard = Some(rules);
            let template =
                crate::skirmish::tests::native_hazard_fixture(&mut game, 6, Origin::Weapon(0));
            game.catalog[template].behavior.death_fx[0].orient_to_object = oriented;
            let source = game.spawn(6, 0, [300.0, 300.0], true);
            game.create_weapon_hazard(source, 0, 0, [300.0, 300.0, 0.0], 1);
            let object = game.hazards[0].object;
            let serial = game.hazards[0].id;
            game.set_object_orientation(object, std::f32::consts::FRAC_PI_2);
            let (original_position, original_basis) = game.object_transform(object);
            let first_death_event = game.presentation.weapon_fx.len();
            game.kill_object(object, DeathType::Normal);
            assert_eq!(game.presentation.object_fx_events.len(), 1);
            let event = &game.presentation.object_fx_events[0];
            assert_eq!(event.object, object);
            assert_eq!(event.effect, "FieldDeath");
            assert_eq!(
                event.position.map(f32::to_bits),
                original_position.map(f32::to_bits)
            );
            assert_eq!(event.oriented_object, oriented);
            assert!(event.secondary_position.is_none());
            let basis = if oriented {
                original_basis
            } else {
                transform::yaw(0.0)
            };
            assert_eq!(
                event.basis.map(|row| row.map(f32::to_bits)),
                basis.map(|row| row.map(f32::to_bits))
            );
            assert!(matches!(
                &game.presentation.weapon_fx[first_death_event..],
                [WeaponFxEvent::BodyParticles(index), WeaponFxEvent::ObjectFx(0)]
                    if game.presentation.body_particles[*index].pose.object == object
            ));
            game.process_update_destruction();
            assert!(!game.entities[object].exists());
            assert!(game.hazard_index(serial).is_none());
            assert!(game.hazards.is_empty());
            assert_eq!(game.presentation.object_fx_events.len(), 1);
            game.create_weapon_hazard(source, 0, 0, [450.0, 300.0, 0.0], 1);
            let deleted = game.hazards[0].object;
            game.destroy_maintenance_object(deleted);
            game.process_update_destruction();
            assert!(!game.entities[deleted].exists());
            assert!(game.hazards.is_empty());
            assert_eq!(game.presentation.object_fx_events.len(), 1);
            // Creation/transform/destruction notifications share this ordered queue.
            // Neither deferred deletion may introduce a second native FX callback.
            assert_eq!(
                game.presentation
                    .weapon_fx
                    .iter()
                    .filter(|event| { matches!(event, WeaponFxEvent::ObjectFx(_)) })
                    .count(),
                1
            );
        }
    }

    #[test]
    fn each_source_module_stop_clears_shared_drawable_tint_without_erasing_other_module_timer() {
        let mut game = game();
        game.catalog[5].behavior.poisoned = vec![
            PoisonRules {
                interval: 3,
                duration: 3,
            },
            PoisonRules {
                interval: 5,
                duration: 9,
            },
        ];
        poison_registration(&mut game, 5);
        poison_registration(&mut game, 5);
        let id = game.spawn(5, 1, [300.0, 300.0], true);
        game.poison_damage_received(id, 0, PoisonSource::Invalid, 1.0, DeathType::Poisoned);
        game.poison_damage_received(id, 1, PoisonSource::Invalid, 1.0, DeathType::Poisoned);
        assert!(game.entities[id].poisoned());
        game.tick = 3;
        game.run_object_updates();
        assert!(!game.entities[id].poisoned());
        assert!(game.entities[id].poison.instances[0].is_none());
        assert_eq!(
            game.entities[id].poison.instances[1].as_ref().unwrap().next,
            5
        );
    }

    #[test]
    fn healing_sleeps_all_ordered_poison_instances_and_reexposure_wakes_same_handle() {
        let mut game = game();
        game.catalog[5].behavior.poisoned = vec![
            PoisonRules {
                interval: 3,
                duration: 6,
            },
            PoisonRules {
                interval: 5,
                duration: 9,
            },
        ];
        poison_registration(&mut game, 5);
        poison_registration(&mut game, 5);
        let id = game.spawn(5, 1, [300.0, 300.0], true);
        let source = direct_poison_source(&mut game);
        let handles = game.update_objects[game.entities[id].update_owner]
            .handles
            .clone();
        game.poison_damage_received(
            id,
            0,
            PoisonSource::Object(source),
            2.0,
            DeathType::Poisoned,
        );
        game.poison_damage_received(
            id,
            1,
            PoisonSource::Object(source),
            2.0,
            DeathType::Poisoned,
        );
        game.tick = 3;
        game.run_object_updates();
        assert_eq!(
            game.entities[id].poison.instances[0].as_ref().unwrap().next,
            6
        );
        assert_eq!(
            game.entities[id].poison.instances[1].as_ref().unwrap().next,
            5
        );
        game.clear_poison(id);
        assert!(!game.entities[id].poisoned());
        game.tick = 6;
        game.run_object_updates();
        assert!(!game.entities[id].poisoned());
        game.poison_damage_received(
            id,
            0,
            PoisonSource::Object(source),
            4.0,
            DeathType::Poisoned,
        );
        game.poison_damage_received(
            id,
            1,
            PoisonSource::Object(source),
            4.0,
            DeathType::Poisoned,
        );
        assert_eq!(
            game.update_objects[game.entities[id].update_owner].handles,
            handles
        );
        game.tick = 9;
        game.run_object_updates();
        assert_eq!(
            game.entities[id].poison.instances[0].as_ref().unwrap().next,
            12
        );
        assert_eq!(
            game.entities[id].poison.instances[1].as_ref().unwrap().next,
            11
        );
    }

    #[test]
    fn zero_stop_sentinel_and_dead_final_frame_keep_native_poison_tint() {
        let mut game = game();
        game.catalog[5].behavior.poisoned = vec![PoisonRules {
            interval: 3,
            duration: 0,
        }];
        poison_registration(&mut game, 5);
        let id = game.spawn(5, 1, [300.0, 300.0], true);
        game.poison_damage_received(id, 0, PoisonSource::Invalid, 2.0, DeathType::Poisoned);
        assert_eq!(
            game.update_poison_module(id, 0),
            i32::try_from(crate::skirmish::module_updates::FOREVER).unwrap()
        );
        assert!(game.entities[id].poisoned());
        game.tick = u64::from(u32::MAX - 2);
        game.catalog[5].behavior.poisoned[0].duration = 3;
        game.poison_damage_received(id, 0, PoisonSource::Invalid, 2.0, DeathType::Poisoned);
        assert_eq!(
            game.entities[id].poison.instances[0]
                .as_ref()
                .unwrap()
                .until,
            0
        );
        assert!(game.entities[id].poisoned());
        game.tick = 10;
        game.catalog[5].behavior.poisoned[0].duration = 3;
        game.clear_poison(id);
        game.poison_damage_received(id, 0, PoisonSource::Invalid, 2.0, DeathType::Poisoned);
        game.entities[id].set_effectively_dead(true);
        game.tick = 13;
        assert_eq!(
            game.update_poison_module(id, 0),
            i32::try_from(crate::skirmish::module_updates::FOREVER).unwrap()
        );
        assert!(game.entities[id].poisoned());
    }

    #[test]
    fn hazard_damage_respects_immortal_body_and_only_notifies_actual_health_loss() {
        let mut game = game();
        let unit = 6;
        game.catalog[unit].minimum_health = 1.0;
        game.catalog[unit].maintenance.regeneration = Some(crate::skirmish::Regeneration {
            percent_per_second: 0.1,
            delay: 9,
        });
        crate::skirmish::tests::register_update(&mut game.catalog[unit], "BaseRegenerateUpdate");
        let target = game.spawn(unit, 1, [300.0, 300.0], true);
        game.tick = 12;
        game.apply_hazard_hit(
            target,
            Some(0),
            500.0,
            crate::skirmish::DamageType::Poison,
            DeathType::Poisoned,
        );
        assert_eq!(game.entities[target].health.to_bits(), 1.0_f32.to_bits());
        game.tick = 13;
        game.apply_hazard_hit(
            target,
            Some(0),
            500.0,
            crate::skirmish::DamageType::Poison,
            DeathType::Poisoned,
        );
        assert!(game.entities[target].alive());
        assert_eq!(game.entities[target].health.to_bits(), 1.0_f32.to_bits());
        game.tick = 21;
        game.run_object_updates();
        assert!(game.entities[target].health > 1.0);
    }

    fn field() -> Rules {
        Rules {
            name: "PoisonFieldMedium".into(),
            damage: 2.0,
            radius: 80.0,
            interval: 15,
            lifetime: 900,
            health: 100.0,
            geometry_radius: 40.0,
            airborne_height: 0.64,
            geometry: Geometry {
                shape: Shape::Cylinder,
                major: 40.0,
                minor: 1.0,
                height: 1.0,
                is_small: false,
            },
            core_radius: 10.0,
            core_damage: 200.0,
            death_type: DeathType::Poisoned,
            fire_fx: String::new(),
            death_fx: String::new(),
            ambient: String::new(),
        }
    }

    #[test]
    fn poison_reexposure_preserves_tick_deadline_and_healing_cures_it() {
        let mut game = game();
        game.catalog[5].behavior.poisoned = vec![PoisonRules {
            interval: 6,
            duration: 18,
        }];
        poison_registration(&mut game, 5);
        let field_source = direct_poison_source(&mut game);
        let id = game.spawn(5, 1, [300.0, 300.0], true);
        let original = game.entities[id].health;
        game.tick = 10;
        game.apply_weapon_hit(
            id,
            field_source,
            2.0,
            crate::skirmish::DamageType::Poison,
            DeathType::Poisoned,
        );
        game.tick = 15;
        game.apply_weapon_hit(
            id,
            field_source,
            3.0,
            crate::skirmish::DamageType::Poison,
            DeathType::PoisonedBeta,
        );
        assert_eq!(
            game.entities[id].poison.instances[0].as_ref().unwrap().next,
            16
        );
        game.tick = 16;
        game.update_poison_module(id, 0);
        assert_eq!(
            game.entities[id].health.to_bits(),
            (original - 8.0).to_bits()
        );
        game.clear_poison(id);
        game.tick = 22;
        game.update_poison_module(id, 0);
        assert_eq!(
            game.entities[id].health.to_bits(),
            (original - 8.0).to_bits()
        );
        assert!(!game.entities[id].poisoned());
    }

    #[test]
    fn poison_reexposure_retains_gamma_death_on_the_existing_damage_deadline() {
        let mut game = game();
        game.catalog[5].behavior.poisoned = vec![PoisonRules {
            interval: 6,
            duration: 18,
        }];
        poison_registration(&mut game, 5);
        let field_source = direct_poison_source(&mut game);
        let id = game.spawn(5, 1, [300.0, 300.0], true);
        game.entities[id].health = 8.0;
        game.tick = 10;
        game.apply_weapon_hit(
            id,
            field_source,
            2.0,
            crate::skirmish::DamageType::Poison,
            DeathType::PoisonedBeta,
        );
        game.tick = 15;
        game.apply_weapon_hit(
            id,
            field_source,
            3.0,
            crate::skirmish::DamageType::Poison,
            DeathType::PoisonedGamma,
        );
        assert!(game.entities[id].alive());
        game.tick = 16;
        game.update_poison_module(id, 0);
        assert!(!game.entities[id].alive());
        assert!(matches!(
            game.entities[id].damage_state.death_type,
            DeathType::PoisonedGamma
        ));
    }

    #[test]
    fn poison_ticks_on_final_duration_frame_before_clearing_status() {
        let mut game = game();
        game.catalog[5].behavior.poisoned = vec![PoisonRules {
            interval: 6,
            duration: 12,
        }];
        poison_registration(&mut game, 5);
        let field_source = direct_poison_source(&mut game);
        let id = game.spawn(5, 1, [300.0, 300.0], true);
        let original = game.entities[id].health;
        game.apply_weapon_hit(
            id,
            field_source,
            2.0,
            crate::skirmish::DamageType::Poison,
            DeathType::Poisoned,
        );
        for frame in [6, 12] {
            game.tick = frame;
            game.update_poison_module(id, 0);
        }
        assert_eq!(
            game.entities[id].health.to_bits(),
            (original - 6.0).to_bits()
        );
        assert!(!game.entities[id].poisoned());
    }

    #[test]
    fn original_poison_field_wakes_next_frame_expires_and_new_core_cleans_overlaps() {
        let mut game = game();
        game.catalog[6].weapon.hazard = Some(field());
        crate::skirmish::tests::native_hazard_fixture(&mut game, 6, Origin::Weapon(0));
        game.catalog[6].armor[crate::skirmish::DamageType::Poison as usize] = 0.0;
        game.catalog[5].armor[crate::skirmish::DamageType::Poison as usize] = 0.5;
        poison_registration(&mut game, 5);
        let source = game.spawn(6, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 1, [310.0, 300.0], true);
        let original = game.entities[target].health;
        game.update_ground_contacts();
        game.create_weapon_hazard(source, 0, 0, [300.0, 300.0, 500.0], 1);
        game.update_ground_contacts();
        game.run_object_updates();
        game.process_update_destruction();
        assert_eq!(game.entities[target].health.to_bits(), original.to_bits());
        game.tick = 1;
        game.run_object_updates();
        game.process_update_destruction();
        assert_eq!(
            game.entities[target].health.to_bits(),
            (original - 1.0).to_bits()
        );
        let retired = game.hazards[0].object;
        game.create_weapon_hazard(source, 0, 0, [340.0, 300.0, 0.0], 2);
        game.update_ground_contacts();
        game.tick = 2;
        game.run_object_updates();
        game.process_update_destruction();
        assert_eq!(game.hazards.len(), 1);
        assert_eq!(game.hazards[0].created, 1);
        assert!(!game.entities[retired].exists());
        game.tick = 901;
        game.run_object_updates();
        game.process_update_destruction();
        assert!(game.hazards.is_empty());
    }

    #[test]
    fn field_exposure_uses_researched_armor_while_recurring_poison_keeps_actual_damage() {
        use crate::skirmish::{ArmorUpgrade, DamageType};

        let mut game = game();
        let mut rules = field();
        rules.damage = 8.0;
        game.catalog[6].weapon.hazard = Some(rules);
        crate::skirmish::tests::native_hazard_fixture(&mut game, 6, Origin::Weapon(0));
        game.catalog[6].armor[DamageType::Poison as usize] = 0.0;
        game.catalog[5].armor[DamageType::Poison as usize] = 0.5;
        let mut coefficients = [1.0; crate::skirmish::DAMAGE_TYPE_COUNT];
        coefficients[DamageType::Poison as usize] = 0.125;
        game.catalog[5].maintenance.armor_upgrade = Some(ArmorUpgrade {
            trigger: "Fortified".into(),
            coefficients,
        });
        crate::skirmish::tests::register_behavior(&mut game.catalog[5], "ArmorUpgrade", 0x80);
        game.catalog[5]
            .behavior_modules
            .last_mut()
            .unwrap()
            .upgrade
            .triggers = vec!["Fortified".into()];
        game.catalog[5].behavior.poisoned = vec![PoisonRules {
            interval: 6,
            duration: 18,
        }];
        poison_registration(&mut game, 5);
        let source = game.spawn(6, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 1, [310.0, 300.0], true);
        game.entities[target].battle_bonuses.armor = 0.5;
        let original = game.entities[target].health;
        game.update_ground_contacts();
        game.create_weapon_hazard(source, 0, 0, [300.0, 300.0, 0.0], 1);
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[target].health.to_bits(),
            (original - 2.0).to_bits()
        );
        game.players[1].upgrades.push("Fortified".into());
        game.activate_upgrade_modules(target);
        game.tick = 7;
        game.run_object_updates();
        // PoisonedBehavior's saved actual damage is UNRESISTABLE. Research must
        // change the next field exposure without reducing this old tick again.
        assert_eq!(
            game.entities[target].health.to_bits(),
            (original - 4.0).to_bits()
        );
        game.tick = 13;
        game.run_object_updates();
        assert_eq!(
            game.entities[target].health.to_bits(),
            (original - 6.0).to_bits()
        );
        game.tick = 16;
        game.run_object_updates();
        assert_eq!(
            game.entities[target].health.to_bits(),
            (original - 6.5).to_bits()
        );
        assert_eq!(
            game.entities[target].poison.instances[0]
                .as_ref()
                .unwrap()
                .damage
                .to_bits(),
            0.5_f32.to_bits()
        );
    }

    #[test]
    fn inert_field_kill_credits_player_without_granting_launcher_experience() {
        let mut game = game();
        let source = game.spawn(6, 0, [300.0, 300.0], true);
        let victim = game.spawn(5, 1, [310.0, 300.0], true);
        let before = game.players[0].statistics.units_destroyed;
        game.apply_hazard_hit(
            victim,
            Some(0),
            10000.0,
            super::super::DamageType::Poison,
            DeathType::PoisonedBeta,
        );
        assert!(matches!(
            game.entities[victim].damage_state.death_type,
            DeathType::PoisonedBeta
        ));
        assert_eq!(game.entities[source].experience, 0);
        assert_eq!(game.players[0].statistics.units_destroyed, before + 1);
    }

    #[test]
    fn field_kill_scores_captured_tech_before_its_death_callback_neutralizes_it() {
        use crate::skirmish::{Faction, Kind, MaintenanceRules, tests::fixture_type};

        let mut game = game();
        let mut tech = fixture_type(Faction::Civilian, Kind::TechBuilding);
        tech.maintenance.kind_flags |= MaintenanceRules::TECH_BUILDING;
        crate::skirmish::neutral_tech::tests::bind_tech(&mut tech);
        let type_index = game.catalog.len();
        game.catalog.push(tech);
        let victim = game.spawn(type_index, 1, [310.0, 300.0], true);
        let destroyed = game.players[0].statistics.buildings_destroyed;
        let lost = game.players[1].statistics.buildings_lost;
        let neutral = game.neutral_owner();
        let neutral_lost = game.players[neutral].statistics.buildings_lost;

        game.apply_hazard_hit(
            victim,
            Some(0),
            10000.0,
            super::super::DamageType::Poison,
            DeathType::Poisoned,
        );

        assert_eq!(game.entities[victim].owner, neutral);
        assert_eq!(
            game.players[0].statistics.buildings_destroyed,
            destroyed + 1
        );
        assert_eq!(game.players[1].statistics.buildings_lost, lost + 1);
        assert_eq!(
            game.players[neutral].statistics.buildings_lost,
            neutral_lost
        );
    }
    #[test]
    fn poison_damage_normal_phase_follows_the_same_frames_ai_attack() {
        let mut game = game();
        game.catalog[6].behavior.poisoned = vec![PoisonRules {
            interval: 1,
            duration: 10,
        }];
        game.catalog[6].weapon.range = 300.0;
        game.catalog[6].ai.vision_range = 300.0;
        poison_registration(&mut game, 6);
        let actor = game.spawn(6, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 1, [350.0, 300.0], true);
        game.set_object_orientation(actor, 0.0);
        game.attack(actor, target).unwrap();
        game.step(); // The AIM update enters FIRE before poison's first due frame.
        assert!(game.entities[actor].is_firing_weapon());
        let source = direct_poison_source(&mut game);
        game.apply_weapon_hit(
            actor,
            source,
            60.0,
            crate::skirmish::DamageType::Poison,
            DeathType::Poisoned,
        );
        assert_eq!(game.entities[actor].health.to_bits(), 40.0_f32.to_bits());
        game.step();
        assert!(
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == actor)
        );
        assert!(!game.entities[actor].alive());
        assert!(matches!(
            game.entities[actor].damage_state.death_type,
            DeathType::Poisoned
        ));
    }
    #[test]
    fn normal_phase_field_creation_keeps_current_frame_registration() {
        let mut game = game();
        game.catalog[6].weapon.hazard = Some(field());
        crate::skirmish::tests::native_hazard_fixture(&mut game, 6, Origin::Weapon(0));
        game.catalog[6].armor[crate::skirmish::DamageType::Poison as usize] = 0.0;
        poison_registration(&mut game, 5);
        let source = game.spawn(6, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 1, [310.0, 300.0], true);
        game.tick = 10;
        let original = game.entities[target].health;
        game.update_ground_contacts();
        game.create_weapon_hazard(source, 0, 0, [300.0, 300.0, 0.0], game.tick);
        game.run_object_updates();
        assert_eq!(
            game.entities[target].health.to_bits(),
            (original - 2.0).to_bits()
        );
    }

    #[test]
    fn poison_field_hive_uses_field_origin_and_recipient_armor_then_swallows_when_empty() {
        let (mut game, hive) = crate::skirmish::containment::tests::setup(0.0, 0.0);
        let hive_type = game.entities[hive].type_index;
        let mask = 1 << crate::skirmish::DamageType::Poison as usize;
        game.catalog[hive_type].hive_body = Some(crate::skirmish::hive_body::Rules {
            propagate: mask,
            swallow: mask,
        });
        game.catalog[hive_type].armor[crate::skirmish::DamageType::Poison as usize] = 0.0;
        game.catalog[5].armor[crate::skirmish::DamageType::Poison as usize] = 0.25;
        game.catalog[5].behavior.poisoned = vec![PoisonRules {
            interval: 6,
            duration: 18,
        }];
        game.catalog[6].weapon.hazard = Some(field());
        crate::skirmish::tests::native_hazard_fixture(&mut game, 6, Origin::Weapon(0));
        poison_registration(&mut game, 5);
        let source = game.spawn(6, 1, [450.0, 255.0], true);
        let near_field = game.spawn(5, 0, [340.0, 255.0], true);
        let near_launcher = game.spawn(5, 0, [370.0, 255.0], true);
        game.board(near_field, hive);
        game.board(near_launcher, hive);
        game.entities[near_field].position = [340.0, 255.0];
        game.entities[near_launcher].position = [370.0, 255.0];
        let health = game.entities[near_field].health;
        let hive_health = game.entities[hive].health;
        game.tick = 10;
        game.update_ground_contacts();
        game.create_weapon_hazard(source, 0, 1, [300.0, 255.0, 0.0], game.tick);
        game.run_object_updates();
        assert_eq!(
            game.entities[near_field].health.to_bits(),
            (health - 0.5).to_bits()
        );
        assert_eq!(
            game.entities[near_launcher].health.to_bits(),
            health.to_bits()
        );
        assert_eq!(game.entities[hive].health.to_bits(), hive_health.to_bits());
        assert!(game.entities[near_field].poisoned());
        assert!(!game.entities[near_launcher].poisoned());
        game.destroy_maintenance_object(near_field);
        game.destroy_maintenance_object(near_launcher);
        game.process_update_destruction();
        game.catalog[hive_type].armor[crate::skirmish::DamageType::Poison as usize] = 1.0;
        game.tick += 15;
        game.run_object_updates();
        assert_eq!(game.entities[hive].health.to_bits(), hive_health.to_bits());
    }

    #[test]
    fn initial_poison_exposure_intercepts_bus_but_saved_recurring_damage_is_unresistable() {
        let mut game = game();
        let mut unit = crate::skirmish::tests::fixture_type(
            super::super::Faction::Gla,
            super::super::Kind::Vehicle,
        );
        unit.battle_bus = Some(crate::skirmish::battle_bus::Rules {
            second_health: 650.0,
            first_armor: [[1.0; crate::skirmish::DAMAGE_TYPE_COUNT]; 2],
            second_armor: [[1.0; crate::skirmish::DAMAGE_TYPE_COUNT]; 3],
        });
        crate::skirmish::battle_bus::tests::bind_death(&mut unit);
        unit.behavior.poisoned = vec![PoisonRules {
            interval: 6,
            duration: 18,
        }];
        unit.armor[super::super::DamageType::Poison as usize] = 0.25;
        unit.initial_health = 400.0;
        unit.max_health = 400.0;
        let index = game.catalog.len();
        game.catalog.push(unit);
        poison_registration(&mut game, index);
        let id = game.spawn(index, 1, [300.0, 300.0], true);
        let field_source = direct_poison_source(&mut game);
        game.apply_weapon_hit(
            id,
            field_source,
            1600.0,
            crate::skirmish::DamageType::Poison,
            DeathType::Poisoned,
        );
        assert!(game.entities[id].battle_bus_second_life());
        assert_eq!(game.entities[id].health.to_bits(), 650.0_f32.to_bits());
        assert_eq!(
            game.entities[id].poison.instances[0]
                .as_ref()
                .unwrap()
                .damage
                .to_bits(),
            99.75_f32.to_bits()
        );
        game.tick = 6;
        game.update_poison_module(id, 0);
        assert_eq!(game.entities[id].health.to_bits(), 550.25_f32.to_bits());
        let first_life = game.spawn(index, 1, [320.0, 300.0], true);
        game.entities[first_life].health = 50.0;
        game.poison_damage_received(
            first_life,
            0,
            PoisonSource::Object(field_source),
            60.0,
            DeathType::Poisoned,
        );
        game.tick = 12;
        game.update_poison_module(first_life, 0);
        assert!(!game.entities[first_life].alive());
        assert!(!game.entities[first_life].battle_bus_second_life());
    }
}
