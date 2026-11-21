//! `FXListAtBonePosFXNugget` current drawable bones, in its two source passes.
use super::{Animations, Players, turret};
use generals_assets::w3d::{BoneControl, Error};
use generals_sim::skirmish::Entity;

impl Animations {
    pub fn effect_bones(
        &self,
        unit: &Entity,
        players: &Players,
        prefix: &str,
    ) -> Result<Vec<BoneControl>, Error> {
        let Some(draws) = self.draws.get(unit.type_index) else {
            return Ok(Vec::new());
        };
        let mut bones = Vec::new();
        for (draw, player) in draws.iter().zip(&players.0) {
            let Some(state) = draw.states.get(player.state) else {
                continue;
            };
            let Some(model) = state.model.and_then(|index| self.models.get(index)) else {
                continue;
            };
            let clip = player
                .animation(draw)
                .and_then(|(_, motion)| self.clips.get(&motion.name));
            let mut controls = turret::controls(
                &state.fields,
                unit.ai.turret.angle,
                unit.ai.turret.pitch,
                unit.ai.secondary_turret.as_ref(),
            );
            player.muzzle.controls(&mut controls);
            let mut posed = model.controlled_bones(clip, player.frame, &controls)?;
            for bone in &mut posed {
                for (axis, offset) in bone.translation.iter_mut().zip(draw.attach_offset) {
                    *axis += offset;
                }
            }
            bones.push(posed);
        }
        Ok(effect_series(
            &bones.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            prefix,
        ))
    }
}

impl Players {
    /// Current render skeleton, without advancing animation or applying later
    /// logic turret changes. Native bone index zero is never a named bone hit.
    pub(crate) fn effect_bones(&self, prefix: &str) -> Vec<BoneControl> {
        let draws: Vec<_> = self
            .0
            .iter()
            .map(|player| player.current_bones.get(1..).unwrap_or(&[]))
            .collect();
        effect_series(&draws, prefix)
    }
}

fn effect_series(draws: &[&[BoneControl]], prefix: &str) -> Vec<BoneControl> {
    let mut result = Vec::new();
    // FXList makes separate Drawable calls, each limited to forty bones across
    // all draw modules. Unlike pristine public bones, no subobject fallback runs.
    for start in [0, 1] {
        let mut count = 0;
        for bones in draws {
            for suffix in start..=if start == 0 { 0 } else { 99 } {
                if count == 40 {
                    break;
                }
                let name = if suffix == 0 {
                    prefix.to_owned()
                } else {
                    format!("{prefix}{suffix:02}")
                };
                let Some(bone) = bones
                    .iter()
                    .find(|bone| bone.name.eq_ignore_ascii_case(&name))
                else {
                    break;
                };
                result.push(BoneControl {
                    name: bone.name.clone(),
                    axes: bone.axes,
                    translation: bone.translation,
                });
                count += 1;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn current_fx_bones_keep_last_rendered_turret_pose_until_next_draw() {
        let (mut battle, animations, players, actor) =
            super::super::laser_snapshot_tests::cached_gattling();
        let before = players.effect_bones("Muzzle");
        assert!(!before.is_empty());
        battle.simulation.entities[actor].ai.turret.angle += 0.8;
        battle.simulation.entities[actor].ai.turret.pitch -= 0.2;
        let retained = players.effect_bones("Muzzle");
        assert_eq!(
            before
                .iter()
                .map(|bone| bone.translation.map(f32::to_bits))
                .collect::<Vec<_>>(),
            retained
                .iter()
                .map(|bone| bone.translation.map(f32::to_bits))
                .collect::<Vec<_>>()
        );
        let recomputed = animations
            .effect_bones(&battle.simulation.entities[actor], &players, "Muzzle")
            .unwrap();
        assert_ne!(
            before
                .iter()
                .map(|bone| bone.axes.map(|axis| axis.map(f32::to_bits)))
                .collect::<Vec<_>>(),
            recomputed
                .iter()
                .map(|bone| bone.axes.map(|axis| axis.map(f32::to_bits)))
                .collect::<Vec<_>>()
        );
        assert!(players.effect_bones("ABSENT_FIREPOINT").is_empty());
        assert!(Players(Vec::new()).effect_bones("Muzzle").is_empty());
    }

    fn bone(name: &str) -> BoneControl {
        BoneControl {
            name: name.into(),
            translation: [0.0; 3],
            axes: [[0.0; 3]; 3],
        }
    }

    #[test]
    fn bone_effects_run_unnumbered_then_contiguous_numbered_draws_with_separate_limits() {
        let first = vec![bone("FIREPOINT"), bone("FirePoint01"), bone("FirePoint03")];
        let mut second = vec![bone("firepoint")];
        second.extend((1..=45).map(|index| bone(&format!("FirePoint{index:02}"))));
        let selected = effect_series(&[&first, &second], "FirePoint");
        assert_eq!(selected.len(), 42);
        assert_eq!(selected[0].name, "FIREPOINT");
        assert_eq!(selected[1].name, "firepoint");
        assert_eq!(selected[2].name, "FirePoint01");
        assert_eq!(selected[3].name, "FirePoint01");
        assert_eq!(selected[41].name, "FirePoint39");
    }
}
