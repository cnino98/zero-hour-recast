//! `W3DOverlordTankDraw`/`W3DDependencyModelDraw` ordered original bone transform.
use super::{Animations, Players};
use crate::{playtest::Match, playtest_view::UnitVisual};
use bevy::prelude::*;
use std::collections::BTreeMap;

type ReadVisual = (
    &'static UnitVisual,
    &'static Players,
    &'static Transform,
    &'static Visibility,
    Option<&'static crate::light_environment::DrawPose>,
);
type WriteVisual = (
    &'static UnitVisual,
    &'static mut Transform,
    &'static mut Visibility,
    Option<&'static mut crate::light_environment::DrawPose>,
);

pub fn update(
    battle: Res<Match>,
    animations: Res<Animations>,
    mut visuals: ParamSet<(Query<ReadVisual>, Query<WriteVisual>)>,
) {
    let battle = battle.into_inner();
    let animations = animations.into_inner();
    if !battle
        .simulation
        .entities
        .iter()
        .any(|actor| actor.alive() && actor.attachment_child().is_some())
    {
        return;
    }
    let mut attachments = BTreeMap::new();
    for (visual, players, transform, visibility, native) in &visuals.p0() {
        if visual.generation != battle.generation {
            continue;
        }
        let actor = &battle.simulation.entities[visual.index];
        let Some(child) = actor.attachment_child() else {
            continue;
        };
        let unit = &battle.simulation.catalog[battle.simulation.entities[child].type_index];
        let Some(name) = &unit.attachment_bone else {
            continue;
        };
        let mut pose = *transform;
        let mut source_pose = native.copied();
        // Source fallback is the parent drawable's transform when its animated
        // bone is absent. Client bones never modify authoritative logic placement.
        if let Some(bone) = animations
            .effect_bones(actor, players, name)
            .expect("validated attachment bones")
            .first()
        {
            let source = |point: [f32; 3]| Vec3::new(point[0], point[2], -point[1]);
            let axes = Mat3::from_cols(
                source(bone.axes[0]),
                source(bone.axes[2]),
                -source(bone.axes[1]),
            );
            pose.translation = transform.transform_point(source(bone.translation) * 0.1);
            pose.rotation = transform.rotation * Quat::from_mat3(&axes);
            source_pose = source_pose.map(|parent| {
                parent.compose(crate::light_environment::DrawPose {
                    position: bone.translation,
                    basis: std::array::from_fn(|row| {
                        std::array::from_fn(|column| bone.axes[column][row])
                    }),
                })
            });
        }
        attachments.insert(child, (pose, *visibility, source_pose));
    }
    for (visual, mut transform, mut visibility, native) in &mut visuals.p1() {
        if visual.generation == battle.generation
            && let Some((pose, shown, source_pose)) = attachments.get(&visual.index)
        {
            transform.set_if_neq(*pose);
            visibility.set_if_neq(*shown);
            if let (Some(mut native), Some(source)) = (native, source_pose) {
                *native = *source;
            }
        }
    }
}
