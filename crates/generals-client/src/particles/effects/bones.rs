//! `FXListAtBonePosFXNugget::doFXObj`, including its source orientation quirk.
use super::{
    Animations, ContentFile, ContentLine, Error, Nugget, Particles, Players, Pose, Template, field,
    parse_list, scalar,
};

pub(super) fn parse(
    files: &[ContentFile],
    fields: &[ContentLine],
    templates: &[Template],
    lists: &mut Vec<Vec<Nugget>>,
) -> Result<Nugget, Box<dyn Error>> {
    let value = |key| field(fields, 1, key).map(scalar).transpose();
    for line in &fields[1..] {
        if !matches!(line.tokens[0].as_str(), "FX" | "BoneName" | "OrientToBone") {
            return Err(format!("unsupported bone FX field {}", line.tokens[0]).into());
        }
    }
    // The original parses OrientToBone (default true) but never reads it in
    // doFxAtBones: both values pass the actual bone matrix to the nested list.
    if !matches!(
        value("OrientToBone")?
            .unwrap_or("Yes")
            .to_ascii_lowercase()
            .as_str(),
        "yes" | "no"
    ) {
        return Err("invalid bone FX orientation boolean".into());
    }
    let name = value("FX")?.unwrap_or("NONE");
    let nested = if name.eq_ignore_ascii_case("NONE") {
        Vec::new()
    } else {
        parse_list(
            files,
            {
                let fx = crate::skirmish_data::fx_definition(files, name)?
                    .ok_or_else(|| format!("missing FXList {name}"))?;
                &fx.lines
            },
            templates,
            lists,
            false,
        )?
    };
    let list = lists.len();
    lists.push(nested);
    Ok(Nugget::AtBones {
        list,
        bone: value("BoneName")?.unwrap_or("").into(),
    })
}

impl Particles {
    pub(super) fn effect_bones(
        &self,
        index: usize,
        unit: &generals_sim::skirmish::Entity,
        players: &Players,
        animations: &Animations,
        object: &Pose,
    ) -> Result<Vec<(usize, Vec<Pose>)>, generals_assets::w3d::Error> {
        let mut result = Vec::new();
        for (index, nugget) in self.lists[index].iter().enumerate() {
            let Nugget::AtBones { bone, .. } = nugget else {
                continue;
            };
            let poses = animations
                .effect_bones(unit, players, bone)?
                .into_iter()
                .map(|bone| {
                    let offset = object.rotate(bone.translation);
                    Pose {
                        position: std::array::from_fn(|axis| object.position[axis] + offset[axis]),
                        axes: bone.axes.map(|axis| object.rotate(axis)),
                    }
                })
                .collect();
            result.push((index, poses));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::tests::{system, template};
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn bone_effects_preserve_authored_order_and_orientation_even_when_flag_is_no() {
        let files = [content::scan(Asset {
            path: "fx.ini".into(), origin: "native FX fixture".into(),
            bytes: b"FXList Parent\nSound\nName=Before\nEnd\nFXListAtBonePos\nFX=Child\nBoneName=FIREPOINT\nOrientToBone=No\nEnd\nSound\nName=After\nEnd\nEnd\nFXList Child\nParticleSystem\nName=fixture\nOrientToObject=Yes\nOffset=X:1 Y:0 Z:0\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let mut particles = system(vec![template("fixture")]);
        let mut lists = Vec::new();
        let root = parse_list(
            &files,
            &files[0].declarations[0].lines,
            &particles.templates,
            &mut lists,
            true,
        )
        .unwrap();
        assert!(matches!(&root[1], Nugget::AtBones { list: 0, bone } if bone == "FIREPOINT"));
        lists.push(root);
        particles.lists = lists;
        let world = Pose {
            position: [10.0, 20.0, 30.0],
            axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        };
        particles.fire(
            1,
            &Pose::at([0.0; 3]),
            &[(1, vec![world])],
            &|_| panic!("unrequested terrain height"),
            0.0,
            &mut super::super::rng::real_only(|low, _| low),
        );
        assert_eq!(
            particles
                .sounds
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["Before", "After"]
        );
        assert_eq!(particles.emitters.len(), 1);
        let transform = &particles.emitters[0].transform;
        assert_eq!(
            transform.position.map(f32::to_bits),
            [10.0_f32, 21.0, 30.0].map(f32::to_bits)
        );
        assert_eq!(
            transform.axes[0].map(f32::to_bits),
            [0.0_f32, 1.0, 0.0].map(f32::to_bits)
        );
    }

    #[test]
    fn recursive_object_effect_cannot_be_invoked_as_a_position_effect() {
        let files = [content::scan(Asset {
            path: "fx.ini".into(),
            origin: "native invalid FX fixture".into(),
            bytes:
                b"FXList Recursive\nFXListAtBonePos\nFX=Recursive\nBoneName=FIREPOINT\nEnd\nEnd\n"
                    .to_vec(),
        })
        .unwrap()];
        let error = parse_list(
            &files,
            &files[0].declarations[0].lines,
            &[],
            &mut Vec::new(),
            true,
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("requires an object"));
    }
}
