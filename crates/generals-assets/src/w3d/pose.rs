//! `HTreeClass::Anim_Update` and single-bone `MeshGeometryClass` deformation.
use super::{Error, Mesh, Model, Pivot, animation::Clip, rotate};

pub struct Hierarchy {
    name: String,
    pivots: Vec<Pivot>,
}

pub struct Deformation {
    positions: Vec<[f32; 3]>,
    normals: Option<Vec<[f32; 3]>>,
    bones: Vec<usize>,
    rigid: Option<usize>,
}

/// Local captured-bone transform, post-multiplied after authored animation.
pub struct BoneControl {
    pub name: String,
    pub axes: [[f32; 3]; 3],
    pub translation: [f32; 3],
}

pub struct MeshPose {
    pub bound_transform: [[f32; 3]; 4],
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub visible: bool,
}

struct Transform {
    axes: [[f32; 3]; 3],
    position: [f32; 3],
}

impl Transform {
    const fn identity() -> Self {
        Self {
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            position: [0.0; 3],
        }
    }
    fn vector(&self, vector: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|axis| {
            let x = self.axes[0][axis] * vector[0];
            let y = self.axes[1][axis] * vector[1];
            let z = self.axes[2][axis] * vector[2];
            x + y + z
        })
    }
    fn point(&self, point: [f32; 3]) -> [f32; 3] {
        let rotated = self.vector(point);
        std::array::from_fn(|axis| rotated[axis] + self.position[axis])
    }
}

impl Hierarchy {
    pub(super) const fn new(name: String, pivots: Vec<Pivot>) -> Self {
        Self { name, pivots }
    }

    fn descends_from(&self, name: Option<&str>, ancestor: usize) -> bool {
        let Some(mut index) = name.and_then(|name| {
            self.pivots
                .iter()
                .position(|pivot| pivot.name.eq_ignore_ascii_case(name))
        }) else {
            return false;
        };
        // Native traversal starts at the parent: another subobject on the same
        // bone is not a descendant of the selected subobject.
        while let Some(parent) = self.pivots[index].parent {
            if parent == ancestor {
                return true;
            }
            index = parent;
        }
        false
    }

    fn bone_transforms(&self, transforms: &[Transform]) -> Vec<BoneControl> {
        self.pivots
            .iter()
            .zip(transforms)
            .skip(1)
            .map(|(pivot, transform)| BoneControl {
                name: pivot.name.clone(),
                axes: transform.axes,
                translation: transform.position,
            })
            .collect()
    }

    fn pose(
        &self,
        clip: Option<&Clip>,
        frame: f32,
        controls: &[BoneControl],
    ) -> Result<(Vec<Transform>, Vec<bool>), Error> {
        // HTreeClass::Anim_Update applies by index, not hierarchy/bone name.
        // Damaged models can inherit a pristine model's animation.
        let animated_pivots = clip
            .map(|clip| clip.pivot_limit(&self.name, self.pivots.len()))
            .transpose()?
            .unwrap_or(0);
        let mut transforms = vec![Transform::identity()];
        let mut visible = vec![true];
        for (index, pivot) in self.pivots.iter().enumerate().skip(1) {
            let parent = pivot
                .parent
                .and_then(|parent| transforms.get(parent))
                .ok_or(Error::InvalidChunk("animation hierarchy parent order"))?;
            let (translation, rotation, mut visibility) = clip
                .filter(|_| index < animated_pivots)
                .map_or(([0.0; 3], [0.0, 0.0, 0.0, 1.0], true), |clip| {
                    clip.sample(index, frame)
                });
            let mut local = Transform::identity();
            local.axes = local
                .axes
                .map(|axis| rotate(rotate(axis, rotation), pivot.rotation));
            let translated = rotate(translation, pivot.rotation);
            local.position = std::array::from_fn(|axis| translated[axis] + pivot.translation[axis]);
            let mut transform = Transform {
                axes: local.axes.map(|axis| parent.vector(axis)),
                position: parent.point(local.position),
            };
            if let Some(control) = controls
                .iter()
                .rev()
                .find(|control| control.name.eq_ignore_ascii_case(&pivot.name))
            {
                let position = transform.point(control.translation);
                transform.axes = control.axes.map(|axis| transform.vector(axis));
                transform.position = position;
                visibility = true;
            }
            transforms.push(transform);
            visible.push(visibility);
        }
        Ok((transforms, visible))
    }
}

