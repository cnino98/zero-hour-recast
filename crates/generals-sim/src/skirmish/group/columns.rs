//! `AIGroup::friend_moveInfantryToPos` and `friend_moveVehicleToPos`.
//! Native three/five infantry and two/three vehicle column assignment, priority
//! sorting, corner offsets, final row spacing and directional waypoint trimming.

use super::super::movement::distance_squared;
use super::super::{CELL_SIZE, Skirmish};

struct Member {
    id: usize,
    narrow: i32,
    wide: i32,
}

impl Member {
    fn set_columns(&mut self, index: i32, count: i32, infantry: bool) {
        if infantry {
            self.narrow = (1 - index / ((count + 1) / 3).max(1)).max(-1);
            self.wide = (2 - index / ((count + 3) / 5).max(1)).max(-2);
        } else {
            self.narrow = 1 - index / ((count + 1) / 2).max(1);
            if self.narrow == 0 {
                self.narrow = -1;
            }
            self.wide = (1 - index / ((count + 1) / 3).max(1)).max(-1);
        }
        if count < if infantry { 16 } else { 5 } {
            self.wide = self.narrow;
        }
    }
}

impl Skirmish {
    pub(super) fn move_columns(
        &mut self,
        ids: &[usize],
        center: [f32; 2],
        destination: [f32; 3],
        route: &[[f32; 3]],
        infantry: bool,
        source: i32,
    ) -> bool {
        let Some((start_node, end_node)) = column_anchors(route, infantry) else {
            return false;
        };
        let mut members: Vec<_> = ids
            .iter()
            .copied()
            .filter(|&id| {
                let actor = &self.entities[id];
                actor.ai.registration.is_some()
                    && !actor.disabled_held()
                    && if infantry {
                        self.attack_kind(id, "INFANTRY")
                    } else {
                        self.attack_kind(id, "VEHICLE") && self.attack_ground_movement(id)
                    }
            })
            .map(|id| Member {
                id,
                narrow: 0,
                wide: 0,
            })
            .collect();
        // AIGroup.cpp801: a nexus needs a distant goal for its mobsters;
        // it must not enter the infantry column path producer.
        if infantry
            && members.iter().any(|member| {
                let actor = &self.entities[member.id];
                !actor.disabled_held()
                    && actor.ai.registration.is_some()
                    && self.catalog[actor.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "MOB_NEXUS")
            })
        {
            return false;
        }
        let minimum = if infantry {
            self.ai_rules.min_infantry_for_group
        } else {
            self.ai_rules.min_vehicles_for_group
        };
        if members.len() < usize::try_from(minimum).unwrap_or(usize::MAX) {
            return false;
        }
        let count = i32::try_from(members.len()).expect("Group member count checked by caller");
        let end = route[route.len() - 1];
        let end_vector = direction(xy(route[end_node]), xy(end));
        let start_vector = if members.iter().any(|member| {
            let position = self.entities[member.id].position;
            distance_squared(position, xy(route[0])) > distance_squared(position, xy(end))
        }) {
            end_vector
        } else {
            direction(xy(route[0]), xy(route[start_node]))
        };
        let normal = direction([0.0, 0.0], [-start_vector[1], start_vector[0]]);
        // SimpleObjectIterator::insert prepends and its mergesort is stable.
        members.reverse();
        members.sort_by(|a, b| {
            dot_position(self.entities[b.id].position, center, normal).total_cmp(&dot_position(
                self.entities[a.id].position,
                center,
                normal,
            ))
        });
        for (index, member) in members.iter_mut().enumerate() {
            let index = i32::try_from(index).expect("Group member count checked by caller");
            member.set_columns(index, count, infantry);
        }
        // The second iterator is built by prepending the first sorted list.
        members.reverse();
        members.sort_by(|a, b| {
            let priority = |id: usize| {
                if infantry {
                    (self.entities[id]
                        .active_locomotor(&self.catalog[self.entities[id].type_index])
                        .move_priority
                        - 2)
                        * 1000
                } else {
                    0
                }
            };
            let score = |id: usize| {
                let point = self.entities[id].position;
                let x = (point[0] - center[0]) * start_vector[0];
                let y = (point[1] - center[1]) * start_vector[1];
                (small_real(priority(id)) + x) + y
            };
            score(b.id).total_cmp(&score(a.id))
        });
        self.balance_columns(&mut members, infantry);
        self.assign_column_paths(
            &members,
            destination,
            route,
            (start_node, end_vector),
            infantry,
            source,
        );
        true
    }

