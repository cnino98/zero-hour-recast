//! `ProjectileStreamUpdate`'s original 20-slot ID ring and roof-adjusted draw points.
use super::{Skirmish, projectiles::ProjectileEvent};

const CAPACITY: usize = 20;

#[derive(PartialEq)]
pub(super) enum Target {
    Entity(usize),
    Hazard(u64),
    Position([f32; 3]),
}

pub struct Stream {
    pub source: usize,
    pub weapon_index: usize,
    pub position: [f32; 3],
    projectiles: [Option<u64>; CAPACITY],
    first: usize,
    next: usize,
    target: Option<Target>,
}

impl Stream {
    pub(super) const fn new(source: usize, weapon: usize, position: [f32; 3]) -> Self {
        Self {
            source,
            weapon_index: weapon,
            position,
            projectiles: [None; CAPACITY],
            first: 0,
            next: 0,
            target: None,
        }
    }

    pub(super) fn add(&mut self, id: u64, target: Target) {
        if self.target.as_ref() != Some(&target) {
            self.projectiles[self.next] = None;
            self.next = (self.next + 1) % CAPACITY;
            self.target = Some(target);
        }
        self.projectiles[self.next] = Some(id);
        self.next = (self.next + 1) % CAPACITY;
        debug_assert_ne!(
            self.next, self.first,
            "native projectile stream capacity exceeded"
        );
    }

    pub(super) fn cull(&mut self, exists: impl Fn(u64) -> bool) {
        while self.first != self.next && self.projectiles[self.first].is_none_or(|id| !exists(id)) {
            self.first = (self.first + 1) % CAPACITY;
        }
    }

    fn points(&self, position: impl Fn(u64) -> Option<[f32; 3]>) -> Vec<[f32; 3]> {
        let mut result = Vec::new();
        let mut index = self.first;
        while index != self.next {
            result.push(
                self.projectiles[index]
                    .and_then(&position)
                    .unwrap_or([0.0; 3]),
            );
            index = (index + 1) % CAPACITY;
        }
        result
    }
}

impl Skirmish {
    pub(super) fn record_stream_projectile(
        &mut self,
        projectile: &ProjectileEvent,
        position: [f32; 3],
    ) {
        let unit = &self.catalog[self.entities[projectile.source].type_index];
        if unit
            .weapon_template(projectile.weapon_index)
            .is_none_or(|weapon| weapon.projectile.stream.is_none())
        {
            return;
        }
        let target = projectile.hazard_target.map_or_else(
            || {
                projectile
                    .target
                    .map_or(Target::Position(position), Target::Entity)
            },
            Target::Hazard,
        );
        let source_position = self.object_position(projectile.source);
        let stream = if let Some(index) = self.projectile_streams.iter().position(|stream| {
            stream.source == projectile.source && stream.weapon_index == projectile.weapon_index
        }) {
            &mut self.projectile_streams[index]
        } else {
            self.projectile_streams.push(Stream::new(
                projectile.source,
                projectile.weapon_index,
                source_position,
            ));
            self.projectile_streams.last_mut().expect("new stream")
        };
        stream.position = source_position;
        stream.add(projectile.id, target);
    }

    pub(super) fn update_projectile_streams(&mut self) {
        for stream in &mut self.projectile_streams {
            stream.cull(|id| {
                self.projectiles
                    .iter()
                    .any(|projectile| projectile.id == id)
            });
        }
    }

    #[must_use]
    pub fn projectile_stream_points(&self, stream: &Stream) -> Vec<[f32; 3]> {
        let actor = &self.entities[stream.source];
        let unit = &self.catalog[actor.type_index];
        let origin = self.object_position(stream.source);
        let top = origin[2] + unit.geometry_bounds.max_height + 0.5;
        let roof = unit.kind_of.iter().any(|kind| kind == "VEHICLE");
        stream.points(|id| {
            let projectile = &self.projectiles[self.projectile_index(id)?];
            let (mut point, _) = projectile.effect_transform(&self.entities)?;
            let delta = [point[0] - origin[0], point[1] - origin[1]];
            let x_squared = delta[0] * delta[0];
            let y_squared = delta[1] * delta[1];
            let distance = (x_squared + y_squared).sqrt();
            if roof && distance <= unit.geometry.major * 1.5 {
                point[2] = point[2].max(top);
            }
            Some(point)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Stream, Target};

    #[test]
    fn changed_targets_split_the_stream_and_only_missing_front_entries_are_culled() {
        let mut stream = Stream::new(0, 0, [0.0; 3]);
        stream.add(1, Target::Hazard(8));
        stream.add(2, Target::Hazard(8));
        stream.add(3, Target::Hazard(9));
        stream.add(4, Target::Hazard(9));
        stream.cull(|id| id != 1);
        let points = stream.points(|id| match id {
            2 => Some([2.0, 0.0, 0.0]),
            4 => Some([4.0, 0.0, 0.0]),
            _ => None,
        });
        assert_eq!(
            points,
            vec![[2.0, 0.0, 0.0], [0.0; 3], [0.0; 3], [4.0, 0.0, 0.0]]
        );
        stream.cull(|id| id == 4);
        assert_eq!(
            stream.points(|_| Some([4.0, 0.0, 0.0])),
            vec![[4.0, 0.0, 0.0]]
        );
    }
}