impl Deformation {
    /// Original single-bone influence for a vertex, or the rigid mesh attachment.
    #[must_use]
    pub fn vertex_bone(&self, vertex: usize) -> Option<usize> {
        self.positions.get(vertex)?;
        self.rigid.or_else(|| self.bones.get(vertex).copied())
    }

    pub(super) fn skin(mesh: &Mesh, influences: &[u8]) -> Self {
        Self {
            positions: mesh.positions.clone(),
            normals: mesh.normals.clone(),
            bones: influences
                .chunks_exact(8)
                .map(|data| usize::from(u16::from_le_bytes([data[0], data[1]])))
                .collect(),
            rigid: None,
        }
    }
    pub(super) fn rigid(mesh: &Mesh, bone: usize) -> Self {
        Self {
            positions: mesh.positions.clone(),
            normals: mesh.normals.clone(),
            bones: Vec::new(),
            rigid: Some(bone),
        }
    }

    fn pose(&self, transforms: &[Transform], visible: &[bool]) -> Result<MeshPose, Error> {
        let transforms = self
            .positions
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let bone = self.rigid.unwrap_or_else(|| self.bones[index]);
                transforms
                    .get(bone)
                    .ok_or(Error::InvalidChunk("animation skin bone missing"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let positions = self
            .positions
            .iter()
            .zip(&transforms)
            .map(|(&point, transform)| transform.point(point))
            .collect::<Vec<_>>();
        let normals = self.normals.as_ref().map(|normals| {
            normals
                .iter()
                .zip(&transforms)
                .map(|(&normal, transform)| transform.vector(normal))
                .collect::<Vec<_>>()
        });
        if positions
            .iter()
            .flatten()
            .chain(normals.iter().flatten().flatten())
            .any(|value| !value.is_finite())
        {
            return Err(Error::NonFinite);
        }
        Ok(MeshPose {
            bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
            positions,
            normals,
            visible: self.rigid.is_none_or(|bone| visible[bone]),
        })
    }
}

fn mesh_transform(mesh: &Mesh, hierarchy: &Hierarchy, transforms: &[Transform]) -> [[f32; 3]; 4] {
    mesh.bone_name
        .as_ref()
        .and_then(|name| {
            hierarchy
                .pivots
                .iter()
                .position(|pivot| &pivot.name == name)
        })
        .map_or(mesh.bound_transform, |index| {
            let transform = &transforms[index];
            [
                transform.axes[0],
                transform.axes[1],
                transform.axes[2],
                transform.position,
            ]
        })
}

impl Model {
    /// `W3DModelDraw::doHideShowSubObjs`: affect the named subobject and all
    /// subobjects attached to descendant bones, preserving unrelated attributes.
    pub fn set_subobject_hidden(&mut self, name: &str, hidden: bool) {
        let Some(selected) = self
            .meshes
            .iter()
            .position(|mesh| mesh.name.eq_ignore_ascii_case(name))
            .or_else(|| {
                self.meshes.iter().position(|mesh| {
                    mesh.name
                        .split_once('.')
                        .map_or(mesh.name.as_str(), |(_, suffix)| suffix)
                        .eq_ignore_ascii_case(name)
                })
            })
        else {
            return;
        };
        let ancestor = self.hierarchy.as_ref().and_then(|hierarchy| {
            let name = self.meshes[selected].bone_name.as_ref()?;
            hierarchy
                .pivots
                .iter()
                .position(|pivot| pivot.name.eq_ignore_ascii_case(name))
        });
        for (index, mesh) in self.meshes.iter_mut().enumerate() {
            let descendant = self.hierarchy.as_ref().is_some_and(|hierarchy| {
                ancestor.is_some_and(|ancestor| {
                    hierarchy.descends_from(mesh.bone_name.as_deref(), ancestor)
                })
            });
            if index == selected || descendant {
                if hidden {
                    mesh.attributes |= 0x1000;
                } else {
                    mesh.attributes &= !0x1000;
                }
            }
        }
    }

    /// Current native hierarchy position, used by animated weapon FX bones.
    /// # Errors
    /// Rejects invalid animation frames, hierarchies and bone references.
    pub fn posed_bone_position(
        &self,
        name: &str,
        clip: Option<&Clip>,
        frame: f32,
    ) -> Result<Option<[f32; 3]>, Error> {
        Ok(self
            .posed_bone_matrix(name, clip, frame)?
            .map(|matrix| matrix.map(|row| row[3])))
    }

    /// Native row-major pristine or animated bone matrix in model coordinates.
    /// # Errors
    /// Rejects invalid animation frames, hierarchies and bone references.
    pub fn posed_bone_matrix(
        &self,
        name: &str,
        clip: Option<&Clip>,
        frame: f32,
    ) -> Result<Option<[[f32; 4]; 3]>, Error> {
        let transform = if let Some(clip) = clip {
            if !frame.is_finite() || frame < 0.0 || frame >= f32::from(clip.frame_count) {
                return Err(Error::InvalidChunk("animation sample frame"));
            }
            let Some(hierarchy) = &self.hierarchy else {
                return Ok(None);
            };
            let Some(index) = hierarchy
                .pivots
                .iter()
                .position(|pivot| pivot.name.eq_ignore_ascii_case(name))
            else {
                return Ok(None);
            };
            let (mut transforms, _) = hierarchy.pose(Some(clip), frame, &[])?;
            transforms.remove(index)
        } else {
            let Some(transform) = self.pristine_transform(name) else {
                return Ok(None);
            };
            transform
        };
        Ok(Some(std::array::from_fn(|row| {
            [
                transform.axes[0][row],
                transform.axes[1][row],
                transform.axes[2][row],
                transform.position[row],
            ]
        })))
    }

    /// Animated and captured bone position, including turret and barrel controls.
    /// # Errors
    /// Rejects malformed animation frames or hierarchy indices.
    pub fn controlled_bone_position(
        &self,
        name: &str,
        clip: Option<&Clip>,
        frame: f32,
        controls: &[BoneControl],
    ) -> Result<Option<[f32; 3]>, Error> {
        validate_frame(clip, frame)?;
        let Some(hierarchy) = &self.hierarchy else {
            return Ok(None);
        };
        let Some(index) = hierarchy
            .pivots
            .iter()
            .position(|pivot| pivot.name.eq_ignore_ascii_case(name))
        else {
            return Ok(None);
        };
        let (transforms, _) = hierarchy.pose(clip, frame, controls)?;
        Ok(Some(transforms[index].position))
    }

    /// Current model-space bones for native client effect placement.
    /// # Errors
    /// Rejects malformed animation frames or unresolved hierarchy indices.
    pub fn controlled_bones(
        &self,
        clip: Option<&Clip>,
        frame: f32,
        controls: &[BoneControl],
    ) -> Result<Vec<BoneControl>, Error> {
        validate_frame(clip, frame)?;
        let Some(hierarchy) = &self.hierarchy else {
            return Ok(Vec::new());
        };
        let (transforms, _) = hierarchy.pose(clip, frame, controls)?;
        Ok(hierarchy.bone_transforms(&transforms))
    }

    /// Evaluates `HTreeClass` base/animation transforms and `PivotClass::Capture_Update`.
    /// # Errors
    /// Rejects malformed animation frames or unresolved hierarchy indices.
    pub fn controlled_pose(
        &self,
        clip: Option<&Clip>,
        frame: f32,
        controls: &[BoneControl],
    ) -> Result<Vec<MeshPose>, Error> {
        validate_frame(clip, frame)?;
        let hierarchy = self.hierarchy.as_ref();
        let transformed = hierarchy
            .map(|hierarchy| hierarchy.pose(clip, frame, controls))
            .transpose()?;
        self.pose_meshes(transformed.as_ref())
    }

    /// Evaluates one hierarchy for both the drawn meshes and their client bone snapshot.
    /// # Errors
    /// Rejects malformed animation frames or unresolved hierarchy indices.
    pub fn controlled_pose_and_bones(
        &self,
        clip: Option<&Clip>,
        frame: f32,
        controls: &[BoneControl],
    ) -> Result<(Vec<MeshPose>, Vec<BoneControl>), Error> {
        validate_frame(clip, frame)?;
        let Some(hierarchy) = &self.hierarchy else {
            return Ok((self.pose_meshes(None)?, Vec::new()));
        };
        let transformed = hierarchy.pose(clip, frame, controls)?;
        let meshes = self.pose_meshes(Some(&transformed))?;
        let bones = hierarchy.bone_transforms(&transformed.0);
        Ok((meshes, bones))
    }

    fn pose_meshes(
        &self,
        transformed: Option<&(Vec<Transform>, Vec<bool>)>,
    ) -> Result<Vec<MeshPose>, Error> {
        let hierarchy = self.hierarchy.as_ref();
        self.meshes
            .iter()
            .map(|mesh| {
                let mut pose = match (&mesh.deformation, transformed) {
                    (Some(deformation), Some((transforms, visibility))) => {
                        deformation.pose(transforms, visibility)?
                    }
                    _ => MeshPose {
                        bound_transform: mesh.bound_transform,
                        positions: mesh.positions.clone(),
                        normals: mesh.normals.clone(),
                        visible: true,
                    },
                };
                if let (Some(hierarchy), Some((transforms, _))) = (hierarchy, transformed) {
                    pose.bound_transform = mesh_transform(mesh, hierarchy, transforms);
                }
                Ok(pose)
            })
            .collect()
    }

    /// Native hide/show propagation visits ancestors, excluding the child's own pivot.
    #[must_use]
    pub fn bone_is_descendant(&self, child: &str, ancestor: &str) -> bool {
        let Some(hierarchy) = &self.hierarchy else {
            return false;
        };
        let Some(mut child) = hierarchy
            .pivots
            .iter()
            .position(|pivot| pivot.name.eq_ignore_ascii_case(child))
        else {
            return false;
        };
        let Some(ancestor) = hierarchy
            .pivots
            .iter()
            .position(|pivot| pivot.name.eq_ignore_ascii_case(ancestor))
        else {
            return false;
        };
        while child != 0 {
            let Some(parent) = hierarchy.pivots[child].parent else {
                break;
            };
            if parent == ancestor {
                return true;
            }
            child = parent;
        }
        false
    }

    /// Source pristine hierarchy position used by drawable docking bones.
    #[must_use]
    pub fn bone_position(&self, name: &str) -> Option<[f32; 3]> {
        self.pristine_transform(name)
            .map(|transform| transform.position)
    }

    /// Native pristine basis columns, before the object's world transform.
    #[must_use]
    pub fn bone_axes(&self, name: &str) -> Option<[[f32; 3]; 3]> {
        self.pristine_transform(name)
            .map(|transform| transform.axes)
    }

    fn pristine_transform(&self, name: &str) -> Option<Transform> {
        let hierarchy = self.hierarchy.as_ref()?;
        let mut index = hierarchy
            .pivots
            .iter()
            .position(|pivot| pivot.name.eq_ignore_ascii_case(name))?;
        let mut transform = Transform::identity();
        // HTreeClass::Base_Update replaces pivot zero with the object transform.
        while index > 0 {
            let pivot = &hierarchy.pivots[index];
            transform.axes = transform.axes.map(|axis| rotate(axis, pivot.rotation));
            transform.position = rotate(transform.position, pivot.rotation);
            for (component, offset) in transform.position.iter_mut().zip(pivot.translation) {
                *component += offset;
            }
            index = pivot.parent?;
        }
        Some(transform)
    }

    /// Evaluates original animation deltas after the base hierarchy transforms.
    /// Returned meshes retain the same order and vertex indices as this model.
    /// # Errors
    /// Rejects nonfinite/out-of-range frames, unresolved hierarchies and invalid
    /// bone references. Playback mode and frame advancement belong to the caller.
    pub fn pose(&self, clip: &Clip, frame: f32) -> Result<Vec<MeshPose>, Error> {
        // MeshClass inherits RenderObjClass::Set_Animation's no-op. HLOD models
        // evaluate the same hierarchy path used by controlled animation.
        self.controlled_pose(Some(clip), frame, &[])
    }
}

fn validate_frame(clip: Option<&Clip>, frame: f32) -> Result<(), Error> {
    if !frame.is_finite()
        || frame < 0.0
        || clip.is_some_and(|clip| frame >= f32::from(clip.frame_count))
    {
        return Err(Error::InvalidChunk("animation sample frame"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Clip, Deformation, Hierarchy, Mesh, Model, Pivot};
    use crate::w3d::Bounds;

    fn chunk(id: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = id.to_le_bytes().to_vec();
        let size = u32::try_from(body.len()).unwrap()
            | if matches!(id, 0x100 | 0x200) {
                0x8000_0000
            } else {
                0
            };
        bytes.extend(size.to_le_bytes());
        bytes.extend(body);
        bytes
    }

    fn animated_fixture() -> (Model, Clip) {
        let mut header = [0; 44];
        header[..4].copy_from_slice(&0x0004_0001_u32.to_le_bytes());
        header[4..8].copy_from_slice(b"MOVE");
        header[20..23].copy_from_slice(b"RIG");
        header[36..40].copy_from_slice(&2_u32.to_le_bytes());
        header[40..].copy_from_slice(&30_u32.to_le_bytes());
        let mut channel = Vec::new();
        for value in [0_u16, 1, 1, 0, 1, 0] {
            channel.extend(value.to_le_bytes());
        }
        for value in [0_f32, 2.0] {
            channel.extend(value.to_le_bytes());
        }
        let mut body = chunk(0x201, &header);
        body.extend(chunk(0x202, &channel));
        let clip = Clip::parse(&chunk(0x200, &body)).unwrap();
        let mut mesh = Mesh {
            name: "RIG.MESH".into(),
            bone_name: Some("BONE".into()),
            attributes: 0,
            sort_level: 0,
            subobject_order: 0,
            bounds: crate::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            sphere_center: [0.0; 3],
            sphere_radius: 1.0,
            bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
            positions: vec![[1.0, 0.0, 0.0]],
            normals: Some(vec![[1.0, 0.0, 0.0]]),
            indices: Vec::new(),
            materials: None,
            deformation: None,
        };
        mesh.deformation = Some(Deformation::rigid(&mesh, 1));
        let model = Model {
            hlod: true,
            emitter: None,
            scene_sphere: None,
            scene_bounds: crate::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            meshes: vec![mesh],
            bone_names: vec!["ROOT".into(), "BONE".into()],
            bounds: Bounds {
                min: [0.0; 3],
                max: [1.0; 3],
            },
            hierarchy: Some(Hierarchy::new(
                "RIG".into(),
                vec![
                    Pivot {
                        name: "ROOT".into(),
                        parent: None,
                        translation: [99.0; 3],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                    },
                    Pivot {
                        name: "BONE".into(),
                        parent: Some(0),
                        translation: [1.0, 0.0, 0.0],
                        rotation: [
                            0.0,
                            0.0,
                            std::f32::consts::FRAC_1_SQRT_2,
                            std::f32::consts::FRAC_1_SQRT_2,
                        ],
                    },
                ],
            )),
        };
        (model, clip)
    }

    #[test]
    fn subobject_visibility_follows_strict_descendants_and_ordered_overrides() {
        let (mut model, _) = animated_fixture();
        for (name, parent) in [("TURRET", 1), ("BARREL", 2), ("SIBLING", 0)] {
            model.hierarchy.as_mut().unwrap().pivots.push(Pivot {
                name: name.into(),
                parent: Some(parent),
                translation: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
            });
            model.bone_names.push(name.into());
        }
        for (name, bone) in [
            ("TURRET", "TURRET"),
            ("BARREL", "BARREL"),
            ("SIBLING", "SIBLING"),
            ("PEER", "BONE"),
            ("ROOT", "ROOT"),
        ] {
            model.meshes.push(Mesh {
                name: format!("RIG.{name}"),
                bone_name: Some(bone.into()),
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
                positions: Vec::new(),
                normals: None,
                indices: Vec::new(),
                materials: None,
                deformation: None,
            });
        }
        let hidden = |model: &Model| {
            model
                .meshes
                .iter()
                .map(|mesh| mesh.attributes & 0x1000 != 0)
                .collect::<Vec<_>>()
        };
        model.set_subobject_hidden("mesh", true);
        assert_eq!(hidden(&model), [true, true, true, false, false, false]);
        model.set_subobject_hidden("rig.turret", false);
        assert_eq!(hidden(&model), [true, false, false, false, false, false]);
        model.set_subobject_hidden("MISSING", true);
        assert_eq!(hidden(&model), [true, false, false, false, false, false]);
        model.set_subobject_hidden("root", true);
        assert_eq!(hidden(&model), [true; 6]);
        model.set_subobject_hidden("ROOT", false);
        assert_eq!(hidden(&model), [false; 6]);
        assert!(
            model.meshes[1..]
                .iter()
                .all(|mesh| mesh.attributes == 0x2000)
        );
        model.hierarchy = None;
        model.set_subobject_hidden("mesh", true);
        assert_eq!(hidden(&model), [true, false, false, false, false, false]);
    }

    #[test]
    fn animated_translation_is_post_multiplied_after_base_rotation() {
        let (model, clip) = animated_fixture();
        let posed = model.pose(&clip, 0.5).unwrap();
        for (actual, expected) in posed[0].positions[0].into_iter().zip([1.0, 2.0, 0.0]) {
            assert!((actual - expected).abs() < 0.000_01);
        }
        let normal = posed[0].normals.as_ref().unwrap()[0];
        for (actual, expected) in normal.into_iter().zip([0.0, 1.0, 0.0]) {
            assert!((actual - expected).abs() < 0.000_01);
        }
        assert!(model.pose(&clip, 2.0).is_err());
    }

    #[test]
    fn animated_weapon_bone_follows_pose_instead_of_pristine_attachment() {
        let (model, clip) = animated_fixture();
        let pristine = model
            .posed_bone_position("BONE", None, 0.0)
            .unwrap()
            .unwrap();
        let animated = model
            .posed_bone_position("bone", Some(&clip), 0.5)
            .unwrap()
            .unwrap();
        for (actual, expected) in pristine.into_iter().zip([1.0, 0.0, 0.0]) {
            assert!((actual - expected).abs() < 0.000_01);
        }
        for (actual, expected) in animated.into_iter().zip([1.0, 1.0, 0.0]) {
            assert!((actual - expected).abs() < 0.000_01);
        }
        assert!(
            model
                .posed_bone_position("missing", Some(&clip), 0.5)
                .unwrap()
                .is_none()
        );
        assert!(model.posed_bone_position("BONE", Some(&clip), 2.0).is_err());
    }

    #[test]
    fn logical_bone_matrix_keeps_animated_basis_and_translation() {
        let (model, clip) = animated_fixture();
        let matrix = model
            .posed_bone_matrix("BONE", Some(&clip), 0.5)
            .unwrap()
            .unwrap();
        // The authored quarter-turn sends local +X to +Y; animation translates
        // on that rotated axis. Returning just position or transposed axes fails.
        let expected = [
            [0.0, -1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        for (actual, expected) in matrix
            .into_iter()
            .flatten()
            .zip(expected.into_iter().flatten())
        {
            assert!((actual - expected).abs() < 0.00001);
        }
        let mesh = &model.pose(&clip, 0.5).unwrap()[0];
        let origin = matrix.map(|row| row[3]);
        let point = std::array::from_fn::<_, 3, _>(|axis| origin[axis] + matrix[axis][0]);
        assert_eq!(point.map(f32::to_bits), mesh.positions[0].map(f32::to_bits));
    }

    #[test]
    fn current_effect_bones_preserve_animated_translation_and_captured_axes() {
        let (model, clip) = animated_fixture();
        let controls = [super::BoneControl {
            name: "BONE".into(),
            translation: [0.0, 0.0, 2.0],
            axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        }];
        let bones = model.controlled_bones(Some(&clip), 0.5, &controls).unwrap();
        assert_eq!(bones.len(), 1);
        assert_eq!(bones[0].name, "BONE");
        for (actual, expected) in bones[0].translation.into_iter().zip([1.0, 1.0, 2.0]) {
            assert!((actual - expected).abs() < 0.000_01);
        }
        for (actual, expected) in bones[0].axes[0].into_iter().zip([-1.0, 0.0, 0.0]) {
            assert!((actual - expected).abs() < 0.000_01);
        }
    }

    #[test]
    fn captured_turret_rotation_follows_base_and_animation_then_recoil_translation() {
        let (mut model, clip) = animated_fixture();
        let child = Pivot {
            name: "BARREL".into(),
            parent: Some(1),
            translation: [2.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
        };
        model.hierarchy.as_mut().unwrap().pivots.push(child);
        model.bone_names.push("BARREL".into());
        let controls = [
            super::BoneControl {
                name: "BONE".into(),
                axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                translation: [0.0; 3],
            },
            super::BoneControl {
                name: "BARREL".into(),
                axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                translation: [-0.5, 0.0, 0.0],
            },
        ];
        // Base BONE is +90 degrees; captured +90 is post-multiplied.
        // Its animated translation remains [1,1,0]; the barrel moves along -X.
        let point = model
            .controlled_bone_position("BARREL", Some(&clip), 0.5, &controls)
            .unwrap()
            .unwrap();
        for (actual, expected) in point.into_iter().zip([-0.5, 1.0, 0.0]) {
            assert!((actual - expected).abs() < 0.000_001);
        }
        let (pose, bones) = model
            .controlled_pose_and_bones(Some(&clip), 0.5, &controls)
            .unwrap();
        let barrel = bones.iter().find(|bone| bone.name == "BARREL").unwrap();
        assert_eq!(
            barrel.translation.map(f32::to_bits),
            point.map(f32::to_bits)
        );
        for (actual, expected) in pose[0].positions[0].into_iter().zip([0.0, 1.0, 0.0]) {
            assert!((actual - expected).abs() < 0.000_001);
        }
        let normal = pose[0].normals.as_ref().unwrap()[0];
        assert!((normal[0] + 1.0).abs() < 0.000_001);
        assert!(normal[1].abs() < 0.000_001);
        let idle = model.controlled_pose(None, 0.0, &[]).unwrap();
        assert!((idle[0].positions[0][0] - 1.0).abs() < 0.000_001);
        assert!((idle[0].positions[0][1] - 1.0).abs() < 0.000_001);
    }

    #[test]
    fn later_weapon_slot_capture_replaces_shared_bone_instead_of_accumulating() {
        let (model, _) = animated_fixture();
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let controls = [
            super::BoneControl {
                name: "BONE".into(),
                axes,
                translation: [-2.0, 0.0, 0.0],
            },
            super::BoneControl {
                name: "bone".into(),
                axes,
                translation: [0.0; 3],
            },
        ];
        let fired = model
            .controlled_bone_position("BONE", None, 0.0, &controls[..1])
            .unwrap()
            .unwrap();
        let rest = model.bone_position("BONE").unwrap();
        assert!((fired[1] - rest[1]).abs() > 1.9);
        let final_capture = model
            .controlled_bone_position("BONE", None, 0.0, &controls)
            .unwrap()
            .unwrap();
        assert_eq!(final_capture.map(f32::to_bits), rest.map(f32::to_bits));
    }

    fn animation_hierarchy(count: u16) -> Vec<u8> {
        let mut header = [0; 36];
        header[4..7].copy_from_slice(b"RIG");
        header[20..24].copy_from_slice(&u32::from(count).to_le_bytes());
        let mut body = chunk(0x101, &header);
        let mut pivots = Vec::new();
        for index in 0..count {
            let mut pivot = [0; 60];
            let parent = if index == 0 { u32::MAX } else { 0 };
            pivot[16..20].copy_from_slice(&parent.to_le_bytes());
            pivot[56..60].copy_from_slice(&1.0_f32.to_le_bytes());
            pivots.extend(pivot);
        }
        body.extend(chunk(0x102, &pivots));
        chunk(0x100, &body)
    }

    #[test]
    fn different_named_model_uses_animation_pivot_indices_and_declared_count() {
        let (mut model, mut clip) = animated_fixture();
        model.hierarchy.as_mut().unwrap().name = "DAMAGED".into();
        assert!(model.pose(&clip, 0.5).is_err());
        clip.bind_hierarchy(&animation_hierarchy(2)).unwrap();
        let posed = model.pose(&clip, 0.5).unwrap();
        assert!((posed[0].positions[0][1] - 2.0).abs() < 0.000_01);
        // HRawAnim ignores channels outside the declared animation hierarchy;
        // HTree leaves additional drawable pivots at their base transforms.
        clip.bind_hierarchy(&animation_hierarchy(1)).unwrap();
        let posed = model.pose(&clip, 0.5).unwrap();
        assert!((posed[0].positions[0][1] - 1.0).abs() < 0.000_01);
    }
}
