//! Native `LaserUpdate` is a client module; logic supplies ordered initialization.
use super::{Skirmish, WeaponFxEvent};

pub struct Rules {
    pub particles: [String; 2],
    pub punch_through: f32,
}

/// Original initLaser arguments; an explicit end takes precedence during initialization.
pub struct Initialization {
    pub parent: Option<usize>,
    pub target: Option<usize>,
    pub start: Option<[f32; 3]>,
    pub end: Option<[f32; 3]>,
    pub bone: String,
    pub size_delta: i32,
}

/// Drawable state read synchronously by `LaserUpdate::initLaser`.
pub struct DrawableSnapshot {
    pub position: [f32; 3],
    pub basis: [[f32; 3]; 3],
    pub revision: u64,
}

pub struct Event {
    pub object: usize,
    pub frame: u32,
    pub init: Initialization,
    pub parent_drawable: Option<usize>,
    pub parent_pose: Option<DrawableSnapshot>,
    pub target_drawable: Option<usize>,
    pub parent_position: Option<[f32; 3]>,
    pub target_position: Option<[f32; 3]>,
    pub parent_visible: u16,
}

impl Skirmish {
    pub(in crate::skirmish) fn init_native_laser(
        &mut self,
        object: usize,
        init: Initialization,
    ) -> bool {
        if !self.entities[object].has_drawable()
            || self.catalog[self.entities[object].type_index]
                .laser_update
                .is_none()
        {
            return false;
        }
        // Native missing both alternatives destroys the Drawable, not its Object.
        // Valid callers here always supply their actual Object or position.
        self.presentation.weapon_fx.push(WeaponFxEvent::LaserInit(
            self.presentation.laser_initializations.len(),
        ));
        let parent_drawable = init.parent.filter(|&id| self.entities[id].has_drawable());
        let parent_pose = parent_drawable.map(|id| {
            let actor = &self.entities[id];
            DrawableSnapshot {
                position: actor.drawable_position().expect("present parent Drawable"),
                basis: self.object_transform(id).1,
                revision: actor.drawable_transform_revision(),
            }
        });
        let target_drawable = init.target.filter(|&id| self.entities[id].has_drawable());
        let parent_position = init.parent.map(|id| self.object_position(id));
        let target_position = init.target.map(|id| self.object_position(id));
        let parent_visible = (0..self.players.len()).fold(0_u16, |mask, player| {
            if init.parent.is_none_or(|id| {
                matches!(
                    self.visibility_status(player, id),
                    super::visibility::Status::Clear | super::visibility::Status::Partial
                )
            }) {
                mask | (1 << player)
            } else {
                mask
            }
        });
        self.presentation.laser_initializations.push(Event {
            object,
            frame: self.update_frame(),
            init,
            parent_drawable,
            parent_pose,
            target_drawable,
            parent_position,
            target_position,
            parent_visible,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{DeathType, Kind, tests};

    #[test]
    fn laser_snapshot_revision_tracks_even_identical_native_setters_and_direct_motion_fallback() {
        let mut game = tests::game();
        let kind = game.type_for(0, Kind::Infantry).unwrap();
        let parent = game.spawn(kind, 0, [100.0, 200.0], true);
        let first = game.entities[parent].drawable_transform_revision();
        game.set_object_position(parent, game.object_position(parent));
        let second = game.entities[parent].drawable_transform_revision();
        assert_ne!(first, second);
        game.set_object_orientation(parent, game.entities[parent].facing);
        let third = game.entities[parent].drawable_transform_revision();
        assert_ne!(second, third);
        game.observe_ground_partition(parent);
        assert_eq!(third, game.entities[parent].drawable_transform_revision());
        game.entities[parent].position[0] += 0.001;
        game.observe_ground_partition(parent);
        assert_ne!(third, game.entities[parent].drawable_transform_revision());
    }

    #[test]
    fn laser_retirement_preserves_callback_order_and_clears_the_drawable_binding() {
        let mut game = tests::game();
        let kind = game.type_for(0, Kind::Infantry).unwrap();
        let parent = game.spawn(kind, 0, [100.0, 200.0], true);
        let mut stream = tests::fixture_type(crate::skirmish::Faction::Civilian, Kind::Effect);
        stream.ai.update = crate::skirmish::UnitAiUpdate::Absent;
        stream.body_kind = crate::skirmish::BodyKind::Inactive;
        stream.behavior_modules.clear();
        stream.update_modules.clear();
        stream.laser_update = Some(Rules {
            particles: [String::new(), String::new()],
            punch_through: 0.0,
        });
        let index = game.catalog.len();
        game.catalog.push(stream);
        let laser = game.spawn(index, 0, [100.0, 200.0], true);
        let init = || Initialization {
            parent: Some(parent),
            target: None,
            start: None,
            end: Some([500.0, 600.0, 70.0]),
            bone: String::new(),
            size_delta: 0,
        };
        game.presentation.weapon_fx.clear();
        assert!(game.init_native_laser(laser, init()));
        game.destroy_maintenance_object(laser);
        assert!(game.entities[laser].has_drawable());
        assert!(game.init_native_laser(laser, init()));
        assert_eq!(game.presentation.weapon_fx.len(), 2);
        game.process_update_destruction();
        assert!(!game.entities[laser].has_drawable());
        assert!(!game.init_native_laser(laser, init()));
        assert!(matches!(game.presentation.weapon_fx.as_slice(), [
            WeaponFxEvent::LaserInit(_),
            WeaponFxEvent::LaserInit(_),
            WeaponFxEvent::DrawableDestroyed(id),
        ] if *id == laser));
        game.destroy_maintenance_object(parent);
        game.process_update_destruction();
        let next = game.spawn(index, 0, [100.0, 200.0], true);
        assert!(game.init_native_laser(next, init()));
        let event = game.presentation.laser_initializations.last().unwrap();
        assert!(event.parent_drawable.is_none());
        assert!(event.parent_pose.is_none());
        assert!(matches!(
            game.presentation.weapon_fx.last(),
            Some(WeaponFxEvent::LaserInit(_))
        ));
    }

    #[test]
    fn laser_snapshot_retains_original_drawable_after_same_tick_move_and_death() {
        let mut game = tests::game();
        let kind = game.type_for(0, Kind::Infantry).unwrap();
        game.catalog[kind]
            .behavior
            .destroy_die
            .push(tests::death_conditions());
        tests::register_behavior(&mut game.catalog[kind], "DestroyDie", 2);
        let parent = game.spawn(kind, 0, [100.0, 200.0], true);
        game.set_object_position(parent, [100.0, 200.0, 30.0]);
        game.set_object_orientation(parent, 0.5);
        game.entities[parent].ai.turret.angle = 0.25;
        game.entities[parent].ai.turret.pitch = 0.125;
        let mut stream = tests::fixture_type(crate::skirmish::Faction::Civilian, Kind::Effect);
        stream.ai.update = crate::skirmish::UnitAiUpdate::Absent;
        stream.body_kind = crate::skirmish::BodyKind::Inactive;
        stream.behavior_modules.clear();
        stream.update_modules.clear();
        stream.laser_update = Some(Rules {
            particles: [String::new(), String::new()],
            punch_through: 0.0,
        });
        let index = game.catalog.len();
        game.catalog.push(stream);
        let laser = game.spawn(index, 0, [100.0, 200.0], true);
        assert!(game.init_native_laser(
            laser,
            Initialization {
                parent: Some(parent),
                target: None,
                start: None,
                end: Some([500.0, 600.0, 70.0]),
                bone: "Muzzle01".into(),
                size_delta: 0,
            }
        ));
        let basis = game.object_transform(parent).1;
        game.set_object_position(parent, [900.0, 800.0, 20.0]);
        game.set_object_orientation(parent, 1.5);
        game.entities[parent].ai.turret.angle = 0.75;
        game.kill_object(parent, DeathType::Normal);
        game.process_update_destruction();
        assert!(!game.entities[parent].exists());
        assert!(!game.entities[parent].has_drawable());
        assert!(game.entities[laser].exists());
        let event = game.presentation.laser_initializations.last().unwrap();
        let pose = event.parent_pose.as_ref().unwrap();
        assert_eq!(
            pose.position.map(f32::to_bits),
            [100.0_f32, 200.0, 30.0].map(f32::to_bits)
        );
        assert_eq!(
            pose.basis.map(|row| row.map(f32::to_bits)),
            basis.map(|row| row.map(f32::to_bits))
        );
        assert_ne!(
            pose.revision,
            game.entities[parent].drawable_transform_revision()
        );
        assert_eq!(event.parent_drawable, Some(parent));
        assert_eq!(
            event.init.end.unwrap().map(f32::to_bits),
            [500.0_f32, 600.0, 70.0].map(f32::to_bits)
        );
    }
}
