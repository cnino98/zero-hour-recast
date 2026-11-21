//! Original model-state particle bindings and static or animated bone transforms.
use super::{Players, data};
use crate::skirmish_data::{field, scalar};
use generals_assets::content::ContentLine;
use std::error::Error;

pub struct ParticleDraw {
    pub states: Vec<Vec<String>>,
    pub bones: Vec<Vec<String>>,
    pub animated: bool,
}

pub fn particle_draw(lines: &[ContentLine]) -> Result<ParticleDraw, Box<dyn Error>> {
    let animated = match field(lines, 1, "ParticlesAttachedToAnimatedBones")
        .map(scalar)
        .transpose()?
    {
        None => false,
        Some(value) if value.eq_ignore_ascii_case("No") => false,
        Some(value) if value.eq_ignore_ascii_case("Yes") => true,
        Some(_) => return Err("invalid ParticlesAttachedToAnimatedBones boolean".into()),
    };
    if field(lines, 1, "AttachToBoneInAnotherModule").is_some() {
        return Err("attached draw model-state particles are not translated".into());
    }
    let draw = data::parse(lines)?;
    let states = draw
        .states
        .iter()
        .map(|state| {
            state
                .fields
                .iter()
                .filter(|line| line.first().is_some_and(|key| key == "ParticleSysBone"))
                .map(|line| {
                    let [_, _, template] = line.as_slice() else {
                        return Err("invalid model-state particle binding".into());
                    };
                    Ok(template.clone())
                })
                .collect::<Result<_, Box<dyn Error>>>()
        })
        .collect::<Result<_, _>>()?;
    let bones = draw
        .states
        .iter()
        .map(|state| {
            state
                .fields
                .iter()
                .filter(|line| line.first().is_some_and(|key| key == "ParticleSysBone"))
                .map(|line| line[1].clone())
                .collect()
        })
        .collect();
    Ok(ParticleDraw {
        states,
        bones,
        animated,
    })
}

impl super::Animations {
    pub fn particle_bone(
        &self,
        unit: &generals_sim::skirmish::Entity,
        players: &Players,
        module: usize,
        name: &str,
    ) -> Result<Option<generals_assets::w3d::BoneControl>, generals_assets::w3d::Error> {
        let Some(player) = players.0.get(module) else {
            return Ok(None);
        };
        let Some(draw) = self
            .draws
            .get(player.draw_type(unit.type_index))
            .and_then(|draws| draws.get(module))
        else {
            return Ok(None);
        };
        let Some(state) = draw.states.get(player.state) else {
            return Ok(None);
        };
        let Some(model) = state.model.and_then(|index| self.models.get(index)) else {
            return Ok(None);
        };
        let clip = player
            .animation(draw)
            .and_then(|(_, motion)| self.clips.get(&motion.name));
        let controls = super::turret::controls(
            &state.fields,
            unit.ai.turret.angle,
            unit.ai.turret.pitch,
            unit.ai.secondary_turret.as_ref(),
        );
        // W3DModelDraw samples after turret control but before weapon recoil.
        Ok(model
            .controlled_bones(clip, player.frame, &controls)?
            .into_iter()
            .find(|bone| bone.name.eq_ignore_ascii_case(name)))
    }
}

impl Players {
    pub(crate) fn has_render_object(&self, module: usize) -> bool {
        self.0
            .get(module)
            .is_some_and(|player| player.model.is_some())
    }

    pub fn particle_states(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.0
            .iter()
            .enumerate()
            .filter(|(_, player)| player.state != usize::MAX)
            .map(|(module, player)| (module, player.state))
    }
}

