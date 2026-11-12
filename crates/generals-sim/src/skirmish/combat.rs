#[cfg(test)]
mod map_controls;
#[cfg(test)]
mod tests;

pub(in crate::skirmish) mod attack;
pub(super) mod body_callbacks;
pub mod damage_fx;
mod direct;
mod highlander;
mod last_damage;
mod launch;
pub(super) mod targeting;

use super::movement::distance_squared;
use super::{ConstructionState, Order, Shot, Skirmish};

impl Skirmish {
    pub(super) fn update_attack(&mut self, id: usize, target: usize) -> bool {
        self.update_native_attack(id, attack::Goal::Object(target))
    }

    pub(super) fn within_attack_range(&self, id: usize, target: usize) -> bool {
        self.attack_range_from(id, target, self.entities[id].position)
    }

    pub(super) fn attack_range_from(&self, id: usize, target: usize, position: [f32; 2]) -> bool {
        let unit = &self.catalog[self.entities[id].type_index];
        let weapon = self.entities[id].active_weapon(unit);
        self.attack_range_for(id, target, position, weapon)
    }

    pub(super) fn any_weapon_in_range(&self, id: usize, target: usize) -> bool {
        // AI.cpp::PartitionFilterWithinAttackRange checks every allocated slot,
        // independently of auto-selection masks and primary damage estimates.
        (0..3).any(|slot| {
            self.allocated_weapon(id, slot).is_some_and(|weapon| {
                self.attack_range_for(id, target, self.entities[id].position, weapon)
            })
        })
    }

    pub(super) fn attack_range_for(
        &self,
        id: usize,
        target: usize,
        position: [f32; 2],
        weapon: &super::WeaponRules,
    ) -> bool {
        let extra = self.passenger_bonus_flags(id);
        let range = self.entities[id].weapon_range_for(weapon, extra);
        let victim = &self.catalog[self.entities[target].type_index];
        let squared = if victim.kind_of.iter().any(|kind| kind == "BRIDGE") {
            let points = self
                .bridge_attack_points(target)
                .expect("existing weapon target");
            let first = circle_distance_squared(
                position,
                [points[0][0], points[0][1]],
                self.object_radius(id),
            );
            // Weapon::isWithinAttackRange only tries endpoint two when the
            // first exceeds maximum range. A too-close first point still fails.
            if first > range * range {
                circle_distance_squared(
                    position,
                    [points[1][0], points[1][1]],
                    self.object_radius(id),
                )
            } else {
                first
            }
        } else {
            circle_distance_squared(
                position,
                self.entities[target].position,
                self.object_radius(id) + self.object_radius(target),
            )
        };
        squared >= weapon.min_range * weapon.min_range
            && squared <= range * range
            && (!weapon.contact() || self.contact_attack_intersects(id, target, position))
    }

    fn contact_attack_intersects(&self, id: usize, target: usize, position: [f32; 2]) -> bool {
        let victim = &self.entities[target];
        let other = &self.catalog[victim.type_index];
        if !other.kind_of.iter().any(|kind| kind == "STRUCTURE") {
            return true;
        }
        let source = &self.catalog[self.entities[id].type_index];
        // Weapon::isWithinAttackRange uses iteratePotentialCollisions at angle zero.
        crate::collision_geometry::intersects(
            &crate::collision_geometry::Body {
                geometry: &source.geometry,
                position: [position[0], position[1], self.object_position(id)[2]],
                angle: 0.0,
            },
            &crate::collision_geometry::Body {
                geometry: &other.geometry,
                position: self.object_position(target),
                angle: victim.facing,
            },
        )
    }

    pub(super) fn fire_weapon(&mut self, id: usize, target: usize) {
        let index = self.entities[id].weapon_index(self.entities[id].weapon_slot());
        if let Some(reloaded) = self.fire_weapon_inner(id, target) {
            self.notify_firing_tracker_shot(id, index, Some(target));
            if reloaded
                && matches!(
                    self.entities[id].weapon_set.lock,
                    super::weapons::Lock::Temporary
                )
            {
                self.entities[id].weapon_set.lock = super::weapons::Lock::None;
            }
            self.entities[id].helpers.defection.undetected = false;
        }
    }

