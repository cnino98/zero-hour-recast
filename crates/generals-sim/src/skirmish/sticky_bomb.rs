//! Native `StickyBombUpdate` and `Object::checkAndDetonateBoobyTrap`, SH 2bfaea60.
use super::{
    DeathType, Entity, Skirmish, WeaponRules, module_updates::FOREVER, object_sounds::Action,
};

pub struct Rules {
    // Source parses this field but never reads it; the bomb follows target XYZ.
    pub attach_bone: String,
    pub offset_z: f32,
    pub geometry_weapon: Option<WeaponRules>,
    pub geometry_fx: Option<String>,
    pub created_sound: String,
    pub ping_sound: String,
}

pub(super) struct State {
    target: Option<usize>,
    die_frame: u32,
    next_ping: u32,
}

impl Entity {
    #[must_use]
    pub const fn booby_trapped(&self) -> bool {
        self.object_status & (1 << 41) != 0
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn transfer_rebuild_bombs(&mut self, previous: usize, next: usize) {
        // GameLogic's object list is newest first; findUpdateModule selects the first instance.
        for id in (0..self.entities.len()).rev() {
            if self.entities[id].exists()
                && self.attack_kind(id, "MINE")
                && let Some(state) = self.entities[id].behaviors.sticky_bombs.first_mut()
                && state.target == Some(previous)
            {
                // StickyBombUpdate::setTargetObject changes only m_targetID.
                state.target = Some(next);
            }
        }
    }

    pub(in crate::skirmish) fn construct_sticky_bomb(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.sticky_bombs.len(), ordinal);
        self.entities[id].behaviors.sticky_bombs.push(State {
            target: None,
            die_frame: 0,
            next_ping: 0,
        });
        self.wake_family_update(id, "StickyBombUpdate", ordinal, FOREVER);
    }

    pub(in crate::skirmish) fn created_sticky_bomb(&mut self, id: usize, ordinal: usize) {
        let Some(shooter) = self.entities[id]
            .producer
            .filter(|&id| self.entities[id].exists())
        else {
            return;
        };
        if self.entities[shooter].ai.registration.is_some()
            && let Some(target) = self.entities[shooter]
                .ai
                .command
                .goal_object
                .filter(|&id| self.entities[id].exists())
        {
            self.init_sticky_bomb(id, ordinal, Some(target), None, None);
        }
    }

    pub(in crate::skirmish) fn init_sticky_bomb(
        &mut self,
        id: usize,
        ordinal: usize,
        target: Option<usize>,
        bomber: Option<usize>,
        specific: Option<[f32; 3]>,
    ) {
        let now = self.update_frame();
        let deadline = self.entities[id]
            .behaviors
            .lifetime_die_frames
            .first()
            .copied();
        let state = &mut self.entities[id].behaviors.sticky_bombs[ordinal];
        state.target = target;
        self.entities[id].producer = target;
        let state = &mut self.entities[id].behaviors.sticky_bombs[ordinal];
        state.die_frame = deadline.unwrap_or(0);
        state.next_ping = if deadline.is_some() {
            let pings = state.die_frame.wrapping_sub(now) / 30;
            state.die_frame.wrapping_sub(pings.wrapping_mul(30))
        } else {
            now.wrapping_add(30)
        };
        self.wake_family_update(id, "StickyBombUpdate", ordinal, 1);
        let Some(target) = target else {
            return;
        };
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .sticky_bombs[ordinal];
        let sound = rules.created_sound.clone();
        let mut position = self.object_position(target);
        if let Some(specific) = specific {
            position = specific;
            position[2] = self.ground_height([position[0], position[1]]);
        } else if self.attack_kind(target, "IMMOBILE")
            && let Some(bomber) = bomber
        {
            position = self.object_position(bomber);
            position[2] = self.ground_height([position[0], position[1]]);
        } else {
            position[2] += rules.offset_z;
        }
        self.set_object_position(id, position);
        if self.attack_kind(id, "BOOBY_TRAP") {
            self.entities[target].object_status |= 1 << 41;
        }
        self.object_sound(id, sound, Action::PlayAtPositionUnowned);
    }

