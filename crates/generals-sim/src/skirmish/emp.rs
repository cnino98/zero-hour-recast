//! SH `EMPUpdate` construction, spherical pulse and drawable envelope.
use super::{DeathType, Entity, Skirmish, startup};
use crate::collision_geometry::Shape;
use std::f32::consts::PI;

pub struct Rules {
    pub lifetime: u32,
    pub fade_frame: u32,
    pub disabled_frames: u32,
    pub start_scale: f32,
    pub target_scale: [f32; 2],
    pub start_color: [f32; 3],
    pub end_color: [f32; 3],
    pub sparks: String,
    pub sparks_per_cubic_foot: f32,
    pub radius: f32,
    pub reject_allies: bool,
    pub ignore_own_buildings: bool,
}

pub struct Sparks {
    pub target: usize,
    pub name: String,
    pub count: u32,
    pub lifetime: u32,
    pub height: f32,
}

pub struct Tint {
    pub frame: u32,
    pub peak: [f32; 3],
    pub attack_frames: u32,
}

#[derive(Default)]
pub(in crate::skirmish) struct State {
    die_frame: u32,
    fade_frame: u32,
    target_scale: f32,
    scale: f32,
    tint: Option<Tint>,
}

impl Entity {
    #[must_use]
    pub const fn emp_tint(&self) -> Option<&Tint> {
        self.emp.tint.as_ref()
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_emp(&mut self, id: usize) {
        let rules = self.catalog[self.entities[id].type_index]
            .behavior
            .emp
            .as_ref()
            .expect("bound EMPUpdate");
        let now = self.update_frame();
        let target_scale = startup::random_real(
            &mut self.random,
            rules.target_scale[0],
            rules.target_scale[1],
        );
        self.entities[id].emp = State {
            die_frame: now.wrapping_add(rules.lifetime),
            fade_frame: now.wrapping_add(rules.fade_frame),
            target_scale,
            scale: rules.start_scale,
            tint: None,
        };
        let angle = startup::random_real(&mut self.random, -PI, PI);
        self.set_object_orientation(id, angle);
    }

    pub(in crate::skirmish) fn update_emp(&mut self, id: usize) -> i32 {
        let now = self.update_frame();
        let kind = self.entities[id].type_index;
        let rules = self.catalog[kind]
            .behavior
            .emp
            .as_ref()
            .expect("bound EMPUpdate");
        let actor = &mut self.entities[id];
        let state = &mut actor.emp;
        let increment = (state.target_scale - state.scale) * 0.05;
        state.scale += increment;
        actor.drawable.instance_matrix = Some([
            [state.scale, 0.0, 0.0, 0.0],
            [0.0, state.scale, 0.0, 0.0],
            [0.0, 0.0, state.scale, 0.0],
        ]);
        if now < state.fade_frame {
            state.tint = Some(Tint {
                frame: now,
                peak: saturate(rules.start_color, 2.0),
                attack_frames: 0,
            });
        } else if now == state.fade_frame {
            state.tint = Some(Tint {
                frame: now,
                peak: saturate(rules.end_color, 5.0),
                attack_frames: state.die_frame.wrapping_sub(state.fade_frame),
            });
            self.apply_emp(id, kind);
        }
        if now >= self.entities[id].emp.die_frame && self.entities[id].exists() {
            self.kill_object(id, DeathType::Normal);
        }
        1
    }

    /// The positional `PartitionManager` query treats the source position as a point.
    pub(in crate::skirmish) fn emp_range_targets(&self, source: usize, radius: f32) -> Vec<usize> {
        let mut targets = self.sphere_range_targets(self.object_position(source), radius);
        targets.retain(|&target| target != source);
        targets
    }

    pub(in crate::skirmish) fn sphere_range_targets(
        &self,
        position: [f32; 3],
        radius: f32,
    ) -> Vec<usize> {
        if radius <= 0.0 {
            return Vec::new();
        }
        self.partition
            .fastest_range_objects([position[0], position[1]], radius)
            .into_iter()
            .filter(|&target| {
                let bounds = self.body_geometry_bounds(target);
                let other = self.object_position(target);
                let dx = other[0] - position[0];
                let dy = other[1] - position[1];
                let dz = other[2] + bounds.center_height - position[2];
                let distance = square_length([dx, dy, dz]).sqrt();
                let separation = (distance - bounds.sphere_radius).max(0.0);
                separation * separation < radius * radius
            })
            .collect()
    }

    fn apply_emp(&mut self, source: usize, kind: usize) {
        let rules = self.catalog[kind]
            .behavior
            .emp
            .as_ref()
            .expect("bound EMPUpdate");
        let radius = rules.radius;
        let duration = rules.disabled_frames;
        let (ignore_own_buildings, reject_allies) =
            (rules.ignore_own_buildings, rules.reject_allies);
        let deadline = self.update_frame().wrapping_add(duration);
        let owner = self.entities[source].owner;
        let intended = self.entities[source]
            .producer
            .and_then(|id| self.entities.get(id))
            .filter(|actor| actor.exists() && actor.ai.registration.is_some())
            .and_then(|actor| actor.ai.current_victim)
            .filter(|&id| self.entities[id].exists());
        let air_only = intended.is_some_and(|id| self.entities[id].is_airborne_target());
        let mut processed = false;
        for target in self.emp_range_targets(source, radius) {
            let structure = self.attack_kind(target, "STRUCTURE");
            if (air_only && !self.entities[target].is_airborne_target())
                || (structure && ignore_own_buildings && self.entities[target].owner == owner)
                || !(structure
                    || self.attack_kind(target, "VEHICLE")
                    || self.attack_kind(target, "SPAWNS_ARE_THE_WEAPONS"))
            {
                continue;
            }
            if self.attack_kind(target, "AIRCRAFT") && self.entities[target].is_airborne_target() {
                if !self.attack_kind(target, "EMP_HARDENED") {
                    self.kill_object(target, DeathType::Normal);
                    if self.entities[target].drawable.present {
                        self.entities[target].drawable.tint_status |= 1;
                    }
                }
                continue;
            }
            if structure {
                if !faction_structure(&self.catalog[self.entities[target].type_index].kind_of) {
                    continue;
                }
            } else if reject_allies && self.object_relationship(target, source) == 2 {
                continue;
            }
            self.set_emp_disabled_until(target, deadline);
            processed |= intended == Some(target);
            self.emp_sparks(kind, target);
        }
        if let Some(target) = intended
            && !processed
            && self.attack_kind(target, "AIRCRAFT")
            && !self.attack_kind(target, "EMP_HARDENED")
        {
            let origin = self.object_position(source);
            let target_position = self.object_position(target);
            let delta = std::array::from_fn(|axis| target_position[axis] - origin[axis]);
            let distance = square_length(delta);
            if distance <= radius * 2.0 || distance <= 40.0 * 40.0 {
                self.set_emp_disabled_until(target, deadline);
            }
        }
    }

    fn emp_sparks(&mut self, kind: usize, target: usize) {
        let rules = self.catalog[kind]
            .behavior
            .emp
            .as_ref()
            .expect("bound EMPUpdate");
        if rules.sparks.is_empty() || !self.entities[target].drawable.present {
            return;
        }
        let geometry = self.body_geometry(target);
        let height = self.body_geometry_bounds(target).max_height;
        let area = match geometry.shape {
            Shape::Sphere | Shape::Cylinder => PI * (geometry.major * geometry.major),
            Shape::Box => 4.0 * geometry.major * geometry.minor,
        };
        let count =
            crate::native_real::ceil(rules.sparks_per_cubic_foot * (area * height.min(10.0)));
        let sparks = Sparks {
            target,
            name: rules.sparks.clone(),
            count: u32::try_from(count.max(15)).expect("native EMP emitter count"),
            lifetime: rules.disabled_frames.saturating_sub(30),
            height,
        };
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::EmpSparks(
                self.presentation.emp_sparks.len(),
            ));
        self.presentation.emp_sparks.push(sparks);
    }
}

