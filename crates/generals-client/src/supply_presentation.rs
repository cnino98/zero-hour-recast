//! Original `W3DSupplyDraw::updateDrawModuleSupplyStatus` crate visibility.
use crate::playtest::Match;
use bevy::{ecs::system::EntityCommands, prelude::*};
use generals_assets::w3d::Model;
use std::{collections::BTreeSet, error::Error};

// Each map instance owns the same immutable model-to-supply mapping.
#[derive(Clone)]
pub struct SupplyDraw {
    initial_boxes: u16,
    delete_when_empty: bool,
    total_bones: u16,
    mesh_bones: Vec<Option<u16>>,
}

impl SupplyDraw {
    pub fn visible(&self, mesh: usize, boxes: u32) -> bool {
        !(self.delete_when_empty && boxes == 0)
            && self
                .mesh_bones
                .get(mesh)
                .copied()
                .flatten()
                .is_none_or(|bone| bone <= shown_bones(self.total_bones, self.initial_boxes, boxes))
    }

    pub fn new(
        model: &Model,
        prefix: Option<&str>,
        initial_boxes: u32,
        delete_when_empty: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let initial_boxes = u16::try_from(initial_boxes)?;
        if initial_boxes == 0 {
            return Err("Supply draw has no starting boxes".into());
        }
        // getPristineBonePositions(prefix,1) stops at the first missing suffix.
        let mut total_bones = 0_u16;
        if let Some(prefix) = prefix {
            while let Some(next) = total_bones.checked_add(1) {
                let name = format!("{prefix}{next:02}");
                if !model
                    .bone_names
                    .iter()
                    .any(|bone| bone.eq_ignore_ascii_case(&name))
                {
                    break;
                }
                total_bones = next;
            }
        }
        let mesh_bones = model
            .meshes
            .iter()
            .map(|mesh| {
                let short = mesh.name.rsplit('.').next().unwrap_or(&mesh.name);
                prefix.and_then(|prefix| {
                    (1..=total_bones).find(|ordinal| {
                        short.eq_ignore_ascii_case(&format!("{prefix}{ordinal:02}"))
                    })
                })
            })
            .collect();
        Ok(Self {
            initial_boxes,
            delete_when_empty,
            total_bones,
            mesh_bones,
        })
    }
}

#[derive(Component)]
pub struct SupplyPart {
    source: usize,
    initial_boxes: u16,
    delete_when_empty: bool,
    total_bones: u16,
    bone: Option<u16>,
}

pub fn attach(child: &mut EntityCommands, source: usize, draw: &SupplyDraw, mesh: usize) {
    child.insert(SupplyPart {
        source,
        initial_boxes: draw.initial_boxes,
        delete_when_empty: draw.delete_when_empty,
        total_bones: draw.total_bones,
        bone: draw.mesh_bones.get(mesh).copied().flatten(),
    });
}

pub fn update(battle: Res<Match>, mut parts: Query<(&SupplyPart, &mut Visibility)>) {
    let battle = battle.into_inner();
    for (part, mut visibility) in &mut parts {
        let boxes = battle
            .simulation
            .supplies
            .get(part.source)
            .map_or(0, |supply| supply.boxes);
        let visible = !(part.delete_when_empty && boxes == 0)
            && part.bone.is_none_or(|bone| {
                bone <= shown_bones(part.total_bones, part.initial_boxes, boxes)
            });
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub fn verify_visibility(
    source: usize,
    current: u32,
    parts: &Query<(&SupplyPart, &Visibility, &InheritedVisibility)>,
) -> Result<usize, Box<dyn Error>> {
    let mut ordinals = BTreeSet::new();
    let mut visible = BTreeSet::new();
    let mut total = None;
    for (part, visibility, inherited) in parts {
        if part.source != source {
            continue;
        }
        let Some(bone) = part.bone else {
            continue;
        };
        if total.is_some_and(|count| count != part.total_bones) {
            return Err("Supply crate parts disagree on their source bone count".into());
        }
        total = Some(part.total_bones);
        ordinals.insert(bone);
        let expected = bone <= shown_bones(part.total_bones, part.initial_boxes, current);
        let locally_visible = !matches!(visibility, Visibility::Hidden);
        if locally_visible != expected || inherited.get() != expected {
            return Err(
                format!("Supply {source} crate {bone} has incorrect rendered visibility").into(),
            );
        }
        if inherited.get() {
            visible.insert(bone);
        }
    }
    let total = total.ok_or("Supply source has no numbered crate mesh parts")?;
    if ordinals.len() != usize::from(total) {
        return Err("Supply source is missing numbered crate mesh parts".into());
    }
    Ok(visible.len())
}

fn shown_bones(total: u16, initial: u16, current: u32) -> u16 {
    // Native floating division, multiplication and ceil, bounded before converting.
    let current = u16::try_from(current.min(u32::from(initial))).expect("clamped supply count");
    let ratio = f32::from(current) / f32::from(initial);
    let shown = (f32::from(total) * ratio).ceil();
    (0..=total)
        .find(|count| f32::from(*count) >= shown)
        .unwrap_or(total)
}

#[cfg(test)]
mod tests {
    use super::shown_bones;

    #[test]
    fn partial_supplies_retain_the_original_ceil_number_of_crates() {
        assert_eq!(shown_bones(64, 400, 400), 64);
        assert_eq!(shown_bones(64, 400, 399), 64);
        assert_eq!(shown_bones(64, 400, 393), 63);
        assert_eq!(shown_bones(64, 400, 1), 1);
        assert_eq!(shown_bones(64, 400, 0), 0);
        assert_eq!(shown_bones(8, 50, 51), 8);
    }
}
