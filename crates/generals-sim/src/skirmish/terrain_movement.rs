//! `Object::setTriggerAreaFlagsForChangeInPosition`, SH 2bfaea60.
use super::Skirmish;
use crate::collision_geometry::Geometry;
use num_traits::ToPrimitive;

pub enum Event {
    TreeAdded(usize),
    Moved {
        frame: u64,
        object: usize,
        position: [f32; 3],
        direction: [f32; 3],
        geometry: Geometry,
        crusher_level: u8,
        clear_region: super::visibility::ClearRegion,
    },
}

impl Skirmish {
    pub(in crate::skirmish) fn record_terrain_movement(&mut self, event: Event) {
        let index = self.presentation.terrain_movement_events.len();
        self.presentation.terrain_movement_events.push(event);
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::TerrainMovement(index));
    }

    pub(in crate::skirmish) fn notify_terrain_movement(&mut self, id: usize) {
        let actor = &self.entities[id];
        let position = self.object_position(id);
        if !position
            .iter()
            .zip(actor.drawable.transform_position)
            .any(|(new, old)| (*new - old).abs() > 0.01)
        {
            return;
        }
        let unit = &self.catalog[actor.type_index];
        if unit
            .kind_of
            .iter()
            .any(|kind| matches!(kind.as_str(), "PROJECTILE" | "INERT"))
        {
            return;
        }
        let integer =
            [position[0], position[1]].map(|value| value.to_i32().expect("native map coordinate"));
        if integer == actor.trigger_position {
            return;
        }
        if !unit.kind_of.iter().any(|kind| kind == "IMMOBILE")
            && unit
                .kind_of
                .iter()
                .any(|kind| matches!(kind.as_str(), "INFANTRY" | "VEHICLE"))
        {
            let geometry = self.body_geometry(id);
            let radius = if matches!(geometry.shape, crate::collision_geometry::Shape::Box) {
                geometry.major.min(geometry.minor)
            } else {
                geometry.major
            } + 7.0;
            let clear_region = self.terrain_visibility_region(
                [position[0] - radius, position[1] - radius],
                [position[0] + radius, position[1] + radius],
            );
            self.record_terrain_movement(Event::Moved {
                frame: self.tick,
                object: id,
                position,
                // Thing::getUnitDirectionVector2D ignores pitch and roll.
                direction: [actor.facing.cos(), actor.facing.sin(), 0.0],
                geometry: Geometry {
                    shape: geometry.shape,
                    is_small: geometry.is_small,
                    major: geometry.major,
                    minor: geometry.minor,
                    height: geometry.height,
                },
                crusher_level: unit.crush.crusher_level,
                clear_region,
            });
        }
        self.entities[id].trigger_position = integer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Kind, tests::game};

    #[test]
    fn terrain_contact_keeps_its_position_between_native_sound_callbacks() {
        use crate::skirmish::{WeaponFxEvent, object_sounds::Action};

        let mut game = game();
        let kind = game.type_for(0, Kind::Infantry).unwrap();
        let id = game.spawn(kind, 0, [350.0, 200.0], true);
        game.presentation.terrain_movement_events.clear();
        game.presentation.weapon_fx.clear();
        game.presentation.object_sounds.clear();
        game.object_sound(id, "BeforeContact".into(), Action::Play);
        game.set_object_position(id, [352.0, 200.0, 0.0]);
        game.object_sound(id, "AfterContact".into(), Action::Play);
        assert!(matches!(
            game.presentation.weapon_fx.as_slice(),
            [
                WeaponFxEvent::ObjectSound(0),
                WeaponFxEvent::TerrainMovement(0),
                WeaponFxEvent::ObjectSound(1),
            ]
        ));
        assert_eq!(game.presentation.terrain_movement_events.len(), 1);
    }

    #[test]
    fn transform_epsilon_and_integer_xy_gate_notifications_before_position_claims() {
        let mut game = game();
        let kind = game.type_for(0, Kind::Infantry).unwrap();
        let id = game.spawn(kind, 0, [350.995, 200.0], true);
        game.presentation.terrain_movement_events.clear();
        game.set_object_position(id, [351.004, 200.0, 0.0]);
        game.set_object_position(id, [351.013, 200.0, 0.0]);
        assert!(game.presentation.terrain_movement_events.is_empty());
        game.set_object_position(id, [351.1, 200.0, 0.0]);
        assert_eq!(game.presentation.terrain_movement_events.len(), 1);
        game.set_object_orientation(id, 1.0);
        game.observe_ground_partition(id);
        game.set_object_position(id, [351.1, 200.0, 20.0]);
        assert_eq!(game.presentation.terrain_movement_events.len(), 1);
        game.set_object_position(id, [352.0, 200.0, 20.0]);
        assert_eq!(game.presentation.terrain_movement_events.len(), 2);
        let Event::Moved {
            object,
            position,
            direction,
            ..
        } = &game.presentation.terrain_movement_events[1]
        else {
            panic!("movement callback")
        };
        assert_eq!(*object, id);
        assert_eq!(
            position.map(f32::to_bits),
            [352.0_f32, 200.0, 20.0].map(f32::to_bits)
        );
        assert_eq!(
            direction.map(f32::to_bits),
            [1.0_f32.cos(), 1.0_f32.sin(), 0.0].map(f32::to_bits)
        );
    }

    #[test]
    fn source_kind_filters_exclude_inert_projectile_and_immobile_but_not_dead_health() {
        for exclusion in ["INERT", "PROJECTILE", "IMMOBILE", ""] {
            let mut game = game();
            let kind = game.type_for(0, Kind::Infantry).unwrap();
            let id = game.spawn(kind, 0, [350.0, 200.0], true);
            if !exclusion.is_empty() {
                game.catalog[kind].kind_of.push(exclusion.into());
            }
            game.presentation.terrain_movement_events.clear();
            game.entities[id].health = 0.0;
            game.set_object_position(id, [351.0, 200.0, 0.0]);
            assert_eq!(
                game.presentation.terrain_movement_events.len(),
                usize::from(exclusion.is_empty()),
                "{exclusion}"
            );
            game.step();
            assert!(game.presentation.terrain_movement_events.is_empty());
        }
    }
}