    fn balance_columns(&self, members: &mut [Member], infantry: bool) {
        let mut narrow_counts = [0; 3];
        let mut wide_counts = [0; 5];
        let count = members.len();
        for priority in (0..=2).rev() {
            for member in &mut *members {
                if infantry
                    && self.entities[member.id]
                        .active_locomotor(&self.catalog[self.entities[member.id].type_index])
                        .move_priority
                        != priority
                {
                    continue;
                }
                // Source vehicle priority checks are #if 0: all three passes
                // intentionally visit every vehicle and retain their counts.
                member.narrow = balance(
                    &mut narrow_counts,
                    member.narrow,
                    1,
                    if infantry { 1 } else { 2 },
                );
                member.wide = if infantry {
                    balance(&mut wide_counts, member.wide, 2, 1)
                } else {
                    balance(&mut wide_counts[..3], member.wide, 1, 1)
                };
                if count < if infantry { 16 } else { 5 } {
                    member.wide = member.narrow;
                }
            }
        }
    }

    fn assign_column_paths(
        &mut self,
        members: &[Member],
        destination: [f32; 3],
        route: &[[f32; 3]],
        (start_node, end_vector): (usize, [f32; 2]),
        infantry: bool,
        source: i32,
    ) {
        let mut factors = [0_u16; 5];
        let normal = direction([0.0, 0.0], [-end_vector[1], end_vector[0]]);
        for member in members {
            let index =
                usize::try_from(member.wide + 2).expect("Native column index is nonnegative");
            let factor = factors[index];
            factors[index] += 1;
            let mut path = self.column_corner_path(
                route,
                start_node,
                self.entities[member.id].position,
                member.narrow,
                factor,
                infantry,
            );
            let offset = if infantry {
                22.0
            } else if members.len() < 5 {
                15.0
            } else {
                32.0
            };
            let side = offset * small_real(member.wide);
            let priority = if infantry {
                small_real(
                    self.entities[member.id]
                        .active_locomotor(&self.catalog[self.entities[member.id].type_index])
                        .move_priority
                        - 2,
                ) * CELL_SIZE
            } else {
                0.0
            };
            let back = f32::from(factor) * offset;
            let mut goal = destination;
            for axis in 0..2 {
                let side_delta = side * normal[axis];
                goal[axis] += side_delta;
                if factor & 1 == 1 {
                    let stagger = CELL_SIZE * normal[axis];
                    goal[axis] += stagger;
                }
                let priority_delta = priority * end_vector[axis];
                goal[axis] += priority_delta;
                let backward = back * end_vector[axis];
                goal[axis] -= backward;
            }
            while path
                .last()
                .is_some_and(|&point| dot_position(xy(goal), xy(point), end_vector) <= 0.0)
            {
                path.pop();
            }
            let clamped = self.clamp_group_point(xy(goal), CELL_SIZE);
            goal = [clamped[0], clamped[1], goal[2]];
            let Some(adjusted) = self.adjusted_destination_world(member.id, goal) else {
                continue;
            };
            let goal = adjusted;
            path.push(goal);
            self.native_follow_path_from(member.id, path, None, source, false);
            self.reserve_goal(member.id, xy(goal));
        }
    }

    fn column_corner_path(
        &self,
        route: &[[f32; 3]],
        first: usize,
        mut previous_position: [f32; 2],
        column: i32,
        factor: u16,
        infantry: bool,
    ) -> Vec<[f32; 3]> {
        let mut result = Vec::new();
        let mut previous = 0;
        for node in first..route.len() {
            let Some(next) = (node + 1..route.len())
                .find(|&index| distance_squared(xy(route[node]), xy(route[index])) > 3600.0)
            else {
                break;
            };
            let vector = [
                route[next][0] - route[previous][0],
                route[next][1] - route[previous][1],
            ];
            let normal = direction([0.0, 0.0], [-vector[1], vector[0]]);
            let offset = if infantry { 21.0 } else { 15.0 };
            let side = offset * small_real(column);
            let mut point = route[node];
            for axis in 0..2 {
                let delta = side * normal[axis];
                point[axis] += delta;
                let stagger = 5.0 * normal[axis];
                if factor & 1 == 1 {
                    point[axis] += stagger;
                } else {
                    point[axis] -= stagger;
                }
            }
            // Native computes curVector before clamping, but saves the clamped
            // point as prevPos for the following corner.
            let forward = dot_position(xy(point), previous_position, vector);
            let clamped = self.clamp_group_point(xy(point), CELL_SIZE);
            let point = [clamped[0], clamped[1], point[2]];
            if forward > 0.0 {
                result.push(point);
                previous_position = xy(point);
            }
            if node + 1 < route.len() {
                for index in (previous + 1)..=node {
                    if distance_squared(xy(route[index]), xy(route[node + 1])) > 3600.0 {
                        previous = index;
                    }
                }
            }
        }
        result
    }
}