fn faction_structure(kinds: &[String]) -> bool {
    kinds.iter().any(|kind| {
        matches!(
            kind.as_str(),
            "FS_FACTORY"
                | "FS_BASE_DEFENSE"
                | "FS_TECHNOLOGY"
                | "FS_SUPPLY_DROPZONE"
                | "FS_SUPERWEAPON"
                | "FS_BLACK_MARKET"
                | "FS_SUPPLY_CENTER"
                | "FS_STRATEGY_CENTER"
                | "FS_FAKE"
                | "FS_INTERNET_CENTER"
                | "FS_ADVANCED_TECH"
                | "FS_BARRACKS"
                | "FS_WARFACTORY"
                | "FS_AIRFIELD"
        )
    })
}

fn saturate(color: [f32; 3], factor: f32) -> [f32; 3] {
    let bias = factor * 0.5;
    color.map(|channel| {
        let scaled = channel * factor;
        scaled - bias
    })
}

fn square_length([x, y, z]: [f32; 3]) -> f32 {
    let xx = x * x;
    let yy = y * y;
    let zz = z * z;
    (xx + yy) + zz
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Faction, Kind, tests};

    #[test]
    fn pulse_applies_on_fade_and_preserves_other_disable_causes_at_expiry() {
        let mut game = tests::game();
        let mut source = tests::fixture_type(Faction::China, Kind::Effect);
        source.behavior.emp = Some(Rules {
            lifetime: 90,
            fade_frame: 9,
            disabled_frames: 900,
            start_scale: 0.01,
            target_scale: [3.0, 4.0],
            start_color: [32.0 / 255.0, 64.0 / 255.0, 1.0],
            end_color: [0.0; 3],
            sparks: "EMPSparks".into(),
            sparks_per_cubic_foot: 0.001,
            radius: 200.0,
            reject_allies: false,
            ignore_own_buildings: false,
        });
        tests::register_update(&mut source, "EMPUpdate");
        let source_type = game.catalog.len();
        game.catalog.push(source);
        let mut vehicle = tests::fixture_type(Faction::China, Kind::Tank);
        vehicle.kind_of.push("VEHICLE".into());
        let vehicle_type = game.catalog.len();
        game.catalog.push(vehicle);
        let mut infantry = tests::fixture_type(Faction::China, Kind::Infantry);
        infantry.kind_of.push("INFANTRY".into());
        let infantry_type = game.catalog.len();
        game.catalog.push(infantry);
        let source = game.spawn(source_type, 0, [300.0, 300.0], true);
        let inside = game.spawn(vehicle_type, 1, [400.0, 300.0], true);
        let outside = game.spawn(vehicle_type, 1, [650.0, 300.0], true);
        let infantry = game.spawn(infantry_type, 1, [390.0, 300.0], true);
        game.update_ground_contacts();

        game.tick = 8;
        game.update_emp(source);
        assert_eq!(game.object_disabled_mask(inside) & 4, 0);
        game.tick += 1;
        game.update_emp(source);
        assert_eq!(game.entities[inside].emp_until, 909);
        assert_eq!(game.object_disabled_mask(outside) & 4, 0);
        assert_eq!(game.object_disabled_mask(infantry) & 4, 0);
        assert!(
            game.presentation.emp_sparks.iter().any(|event| {
                event.target == inside && event.count >= 15 && event.lifetime == 870
            })
        );
        game.set_hacked_until(inside, 1000);
        game.tick = 908;
        game.expire_emp();
        assert_eq!(game.object_disabled_mask(inside) & 6, 6);
        game.tick += 1;
        game.expire_emp();
        assert_eq!(game.object_disabled_mask(inside) & 6, 2);
        assert!(game.entities[inside].drawable_disabled_tint());
    }
}
