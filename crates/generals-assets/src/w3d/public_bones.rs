//! `W3DModelDraw.cpp::doSingleBoneName` / `findSingleSubObj`, `SuperHackers` 2bfaea60.
use super::Model;
use std::collections::BTreeMap;

impl Model {
    /// Resolve a source public-bone prefix, including its contiguous 01..99
    /// suffixes. Only when none are real bones does the source use subobjects.
    #[must_use]
    pub fn public_bone_indices(&self, prefix: &str) -> BTreeMap<String, usize> {
        let prefix = prefix.to_ascii_lowercase();
        let bones = series(&prefix, |name| {
            self.bone_names
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, bone)| bone.eq_ignore_ascii_case(name))
                .map(|(index, _)| index)
        });
        if !bones.is_empty() {
            return bones;
        }
        series(&prefix, |name| {
            // RenderObj::Get_Sub_Object_By_Name checks full names first, then
            // the suffix after the first dot. Its result contributes the HLOD
            // attachment bone, which need not have the subobject's name. HLOD
            // enumeration follows binding order, not W3D mesh-chunk order.
            let mesh = self
                .meshes
                .iter()
                .filter(|mesh| mesh.name.eq_ignore_ascii_case(name))
                .min_by_key(|mesh| mesh.subobject_order)
                .or_else(|| {
                    self.meshes
                        .iter()
                        .filter(|mesh| {
                            mesh.name
                                .split_once('.')
                                .map_or(mesh.name.as_str(), |(_, suffix)| suffix)
                                .eq_ignore_ascii_case(name)
                        })
                        .min_by_key(|mesh| mesh.subobject_order)
                })?;
            let bone = mesh.bone_name.as_ref()?;
            self.bone_names
                .iter()
                .position(|name| name.eq_ignore_ascii_case(bone))
        })
    }
}

fn series(prefix: &str, mut find: impl FnMut(&str) -> Option<usize>) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::new();
    if !prefix.is_empty()
        && prefix != "none"
        && let Some(index) = find(prefix)
    {
        result.insert(prefix.to_owned(), index);
    }
    for suffix in 1..=99 {
        let name = format!("{prefix}{suffix:02}");
        let Some(index) = find(&name) else { break };
        result.insert(name, index);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::w3d::{Bounds, Mesh};

    fn model() -> Model {
        Model {
            hlod: true,
            emitter: None,
            scene_sphere: None,
            scene_bounds: crate::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            bone_names: ["ROOT", "MUZZLE01"].map(String::from).to_vec(),
            meshes: vec![Mesh {
                name: "AIRNGR_SKN.MUZZLEFX01".into(),
                bone_name: Some("MUZZLE01".into()),
                attributes: 0x2000,
                sort_level: 0,
                subobject_order: 0,
                bounds: crate::w3d::Bounds {
                    min: [0.0; 3],
                    max: [0.0; 3],
                },
                sphere_center: [0.0; 3],
                sphere_radius: 1.0,
                bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
                positions: vec![[0.0; 3]],
                indices: Vec::new(),
                normals: None,
                materials: None,
                deformation: None,
            }],
            bounds: Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            hierarchy: None,
        }
    }

    #[test]
    fn installed_ranger_attachment_name_resolves_through_the_subobject() {
        let model = model();
        assert!(!model.bone_names.iter().any(|bone| bone == "MUZZLEFX01"));
        assert_eq!(
            model.public_bone_indices("MuzzleFX").get("muzzlefx01"),
            Some(&1)
        );
    }

    #[test]
    fn subobject_fallback_uses_hlod_order_and_keeps_full_name_priority() {
        let mut source = model();
        source.bone_names.push("MUZZLE02".into());
        source.meshes[0].subobject_order = 1;
        let mut earlier = model().meshes.remove(0);
        earlier.name = "OTHER.MUZZLEFX01".into();
        earlier.bone_name = Some("MUZZLE02".into());
        source.meshes.push(earlier);
        assert_eq!(source.public_bone_indices("MuzzleFX")["muzzlefx01"], 2);

        // A full-name match wins the first search even when a suffix match
        // occurs earlier in the native subobject enumeration.
        source.meshes[0].name = "MUZZLEFX01".into();
        assert_eq!(source.public_bone_indices("MuzzleFX")["muzzlefx01"], 1);
        source.meshes.swap(0, 1);
        assert_eq!(source.public_bone_indices("MuzzleFX")["muzzlefx01"], 1);
    }

    #[test]
    fn any_real_bone_in_the_series_prevents_subobject_fallback() {
        let mut model = model();
        model.bone_names.push("MUZZLEFX".into());
        let bindings = model.public_bone_indices("MuzzleFX");
        assert_eq!(bindings.get("muzzlefx"), Some(&2));
        assert!(!bindings.contains_key("muzzlefx01"));
        model.bone_names.pop();
        model.meshes[0].name = "AIRNGR_SKN.MUZZLEFX02".into();
        assert!(model.public_bone_indices("MuzzleFX").is_empty());
    }
}
