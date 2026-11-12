//! Visible source poses, without extending an instant kill's firing state.
use bevy::{mesh::VertexAttributeValues, prelude::*};
use std::collections::BTreeMap;

pub(super) struct Pose(BTreeMap<Entity, Vec<[u32; 3]>>);

impl Pose {
    pub(super) fn from_meshes<'a>(
        meshes: impl Iterator<Item = (Entity, &'a Mesh)>,
    ) -> Option<Self> {
        let parts: BTreeMap<_, _> = meshes
            .filter_map(|(entity, mesh)| {
                let VertexAttributeValues::Float32x3(points) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
                else {
                    return None;
                };
                (!points.is_empty() && points.iter().flatten().all(|value| value.is_finite())).then(
                    || {
                        (
                            entity,
                            points.iter().map(|point| point.map(f32::to_bits)).collect(),
                        )
                    },
                )
            })
            .collect();
        (!parts.is_empty()).then_some(Self(parts))
    }

    fn changed_from(&self, previous: &Self) -> bool {
        // A newly visible muzzle alone is not body animation. Require changed
        // vertices on the same previously visible, stable animated part.
        self.0.iter().any(|(entity, points)| {
            previous
                .0
                .get(entity)
                .is_some_and(|before| points.len() == before.len() && points != before)
        })
    }
}

pub(super) struct Proof {
    pose: Option<Pose>,
    movement_frame: Option<u32>,
    pub(super) movement: bool,
    pub(super) shot: bool,
}

impl Proof {
    pub(super) const fn new() -> Self {
        Self {
            pose: None,
            movement_frame: None,
            movement: false,
            shot: false,
        }
    }

    pub(super) fn observe(
        &mut self,
        clip: &str,
        frame: f32,
        pose: Pose,
        moving: bool,
        firing: bool,
    ) {
        let changed = self
            .pose
            .as_ref()
            .is_some_and(|before| pose.changed_from(before));
        let movement = moving && clip == "uihero_skl.uihero_rna2";
        self.movement |= movement
            && changed
            && self
                .movement_frame
                .is_some_and(|before| before != frame.to_bits());
        self.shot |= firing && clip == "uihero_skl.uihero_ata" && changed;
        self.movement_frame = movement.then_some(frame.to_bits());
        self.pose = Some(pose);
    }

    pub(super) const fn complete(&self) -> bool {
        self.movement && self.shot
    }
}
