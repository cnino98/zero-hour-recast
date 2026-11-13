//! `RiderChangeContain` and `AnimationSteeringUpdate`, SH 2bfaea60.
use super::{DamageType, DeathType, Entity, Skirmish, locomotor::LocomotorVariant, maintenance};

pub const RIDER_MODELS: [&str; 8] = [
    "RIDER1", "RIDER2", "RIDER3", "RIDER4", "RIDER5", "RIDER6", "RIDER7", "RIDER8",
];

#[must_use]
pub fn rider_weapon_flag(name: &str) -> Option<u16> {
    let model = name.strip_prefix("WEAPON_")?;
    RIDER_MODELS
        .iter()
        .position(|flag| *flag == model)
        .map(|index| 1 << (6 + index))
}

#[must_use]
pub fn rider_status_bit(name: &str) -> Option<u64> {
    let model = name.strip_prefix("STATUS_")?;
    RIDER_MODELS
        .iter()
        .position(|flag| *flag == model)
        .map(|index| 1 << (30 + index))
}

pub struct Rider {
    pub template: String,
    pub model: &'static str,
    pub weapon: u16,
    pub status: u64,
    pub command_set: String,
    pub locomotor: u8,
}

pub struct Rules {
    pub riders: Vec<Rider>,
    pub sluggish: Vec<LocomotorVariant>,
    pub scuttle_frames: u32,
    pub steering_frames: u32,
}

pub struct Selection {
    pub from: usize,
    pub to: usize,
    pub dismount: bool,
}

pub(super) struct State {
    pub(super) rider_model: Option<&'static str>,
    pub(super) rider_weapon: u16,
    pub(super) rider_trainable: Option<bool>,
    pub(super) steering: Option<&'static str>,
    replacing: bool,
    scuttle_at: Option<u32>,
    next_steering: u32,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            rider_model: None,
            rider_weapon: 0,
            rider_trainable: None,
            steering: None,
            replacing: false,
            scuttle_at: None,
            next_steering: 0,
        }
    }
}

/// Read-only inputs from `PhysicsBehavior` and the current native locomotor.
pub struct DrawMotion {
    pub velocity: [f32; 3],
    pub acceleration: [f32; 2],
    pub normal: [f32; 3],
    pub altitude: f32,
    pub airborne: bool,
    pub motive: bool,
    pub turning: i8,
    pub backwards: bool,
    pub locomotor_set: u8,
    pub surface: usize,
}

impl Entity {
    fn bike_is_moving(&self) -> bool {
        !self.ai_idle() && (self.ai.motion.has_goal() || self.ai.motion.is_moving())
    }
}

impl Skirmish {
    /// Tank dust separates planar movement from the full physics velocity.
    #[must_use]
    pub fn tread_draw_velocity(&self, id: usize) -> Option<[f32; 3]> {
        let actor = self.entities.get(id)?;
        actor
            .active_locomotor(&self.catalog[actor.type_index])
            .ground
            .as_ref()?;
        Some(actor.physics_velocity())
    }

    /// Physics velocity and locomotor maximum use source units per logic frame.
    #[must_use]
    pub fn tread_draw_motion(&self, id: usize) -> Option<(f32, f32, bool, i8)> {
        let actor = self.entities.get(id)?;
        let unit = &self.catalog[actor.type_index];
        actor.active_locomotor(unit).ground.as_ref()?;
        let velocity = actor.physics_velocity();
        let squared = velocity.map(|component| component * component);
        let magnitude = (squared[0] + squared[1] + squared[2]).sqrt();
        Some((
            magnitude,
            super::locomotor::speed(actor, unit) / 30.0,
            actor.ai.motion.motive_until > self.tick,
            actor.physics_control.wheel_turning,
        ))
    }