    pub(in crate::skirmish) fn update_sticky_bomb(&mut self, id: usize, ordinal: usize) -> i32 {
        if let Some(target) = self.entities[id].behaviors.sticky_bombs[ordinal]
            .target
            .filter(|&id| self.entities[id].exists())
        {
            if self.entities[target].is_effectively_dead() {
                self.destroy_maintenance_object(id);
                return 1;
            }
            let position = if self.attack_kind(target, "IMMOBILE") {
                let position = self.object_position(id);
                [
                    position[0],
                    position[1],
                    self.ground_height([position[0], position[1]]),
                ]
            } else {
                let mut position = self.object_position(target);
                position[2] += self.catalog[self.entities[id].type_index]
                    .behavior
                    .sticky_bombs[ordinal]
                    .offset_z;
                position
            };
            self.set_object_position(id, position);
        }
        if self.update_frame() >= self.entities[id].behaviors.sticky_bombs[ordinal].next_ping {
            let state = &mut self.entities[id].behaviors.sticky_bombs[ordinal];
            state.next_ping = state.next_ping.wrapping_add(30);
            let sound = self.catalog[self.entities[id].type_index]
                .behavior
                .sticky_bombs[ordinal]
                .ping_sound
                .clone();
            self.object_sound(id, sound, Action::Play);
        }
        1
    }

    pub(in crate::skirmish) fn detonate_sticky_bomb(&mut self, id: usize, ordinal: usize) {
        let target = self.entities[id].behaviors.sticky_bombs[ordinal]
            .target
            .filter(|&id| self.entities[id].exists());
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .sticky_bombs[ordinal];
        if let Some(weapon) = &rules.geometry_weapon
            && let Some(target) = target
        {
            let radius = self.object_radius(target);
            let primary_radius =
                weapon.area.as_ref().map_or(0.0, |area| area.primary_radius) + radius;
            let secondary_radius = weapon
                .area
                .as_ref()
                .map_or(0.0, |area| area.secondary_radius)
                + radius;
            let primary_squared = primary_radius * primary_radius;
            let query_radius = primary_radius.max(secondary_radius);
            let primary_damage = weapon.damage;
            let secondary_damage = weapon
                .area
                .as_ref()
                .map_or(0.0, |area| area.secondary_damage);
            let damage_type = weapon.damage_type;
            let death_type = weapon.death_type;
            let fx = rules.geometry_fx.clone();
            let position = self.object_position(target);
            let victims = self.sticky_bomb_damage_targets(position, query_radius);
            for (victim, squared) in victims {
                let amount = if squared <= primary_squared {
                    primary_damage
                } else {
                    secondary_damage
                };
                self.apply_weapon_hit(victim, id, amount, damage_type, death_type);
            }
            if let Some(fx) = fx {
                self.positional_fx_with_radius(
                    id,
                    fx,
                    self.object_position(target),
                    secondary_radius,
                );
            }
        }
        // CRC0 clears BOOBY_TRAPPED onDelete, including non-detonation deletion.
        self.kill_object(id, DeathType::Normal);
    }

    pub(in crate::skirmish) fn delete_sticky_bomb(&mut self, id: usize, ordinal: usize) {
        if self.attack_kind(id, "BOOBY_TRAP")
            && let Some(target) = self.entities[id].behaviors.sticky_bombs[ordinal]
                .target
                .filter(|&id| self.entities[id].exists())
        {
            self.entities[target].object_status &= !(1 << 41);
        }
    }

    fn sticky_bomb_damage_targets(&self, position: [f32; 3], range: f32) -> Vec<(usize, f32)> {
        let mut victims = Vec::new();
        for target in self
            .partition
            .fastest_range_objects([position[0], position[1]], range)
        {
            // PartitionManager::distCalcProc_BoundaryAndBoundary_3D with a null
            // query Object: only the candidate contributes its sphere radius.
            let center = self.target_center(target);
            let delta = std::array::from_fn::<_, 3, _>(|axis| center[axis] - position[axis]);
            let components = delta.map(|component| component * component);
            let actual_squared = (components[0] + components[1]) + components[2];
            let radius = self.body_geometry_bounds(target).sphere_radius;
            let squared = if radius > 0.0 {
                let boundary = actual_squared.sqrt() - radius;
                if boundary <= 0.0 {
                    0.0
                } else {
                    boundary * boundary
                }
            } else {
                actual_squared
            };
            if squared < range * range {
                victims.push((target, squared));
            }
        }
        victims
    }