    fn fire_weapon_inner(&mut self, id: usize, target: usize) -> Option<bool> {
        let position = self.object_position(target);
        self.fire_current_weapon_payload(
            id,
            super::damage::WeaponTarget {
                object: Some(target),
                position,
                projectile_position: position,
                scatter_radius: 0.0,
            },
        )
    }

    /// None means `Object::fireCurrentWeapon` rejected a non-ready allocated Weapon.
    /// Some carries privateFireWeapon's actual auto-reload return, including same-frame reloads.
    pub(in crate::skirmish) fn fire_current_weapon_payload(
        &mut self,
        id: usize,
        target: super::damage::WeaponTarget,
    ) -> Option<bool> {
        let actor = &self.entities[id];
        let slot = actor.weapon_slot();
        let binding = super::WeaponBinding {
            type_index: actor.type_index,
            weapon_index: actor.weapon_index(slot),
            weapon_slot: slot,
        };
        let outer_now = self.update_frame();
        let unit = &self.catalog[binding.type_index];
        if !matches!(
            self.entities[id].weapon_status(slot, unit, outer_now),
            Some(super::ai_dispatch::WeaponStatus::Ready)
        ) {
            return None;
        }
        Some(self.fire_allocated_weapon_payload(id, binding, target))
    }

    pub(in crate::skirmish) fn fire_allocated_weapon_payload(
        &mut self,
        id: usize,
        binding: super::WeaponBinding,
        mut target: super::damage::WeaponTarget,
    ) -> bool {
        let slot = binding.weapon_slot;
        self.request_owned_weapon_assistance(id, binding, target.object);
        if self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("allocated current Weapon")
            .leech_range
        {
            self.entities[id].weapon_set.weapons[slot].leech_range_active = true;
        }
        let template_branch = self.fire_owned_weapon_prelude(id, binding, target.object);
        let now = self.update_frame();
        let extra = self.passenger_bonus_flags(id);
        let unit = &self.catalog[binding.type_index];
        let weapon = unit
            .weapon_template(binding.weapon_index)
            .expect("allocated current Weapon");
        let actor = &mut self.entities[id];
        let shot_rate = actor.weapon_rate_for(weapon, extra);
        if template_branch {
            if !matches!(
                actor.weapon_status(slot, unit, now),
                Some(super::ai_dispatch::WeaponStatus::Ready)
            ) || actor.weapon_set.weapons[slot].clip_remaining == 0
            {
                return false;
            }
            let barrel =
                actor.weapon_set.weapons[slot].owned_barrel(weapon, actor.health, actor.max_health);
            if let Some(position) = self.scatter_weapon_target(id, slot, target.position) {
                target.object = None;
                target.position = position;
                target.projectile_position = position;
            }
            self.fire_owned_weapon_payload(id, binding, barrel, target, false);
        }
        let extra = self.passenger_bonus_flags(id);
        let unit = &self.catalog[binding.type_index];
        let weapon = unit
            .weapon_template(binding.weapon_index)
            .expect("retained fired Weapon");
        let actor = &mut self.entities[id];
        let reload_rate = actor.weapon_rate_for(weapon, extra);
        let state = &mut actor.weapon_set.weapons[slot];
        let completion = state.finish_owned_shot(
            weapon,
            now,
            shot_rate,
            reload_rate,
            unit.weapon_set_rules.share_reload,
        );
        // The remaining order adapters read a countdown; the native deadline stays authoritative.
        let frames = super::ai_dispatch::native_frame(state.possible_next_shot).saturating_sub(now);
        state.cooldown = super::progression::integer_real(frames);
        if let Some((status, deadline)) = completion.shared {
            self.share_owned_weapon_deadline(id, status, deadline);
        }
        completion.reloaded
    }

