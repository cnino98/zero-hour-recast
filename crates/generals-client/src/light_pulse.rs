//! Source-ordered `W3DDynamicLight` pool and `W3DDisplay::createLightPulse` fade.

use num_traits::ToPrimitive;

pub struct Pool {
    lights: Vec<Light>,
}

pub struct Light {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub outer_radius: f32,
    pub enabled: bool,
    pub prior_enabled: bool,
    pub previous_bounds: [i32; 4],
    pub current_bounds: [i32; 4],
    target_color: [f32; 3],
    target_outer_radius: f32,
    increase_remaining: u32,
    increase_frames: u32,
    decrease_remaining: u32,
    decrease_frames: u32,
}

impl Pool {
    pub const fn new() -> Self {
        Self { lights: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.lights.clear();
    }

    /// Create a pulse with the native fixed inner radius of one world unit.
    /// Returns false when `W3DDisplay` rejects a pulse too small to affect a cell.
    pub fn create(
        &mut self,
        position: [f32; 3],
        color: [f32; 3],
        radius: f32,
        increase_frames: u32,
        decrease_frames: u32,
    ) -> bool {
        if 1.0 + radius < 21.0 {
            return false;
        }

        let Some(light) = self.lights.iter_mut().find(|light| !light.enabled) else {
            self.lights.insert(
                0,
                Light {
                    position,
                    color,
                    outer_radius: 1.0 + radius,
                    enabled: true,
                    prior_enabled: false,
                    previous_bounds: [0; 4],
                    current_bounds: [0; 4],
                    target_color: color,
                    target_outer_radius: 1.0 + radius,
                    increase_remaining: increase_frames,
                    increase_frames,
                    decrease_remaining: decrease_frames,
                    decrease_frames,
                },
            );
            return true;
        };

        light.enabled = true;
        light.position = position;
        light.color = color;
        light.outer_radius = 1.0 + radius;
        light.increase_remaining = increase_frames;
        light.increase_frames = increase_frames;
        light.decrease_remaining = decrease_frames;
        light.decrease_frames = decrease_frames;
        light.target_color = color;
        light.target_outer_radius = 1.0 + radius;
        true
    }

    /// Advance once per display frame, whether or not simulation logic advanced.
    pub fn advance_frame(&mut self) {
        for light in &mut self.lights {
            if !light.enabled {
                continue;
            }

            let factor = if light.increase_remaining > 0 && light.increase_frames > 0 {
                light.increase_remaining -= 1;
                frames_f32(light.increase_frames - light.increase_remaining)
                    / frames_f32(light.increase_frames)
            } else if light.decrease_frames == 0 {
                1.0
            } else {
                light.decrease_remaining -= 1;
                if light.decrease_remaining == 0 {
                    light.enabled = false;
                    continue;
                }
                frames_f32(light.decrease_remaining) / frames_f32(light.decrease_frames)
            };

            let outer_radius = factor * light.target_outer_radius;
            light.outer_radius = if outer_radius < 1.0 {
                1.0
            } else {
                outer_radius
            };
            for (channel, target) in light.color.iter_mut().zip(light.target_color) {
                *channel = target * factor;
            }
        }
    }

    pub fn lights_mut(&mut self) -> &mut [Light] {
        &mut self.lights
    }

    /// Return pool entries in native `RefMultiListClass::Add` traversal order.
    pub fn lights(&self) -> &[Light] {
        &self.lights
    }
}

fn frames_f32(frames: u32) -> f32 {
    frames.to_f32().expect("u32 frame count converts to f32")
}

#[cfg(test)]
mod tests {
    use super::Pool;

    fn assert_f32(actual: f32, expected: f32) {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }

    fn assert_vec3(actual: [f32; 3], expected: [f32; 3]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_f32(actual, expected);
        }
    }

    #[test]
    fn native_radius_threshold_rejects_below_twenty_and_accepts_twenty() {
        let mut pool = Pool::new();
        assert!(!pool.create([1.0, 2.0, 3.0], [1.0, 0.0, 0.0], 0.0, 0, 0));
        assert!(!pool.create([1.0, 2.0, 3.0], [1.0, 0.0, 0.0], 19.999, 0, 0));
        assert!(pool.lights().is_empty());

        assert!(pool.create([1.0, 2.0, 3.0], [1.0, 0.0, 0.0], 20.0, 0, 0));
        assert_eq!(pool.lights().len(), 1);
        assert_f32(pool.lights()[0].outer_radius, 21.0);
    }

