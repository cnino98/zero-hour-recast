//! `ChinookCombatDropState`, selected CRC=0 bypasses door reservation.
use super::{Flight, Skirmish};
use crate::skirmish::{ai_goals::COMMAND_FROM_AI, ground_movement::MoveResult};

pub(super) struct Rope {
    drawable: u64,
    drop_matrix: [[f32; 4]; 3],
    speed: f32,
    length: f32,
    maximum: f32,
    next_drop: u32,
    rappellers: Vec<usize>,
}

/// Actual drawable-only `W3DRopeDraw` inputs. Animation, wobble RNG, and released
/// rope falling are client-frame work; they never advance the logic random stream.
pub struct RopeDrawable {
    pub id: u64,
    pub template: String,
    pub position: [f32; 3],
    pub maximum: f32,
    pub length: f32,
    pub width: f32,
    pub color: [f32; 3],
    pub wobble: [f32; 3],
    pub released: Option<Release>,
}

pub struct Release {
    pub speed: f32,
    pub maximum_speed: f32,
    pub acceleration: f32,
    pub expires: u32,
}

pub struct RopeDrawables {
    pub drawables: Vec<RopeDrawable>,
    next_id: u64,
}

impl RopeDrawables {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            drawables: Vec::new(),
            next_id: 0,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish::chinook) fn enter_chinook_combat_drop(
        &mut self,
        id: usize,
    ) -> MoveResult {
        if !self.entities[id].drawable.present {
            return MoveResult::Failure;
        }
        self.set_disabled_held(id);
        self.entities[id].chinook.as_mut().unwrap().flight = Flight::CombatDrop;
        self.entities[id].cargo = 0;
        let rules = &self.catalog[self.entities[id].type_index]
            .chinook
            .as_ref()
            .unwrap()
            .drop;
        let maximum = rules.ropes.min(32);
        let mut bones = Vec::new();
        for number in 1..=maximum {
            let Some(start) = self.logical_bone_position(id, &format!("RopeStart{number:02}"))
            else {
                break;
            };
            let Some(end) = self.logical_bone_matrix(id, &format!("RopeEnd{number:02}")) else {
                break;
            };
            bones.push((start, end));
        }
        if bones.is_empty() {
            return MoveResult::Failure;
        }
        let now = self.update_frame();
        self.rope_drawables.drawables.retain(|rope| {
            rope.released
                .as_ref()
                .is_none_or(|release| now < release.expires)
        });
        let mut ropes = Vec::new();
        for (start, end) in bones {
            let position = self.chinook_bone_world_position(id, start);
            let drop_matrix = self.chinook_bone_world_matrix(id, end);
            let point = [position[0], position[1]];
            let ground = self.ground_height(point);
            let layer = self
                .movement_terrain
                .get_highest_layer_for_destination(position, ground, true);
            let ground = self
                .movement_terrain
                .bridge_height(layer, point)
                .unwrap_or(ground);
            let rules = &self.catalog[self.entities[id].type_index]
                .chinook
                .as_ref()
                .unwrap()
                .drop;
            let length = position[2] - ground - rules.final_height;
            let drawable = self.rope_drawables.next_id;
            self.rope_drawables.next_id = drawable.checked_add(1).expect("drawable ID overflow");
            self.rope_drawables.drawables.push(RopeDrawable {
                id: drawable,
                template: rules.rope_name.clone(),
                position,
                maximum: length.max(1.0),
                length: 0.0,
                width: rules.width,
                color: rules.color,
                wobble: [
                    rules.wobble_length.min(length.max(1.0)),
                    rules.wobble_amplitude,
                    rules.wobble_rate,
                ],
                released: None,
            });
            let delay = rules.delay;
            self.presentation
                .weapon_fx
                .push(crate::skirmish::WeaponFxEvent::Chinook(
                    super::VisualEvent::RopeCreated(drawable),
                ));
            let offset = self.random_between(delay[1].wrapping_sub(delay[0]));
            ropes.push(Rope {
                drawable,
                drop_matrix,
                speed: 0.0,
                length: 1.0,
                maximum: length,
                next_drop: now.wrapping_add(offset),
                rappellers: Vec::new(),
            });
        }
        self.entities[id].chinook.as_mut().unwrap().ropes = ropes;
        MoveResult::Continue
    }

    pub(in crate::skirmish::chinook) fn update_chinook_combat_drop(
        &mut self,
        id: usize,
    ) -> MoveResult {
        if self.entities[id].is_effectively_dead() {
            return MoveResult::Failure;
        }
        let mut ropes = std::mem::take(&mut self.entities[id].chinook.as_mut().unwrap().ropes);
        for rope in &mut ropes {
            rope.rappellers.retain(|&rider| {
                self.entities.get(rider).is_some_and(|actor| {
                    actor.exists()
                        && !actor.is_effectively_dead()
                        && actor.contained_by().is_none()
                        && self.height_above_ground(rider) > 0.0
                })
            });
        }
        let now = self.update_frame();
        let mut used = 0;
        for rope in &mut ropes {
            let rules = &self.catalog[self.entities[id].type_index]
                .chinook
                .as_ref()
                .unwrap()
                .drop;
            let delay = rules.delay;
            let rappel_speed = rules.rappel_speed;
            if rope.length < rope.maximum {
                rope.speed += self.gravity.abs();
                if rope.speed > rules.drop_speed {
                    rope.speed = rules.drop_speed;
                }
                rope.length += rope.speed;
                self.rope_drawables
                    .drawables
                    .iter_mut()
                    .find(|draw| draw.id == rope.drawable)
                    .expect("attached rope drawable")
                    .length = rope.length;
                if rules.wait_for_ropes {
                    rope.next_drop = rope.next_drop.wrapping_add(1);
                    continue;
                }
            }
            if now >= rope.next_drop
                && let Some(rider) = self.potential_rappeller(id)
            {
                self.exit_spawned_from_container(rider, id);
                let position = rope.drop_matrix.map(|row| row[3]);
                let basis = rope.drop_matrix.map(|row| [row[0], row[1], row[2]]);
                self.set_object_transform(rider, position, basis);
                if self.entities[rider].ai.registration.is_some() {
                    let target = self.entities[id].ai.command.goal_object;
                    let goal = self.entities[id].ai_goal_position();
                    self.entities[rider].ai.command.desired_speed = rappel_speed;
                    self.native_rappel_from(rider, target, goal, COMMAND_FROM_AI);
                }
                rope.rappellers.push(rider);
                let delay =
                    delay[0].wrapping_add(self.random_between(delay[1].wrapping_sub(delay[0])));
                rope.next_drop = now.wrapping_add(delay);
            }
            if !rope.rappellers.is_empty() {
                used += 1;
            }
        }
        self.entities[id].chinook.as_mut().unwrap().ropes = ropes;
        if used == 0 && self.potential_rappeller(id).is_none() {
            MoveResult::Success
        } else {
            MoveResult::Continue
        }
    }

    fn potential_rappeller(&self, id: usize) -> Option<usize> {
        self.passengers(id)
            .iter()
            .copied()
            .find(|&rider| self.attack_kind(rider, "CAN_RAPPEL"))
    }

    pub(in crate::skirmish::chinook) fn exit_chinook_combat_drop(&mut self, id: usize) {
        self.clear_disabled_held(id);
        self.entities[id].chinook.as_mut().unwrap().flight = Flight::Flying;
        let ropes = std::mem::take(&mut self.entities[id].chinook.as_mut().unwrap().ropes);
        if self.entities[id].is_effectively_dead() {
            for rope in &ropes {
                for &rider in &rope.rappellers {
                    if self
                        .entities
                        .get(rider)
                        .is_some_and(|actor| actor.exists() && actor.ai.registration.is_some())
                    {
                        self.native_idle_from(rider, COMMAND_FROM_AI);
                    }
                }
            }
        }
        let expires = self.update_frame().wrapping_add(150);
        let maximum_speed = self.catalog[self.entities[id].type_index]
            .chinook
            .as_ref()
            .unwrap()
            .drop
            .drop_speed;
        for rope in ropes {
            if let Some(drawable) = self
                .rope_drawables
                .drawables
                .iter_mut()
                .find(|draw| draw.id == rope.drawable)
            {
                drawable.released = Some(Release {
                    speed: self.gravity * 30.0,
                    maximum_speed,
                    acceleration: self.gravity,
                    expires,
                });
            }
        }
    }

    fn chinook_bone_world_position(&self, id: usize, local: [f32; 3]) -> [f32; 3] {
        let (position, matrix) = self.object_transform(id);
        std::array::from_fn(|row| {
            let x = matrix[row][0] * local[0];
            let y = matrix[row][1] * local[1];
            let z = matrix[row][2] * local[2];
            x + y + z + position[row]
        })
    }

    fn chinook_bone_world_matrix(&self, id: usize, local: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
        let (position, matrix) = self.object_transform(id);
        std::array::from_fn(|row| {
            std::array::from_fn(|column| {
                let x = matrix[row][0] * local[0][column];
                let y = matrix[row][1] * local[1][column];
                let z = matrix[row][2] * local[2][column];
                let product = x + y + z;
                if column == 3 {
                    product + position[row]
                } else {
                    product
                }
            })
        })
    }
}