    pub(in crate::skirmish) fn check_and_detonate_booby_trap(
        &mut self,
        id: usize,
        victim: Option<usize>,
    ) -> bool {
        if !self.entities[id].booby_trapped() {
            return false;
        }
        let range = 25.0 + self.object_radius(id);
        let center = self.entities[id].position;
        let mut candidates = Vec::new();
        for (_, target) in self.partition.outward_range_rings(center, range) {
            if !self.attack_kind(target, "BOOBY_TRAP")
                || self.entities[target].is_off_map() != self.entities[id].is_off_map()
            {
                continue;
            }
            let distance =
                super::movement::distance_squared(center, self.entities[target].position);
            if distance < range * range {
                candidates.push((target, distance));
            }
        }
        candidates.reverse();
        candidates.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .expect("finite native position")
        });
        let Some((bomb, _)) = candidates
            .into_iter()
            .find(|&(bomb, _)| self.entities[bomb].producer == Some(id))
        else {
            return false;
        };
        if self.catalog[self.entities[bomb].type_index]
            .behavior
            .sticky_bombs
            .is_empty()
        {
            return false;
        }
        if victim.is_some_and(|victim| {
            self.map_side_relationship(self.entities[bomb].owner, self.entities[victim].owner)
                .map_or_else(
                    || self.allied(self.entities[bomb].owner, self.entities[victim].owner),
                    |relation| relation == 2,
                )
        }) {
            return false;
        }
        self.detonate_sticky_bomb(bomb, 0);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Faction, Kind, tests};

    fn rules() -> Rules {
        Rules {
            attach_bone: String::new(),
            offset_z: 10.0,
            geometry_weapon: None,
            geometry_fx: None,
            created_sound: "BombCreated".into(),
            ping_sound: "BombPing".into(),
        }
    }

    fn bomb_template(game: &mut Skirmish, timed: bool, booby: bool) -> usize {
        let mut unit = tests::fixture_type(Faction::Gla, Kind::Infantry);
        unit.name = "NativeStickyBomb".into();
        unit.ai.update = crate::skirmish::UnitAiUpdate::Absent;
        unit.kind_of = if booby {
            vec!["BOOBY_TRAP".into()]
        } else {
            Vec::new()
        };
        unit.weapon = WeaponRules::unarmed();
        unit.update_modules = tests::fixture_update_helpers(false);
        unit.behavior_modules.clear();
        unit.behavior.sticky_bombs.push(rules());
        tests::register_update(&mut unit, "StickyBombUpdate");
        if timed {
            unit.behavior.lifetimes.push([65; 2]);
            tests::register_update(&mut unit, "LifetimeUpdate");
        }
        let index = game.catalog.len();
        game.catalog.push(unit);
        index
    }

    #[test]
    fn dormant_constructor_then_real_init_aligns_lifetime_pings_and_follows_cached_target() {
        let mut game = tests::game();
        game.tick = 10;
        let kind = bomb_template(&mut game, true, false);
        let bomb = game.spawn(kind, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 0, [330.0, 300.0], true);
        assert_eq!(game.entities[bomb].behaviors.sticky_bombs[0].die_frame, 0);
        game.tick = 11;
        game.run_object_updates();
        assert!(
            !game
                .presentation
                .object_sounds
                .iter()
                .any(|sound| sound.name == "BombPing")
        );
        game.init_sticky_bomb(bomb, 0, Some(target), None, None);
        assert_eq!(game.entities[bomb].producer, Some(target));
        assert_eq!(game.entities[bomb].behaviors.sticky_bombs[0].die_frame, 75);
        assert_eq!(game.entities[bomb].behaviors.sticky_bombs[0].next_ping, 15);
        game.set_object_position(target, [360.0, 310.0, 20.0]);
        game.tick = 12;
        game.run_object_updates();
        let target_position = game.object_position(target);
        assert_eq!(
            game.object_position(bomb).map(f32::to_bits),
            [
                target_position[0],
                target_position[1],
                target_position[2] + 10.0
            ]
            .map(f32::to_bits)
        );
        assert!(
            !game
                .presentation
                .object_sounds
                .iter()
                .any(|sound| sound.name == "BombPing")
        );
        game.tick = 15;
        game.run_object_updates();
        assert_eq!(
            game.presentation
                .object_sounds
                .iter()
                .filter(|sound| sound.name == "BombPing")
                .count(),
            1
        );
        assert_eq!(game.entities[bomb].behaviors.sticky_bombs[0].next_ping, 45);
        game.tick = 16;
        game.run_object_updates();
        assert_eq!(
            game.presentation
                .object_sounds
                .iter()
                .filter(|sound| sound.name == "BombPing")
                .count(),
            1
        );
    }

    #[test]
    fn immobile_attachment_stays_at_bomber_ground_and_crc0_disarm_delete_clears_status() {
        let mut game = tests::game();
        let kind = bomb_template(&mut game, false, true);
        let bomb = game.spawn(kind, 0, [300.0, 300.0], true);
        let target = game.spawn(0, 1, [330.0, 300.0], true);
        game.catalog[game.entities[target].type_index]
            .kind_of
            .push("IMMOBILE".into());
        let bomber = game.spawn(5, 0, [340.0, 310.0], true);
        game.init_sticky_bomb(bomb, 0, Some(target), Some(bomber), None);
        assert!(game.entities[target].booby_trapped());
        let position = game.object_position(bomb);
        assert_eq!(
            position[0].to_bits(),
            game.object_position(bomber)[0].to_bits()
        );
        assert_eq!(
            position[1].to_bits(),
            game.object_position(bomber)[1].to_bits()
        );
        assert_eq!(
            position[2].to_bits(),
            game.ground_height([position[0], position[1]]).to_bits()
        );
        game.update_ground_contacts();
        assert!(
            !game.check_and_detonate_booby_trap(target, Some(bomber)),
            "allied bomb owner never detonates for a friend"
        );
        assert!(!game.entities[bomb].is_effectively_dead());
        game.set_object_position(target, [450.0, 400.0, 100.0]);
        game.update_sticky_bomb(bomb, 0);
        assert_eq!(
            game.object_position(bomb).map(f32::to_bits),
            position.map(f32::to_bits)
        );
        game.destroy_maintenance_object(bomb);
        assert!(
            !game.entities[target].booby_trapped(),
            "CRC0 onDelete also clears a disarmed bomb"
        );
        assert!(game.entities[bomb].exists(), "destruction remains deferred");
        game.process_update_destruction();
        assert!(!game.entities[bomb].exists());
    }

    #[test]
    fn geometry_detonation_damages_allies_and_preserves_raw_partition_membership_and_fx_radius() {
        let mut game = tests::game();
        let kind = bomb_template(&mut game, false, false);
        game.catalog[kind].behavior.sticky_bombs[0].geometry_weapon = Some(WeaponRules {
            damage: 7.0,
            area: Some(crate::skirmish::damage::AreaDamage {
                at_source: false,
                primary_radius: 10.0,
                secondary_radius: 20.0,
                secondary_damage: 3.0,
                affects: 0,
            }),
            ..WeaponRules::unarmed()
        });
        game.catalog[kind].behavior.sticky_bombs[0].geometry_fx = Some("GeometryBlast".into());
        let bomb = game.spawn(kind, 0, [300.0, 300.0], true);
        let target = game.spawn(0, 0, [330.0, 300.0], true);
        let ally = game.spawn(5, 0, [335.0, 300.0], true);
        let dead = game.spawn(5, 0, [335.0, 301.0], true);
        game.entities[dead].set_effectively_dead(true);
        game.init_sticky_bomb(bomb, 0, Some(target), None, None);
        game.update_ground_contacts();
        let center = game.object_position(target);
        let secondary_radius = game.object_radius(target) + 20.0;
        assert!(
            game.sticky_bomb_damage_targets(center, secondary_radius)
                .iter()
                .any(|&(id, _)| id == dead)
        );
        let before = game.entities[ally].health;
        game.detonate_sticky_bomb(bomb, 0);
        assert_eq!(
            game.entities[ally].health.to_bits(),
            (before - 7.0).to_bits()
        );
        let fx = game
            .presentation
            .object_fx_events
            .iter()
            .find(|event| event.effect == "GeometryBlast")
            .unwrap();
        assert!(!fx.oriented_object);
        assert_eq!(fx.position.map(f32::to_bits), center.map(f32::to_bits));
        assert_eq!(fx.override_radius.to_bits(), secondary_radius.to_bits());
    }
    #[test]
    fn actual_host_death_detonates_trap_before_its_authored_die_callbacks() {
        let mut game = tests::game();
        let kind = bomb_template(&mut game, false, true);
        game.catalog[kind].behavior.sticky_bombs[0].geometry_weapon = Some(WeaponRules {
            damage: 7.0,
            area: Some(crate::skirmish::damage::AreaDamage {
                at_source: false,
                primary_radius: 10.0,
                secondary_radius: 20.0,
                secondary_damage: 3.0,
                affects: 0,
            }),
            ..WeaponRules::unarmed()
        });
        game.catalog[kind].behavior.sticky_bombs[0].geometry_fx = Some("GeometryBlast".into());
        let host = game.spawn(0, 1, [330.0, 300.0], true);
        let ally = game.spawn(5, 0, [335.0, 300.0], true);
        let bomb = game.spawn(kind, 0, [330.0, 300.0], true);
        game.init_sticky_bomb(bomb, 0, Some(host), None, None);
        game.update_ground_contacts();
        let health = game.entities[ally].health;
        game.kill_object(host, DeathType::Normal);
        assert_eq!(
            game.entities[ally].health.to_bits(),
            (health - 7.0).to_bits()
        );
        assert!(game.entities[bomb].is_effectively_dead());
        assert_eq!(
            game.presentation
                .object_fx_events
                .iter()
                .filter(|event| event.effect == "GeometryBlast")
                .count(),
            1
        );
    }

    #[test]
    fn rebuild_transfer_retargets_only_the_first_mine_instance_without_reinitializing_it() {
        let mut game = tests::game();
        let kind = bomb_template(&mut game, true, false);
        game.catalog[kind].kind_of.push("MINE".into());
        game.catalog[kind].behavior.sticky_bombs.push(rules());
        tests::register_update(&mut game.catalog[kind], "StickyBombUpdate");
        let bomb = game.spawn(kind, 0, [300.0, 300.0], true);
        let old = game.spawn(3, 0, [400.0, 300.0], true);
        let next = game.spawn(3, 0, [500.0, 300.0], false);
        game.init_sticky_bomb(bomb, 0, Some(old), None, None);
        game.init_sticky_bomb(bomb, 1, Some(old), None, None);
        let state = &game.entities[bomb].behaviors.sticky_bombs[0];
        let clocks = (state.die_frame, state.next_ping);
        let position = game.object_position(bomb);
        let sounds = game.presentation.object_sounds.len();
        game.transfer_rebuild_bombs(old, next);
        assert_eq!(
            game.entities[bomb].behaviors.sticky_bombs[0].target,
            Some(next)
        );
        assert_eq!(
            game.entities[bomb].behaviors.sticky_bombs[1].target,
            Some(old)
        );
        let state = &game.entities[bomb].behaviors.sticky_bombs[0];
        assert_eq!((state.die_frame, state.next_ping), clocks);
        assert_eq!(game.entities[bomb].producer, Some(old));
        assert_eq!(
            game.object_position(bomb).map(f32::to_bits),
            position.map(f32::to_bits)
        );
        assert_eq!(game.presentation.object_sounds.len(), sounds);
    }
}