    #[test]
    fn increase_decrements_before_each_display_frame_value() {
        let mut pool = Pool::new();
        assert!(pool.create([3.0, 4.0, 5.0], [1.0, 0.5, 0.0], 30.0, 3, 0));
        assert_vec3(pool.lights()[0].color, [1.0, 0.5, 0.0]);
        assert_f32(pool.lights()[0].outer_radius, 31.0);

        pool.advance_frame();
        assert_vec3(pool.lights()[0].color, [1.0 / 3.0, 1.0 / 6.0, 0.0]);
        // Native computes rounded f32(1/3), then multiplies range31;
        // direct range/3 is one ULP smaller (0x41255555).
        assert_eq!(pool.lights()[0].outer_radius.to_bits(), 0x4125_5556);
        pool.advance_frame();
        assert_vec3(pool.lights()[0].color, [2.0 / 3.0, 1.0 / 3.0, 0.0]);
        pool.advance_frame();
        assert_vec3(pool.lights()[0].color, [1.0, 0.5, 0.0]);
        assert!(pool.lights()[0].enabled);
    }

    #[test]
    fn decay_fades_to_inner_radius_then_disables_before_zero_frame_update() {
        let mut pool = Pool::new();
        assert!(pool.create([0.0; 3], [1.0, 0.0, 0.0], 25.0, 0, 3));
        pool.advance_frame();
        assert_vec3(pool.lights()[0].color, [2.0 / 3.0, 0.0, 0.0]);
        assert_f32(pool.lights()[0].outer_radius, 26.0 * (2.0 / 3.0));
        pool.advance_frame();
        assert_vec3(pool.lights()[0].color, [1.0 / 3.0, 0.0, 0.0]);
        assert_f32(pool.lights()[0].outer_radius, 26.0 * (1.0 / 3.0));
        pool.advance_frame();
        assert!(!pool.lights()[0].enabled);
        assert_vec3(pool.lights()[0].color, [1.0 / 3.0, 0.0, 0.0]);
        assert_f32(pool.lights()[0].outer_radius, 26.0 * (1.0 / 3.0));
    }

    #[test]
    fn newest_pool_entry_is_first_and_first_disabled_entry_is_reused() {
        let mut pool = Pool::new();
        assert!(pool.create([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 25.0, 0, 2));
        assert!(pool.create([2.0, 0.0, 0.0], [0.0, 1.0, 0.0], 30.0, 0, 1));
        assert_vec3(pool.lights()[0].position, [2.0, 0.0, 0.0]);
        assert_vec3(pool.lights()[1].position, [1.0, 0.0, 0.0]);

        pool.advance_frame();
        assert!(!pool.lights()[0].enabled);
        assert!(pool.lights()[1].enabled);
        assert_vec3(pool.lights()[1].color, [0.5, 0.0, 0.0]);

        assert!(pool.create([9.0, 8.0, 7.0], [0.25, 0.5, 1.0], 40.0, 0, 0));
        assert_eq!(pool.lights().len(), 2);
        assert_vec3(pool.lights()[0].position, [9.0, 8.0, 7.0]);
        assert_vec3(pool.lights()[0].color, [0.25, 0.5, 1.0]);
        assert_f32(pool.lights()[0].outer_radius, 41.0);
        assert!(pool.lights()[0].enabled);
        assert_vec3(pool.lights()[1].position, [1.0, 0.0, 0.0]);
        assert!(pool.lights()[1].enabled);

        pool.advance_frame();
        assert!(pool.lights()[0].enabled);
        assert_vec3(pool.lights()[0].color, [0.25, 0.5, 1.0]);
    }

    #[test]
    fn increase_finishes_before_decay_begins() {
        let mut pool = Pool::new();
        assert!(pool.create([0.0; 3], [1.0, 0.0, 0.0], 25.0, 2, 2));
        pool.advance_frame();
        assert_vec3(pool.lights()[0].color, [0.5, 0.0, 0.0]);
        pool.advance_frame();
        assert_vec3(pool.lights()[0].color, [1.0, 0.0, 0.0]);
        pool.advance_frame();
        assert_vec3(pool.lights()[0].color, [0.5, 0.0, 0.0]);
        pool.advance_frame();
        assert!(!pool.lights()[0].enabled);
        assert_vec3(pool.lights()[0].color, [0.5, 0.0, 0.0]);
    }

    #[test]
    fn zero_increase_and_decrease_keeps_full_native_color_and_range() {
        let mut pool = Pool::new();
        assert!(pool.create([4.0, 5.0, 6.0], [1.0, 1.0, 128.0 / 255.0], 30.0, 0, 0));
        for _ in 0..4 {
            pool.advance_frame();
        }
        assert!(pool.lights()[0].enabled);
        assert_vec3(pool.lights()[0].color, [1.0, 1.0, 128.0 / 255.0]);
        assert_f32(pool.lights()[0].outer_radius, 31.0);
    }

    #[test]
    fn clear_discards_all_pool_slots() {
        let mut pool = Pool::new();
        assert!(pool.create([0.0; 3], [1.0; 3], 20.0, 0, 0));
        pool.clear();
        assert!(pool.lights().is_empty());
    }
}
