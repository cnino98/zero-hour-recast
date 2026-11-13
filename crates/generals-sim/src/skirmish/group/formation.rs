//! `AIGroup::groupCreateFormation`, `recompute`, `friend_moveFormationToPos`.
use super::super::{Skirmish, locomotor};
use super::distance_squared;

impl Skirmish {
    /// Toggle the original formation offsets for a selection.
    ///
    /// # Errors
    /// Rejects an empty selection or unavailable actors.
    pub fn group_create_formation(&mut self, ids: &[usize]) -> Result<(), &'static str> {
        let mut members = Vec::new();
        for &id in ids {
            let actor = self
                .entities
                .get(id)
                .filter(|actor| actor.complete())
                .ok_or("Object is dead, unfinished or missing")?;
            if actor.contained_by().is_none() {
                self.ready_actor(id)?;
            }
            if !members.contains(&id) {
                members.push(id);
            }
        }
        if members.is_empty() || members.len() > usize::from(u16::MAX) {
            return Err("Selection has no formation members");
        }
        let active: Vec<_> = members
            .iter()
            .copied()
            .filter(|&id| {
                self.entities[id].contained_by().is_none()
                    && !matches!(
                        self.catalog[self.entities[id].type_index].ai.update,
                        crate::skirmish::UnitAiUpdate::Absent
                    )
            })
            .collect();
        let id = self.next_formation_id;
        self.next_formation_id = self.next_formation_id.wrapping_add(1);
        if active.is_empty() {
            return Ok(());
        }
        let (center, _, _, _) = self.group_bounds(&active);
        // The pinned getMinMaxAndCenter intentionally retains the first ID;
        // its subsequent-member branch never compares against the other ID.
        let remove = if members.len() == 1 {
            self.entities[members[0]].ai.command.formation_id.is_some()
        } else {
            active.len() > 1 && self.entities[active[0]].ai.command.formation_id.is_some()
        };
        for member in members {
            if matches!(
                self.catalog[self.entities[member].type_index].ai.update,
                crate::skirmish::UnitAiUpdate::Absent
            ) {
                continue;
            }
            let actor = &mut self.entities[member];
            actor.ai.command.formation_id = (!remove).then_some(id);
            actor.ai.command.formation_offset =
                [actor.position[0] - center[0], actor.position[1] - center[1]];
        }
        Ok(())
    }

    pub(super) fn update_formation_speed(&mut self, members: &[usize]) {
        let speed = self.group_travel_speed(members);
        for &id in members {
            self.entities[id].ai.command.formation_speed =
                self.entities[id].ai.command.formation_id.map(|_| speed);
        }
    }

    pub(in crate::skirmish) fn group_travel_speed(&self, members: &[usize]) -> f32 {
        let mut speed = 10_000_000_000.0_f32;
        for &id in members {
            let actor = &self.entities[id];
            let unit = &self.catalog[actor.type_index];
            if unit.speed > 0.0
                && !matches!(unit.ai.update, crate::skirmish::UnitAiUpdate::Absent)
                && actor.contained_by().is_none()
                && actor.health / actor.max_health
                    > actor.active_locomotor(unit).damage_health_threshold
            {
                speed = speed.min(locomotor::speed(actor, unit));
            }
        }
        speed
    }

    pub(super) fn move_formation(
        &mut self,
        members: &[usize],
        destination: [f32; 3],
        route: Option<&[[f32; 3]]>,
        source: i32,
    ) -> Result<(), &'static str> {
        let anchors = route.and_then(|route| {
            let first = route.iter().position(|&point| {
                distance_squared([route[0][0], route[0][1]], [point[0], point[1]]) > 3600.0
            })?;
            let last = route.iter().rposition(|&point| {
                distance_squared(
                    [route[route.len() - 1][0], route[route.len() - 1][1]],
                    [point[0], point[1]],
                ) > 3600.0
            })?;
            (last > first).then_some((first, last))
        });
        let end = route
            .and_then(|route| route.last())
            .copied()
            .unwrap_or(destination);
        for &id in members {
            self.remove_goal(id);
            let offset = self.entities[id].ai.command.formation_offset;
            let wanted = [end[0] + offset[0], end[1] + offset[1], end[2]];
            if let (Some(route), Some((first, last))) = (route, anchors) {
                let mut points = Vec::new();
                // Native includes both anchor nodes in the offset path.
                for (index, point) in route.iter().enumerate().skip(first) {
                    points.push([point[0] + offset[0], point[1] + offset[1], point[2]]);
                    if index == last {
                        break;
                    }
                }
                let goal = self
                    .adjusted_destination_world(id, wanted)
                    .unwrap_or(wanted);
                points.push(goal);
                self.reserve_goal_on(id, [goal[0], goal[1]], 1);
                self.native_follow_path_from(id, points, None, source, false);
            } else {
                self.move_group_member_from(id, wanted, source)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