    pub(super) fn record_weapon_shot_at(
        &mut self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
        barrel: usize,
        weapon: super::WeaponBinding,
    ) {
        // Presentation needs the fire-time pose and rank, before direct damage
        // can kill the victim and award experience to the attacker.
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::Shot(self.presentation.shots.len()));
        let actor = &mut self.entities[id];
        if self.catalog[actor.type_index].turret.is_some() && actor.ai.turret.enabled {
            actor.ai.turret.fired(self.tick);
        }
        let source_position = self.object_position(id);
        let actor = &self.entities[id];
        self.presentation.shots.push(Shot {
            weapon_type: weapon.type_index,
            weapon_index: weapon.weapon_index,
            weapon_slot: weapon.weapon_slot,
            barrel,
            source: id,
            target,
            source_position,
            source_facing: actor.facing,
            target_position: position,
            veterancy: actor.veterancy,
            from: actor.position,
            to: [position[0], position[1]],
            owner: actor.owner,
        });
    }

    pub(super) fn apply_hit(&mut self, target: usize, source: usize, damage: f32) {
        self.apply_typed_hit(target, source, damage, super::DeathType::Normal);
    }

    pub(super) fn apply_typed_hit(
        &mut self,
        target: usize,
        source: usize,
        damage: f32,
        death_type: super::DeathType,
    ) {
        self.apply_native_damage(target, Some(source), damage, death_type);
    }

    pub(super) fn apply_native_damage(
        &mut self,
        target: usize,
        source: Option<usize>,
        damage: f32,
        death_type: super::DeathType,
    ) {
        self.apply_damage(
            target,
            source,
            damage,
            super::DamageType::Unresistable,
            death_type,
        );
    }

    pub(super) fn apply_damage(
        &mut self,
        target: usize,
        source: Option<usize>,
        damage: f32,
        damage_type: super::DamageType,
        death_type: super::DeathType,
    ) {
        self.apply_body_damage(target, source, damage, damage_type, death_type, false);
    }

    // Object::kill uses m_kill, not a large fabricated health value. Immortal
    // and indestructible bodies retain their own attemptDamage behavior.
    pub(in crate::skirmish) fn kill_object(&mut self, target: usize, death_type: super::DeathType) {
        self.kill_object_with_type(target, super::DamageType::Unresistable, death_type);
    }

    pub(in crate::skirmish) fn kill_object_with_type(
        &mut self,
        target: usize,
        damage_type: super::DamageType,
        death_type: super::DeathType,
    ) {
        let raw = self.body_damage_input(target, damage_type, self.entities[target].max_health);
        let second_life = self.battle_bus_damage_raw(target, damage_type, raw);
        self.record_slow_death_damage(target, 0.0, 0.0);
        self.apply_body_damage(
            target,
            None,
            second_life.unwrap_or(raw),
            damage_type,
            death_type,
            true,
        );
        if second_life.is_some() {
            self.start_battle_bus_second_life(target, death_type);
        }
    }

    pub(in crate::skirmish) fn apply_body_damage(
        &mut self,
        target: usize,
        source: Option<usize>,
        damage: f32,
        damage_type: super::DamageType,
        death_type: super::DeathType,
        kill: bool,
    ) {
        if matches!(
            self.catalog[self.entities[target].type_index].body_kind,
            crate::skirmish::BodyKind::Inactive
        ) {
            // InactiveBody.cpp: no damage modules/FX or health mutation. The
            // once-only latch is assigned after Object::onDie returns.
            if matches!(damage_type, super::DamageType::Unresistable)
                && !self.entities[target].damage_state.inactive_die_called
            {
                self.entities[target].damage_state.death_type = death_type;
                self.entities[target].damage_state.current_source = source;
                self.object_on_die(target, source);
                self.entities[target].damage_state.inactive_die_called = true;
            }
            return;
        }
        if self.entities[target].indestructible() || self.entities[target].is_effectively_dead() {
            return;
        }
        if matches!(damage_type, super::DamageType::Healing) {
            if !kill {
                self.apply_healing(target, source, damage);
            }
            return;
        }
        let fx_set = self.body_damage_fx_set(target);
        if !kill && damage.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            self.do_body_damage_fx(target, fx_set, 0.0, damage_type, source);
            self.repulse_after_damage(target);
            self.retaliate_after_damage(target, source);
            return;
        }
        let amount = if kill {
            self.entities[target].health
        } else {
            damage
        };
        self.damage_body(target, source, amount, damage_type, death_type);
        if self.entities[target].is_effectively_dead() {
            self.entities[target].damage_state.death_type = death_type;
            self.finish_death(target, source);
            if source.is_none() {
                self.record_player_kill(self.entities[target].owner, target);
            }
        }
        self.do_body_damage_fx(target, fx_set, amount, damage_type, source);
        self.repulse_after_damage(target);
        self.retaliate_after_damage(target, source);
    }

    pub(super) fn damage_body(
        &mut self,
        target: usize,
        source: Option<usize>,
        damage: f32,
        damage_type: super::DamageType,
        death_type: super::DeathType,
    ) {
        if self.entities[target].indestructible() {
            return;
        }
        let old_state = self.body_damage_state(target);
        let before = self.entities[target].health;
        self.entities[target].damage_state.previous_health = before;
        self.entities[target].health =
            if self.catalog[self.entities[target].type_index].minimum_health > 0.0 {
                // ImmortalBody::internalChangeHealth clamps the delta, retaining damage events.
                let delta = (-damage).max(-before + 1.0);
                before + delta
            } else {
                (before - damage).max(0.0)
            };
        self.update_body_rubble(target);
        if self.catalog[self.entities[target].type_index]
            .body_kind
            .is_active()
        {
            self.entities[target].refresh_body_health();
        }
        self.body_particle_state_changed(target, old_state);
        self.record_body_damage(target, source, damage_type);
        self.body_health_loss_callbacks(target, source, damage, damage_type, death_type);
        self.notify_containment_body_state(target, old_state);
        self.structure_damage_transition(target, before);
        self.body_damage_feedback(target, old_state);
    }

    pub(in crate::skirmish) fn body_health_loss_callbacks(
        &mut self,
        target: usize,
        source: Option<usize>,
        damage: f32,
        damage_type: super::DamageType,
        death_type: super::DeathType,
    ) {
        let before = self.entities[target].damage_state.previous_health;
        if self.entities[target].health < before {
            self.record_slow_death_damage(target, damage, before - self.entities[target].health);
            self.notify_body_damage(target, source, damage, damage_type, death_type);
        }
    }

    // ActiveBody.cpp:637–666. Fear consumes logic RNG even without an authored voice.
    pub(in crate::skirmish) fn body_damage_feedback(&mut self, target: usize, old_state: usize) {
        let state = self.body_damage_state(target);
        let unit = &self.catalog[self.entities[target].type_index];
        if state != old_state {
            let sound = match state {
                1 => unit.damaged_sound.clone(),
                2 => unit.really_damaged_sound.clone(),
                _ => String::new(),
            };
            self.object_sound(target, sound, super::object_sounds::Action::Play);
        }
        let actor = &self.entities[target];
        if actor.damage_state.previous_health / actor.max_health > 0.25
            && actor.health / actor.max_health < 0.25
            && actor.health > 0.0
            && super::startup::random_between(&mut self.random, 99) < 25
        {
            let voice = self.catalog[actor.type_index].fear_sound.clone();
            self.position_sound(target, voice, self.object_position(target));
        }
    }

    pub(super) fn finish_death(&mut self, target: usize, source: Option<usize>) {
        self.update_body_rubble(target);
        // ActiveBody credits the incoming damager before Object::onDie.
        if let Some(source) = source.filter(|&id| self.entities[id].exists()) {
            self.score_kill(source, target);
        }
        self.object_on_die(target, source);
    }

    fn object_on_die(&mut self, target: usize, source: Option<usize>) {
        self.check_and_detonate_booby_trap(target, None);
        let type_index = self.entities[target].type_index;
        for index in 0..self.catalog[type_index].behavior_modules.len() {
            let modules = &self.catalog[type_index].behavior_modules;
            let module = &modules[index];
            let ordinal = modules[..index]
                .iter()
                .filter(|prior| prior.family == module.family)
                .count();
            match module.family.as_str() {
                "InstantDeathBehavior" => self.instant_die(target, ordinal),
                "BunkerBusterBehavior" => self.bunker_buster_on_die(target, ordinal),
                "BridgeBehavior" => self.bridge_behavior_on_die(target, ordinal),
                "NeutronBlastBehavior" => self.neutron_blast_on_die(target, ordinal),
                "EjectPilotDie" => self.eject_pilot_on_die(target, ordinal, source),
                "CreateObjectDie" => self.create_object_die_on_die(target, ordinal, source),
                "FireWeaponWhenDeadBehavior" => self.death_weapon_on_die(target, ordinal),
                "FXListDie" => self.death_fx_on_die(target, ordinal, source),
                "CrushDie" => self.crush_on_die(target, ordinal, source),
                "DestroyDie" => self.destroy_on_die(target, ordinal),
                "UpgradeDie" => self.upgrade_on_die(target, ordinal),
                "StructureCollapseUpdate" | "StructureToppleUpdate" => {
                    self.begin_structure_death(target);
                }
                "SlowDeathBehavior" => self.slow_death_on_die(target, ordinal),
                "JetSlowDeathBehavior" => self.jet_slow_death_on_die(target, ordinal),
                "HelicopterSlowDeathBehavior" => self.helicopter_slow_death_on_die(target, ordinal),
                "BattleBusSlowDeathBehavior" => self.battle_bus_slow_death_on_die(target, ordinal),
                "SpawnBehavior" => self.spawn_on_die(target, ordinal),
                "ProductionUpdate" => self.refund_production_instance(target, ordinal),
                "TunnelContain" => self.tunnel_on_die(target),
                "CaveContain" => self.cave_on_die(target),
                "ParachuteContain" => self.parachute_die(target, source),
                "ParkingPlaceBehavior" => self.parking_death(target),
                "TechBuildingBehavior" => self.tech_building_on_die(target),
                "RebuildHoleBehavior" => self.rebuild_hole_on_die(target, ordinal),
                "GenerateMinefieldBehavior" => self.mine_generator_on_die(target, ordinal),
                "MinefieldBehavior" => self.destroy_maintenance_object(target),
                "RebuildHoleExposeDie" => self.expose_rebuild_hole(target, ordinal),
                _ => {}
            }
        }
        self.begin_physical_death(target);
        self.begin_slow_death(target);
        self.notify_spawn_owner_death(target, source);
        self.delete_battle_plan(target);
        self.create_salvage(target, source);
        self.attachment_death(target);
        if self.entities[target].parachute.is_none() {
            self.containment_death(target);
        }
        self.clear_temporary(target);
        let entity = &mut self.entities[target];
        let navigation_changed = self.catalog[entity.type_index].kind.is_building();
        let constructed = if let Order::Build(building) = entity.ai.order {
            Some(building)
        } else {
            None
        };
        entity.drawable.model_conditions.constructing = false;
        entity.cargo = 0;
        if let Some(building) = constructed
            && matches!(
                self.entities[building]
                    .drawable
                    .model_conditions
                    .construction,
                ConstructionState::Active
            )
        {
            self.entities[building]
                .drawable
                .model_conditions
                .construction = ConstructionState::Interrupted;
        }
        if navigation_changed {
            self.rebuild_navigation();
        }
        self.observe_ground_partition(target);
        self.reconstruction_on_die(target);
    }
}

