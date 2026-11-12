use crate::skirmish::{Order, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn container_attack_in_range(&self, id: usize, target: usize) -> bool {
        if self.entities[id].contained_by().is_none() {
            return true;
        }
        if self.entities[id].garrisoned() && !self.entities[id].stationed() {
            self.garrison_attack_position(id, target)
                .is_some_and(|point| self.attack_range_from(id, target, [point[0], point[1]]))
        } else {
            self.within_attack_range(id, target)
        }
    }
    fn garrison_point_index(&self, id: usize, carrier: usize) -> Option<usize> {
        self.entities[carrier]
            .containment
            .garrison
            .as_ref()?
            .points
            .iter()
            .position(|&other| other == Some(id))
    }

    fn closest_garrison_point(&self, carrier: usize, goal: [f32; 3]) -> Option<(usize, [f32; 3])> {
        let state = self.entities[carrier].containment.garrison.as_ref()?;
        let art = self.art(carrier)?;
        let mut closest = None;
        let mut distance = f32::MAX;
        for (index, owner) in state.points.iter().enumerate() {
            if owner.is_some() {
                continue;
            }
            let point = self.container_point(
                carrier,
                art.fire_points
                    .get(index)
                    .map_or([0.0; 3], |point| point.position),
            );
            let candidate = distance_squared(point, goal);
            if candidate < distance {
                distance = candidate;
                closest = Some((index, point));
            }
        }
        closest
    }

    /// Non-mutating `calcBestGarrisonPosition`: the current occupied point is not freed.
    #[must_use]
    pub fn garrison_attack_position(&self, id: usize, target: usize) -> Option<[f32; 3]> {
        self.entities.get(target)?;
        if !self.entities.get(id)?.garrisoned() || self.entities[id].stationed() {
            return None;
        }
        let carrier = self.entities[id].contained_by()?;
        self.closest_garrison_point(carrier, self.object_position(target))
            .map(|(_, point)| point)
    }

    pub(in crate::skirmish) fn garrison_attack_position_at(
        &self,
        id: usize,
        position: [f32; 3],
    ) -> Option<[f32; 3]> {
        let actor = self.entities.get(id)?;
        if !actor.garrisoned() || actor.stationed() {
            return None;
        }
        self.closest_garrison_point(actor.contained_by()?, position)
            .map(|(_, point)| point)
    }

    pub(in crate::skirmish) fn remove_garrison_point(&mut self, id: usize, carrier: usize) {
        let stationed = self.entities[id].stationed();
        if let Some(index) = self.garrison_point_index(id, carrier) {
            let state = self.entities[carrier]
                .containment
                .garrison
                .as_mut()
                .expect("garrison point state");
            state.points[index] = None;
            state.placed[index] = 0;
            state.facing[index] = 0.0;
            state.flash[index] = None;
            if !stationed {
                self.center_garrison_rider(id, carrier);
            }
        }
        self.entities[id].containment.firing.aim_target = None;
    }

    fn put_at_garrison_point(&mut self, id: usize, carrier: usize, index: usize, point: [f32; 3]) {
        let state = self.entities[carrier]
            .containment
            .garrison
            .as_mut()
            .expect("garrison point state");
        state.points[index] = Some(id);
        state.placed[index] = self.tick;
        self.entities[id].position = [point[0], point[1]];
        self.entities[id].containment.height = point[2];
    }

    pub(in crate::skirmish) fn prepare_garrison_attack(
        &mut self,
        id: usize,
        target: usize,
    ) -> bool {
        if !self.entities[id].garrisoned() || self.entities[id].stationed() {
            return true;
        }
        let carrier = self.entities[id].contained_by().expect("garrisoned rider");
        // Native aim-state entry occurs before each shot. Aiming frames keep the
        // assigned point until the fire state transitions back into aim.
        if self.entities[id].containment.firing.aim_target == Some(target) {
            return self.within_attack_range(id, target);
        }
        self.remove_garrison_point(id, carrier);
        if let Some((index, point)) =
            self.closest_garrison_point(carrier, self.object_position(target))
        {
            self.put_at_garrison_point(id, carrier, index, point);
        }
        if self.within_attack_range(id, target) {
            self.entities[id].containment.firing.aim_target = Some(target);
            true
        } else {
            self.remove_garrison_point(id, carrier);
            false
        }
    }

    pub(in crate::skirmish) fn reset_garrison_aim(&mut self, id: usize) {
        self.entities[id].containment.firing.aim_target = None;
    }

    pub(super) fn match_garrison_points(&mut self, carrier: usize) {
        if self.station_garrison(carrier) {
            self.position_station_passengers(carrier);
            return;
        }
        let Some(state) = &self.entities[carrier].containment.garrison else {
            return;
        };
        let points = state.points;
        for id in points.into_iter().flatten() {
            if !matches!(self.entities[id].ai.order, Order::Attack(target) if self.within_attack_range(id, target))
            {
                self.remove_garrison_point(id, carrier);
            }
        }
        for index in 0..self.passengers(carrier).len() {
            let id = self.passengers(carrier)[index];
            if let Order::Attack(target) = self.entities[id].ai.order
                && self.garrison_point_index(id, carrier).is_none()
                && let Some((point_index, point)) =
                    self.closest_garrison_point(carrier, self.object_position(target))
            {
                self.put_at_garrison_point(id, carrier, point_index, point);
            }
        }
        self.update_garrison_effects(carrier);
        self.track_garrison_targets(carrier);
    }

    fn track_garrison_targets(&mut self, carrier: usize) {
        for index in 0..self.passengers(carrier).len() {
            let id = self.passengers(carrier)[index];
            let Order::Attack(target) = self.entities[id].ai.order else {
                continue;
            };
            let Some(previous) = self.garrison_point_index(id, carrier) else {
                continue;
            };
            let goal = self.object_position(target);
            if let Some((point_index, point)) = self.closest_garrison_point(carrier, goal)
                && distance_squared(point, goal) < distance_squared(self.object_position(id), goal)
            {
                self.remove_garrison_point(id, carrier);
                self.put_at_garrison_point(id, carrier, point_index, point);
            }
            let position = self.entities[id].position;
            let state = self.entities[carrier]
                .containment
                .garrison
                .as_mut()
                .expect("garrison point state");
            // Native trackTargets keeps ourIndex after relocating. The old
            // drawable is gone, so the new gun first tracks on the next update.
            if state.points[previous].is_some() {
                state.facing[previous] = (goal[1] - position[1]).atan2(goal[0] - position[0]);
            }
        }
    }

    fn update_garrison_effects(&mut self, carrier: usize) {
        for index in 0..40 {
            let Some(id) = self.entities[carrier]
                .containment
                .garrison
                .as_ref()
                .and_then(|state| state.points[index])
            else {
                continue;
            };
            let actor = &self.entities[id];
            let fired = actor.weapon_set.weapons.iter().any(|weapon| {
                weapon
                    .last_shot
                    .is_some_and(|frame| self.tick.checked_sub(1) == Some(frame))
            });
            let state = self.entities[carrier]
                .containment
                .garrison
                .as_mut()
                .expect("garrison point state");
            if fired {
                state.flash[index] = Some(self.tick);
            }
            if state.flash[index].is_some_and(|frame| self.tick - frame > 30 / 7) {
                state.flash[index] = None;
            }
        }
    }

    /// Placement frame and original four-frame muzzle flash for `GarrisonGun`.
    #[must_use]
    pub fn garrison_effect(&self, carrier: usize, index: usize) -> Option<(u64, bool)> {
        let state = self.entities.get(carrier)?.containment.garrison.as_ref()?;
        if self.station_garrison(carrier) {
            return None;
        }
        state.points.get(index).copied().flatten()?;
        Some((state.placed[index], state.flash[index].is_some()))
    }

    /// Original gun-drawable orientation, including one-update relocation delay.
    #[must_use]
    pub fn garrison_effect_facing(&self, carrier: usize, index: usize) -> Option<f32> {
        let state = self.entities.get(carrier)?.containment.garrison.as_ref()?;
        if self.station_garrison(carrier) {
            return None;
        }
        state.points.get(index).copied().flatten()?;
        Some(state.facing[index])
    }

    /// Original point order, for the `GarrisonGun` presentation drawables.
    #[must_use]
    pub fn garrison_points(&self, carrier: usize) -> Option<&[Option<usize>; 40]> {
        self.entities.get(carrier)?;
        if self.station_garrison(carrier) {
            return None;
        }
        Some(
            &self
                .entities
                .get(carrier)?
                .containment
                .garrison
                .as_ref()?
                .points,
        )
    }
}

fn distance_squared(left: [f32; 3], right: [f32; 3]) -> f32 {
    let x = left[0] - right[0];
    let y = left[1] - right[1];
    let z = left[2] - right[2];
    let horizontal = x * x;
    let vertical = y * y;
    let depth = z * z;
    let planar = horizontal + vertical;
    planar + depth
}