    #[must_use]
    pub fn wheel_draw_motion(&self, id: usize) -> Option<DrawMotion> {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if unit.combat_bike.is_none() && unit.name != "GLAVehicleBombTruck" {
            return None;
        }
        let point = actor.position;
        let (height, normal) = self.track_height_and_normal(actor.layer, point);
        let altitude = self.object_position(id)[2] - height;
        Some(DrawMotion {
            velocity: actor.physics_velocity(),
            acceleration: actor.physics_control.wheel_acceleration,
            normal,
            altitude,
            airborne: self.track_airborne(id),
            motive: actor.ai.motion.motive_until > self.tick,
            turning: actor.physics_control.wheel_turning,
            backwards: actor.ai.motion.moving_backwards(),
            locomotor_set: actor.ai.locomotor_set,
            surface: actor.ai.surface_locomotor,
        })
    }

    /// Drawable callbacks use `Object::isSignificantlyAboveTerrain`.
    #[must_use]
    pub fn track_airborne(&self, id: usize) -> bool {
        self.height_above_ground(id) > -9.0 * self.gravity
    }

    /// `W3DTerrainTracks` uses the same terrain layer as the vehicle's suspension.
    #[must_use]
    pub fn track_height_and_normal(&self, layer: u8, point: [f32; 2]) -> (f32, [f32; 3]) {
        let ground = self.ground_height(point);
        let normal = self
            .projectile_terrain
            .as_ref()
            .map_or([0.0, 0.0, 1.0], |terrain| terrain.normal(point));
        self.movement_terrain
            .layer_height_and_normal(layer, point, ground, normal)
    }