// PartitionManager::getRelativeAngle2D: normalized dot, acosf and cross sign.
pub(super) fn relative_angle(position: [f32; 2], facing: f32, target: [f32; 2]) -> f32 {
    let mut delta = [target[0] - position[0], target[1] - position[1]];
    let x_squared = delta[0] * delta[0];
    let y_squared = delta[1] * delta[1];
    let distance = (x_squared + y_squared).sqrt();
    if distance == 0.0 {
        return 0.0;
    }
    let inverse = 1.0 / distance;
    delta[0] *= inverse;
    delta[1] *= inverse;
    let direction = [facing.cos(), facing.sin()];
    let x = direction[0] * delta[0];
    let y = direction[1] * delta[1];
    let cosine = (x + y).clamp(-1.0, 1.0);
    let angle = cosine.acos();
    let x = direction[0] * delta[1];
    let y = direction[1] * delta[0];
    if x - y < 0.0 { -angle } else { angle }
}

pub(in crate::skirmish) fn circle_distance_squared(
    from: [f32; 2],
    to: [f32; 2],
    radii: f32,
) -> f32 {
    let center = distance_squared(from, to);
    if radii > 0.0 {
        let separation = (center.sqrt() - radii).max(0.0);
        separation * separation
    } else {
        center
    }
}

