//! Ground-layer `Path::computePointOnPath` and native path-arrival calculations.
//! The full optimized route remains intact; sampling does not consume nodes.

pub(super) struct Node {
    pub position: [f32; 3],
    pub layer: u8,
    /// Immediate predecessor in the source path before optimization; ground for none.
    pub previous_layer: u8,
    pub can_optimize: bool,
}

pub(super) struct Sample {
    pub position: [f32; 3],
    pub distance: f32,
    pub layer: u8,
}

pub(super) struct Path {
    nodes: Vec<Node>,
    segments: Vec<Segment>,
    cache: Option<Cache>,
    countdown: u8,
    recent_start: usize,
}

struct Segment {
    direction: [f32; 2],
    length: f32,
}

struct Cache {
    input: [f32; 3],
    output: Sample,
}

struct Closest {
    segment: Option<usize>,
    position: [f32; 3],
    remaining: f32,
}

impl Path {
    pub(super) fn sampling_details(
        &self,
        position: [f32; 3],
        mut passable: impl FnMut(u8, [f32; 3], [f32; 3]) -> bool,
    ) -> String {
        if self.nodes.is_empty() {
            return "empty retained path".into();
        }
        let cache = self.cache.as_ref().map(|cache| {
            (
                cache.input,
                cache.output.position,
                cache.output.distance,
                cache.output.layer,
                self.countdown,
            )
        });
        let next = self.closest(position).segment.map(|index| {
            let layer = self.selected_layer(Some(index));
            let point = self.nodes[index + 1].position;
            let next_layer = self.nodes[index + 1].layer;
            let middle = self.nodes.get(index + 2).map(|after| {
                let goal = [
                    (point[0] + after.position[0]) * 0.5,
                    (point[1] + after.position[1]) * 0.5,
                    point[2],
                ];
                (next_layer, goal, passable(next_layer, position, goal))
            });
            (layer, point, passable(layer, position, point), middle)
        });
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .map(|node| (node.position, node.layer, node.can_optimize))
            .collect();
        format!(
            "nodes(position,layer,can_optimize)={nodes:?} cache(input,goal,distance,layer,countdown)={cache:?} next(layer,point,passable,midpoint)={next:?}"
        )
    }

