//! `Weapon::privateFireWeapon` and `WeaponTemplate` payload for separately allocated weapons.
use crate::skirmish::{DamageType, Skirmish, WeaponBinding, damage::WeaponTarget};

impl Skirmish {
    /// Assistance can invoke other weapons before this instance enters leech mode.
    pub(in crate::skirmish) fn request_owned_weapon_assistance(
        &mut self,
        source: usize,
        binding: WeaponBinding,
        target: Option<usize>,
    ) {
        if let Some(victim) = target {
            let range = self.catalog[binding.type_index]
                .weapon_template(binding.weapon_index)
                .expect("owned Weapon template")
                .request_assist_range;
            self.request_weapon_assistance_for(source, victim, range);
        }
    }

    /// Native privateFire special damage branches follow assistance and leech activation.
    /// False is its DISARM early return: callers spend ammo without normal shot timing.
    pub(in crate::skirmish) fn fire_owned_weapon_prelude(
        &mut self,
        source: usize,
        binding: WeaponBinding,
        target: Option<usize>,
    ) -> bool {
        let type_index = binding.type_index;
        let index = binding.weapon_index;
        let weapon = self.catalog[type_index]
            .weapon_template(index)
            .expect("owned Weapon template");
        let deploy = matches!(weapon.damage_type, crate::skirmish::DamageType::Deploy);
        let disarm = matches!(weapon.damage_type, DamageType::Disarm);
        if deploy {
            self.begin_assault_target(source, target);
        }
        if disarm {
            if let Some(victim) = target {
                self.disarm_owned_weapon(source, binding, victim);
            }
            return false;
        }
        true
    }

    pub(in crate::skirmish) fn fire_owned_weapon_payload(
        &mut self,
        source: usize,
        binding: WeaponBinding,
        barrel: usize,
        mut target: WeaponTarget,
        ignore_ranges: bool,
    ) {
        let type_index = binding.type_index;
        let index = binding.weapon_index;
        // WeaponTemplate resolves sneaky object offsets only after the instance pattern.
        if let Some(object) = target.object {
            target = self.weapon_target(object, None);
        }
        let actor = &self.entities[source];
        let weapon = self.catalog[type_index]
            .weapon_template(index)
            .expect("owned Weapon template");
        let extra = self.passenger_bonus_flags(source);
        let damage_bonus = actor.weapon_damage_bonus(weapon, extra);
        let range = actor.weapon_range_for(weapon, extra);
        let min_range = weapon.min_range;
        let fire = weapon
            .incendiary
            .as_ref()
            .and_then(|fire| fire.impact.clone());
        let origin = self.object_position(source);
        let owner = actor.owner;
        let facing = actor.facing;
        let velocity = actor.physics_velocity();
        let mut radii = self.object_radius(source);
        if let Some(victim) = target.object {
            if self.catalog[self.entities[victim].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "BRIDGE")
            {
                let points = self
                    .bridge_attack_points(victim)
                    .expect("native bridge target");
                let distance = |point: [f32; 3]| {
                    let delta = [point[0] - origin[0], point[1] - origin[1]];
                    let squared = [delta[0] * delta[0], delta[1] * delta[1]];
                    ((squared[0] + squared[1]).sqrt() - radii).max(0.0)
                };
                target.position = if distance(points[0]) > distance(points[1]) {
                    points[1]
                } else {
                    points[0]
                };
                target.projectile_position = target.position;
            } else {
                radii += self.object_radius(victim);
            }
        }
        let delta = [
            target.position[0] - origin[0],
            target.position[1] - origin[1],
        ];
        let squared = [delta[0] * delta[0], delta[1] * delta[1]];
        let distance = ((squared[0] + squared[1]).sqrt() - radii).max(0.0);
        let distance_squared = distance * distance;
        if !ignore_ranges
            && !weapon.leech_range
            && (distance_squared > range * range || distance_squared < min_range * min_range)
        {
            return;
        }
        let aim = target
            .object
            .map_or(target.position, |victim| self.target_center(victim));
        self.record_weapon_shot_at(source, target.object, aim, barrel, binding);
        // Native FireOCL receives the firing Object, before radius scatter. Its producer is the source pose.
        if let Some(fire) = fire {
            self.create_fire_objects(&fire, owner, origin, facing, velocity);
        }
        self.create_bound_weapon_hazard(source, binding, owner, origin);
        self.scatter_weapon_destination(type_index, index, &mut target);
        if self.fire_laser(source, binding, &target, damage_bonus) {
            return;
        }
        self.create_weapon_projectile(source, &target, binding, damage_bonus, barrel);
    }