#[cfg(test)]
mod bridge_tests;

// ActiveBody retains damage and death types independently of the clearable attacker.
pub(super) struct DamageState {
    inactive_die_called: bool,
    pub damage_fx: damage_fx::State,
    pub previous_health: f32,
    pub initial_health: f32,
    pub front_crushed: bool,
    pub back_crushed: bool,
    thresholds: [f32; 2],
    body_state: usize,
    pub last_attacker: Option<usize>,
    pub current_source: Option<usize>,
    pub retained_source: Option<usize>,
    pub retained_frame: u32,
    pub damage_type: super::DamageType,
    pub death_type: super::DeathType,
}

impl DamageState {
    pub(super) fn for_unit(unit: &super::UnitType, complete: bool) -> Self {
        let health = if complete { unit.initial_health } else { 1.0 };
        let mut state = Self::new(unit.body_damage_thresholds, health, unit.max_health);
        state.initial_health = unit.initial_health;
        state
    }

    pub(super) fn new(thresholds: [f32; 2], health: f32, max_health: f32) -> Self {
        Self {
            thresholds,
            previous_health: health,
            initial_health: health,
            damage_fx: damage_fx::State::new(),
            inactive_die_called: false,
            front_crushed: false,
            back_crushed: false,
            body_state: body_damage_state(health, max_health, thresholds),
            last_attacker: None,
            current_source: None,
            retained_source: None,
            retained_frame: u32::MAX,
            damage_type: super::DamageType::Unresistable,
            death_type: super::DeathType::Normal,
        }
    }
}