fn column_anchors(route: &[[f32; 3]], infantry: bool) -> Option<(usize, usize)> {
    let start = *route.first()?;
    let end = *route.last()?;
    let first = route
        .iter()
        .position(|&point| distance_squared(xy(start), xy(point)) > 3600.0)?;
    let last = route
        .iter()
        .rposition(|&point| distance_squared(xy(end), xy(point)) > 3600.0)?;
    (infantry || last != 0).then_some((first, last))
}

pub(super) fn valid_infantry_anchors(route: &[[f32; 3]]) -> bool {
    column_anchors(route, true).is_some()
}

fn balance(counts: &mut [u32], preferred: i32, center: i32, step: usize) -> i32 {
    let minimum = counts
        .iter()
        .step_by(step)
        .copied()
        .min()
        .expect("Native columns are nonempty");
    let index = (0..counts.len())
        .step_by(step)
        .filter(|&index| counts[index] == minimum)
        .min_by_key(|&index| {
            (center + preferred - i32::try_from(index).expect("At most five columns")).abs()
        })
        .expect("A least-filled column exists");
    counts[index] += 1;
    i32::try_from(index).expect("At most five columns") - center
}

pub(super) fn direction(from: [f32; 2], to: [f32; 2]) -> [f32; 2] {
    let delta = [to[0] - from[0], to[1] - from[1]];
    let x_squared = delta[0] * delta[0];
    let y_squared = delta[1] * delta[1];
    let length = (x_squared + y_squared).sqrt();
    if length > 0.0 {
        [delta[0] / length, delta[1] / length]
    } else {
        [0.0, 0.0]
    }
}

fn dot_position(point: [f32; 2], center: [f32; 2], vector: [f32; 2]) -> f32 {
    let x = (point[0] - center[0]) * vector[0];
    let y = (point[1] - center[1]) * vector[1];
    x + y
}

fn small_real(value: i32) -> f32 {
    f32::from(i16::try_from(value).expect("Native column and priority values fit i16"))
}

const fn xy(point: [f32; 3]) -> [f32; 2] {
    [point[0], point[1]]
}

#[cfg(test)]
mod nexus_tests {
    use crate::skirmish::{Faction, Kind, tests};

    #[test]
    fn nexus_excludes_infantry_columns_without_assigning_a_near_corner_goal() {
        let mut game = tests::game();
        let mut nexus = tests::fixture_type(Faction::Gla, Kind::Infantry);
        nexus.kind_of.push("MOB_NEXUS".into());
        let kind = game.catalog.len();
        game.catalog.push(nexus);
        let parent = game.spawn(kind, 0, [355.0, 255.0], true);
        let child = game.spawn(5, 0, [355.0, 265.0], true);
        let kind = game.entities[parent].type_index;
        game.catalog[kind]
            .kind_of
            .extend(["INFANTRY".into(), "MOB_NEXUS".into()]);
        game.catalog[kind].group_kind = crate::skirmish::GroupKind::Infantry;
        let minimum = usize::try_from(game.ai_rules.min_infantry_for_group)
            .unwrap()
            .max(2);
        let mut members = vec![child];
        for index in 1..minimum {
            let spacing = f32::from(u16::try_from(index).unwrap()) * 10.0;
            let rider = game.spawn(5, 0, [355.0, 255.0 + spacing], true);
            members.push(rider);
        }
        let route = [
            [355.0, 255.0, 0.0],
            [455.0, 255.0, 0.0],
            [600.0, 255.0, 0.0],
        ];
        assert!(
            game.move_columns(
                &members,
                [355.0, 255.0],
                [600.0, 255.0, 0.0],
                &route,
                true,
                crate::skirmish::ai_goals::COMMAND_FROM_PLAYER
            ),
            "the same ordinary infantry meet the real column-path prerequisites"
        );
        members.push(parent);
        let before = game.entities[parent].ai_goal_position();
        let result = game.move_columns(
            &members,
            [355.0, 255.0],
            [600.0, 255.0, 0.0],
            &route,
            true,
            crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
        );
        assert!(!result);
        assert_eq!(
            game.entities[parent].ai_goal_position().map(f32::to_bits),
            before.map(f32::to_bits)
        );
    }
}