    pub(super) fn positions(&self) -> impl Iterator<Item = [f32; 2]> + '_ {
        self.nodes
            .iter()
            .map(|node| [node.position[0], node.position[1]])
    }

    pub(super) fn last_position(&self) -> Option<[f32; 3]> {
        self.nodes.last().map(|node| node.position)
    }

    pub(super) fn append_ground_node(&mut self, position: [f32; 3]) {
        if let Some(last) = self.nodes.last() {
            let direction = delta(position, last.position);
            let length = length_squared(direction).sqrt();
            let length = if length == 0.0 { 0.01 } else { length };
            self.segments.push(Segment {
                direction: [direction[0] / length, direction[1] / length],
                length,
            });
        }
        self.nodes.push(Node {
            position,
            layer: 1,
            previous_layer: self.nodes.last().map_or(1, |node| node.layer),
            can_optimize: true,
        });
    }

    pub(super) fn update_last_position(&mut self, position: [f32; 3], layer: u8) {
        let count = self.nodes.len();
        let Some(last) = self.nodes.last_mut() else {
            return;
        };
        last.position = position;
        last.layer = layer;
        if count >= 2 {
            let direction = delta(position, self.nodes[count - 2].position);
            let length = length_squared(direction).sqrt();
            let length = if length == 0.0 { 0.01 } else { length };
            self.segments[count - 2] = Segment {
                direction: [direction[0] / length, direction[1] / length],
                length,
            };
        }
    }

    pub(super) const fn is_direct(&self) -> bool {
        self.nodes.len() == 2
    }

    pub fn new(nodes: Vec<Node>) -> Self {
        // PathNode::setNextOptimized caches XY length/direction, including its
        // deliberate nonzero length for a segment with coincident endpoints.
        let segments = nodes
            .windows(2)
            .map(|pair| {
                let direction = delta(pair[1].position, pair[0].position);
                let length = length_squared(direction).sqrt();
                let length = if length == 0.0 { 0.01 } else { length };
                Segment {
                    direction: [direction[0] / length, direction[1] / length],
                    length,
                }
            })
            .collect();
        Self {
            nodes,
            segments,
            cache: None,
            countdown: 20,
            recent_start: 0,
        }
    }

    /// The callback is native `isLinePassable` with blocked=false and
    /// allowPinched=true, evaluated from the actor to the proposed goal.
    pub fn sample_layer(
        &mut self,
        position: [f32; 3],
        current_layer: u8,
        mut passable: impl FnMut(u8, [f32; 3], [f32; 3]) -> bool,
    ) -> Sample {
        if self.nodes.is_empty() {
            self.cache = None;
            return Sample {
                position: [0.0; 3],
                distance: 0.0,
                layer: 1,
            };
        }
        if let Some(cache) = &self.cache
            && self.countdown > 0
            && position
                .iter()
                .zip(cache.input)
                .all(|(value, previous)| (value - previous).abs() <= 0.1)
        {
            self.countdown -= 1;
            return Sample {
                position: cache.output.position,
                distance: cache.output.distance,
                layer: cache.output.layer,
            };
        }
        self.countdown = 20;
        let closest = self.closest(position);
        let layer = self.selected_layer(closest.segment);
        let goal = closest.segment.map_or(closest.position, |index| {
            self.movement_goal(index, position, current_layer, layer, &mut passable)
        });
        let mut distance = closest.remaining;
        let direct = length_squared(delta(goal, position)).sqrt();
        // PATHFIND_CLOSE_ENOUGH is 1.0, independent of the locomotor setting.
        if direct > distance && distance > 1.0 {
            distance = direct;
        }
        self.cache = Some(Cache {
            input: position,
            output: Sample {
                position: goal,
                distance,
                layer,
            },
        });
        Sample {
            position: goal,
            distance,
            layer,
        }
    }

    /// Ground branch of `AIUpdateInterface::getLocomotorDistanceToGoal`.
    pub fn distance_to_goal(&self, position: [f32; 3], sample: &Sample) -> f32 {
        let goal = self
            .nodes
            .last()
            .map_or(sample.position, |node| node.position);
        let direct_squared = length_squared(delta(goal, position));
        if sample.distance < 10.0 || sample.distance * sample.distance < direct_squared {
            direct_squared.sqrt()
        } else {
            sample.distance
        }
    }

    /// `Path::computeFlightDistToGoal`, also used for ground waypoint arrival.
    pub fn flight_sample(&mut self, position: [f32; 3]) -> Sample {
        let Some(start) = self.nodes.get(self.recent_start) else {
            return Sample {
                position: [0.0; 3],
                distance: 0.0,
                layer: 1,
            };
        };
        let mut goal = start.position;
        let mut distance = 0.0;
        let mut use_next = true;
        for index in self.recent_start..self.segments.len() {
            let end = self.nodes[index + 1].position;
            if use_next {
                goal = end;
            }
            let projection = dot(delta(end, position), self.segments[index].direction);
            if projection >= 0.0 {
                distance += projection;
                use_next = false;
            } else if use_next {
                self.recent_start = index + 1;
            }
        }
        Sample {
            position: goal,
            distance,
            layer: start.layer,
        }
    }

    /// Native `getTreatAsAircraftForLocoDistToGoal` arrival correction.
    /// Ground locomotion still uses the ordinary obstacle-aware sampler.
    pub fn waypoint_distance_to_goal(&mut self, position: [f32; 3]) -> f32 {
        let sample = self.flight_sample(position);
        let goal = self
            .nodes
            .last()
            .map_or(sample.position, |node| node.position);
        let direct_squared = length_squared(delta(goal, position));
        if sample.distance * sample.distance > direct_squared {
            direct_squared.sqrt()
        } else {
            sample.distance
        }
    }

    /// `AIInternalMoveToState::update` uses strict arrival and a 40-unit ground
    /// sanity check. The caller supplies the selected locomotor's distance.
    pub fn arrived(&self, position: [f32; 3], distance: f32, close_enough: f32) -> bool {
        distance < close_enough
            && self
                .nodes
                .last()
                .is_some_and(|node| length_squared(delta(position, node.position)).sqrt() <= 40.0)
    }

    fn selected_layer(&self, segment: Option<usize>) -> u8 {
        let mut layer = self.nodes[0].layer;
        if let Some(index) = segment {
            if self.nodes[index].previous_layer > 1 {
                layer = self.nodes[index].layer;
            }
            if self.nodes[index].layer > 1 {
                layer = self.nodes[index].layer;
            }
            if self.nodes[index + 1].layer > 1 {
                layer = self.nodes[index + 1].layer;
            }
        }
        layer
    }

    fn closest(&self, position: [f32; 3]) -> Closest {
        let mut closest = Closest {
            segment: None,
            position: self
                .nodes
                .last()
                .expect("A nonempty path is sampled")
                .position,
            remaining: 0.0,
        };
        // The source literal 99999999.9 rounds to this Real (f32) value.
        let mut closest_squared = 100_000_000.0_f32;
        let mut total_length = 0.0;
        let mut length_to_position = 0.0;
        for (index, segment) in self.segments.iter().enumerate() {
            let start = self.nodes[index].position;
            let mut along = dot(segment.direction, delta(position, start));
            let point = if along < 0.0 {
                along = 0.0;
                start
            } else if along > segment.length {
                if index + 1 < self.segments.len() {
                    total_length += segment.length;
                    continue;
                }
                along = segment.length;
                self.nodes[index + 1].position
            } else {
                let mut point = projected(start, segment.direction, along);
                point[2] = 0.0;
                point
            };
            let squared = length_squared(delta(position, point));
            if squared < closest_squared {
                closest_squared = squared;
                closest.segment = Some(index);
                closest.position = point;
                length_to_position = total_length + along;
            }
            total_length += segment.length;
        }
        closest.remaining = total_length - length_to_position;
        closest
    }

    fn movement_goal(
        &self,
        index: usize,
        position: [f32; 3],
        current_layer: u8,
        selected_layer: u8,
        passable: &mut impl FnMut(u8, [f32; 3], [f32; 3]) -> bool,
    ) -> [f32; 3] {
        let segment = &self.segments[index];
        let start = self.nodes[index].position;
        let next = &self.nodes[index + 1];
        let to_position = delta(position, start);
        let along = dot(segment.direction, to_position).max(0.0);
        let offset_squared = length_squared(to_position) - along * along;
        let offset = if offset_squared <= 0.0 {
            0.0
        } else {
            offset_squared.sqrt()
        };
        let mut weight = (offset * (1.0 / 30.0)).min(1.0);
        if passable(selected_layer, position, next.position) {
            let very_close = segment.length - along < 1.0;
            let try_ahead = (along > segment.length * 0.5
                && next.can_optimize
                && self.nodes[index].layer == next.layer
                && current_layer == 1)
                || very_close;
            if try_ahead && let Some(after) = self.nodes.get(index + 2) {
                let middle = [
                    (next.position[0] + after.position[0]) * 0.5,
                    (next.position[1] + after.position[1]) * 0.5,
                    next.position[2],
                ];
                if very_close || passable(next.layer, position, middle) {
                    return middle;
                }
            }
            return next.position;
        }
        if weight > 0.5 {
            let half_remaining = 0.5 * (segment.length - along);
            let halfway = projected(start, segment.direction, along + half_remaining);
            if passable(selected_layer, position, halfway) {
                return halfway;
            }
        }
        // Preserve the source weighted fallback when no proposed line succeeds.
        // The successful half-way branch returns directly instead of assigning
        // k=0.5 and running the source's unused final interpolation.
        weight = 1.0 - weight;
        let remaining = weight * (segment.length - along);
        let along = along + remaining;
        if along > segment.length {
            return next.position;
        }
        let goal = projected(start, segment.direction, along);
        if (position[0] - goal[0]).abs() < 1.0
            && (position[1] - goal[1]).abs() < 1.0
            && let Some(after) = self.nodes.get(index + 2)
        {
            return after.position;
        }
        goal
    }
}

fn delta(first: [f32; 3], second: [f32; 3]) -> [f32; 2] {
    [first[0] - second[0], first[1] - second[1]]
}

fn length_squared(value: [f32; 2]) -> f32 {
    dot(value, value)
}

fn dot(first: [f32; 2], second: [f32; 2]) -> f32 {
    let products = [first[0] * second[0], first[1] * second[1]];
    products[0] + products[1]
}

fn projected(start: [f32; 3], direction: [f32; 2], distance: f32) -> [f32; 3] {
    let movement = [distance * direction[0], distance * direction[1]];
    [start[0] + movement[0], start[1] + movement[1], start[2]]
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ground_height_tests;