/// Select the authored drawable particle state with the shared source matcher.
/// # Errors
/// Rejects malformed source draw declarations.
pub fn selected_particle_state(
    lines: &[ContentLine],
    conditions: &std::collections::BTreeSet<String>,
) -> Result<usize, Box<dyn Error>> {
    Ok(super::data::parse(lines)?.select(conditions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    fn lines(body: &str) -> Vec<ContentLine> {
        content::scan(Asset {
            path: "model.ini".into(),
            origin: "test".into(),
            bytes: format!("Object Tractor\nDraw=W3DTruckDraw Tag\n{body}\nEnd\nEnd\n")
                .into_bytes(),
        })
        .unwrap()
        .declarations
        .remove(0)
        .lines
    }

    #[test]
    fn copied_default_bindings_append_and_aliases_share_the_same_state() {
        let lines = lines(
            "DefaultConditionState\nModel=BODY\nParticleSysBone=none first\nEnd\nConditionState=USING_WEAPON_B\nParticleSysBone=none second\nEnd\nAliasConditionState=USING_WEAPON_B DAMAGED",
        );
        let particles = particle_draw(&lines).unwrap();
        assert_eq!(
            particles.states,
            vec![
                vec!["first".to_owned()],
                vec!["first".to_owned(), "second".to_owned()]
            ]
        );
        let draw = data::parse(&lines).unwrap();
        let state = draw.select(&["USING_WEAPON_B".into()].into());
        let alias = draw.select(&["USING_WEAPON_B".into(), "DAMAGED".into()].into());
        assert_eq!(state, alias);
    }

    #[test]
    fn named_bones_and_native_animated_policy_are_retained() {
        let particles = particle_draw(&lines(
            "DefaultConditionState\nModel=BODY\nParticleSysBone=Bone_FX1 effect\nEnd",
        ))
        .unwrap();
        assert_eq!(particles.bones, [vec!["Bone_FX1"]]);
        assert!(!particles.animated);
        for (policy, expected) in [("Yes", true), ("No", false), ("yes", true)] {
            let particles = particle_draw(&lines(&format!(
                "ParticlesAttachedToAnimatedBones={policy}\nDefaultConditionState\nModel=BODY\nParticleSysBone=none effect\nEnd"
            ))).unwrap();
            assert_eq!(particles.animated, expected);
        }
        for body in [
            "ParticlesAttachedToAnimatedBones=invalid\nDefaultConditionState\nModel=BODY\nParticleSysBone=none effect\nEnd",
            "AttachToBoneInAnotherModule=Bone\nDefaultConditionState\nModel=BODY\nParticleSysBone=none effect\nEnd",
            "DefaultConditionState\nModel=BODY\nParticleSysBone=none\nEnd",
        ] {
            assert!(particle_draw(&lines(body)).is_err());
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_usa_dozer_preattack_samples_animated_dirt_bone_and_excludes_root() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let mut models = Vec::new();
        let mut animations =
            super::super::load(&assets, &mut models, &battle.simulation.catalog).unwrap();
        animations.retain_models(models, 0);
        let unit = battle
            .simulation
            .entities
            .iter()
            .find(|actor| battle.simulation.catalog[actor.type_index].name == "AmericaVehicleDozer")
            .unwrap();
        let draw = &animations.draws[unit.type_index][0];
        let selected = draw.select(&["PREATTACK_A".to_owned()].into());
        let state = &draw.states[selected];
        assert_eq!(state.name, "avconstdoz_a");
        assert_eq!(state.mode, "ONCE");
        let model = &animations.models[state.model.unwrap()];
        let motion = &state.motions[0];
        assert_eq!(motion.name, "avconstdoz_a.avconstdoz_a");
        let clip = &animations.clips[&motion.name];
        let mut player = super::super::Player::new(unit.position, battle.simulation.tick);
        player.state = selected;
        let mut players = Players(vec![player]);
        let first = animations
            .particle_bone(unit, &players, 0, "DIRTFX01")
            .unwrap()
            .unwrap();
        players.0[0].frame = f32::from(clip.frame_count - 1) * 0.5;
        let middle = animations
            .particle_bone(unit, &players, 0, "DIRTFX01")
            .unwrap()
            .unwrap();
        let bits = |bone: &generals_assets::w3d::BoneControl| {
            [bone.translation, bone.axes[0], bone.axes[1], bone.axes[2]]
                .map(|row| row.map(f32::to_bits))
        };
        assert_ne!(
            bits(&first),
            bits(&middle),
            "original digging bone must animate"
        );
        assert!(
            animations
                .particle_bone(unit, &players, 0, "EXHAUSTFX01")
                .unwrap()
                .is_some()
        );
        for bone in ["NONE", "missing_source_bone", model.bone_names[0].as_str()] {
            assert!(
                animations
                    .particle_bone(unit, &players, 0, bone)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
