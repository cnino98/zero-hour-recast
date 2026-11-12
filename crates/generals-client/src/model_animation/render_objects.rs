//! `W3DModelDraw::setModelState` keeps its render object for compatible states.
use super::{State, data::Draw};
use generals_assets::{
    virtual_assets::VirtualAssets,
    w3d::{MeshPose, Model},
};
use std::{collections::BTreeMap, error::Error};

pub(super) fn bind(
    assets: &VirtualAssets,
    draw: &mut Draw,
    models: &mut Vec<Model>,
) -> Result<(), Box<dyn Error>> {
    // Scope identity to one draw module, so another module's parts cannot
    // receive this module's pose. State identities and models stay independent.
    let mut groups = BTreeMap::<(String, [String; 4]), Vec<usize>>::new();
    for (index, state) in draw.states.iter().enumerate() {
        if state.model.is_some() {
            groups
                .entry((state.name.clone(), turret_names(state)))
                .or_default()
                .push(index);
        }
    }
    for ((name, _), states) in groups {
        let candidates: Vec<_> = states
            .iter()
            .filter_map(|&index| draw.states[index].model)
            .collect();
        let visible: Vec<_> = models[candidates[0]]
            .meshes
            .iter()
            .enumerate()
            .map(|(mesh, _)| {
                draw.upgrade_show
                    .iter()
                    .any(|name| matches_subobject(&models[candidates[0]].meshes[mesh].name, name))
                    || candidates
                        .iter()
                        .any(|&model| models[model].meshes[mesh].attributes & 0x1000 == 0)
            })
            .collect();
        let existing = candidates.iter().find(|&&model| {
            models[model]
                .meshes
                .iter()
                .zip(&visible)
                .all(|(mesh, visible)| !visible || mesh.attributes & 0x1000 == 0)
        });
        let render_model = if let Some(&model) = existing {
            model
        } else {
            // Native hide/show changes existing subobjects, never deletes
            // them. Retain every part shown by at least one compatible state.
            let mut model = crate::skirmish_data::state_model(assets, &name, &[])?;
            for (mesh, visible) in model.meshes.iter_mut().zip(&visible) {
                if *visible {
                    mesh.attributes &= !0x1000;
                } else {
                    mesh.attributes |= 0x1000;
                }
            }
            let index = models.len();
            models.push(model);
            index
        };
        for index in states {
            draw.states[index].render_model = Some(render_model);
        }
    }
    Ok(())
}

fn turret_names(state: &State) -> [String; 4] {
    // parseBoneNameKey lowercases names and maps NONE to NAMEKEY_INVALID.
    // Loaded, nonempty states have native TURRETS_VALID even for missing bones.
    ["Turret", "TurretPitch", "AltTurret", "AltTurretPitch"].map(|key| {
        state
            .fields
            .iter()
            .rev()
            .find(|line| line.first().is_some_and(|name| name == key))
            .and_then(|line| line.get(1))
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map_or_else(String::new, |name| name.to_ascii_lowercase())
    })
}

pub(super) fn hide_subobjects(model: &Model, pose: &mut [MeshPose]) {
    for (mesh, pose) in model.meshes.iter().zip(pose) {
        pose.visible &= mesh.attributes & 0x1000 == 0;
    }
}

pub(super) fn apply_subobject_overrides(
    model: &Model,
    pose: &mut [MeshPose],
    overrides: &[generals_sim::skirmish::SubObjectOverride],
) {
    for setting in overrides {
        let Some(index) = model
            .meshes
            .iter()
            .position(|mesh| matches_subobject(&mesh.name, &setting.name))
        else {
            continue;
        };
        pose[index].visible = setting.visible;
        let bone = model.meshes[index]
            .bone_name
            .as_deref()
            .or_else(|| model.bone_names.first().map(String::as_str));
        if let Some(bone) = bone {
            for (mesh, pose) in model.meshes.iter().zip(&mut *pose) {
                if mesh
                    .bone_name
                    .as_deref()
                    .is_some_and(|child| model.bone_is_descendant(child, bone))
                {
                    pose.visible = setting.visible;
                }
            }
        }
    }
}

fn matches_subobject(mesh: &str, name: &str) -> bool {
    mesh.rsplit('.')
        .next()
        .is_some_and(|short| short.eq_ignore_ascii_case(name))
}