    fn disarm_owned_weapon(&mut self, source: usize, binding: WeaponBinding, target: usize) {
        let has_interface = self.has_land_mine_interface(target);
        if has_interface {
            self.disarm_weapon_fx(source, binding, target);
            self.disarm_native_land_mine(target);
        }
        let fallback = (!has_interface && self.attack_kind(target, "MINE"))
            || self.attack_kind(target, "BOOBY_TRAP")
            || self.attack_kind(target, "DEMOTRAP");
        if fallback {
            self.disarm_weapon_fx(source, binding, target);
            self.destroy_maintenance_object(target);
        }
        if has_interface || fallback {
            let cleared = &mut self.players[self.entities[source].owner]
                .academy
                .mines_cleared;
            *cleared = cleared.wrapping_add(1);
        }
    }

    fn disarm_weapon_fx(&mut self, source: usize, binding: WeaponBinding, target: usize) {
        let position = self.object_position(target);
        self.record_weapon_shot_at(source, Some(target), position, 0, binding);
        let shot = self.presentation.shots.last_mut().expect("disarm FireFX");
        shot.source_position = position;
        shot.source_facing = self.entities[target].facing;
        shot.from = self.entities[target].position;
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{
        Faction, Kind, WeaponRules, fire,
        tests::{fixture_type, game},
    };

    #[test]
    fn temporary_object_weapon_keeps_explicit_slot_target_and_source_ocl_pose() {
        let mut game = game();
        let mut field = fixture_type(Faction::China, Kind::Effect);
        field.name = "PayloadFootFire".into();
        field.fire = Some(fire::Rules {
            body: fire::Body::Inactive,
            ambient: String::new(),
            lifetime: [75; 2],
            initial_delay: 0,
            storm: None,
        });
        field.weapon = WeaponRules::unarmed();
        field.ai = crate::skirmish::UnitAi::default();
        field.body_kind = crate::skirmish::BodyKind::Inactive;
        field.initial_health = 0.0;
        field.max_health = 0.0;
        let field_kind = game.catalog.len();
        game.catalog.push(field);
        let weapon = &mut game.catalog[5].weapon;
        weapon.damage = 37.0;
        weapon.range = 300.0;
        weapon.min_range = 0.0;
        weapon.laser = true;
        weapon.incendiary = Some(fire::Weapon {
            impact: Some(fire::Creation {
                names: vec!["PayloadFootFire".into()],
                count: 1,
                disposition: fire::Disposition::GroundAligned,
            }),
            death: None,
            historic: None,
        });
        let source = game.spawn(6, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 1, [360.0, 300.0], true);
        game.entities[source].weapon_set.current = 1;
        let primary_ammo = game.entities[source].weapon_set.weapons[0].clip_remaining;
        let before = game.entities[target].health;
        game.fire_temporary_weapon_target(
            source,
            crate::skirmish::WeaponBinding {
                type_index: 5,
                weapon_index: 0,
                weapon_slot: 2,
            },
            target,
        );
        let shot = game.presentation.shots.last().unwrap();
        assert_eq!(
            (
                shot.weapon_type,
                shot.weapon_index,
                shot.weapon_slot,
                shot.target
            ),
            (5, 0, 2, Some(target))
        );
        assert_eq!(
            game.entities[target].health.to_bits(),
            (before - 37.0).to_bits()
        );
        assert_eq!(
            game.entities[source].weapon_set.weapons[0].clip_remaining,
            primary_ammo
        );
        let created = game
            .entities
            .iter()
            .find(|actor| actor.type_index == field_kind)
            .unwrap();
        assert_eq!(
            created.position.map(f32::to_bits),
            game.entities[source].position.map(f32::to_bits)
        );
    }
}

#[cfg(test)]
mod prelude_tests {
    use crate::skirmish::{AssistedTargeting, WeaponBinding, tests::game};

    #[test]
    fn temporary_pattern_still_assists_original_object_before_conversion_to_point() {
        let mut game = game();
        let unit = &mut game.catalog[5];
        unit.weapon.request_assist_range = 200.0;
        unit.weapon.scatter_targets = vec![[0.25, -0.5]];
        unit.weapon.scatter_target_scalar = 40.0;
        unit.weapon.range = 300.0;
        unit.weapon.speed = 600.0;
        unit.behavior.assisted_targeting = vec![AssistedTargeting {
            clip_size: 1,
            weapon_slot: 0,
            lasers: [String::new(), String::new()],
        }];
        crate::skirmish::tests::register_update(unit, "AssistedTargetingUpdate");
        let source = game.spawn(5, 0, [300.0, 300.0], true);
        let assistant = game.spawn(5, 0, [310.0, 300.0], true);
        let target = game.spawn(5, 1, [360.0, 300.0], true);
        game.fire_temporary_weapon_target(
            source,
            WeaponBinding {
                type_index: 5,
                weapon_index: 0,
                weapon_slot: 0,
            },
            target,
        );
        assert!(
            game.presentation
                .assistance_events
                .iter()
                .any(|event| event.requester == source
                    && event.assistant == assistant
                    && event.target == target)
        );
        let shot = game.presentation.shots.last().unwrap();
        assert_eq!(shot.target, None);
        assert_eq!(
            shot.target_position.map(f32::to_bits),
            [370.0_f32, 280.0, 0.0].map(f32::to_bits)
        );
    }
}