impl super::Entity {
    pub(in crate::skirmish) const fn cached_body_damage_state(
        &self,
        unit: &super::UnitType,
    ) -> usize {
        if matches!(unit.body_kind, crate::skirmish::BodyKind::Inactive) {
            0
        } else {
            self.damage_state.body_state
        }
    }

    pub(in crate::skirmish) fn refresh_body_health(&mut self) {
        self.damage_state.body_state =
            body_damage_state(self.health, self.max_health, self.damage_state.thresholds);
        self.set_effectively_dead(self.health <= 0.0);
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn body_damage_state(&self, id: usize) -> usize {
        self.entities[id].cached_body_damage_state(&self.catalog[self.entities[id].type_index])
    }

    pub(in crate::skirmish) fn change_body_health(&mut self, id: usize, delta: f32) {
        let previous = self.body_damage_state(id);
        let kind = self.entities[id].type_index;
        let actor = &mut self.entities[id];
        let delta = if self.catalog[kind].minimum_health > 0.0 {
            delta.max(-actor.health + 1.0)
        } else {
            delta
        };
        actor.damage_state.previous_health = actor.health;
        actor.health += delta;
        if actor.health > actor.max_health {
            actor.health = actor.max_health;
        }
        if actor.health < 0.0 {
            actor.health = 0.0;
        }
        actor.refresh_body_health();
        self.update_body_rubble(id);
        self.body_particle_state_changed(id, previous);
    }

    // ActiveBody::setDamageState calls internalChangeHealth, without attempting
    // damage/healing or firing either callback/FX/death pipeline.
    pub(in crate::skirmish) fn set_body_damage_state(&mut self, id: usize, state: usize) {
        let previous = self.body_damage_state(id);
        let unit = &self.catalog[self.entities[id].type_index];
        if matches!(unit.body_kind, super::BodyKind::Inactive) {
            return;
        }
        let ratio = match state {
            0 => 1.0,
            1 => unit.body_damage_thresholds[0],
            2 => unit.body_damage_thresholds[1],
            3 => 0.0,
            _ => unreachable!("native BodyDamageType"),
        };
        let scaled = self.entities[id].max_health * ratio;
        let desired = scaled - 1.0;
        let desired = if desired > 0.0 { desired } else { 0.0 };
        let before = self.entities[id].health;
        let delta = desired - before;
        let delta = if unit.minimum_health > 0.0 {
            if delta > -before + 1.0 {
                delta
            } else {
                -before + 1.0
            }
        } else {
            delta
        };
        let current = before + delta;
        let maximum = self.entities[id].max_health;
        let capped = if current > maximum { maximum } else { current };
        self.entities[id].damage_state.previous_health = before;
        self.entities[id].health = if capped < 0.0 { 0.0 } else { capped };
        self.entities[id].refresh_body_health();
        self.update_body_rubble(id);
        self.body_particle_state_changed(id, previous);
    }
}

// ActiveBody.cpp::calcDamageState uses strict comparisons, including its
// zero-health and zero-maximum-health cases.
fn body_damage_state(health: f32, max_health: f32, thresholds: [f32; 2]) -> usize {
    let ratio = health / max_health;
    if ratio > thresholds[0] {
        0
    } else if ratio > thresholds[1] {
        1
    } else if ratio > 0.0 {
        2
    } else {
        3
    }
}

#[cfg(test)]
mod current_weapon_regressions {
    use crate::skirmish::{ClipReload, ai_dispatch::WeaponStatus, tests::game, weapons::Lock};