    #[must_use]
    pub fn can_scuttle_combat_bike(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            self.catalog[actor.type_index].combat_bike.is_some()
                && actor.loaded()
                && !actor.bike_is_moving()
        })
    }

    fn bike_rider_profile(&self, bike: usize, rider: usize) -> Option<&Rider> {
        let unit = &self.catalog[self.entities[rider].type_index];
        self.catalog[self.entities[bike].type_index]
            .combat_bike
            .as_ref()?
            .riders
            .iter()
            .find(|profile| {
                self.catalog.iter().any(|template| {
                    template.name == profile.template && template.equivalent_to(unit)
                })
            })
    }

    pub(super) fn bike_admits_rider(&self, bike: usize, rider: usize) -> bool {
        self.entities[bike].combat_bike.scuttle_at.is_none()
            && self.bike_rider_profile(bike, rider).is_some()
    }

    pub(super) fn bike_mount(&mut self, bike: usize, rider: usize) {
        if self.entities[bike].containment.payload_created {
            self.entities[bike].combat_bike.replacing = true;
            self.bike_instant_exit(bike);
            self.entities[bike].combat_bike.replacing = false;
        }
        let Some(profile) = self.bike_rider_profile(bike, rider) else {
            return;
        };
        let (model, weapon, status, commands, locomotor) = (
            profile.model,
            profile.weapon,
            profile.status,
            profile.command_set.clone(),
            profile.locomotor,
        );
        let rider_type = self.entities[rider].type_index;
        let state = &mut self.entities[bike].combat_bike;
        state.rider_model = Some(model);
        state.rider_weapon = weapon;
        state.rider_trainable = Some(self.catalog[rider_type].veterancy.trainable);
        self.entities[bike].object_status |= status;
        self.replace_weapon_set(bike);
        self.entities[bike].set_bike_command_set(commands);
        self.choose_locomotor_set(bike, locomotor);
        if self.entities[bike].stealthed() {
            self.mark_detected(bike, 0);
        }
        self.set_hijacker_veterancy(bike, self.entities[rider].veterancy, false);
        self.clear_pilot_veterancy(rider);
        if self.entities[rider].drawable.selected != 0 {
            self.presentation.bike_selection_events.push(Selection {
                from: rider,
                to: bike,
                dismount: false,
            });
        }
    }

    pub(super) fn bike_dismount(&mut self, bike: usize, rider: usize) {
        let Some(profile) = self.bike_rider_profile(bike, rider) else {
            return;
        };
        let status = profile.status;
        self.transfer_bike_sniper_reload(bike, rider, false);
        let state = &mut self.entities[bike].combat_bike;
        state.rider_model = None;
        state.rider_weapon = 0;
        state.rider_trainable = None;
        self.entities[bike].object_status &= !status;
        self.entities[bike].containment.door_flags &= !2;
        self.replace_weapon_set(bike);
        self.set_hijacker_veterancy(rider, self.entities[bike].veterancy, false);
        self.clear_pilot_veterancy(bike);
        if !self.entities[bike].combat_bike.replacing
            && self.entities[bike].has_drawable()
            && self.entities[rider].has_drawable()
        {
            if self.entities[bike].drawable.selected != 0 {
                self.presentation.bike_selection_events.push(Selection {
                    from: bike,
                    to: rider,
                    dismount: true,
                });
            }
            let now = self.update_frame();
            let actor = &mut self.entities[bike];
            actor.combat_bike.scuttle_at = Some(now);
            actor.maintenance.instance_flags |= maintenance::INSTANCE_UNSELECTABLE;
            actor
                .drawable
                .model_conditions
                .set_flag(super::ModelConditions::TOPPLED, true);
            if !actor.bike_is_moving() {
                actor.object_status |= 1 << 42;
            }
        }
    }

    fn bike_instant_exit(&mut self, bike: usize) {
        if self.entities[bike].subdued() || !self.native_ai_command_allowed(bike) {
            return;
        }
        for rider in self.passengers(bike).to_vec() {
            if self.attack_kind(rider, "STEALTH_GARRISON") {
                self.mark_detected(rider, 0);
            }
            self.exit_bike_rider_instantly(rider, bike);
        }
    }

    pub(super) fn bike_kill_pilot(&mut self, bike: usize, source: usize) {
        if self.entities[bike].bike_is_moving() {
            self.score_kill(source, bike);
            self.kill_object(bike, DeathType::Normal);
        } else if let Some(&rider) = self.passengers(bike).first() {
            self.bike_instant_exit(bike);
            self.score_kill(source, rider);
            self.kill_object(rider, DeathType::Normal);
        }
    }

    pub(super) fn transfer_bike_sniper_reload(
        &mut self,
        bike: usize,
        rider: usize,
        mounting: bool,
    ) {
        if !self.attack_kind(bike, "CLIFF_JUMPER")
            || !self.attack_kind(rider, "HERO")
            || !self.attack_kind(rider, "SALVAGER")
            || self.allocated_weapon(bike, 1).is_none()
            || self.allocated_weapon(rider, 1).is_none()
        {
            return;
        }
        let (from, to) = if mounting {
            (rider, bike)
        } else {
            (bike, rider)
        };
        let frame = self.update_frame();
        let unit = &self.catalog[self.entities[from].type_index];
        let status = self.entities[from]
            .weapon_status(1, unit, frame)
            .expect("secondary weapon");
        let weapon = &self.entities[from].weapon_set.weapons[1];
        let (next, reload) = (weapon.possible_next_shot, weapon.last_reload_started);
        let weapon = &mut self.entities[to].weapon_set.weapons[1];
        weapon.possible_next_shot = next;
        weapon.last_reload_started = reload;
        weapon.status = status;
    }

    pub(super) fn update_combat_bike(&mut self, id: usize) -> i32 {
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .combat_bike
            .as_ref()
            .expect("bike data");
        if actor
            .combat_bike
            .scuttle_at
            .is_some_and(|start| start.wrapping_add(rules.scuttle_frames) <= self.update_frame())
        {
            self.kill_object_with_type(id, DamageType::Unresistable, DeathType::Toppled);
        }
        self.update_transport_contain(id)
    }

    pub(super) fn update_bike_steering(&mut self, id: usize) -> i32 {
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        if actor.has_drawable() && now >= actor.combat_bike.next_steering {
            let state = &mut actor.combat_bike;
            let next = match (state.steering, actor.physics_control.wheel_turning) {
                (None, -1) => Some("CENTER_TO_RIGHT"),
                (None, 1) => Some("CENTER_TO_LEFT"),
                (Some("CENTER_TO_RIGHT"), turn) if turn != -1 => Some("RIGHT_TO_CENTER"),
                (Some("CENTER_TO_LEFT"), turn) if turn != 1 => Some("LEFT_TO_CENTER"),
                (Some("LEFT_TO_CENTER" | "RIGHT_TO_CENTER"), 0) => None,
                _ => return 1,
            };
            state.steering = next;
            let rules = self.catalog[actor.type_index]
                .combat_bike
                .as_ref()
                .expect("bike steering data");
            state.next_steering = if next.is_some() {
                now.wrapping_add(rules.steering_frames)
            } else {
                now
            };
        }
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Faction, Kind, WeaponTemplateSet, containment, tests as fixtures};

    #[test]
    fn combat_bike_sniping_on_a_parked_bike_ejects_the_rider_before_scuttle() {
        let mut game = fixtures::game();
        game.catalog[5].name = "GLAInfantryRebel".into();
        let rebel = game.catalog[5].name.clone();
        let mut bike = fixtures::fixture_type(Faction::Gla, Kind::Vehicle);
        bike.name = "Combat Bike".into();
        bike.kind_of = vec!["VEHICLE".into(), "CLIFF_JUMPER".into()];
        let mut contain = containment::tests::rules(0.0, 1.0);
        contain.capacity = 1;
        contain.weapons.fire = false;
        contain.payload = Some(containment::Payload {
            template: rebel.clone(),
            count: 1,
        });
        bike.contain = Some(contain);
        bike.combat_bike = Some(Rules {
            riders: vec![Rider {
                template: rebel,
                model: "RIDER2",
                weapon: rider_weapon_flag("WEAPON_RIDER2").unwrap(),
                status: rider_status_bit("STATUS_RIDER2").unwrap(),
                command_set: "GLAVehicleCombatBikeDefaultCommandSet".into(),
                locomotor: crate::skirmish::locomotor_set::NORMAL,
            }],
            sluggish: Vec::new(),
            scuttle_frames: 45,
            steering_frames: 9,
        });
        bike.weapon_set_rules.replacements = vec![WeaponTemplateSet {
            flags: 0,
            weapons: [None; 3],
            share_lock: false,
        }];
        fixtures::register_behavior(&mut bike, "RiderChangeContain", 0x0057);
        let module = bike.behavior_modules.last().unwrap();
        bike.update_modules.push(
            crate::skirmish::module_updates::ModuleRegistration::Behavior {
                family: module.family.clone(),
                tag: module.tag.clone(),
                source: crate::skirmish::module_updates::ModuleSource {
                    path: module.source.path.clone(),
                    origin: module.source.origin.clone(),
                    line: module.source.line,
                },
            },
        );
        let index = game.catalog.len();
        game.catalog.push(bike);
        let bike = game.spawn(index, 0, [350.0, 260.0], true);
        game.step();
        let rider = game.passengers(bike)[0];
        let sniper = game.spawn(5, 1, [450.0, 260.0], true);
        game.kill_vehicle_pilot(bike, sniper, 1.0);
        assert!(game.entities[bike].alive());
        assert!(!game.entities[bike].unmanned());
        assert!(game.entities[bike].unselectable());
        assert!(game.entities[rider].is_effectively_dead());
        assert!(game.entities[rider].contained_by().is_none());
        assert!(game.passengers(bike).is_empty());
        let scuttle = game.entities[bike].combat_bike.scuttle_at.unwrap();
        while game.update_frame() < scuttle + 44 {
            assert!(game.entities[bike].alive());
            game.step();
        }
        assert!(game.entities[bike].alive());
        game.step();
        assert!(matches!(
            game.entities[bike].death_type(),
            DeathType::Toppled
        ));
    }
}
