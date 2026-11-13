//! `W3DTreeBuffer` and `W3DPropBuffer` construction removal, SH 2bfaea60.
use crate::{
    playtest::Match,
    playtest_view::{WorldObject, WorldObjects},
};
use bevy::prelude::*;
use generals_sim::{
    collision_geometry::{Body, Geometry, Shape, intersects},
    skirmish::ConstructionClear,
};

#[derive(Component)]
pub struct Scenery {
    pub(crate) index: usize,
    pub(crate) removed: bool,
    next_clear: usize,
    generation: Option<u32>,
}

impl Scenery {
    pub(crate) const fn new(index: usize) -> Self {
        Self {
            index,
            removed: false,
            next_clear: 0,
            generation: None,
        }
    }
}

pub fn update(
    game: Res<Match>,
    objects: Res<WorldObjects>,
    mut roots: Query<(&mut Scenery, &mut Visibility)>,
) {
    let game = game.into_inner();
    let objects = objects.into_inner();
    let clears = &game.simulation.construction_clears;
    for (mut state, mut visibility) in &mut roots {
        let object = &objects.0[state.index];
        state.apply(game.generation, clears, object, &mut visibility);
    }
}

impl Scenery {
    fn apply(
        &mut self,
        generation: u32,
        clears: &[ConstructionClear],
        object: &WorldObject,
        visibility: &mut Visibility,
    ) {
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            self.next_clear = 0;
            self.removed = false;
            *visibility = Visibility::Visible;
        }
        if !self.removed
            && clears[self.next_clear..]
                .iter()
                .any(|clear| collides(clear, object))
        {
            self.removed = true;
        }
        if self.removed && *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        self.next_clear = clears.len();
    }
}

pub fn collides(clear: &ConstructionClear, object: &WorldObject) -> bool {
    let radius = if object.tree {
        7.0
    } else {
        let Some(radius) = object.prop_radius else {
            return false;
        };
        radius
    };
    let geometry = Geometry {
        shape: Shape::Cylinder,
        is_small: false,
        height: 5.0 * radius,
        major: 2.0 * radius,
        minor: 2.0 * radius,
    };
    let above = if matches!(clear.geometry.shape, Shape::Sphere) {
        clear.geometry.major
    } else {
        clear.geometry.height
    };
    if clear.position[2] + above < object.position[2]
        || clear.position[2] > object.position[2] + geometry.height
    {
        return false;
    }
    intersects(
        &Body {
            geometry: &clear.geometry,
            position: clear.position,
            angle: clear.angle,
        },
        &Body {
            geometry: &geometry,
            position: object.position,
            angle: 0.0,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> WorldObject {
        WorldObject {
            terrain_bridge: false,
            animation: None,
            model_index: 0,
            position: [100.0, 200.0, 30.0],
            angle: 0.0,
            scale: 4.0,
            tree: true,
            tree_data: None,
            prop_radius: None,
            supply: None,
            supply_draw: None,
        }
    }

    fn foundation(z: f32) -> ConstructionClear {
        ConstructionClear {
            geometry: Geometry {
                shape: Shape::Box,
                is_small: false,
                height: 10.0,
                major: 10.0,
                minor: 10.0,
            },
            position: [100.0, 200.0, z],
            angle: 0.0,
        }
    }

    #[test]
    fn construction_clear_survives_animation_visibility_and_restart_replays_new_history() {
        let object = tree();
        let mut state = Scenery::new(0);
        let mut visibility = Visibility::Visible;
        state.apply(1, &[foundation(30.0)], &object, &mut visibility);
        assert!(state.removed);
        assert_eq!(visibility, Visibility::Hidden);
        // World animation can write Visible again on the following presentation frame.
        visibility = Visibility::Visible;
        state.apply(1, &[foundation(30.0)], &object, &mut visibility);
        assert_eq!(visibility, Visibility::Hidden);
        state.apply(2, &[], &object, &mut visibility);
        assert!(!state.removed);
        assert_eq!(visibility, Visibility::Visible);
        state.apply(2, &[foundation(30.0)], &object, &mut visibility);
        assert!(state.removed);
        assert_eq!(visibility, Visibility::Hidden);
    }

    #[test]
    fn original_tree_radius_ignores_authored_scale_and_preserves_bottom_z_envelope() {
        let object = tree();
        assert!(!collides(&foundation(19.0), &object));
        assert!(collides(&foundation(20.0), &object));
        assert!(collides(&foundation(65.0), &object));
        assert!(!collides(&foundation(66.0), &object));
        let mut scaled = tree();
        scaled.scale = 0.1;
        let mut outside = foundation(30.0);
        outside.position[0] = 125.0;
        assert!(!collides(&outside, &object));
        assert!(!collides(&outside, &scaled));
        let mut prop = tree();
        prop.tree = false;
        prop.prop_radius = Some(3.0);
        assert!(!collides(&foundation(46.0), &prop));
    }
}