    #[test]
    fn same_frame_auto_reload_releases_temporary_lock_without_timestamp_change() {
        let mut game = game();
        game.catalog[5].weapon.clip_size = 1;
        game.catalog[5].weapon.reload = ClipReload::Automatic;
        game.catalog[5].weapon.reload_seconds = 0.0;
        let source = game.spawn(5, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 1, [310.0, 300.0], true);
        game.tick = 10;
        let weapon = &game.catalog[5].weapon;
        game.entities[source].weapon_set.weapons[0].reload_owned(weapon, 10, 1.0, true, false);
        for _ in 0..2 {
            game.entities[source].weapon_set.lock = Lock::Temporary;
            game.entities[source].helpers.defection.undetected = true;
            game.fire_weapon(source, target);
            assert!(matches!(game.entities[source].weapon_set.lock, Lock::None));
            assert!(!game.entities[source].helpers.defection.undetected);
            assert_eq!(
                game.entities[source].weapon_set.weapons[0].last_reload_started,
                10
            );
            assert_eq!(
                game.entities[source].weapon_set.weapons[0].clip_remaining,
                1
            );
        }
        assert_eq!(game.presentation.shots.len(), 2);
    }

    #[test]
    fn unready_object_fire_skips_assistance_tracker_and_defector_revelation() {
        let mut game = game();
        game.catalog[5].weapon.request_assist_range = 200.0;
        game.catalog[5].behavior.assisted_targeting = vec![crate::skirmish::AssistedTargeting {
            clip_size: 1,
            weapon_slot: 0,
            lasers: [String::new(), String::new()],
        }];
        crate::skirmish::tests::register_update(&mut game.catalog[5], "AssistedTargetingUpdate");
        let source = game.spawn(5, 0, [300.0, 300.0], true);
        let _assistant = game.spawn(5, 0, [305.0, 300.0], true);
        let target = game.spawn(5, 1, [310.0, 300.0], true);
        let actor = &mut game.entities[source];
        actor.weapon_set.weapons[0].status = WeaponStatus::BetweenShots;
        actor.weapon_set.weapons[0].possible_next_shot = 30;
        actor.weapon_set.lock = Lock::Temporary;
        actor.helpers.defection.undetected = true;
        let event_count = game.presentation.weapon_fx.len();
        game.fire_weapon(source, target);
        assert!(game.presentation.shots.is_empty());
        assert!(game.presentation.assistance_events.is_empty());
        assert_eq!(game.presentation.weapon_fx.len(), event_count);
        assert_eq!(game.entities[source].firing.shots, 0);
        assert!(game.entities[source].helpers.defection.undetected);
        assert!(matches!(
            game.entities[source].weapon_set.lock,
            Lock::Temporary
        ));
    }
}
