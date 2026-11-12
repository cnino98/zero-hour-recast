//! Original Technical draw-module attachment and both native crate weapon states.
use super::{Animations, turret};
use generals_assets::w3d::Model;
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn verify_salvage_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        for (index, unit) in types
            .iter()
            .enumerate()
            .filter(|(_, unit)| !unit.salvage.weapons.is_empty())
        {
            let technical = unit.name.starts_with("GLAVehicleTechnicalChassis");
            if technical {
                let draws = &self.draws[index];
                if draws.len() != 2 || draws[1].attach_bone.as_deref() != Some("Dum_Turret") {
                    return Err(
                        "Technical must use both original draw modules and attachment bone".into(),
                    );
                }
                let chassis = &draws[0].states[draws[0].select(&BTreeSet::new())];
                let bone = models[chassis.model.ok_or("Technical chassis model absent")?]
                    .bone_position("Dum_Turret")
                    .ok_or("Technical attachment bone absent")?;
                if bone.map(f32::to_bits) != draws[1].attach_offset.map(f32::to_bits) {
                    return Err(
                        "Technical cached pristine draw offset differs from source bone".into(),
                    );
                }
            }
            for level in 0..=2 {
                let weapon_index = if level == 0 { 0 } else { level + 2 };
                for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
                    let state = self
                        .firing_state(index, damage, weapon_index)
                        .ok_or("salvage firing state missing")?;
                    let model = &models[state.model.ok_or("salvage firing model missing")?];
                    let expected = if technical {
                        ["20Cal", "50Cal", "RPG"][level]
                    } else if unit.name == "GLATankScorpion" {
                        ["Turret01", "TurretUP01", "TurretUP01"][level]
                    } else if unit.name == "GLAVehicleQuadCannon" {
                        ["TURRET", "TURRETUP01", "TURRETUP02"][level]
                    } else {
                        ["Turret", "TurretUp01", "TurretUp02"][level]
                    };
                    if !state.fields.iter().any(|line| {
                        line.first().is_some_and(|key| key == "ShowSubObject")
                            // RenderObjClass::Get_Sub_Object_By_Name uses stricmp.
                            && line[1..]
                                .iter()
                                .any(|name| name.eq_ignore_ascii_case(expected))
                    }) {
                        return Err(format!(
                            "{} tier{level} lacks original {expected} subobject",
                            unit.name
                        )
                        .into());
                    }
                    let controls = turret::controls(&state.fields, 0.4, 0.0, None);
                    let clip = state
                        .motions
                        .first()
                        .and_then(|motion| self.clips.get(&motion.name));
                    let pose = model.controlled_pose(clip, 3.0, &controls)?;
                    if pose
                        .iter()
                        .filter(|mesh| mesh.visible)
                        .flat_map(|mesh| mesh.positions.iter().flatten())
                        .any(|value| !value.is_finite())
                    {
                        return Err("salvage model emitted invalid posed coordinates".into());
                    }
                    let weapon = unit
                        .weapon_template(weapon_index)
                        .ok_or("salvage weapon missing")?;
                    if weapon
                        .motion
                        .as_ref()
                        .is_none_or(|motion| motion.launches.iter().any(Vec::is_empty))
                    {
                        return Err(format!(
                            "{} tier{level} {damage:?}: {} launch binding missing",
                            unit.name, weapon.name
                        )
                        .into());
                    }
                }
            }
        }
        println!(
            "Installed salvage art: three Technical chassis, original attached gunner, and all admitted salvage turret/barrel states and launch banks passed"
        );
        Ok(())
    }
}

impl Animations {
    pub fn verify_salvage_draw(
        &self,
        unit: &generals_sim::skirmish::Entity,
        players: &super::Players,
    ) -> Result<Option<usize>, Box<dyn Error>> {
        let draws = &self.draws[unit.type_index];
        if draws.len() != players.0.len() {
            return Err("salvage draw modules have not been instantiated".into());
        }
        for (draw, player) in draws.iter().zip(&players.0) {
            let state = draw
                .states
                .get(player.state)
                .ok_or("salvage current draw state absent")?;
            if player.model != state.render_model || player.model.is_none() {
                return Err("original salvage model state is not displayed".into());
            }
            if draw.attach_bone.is_some() {
                let level = unit.salvage_level();
                let name = ["20Cal", "50Cal", "RPG"][usize::from(level)];
                if !state.fields.iter().any(|line| {
                    line.first().is_some_and(|key| key == "ShowSubObject")
                        && line[1..].iter().any(|item| item.eq_ignore_ascii_case(name))
                }) {
                    return Err("Technical gunner shows wrong weapon subobject".into());
                }
                if self
                    .inspect(players, unit.type_index)?
                    .iter()
                    .all(|draw| draw.module != 1 || draw.clip.is_empty() || draw.bounds.is_none())
                {
                    return Err("Technical original animated gunner geometry absent".into());
                }
            }
        }
        Ok(draws
            .iter()
            .zip(&players.0)
            .find_map(|(draw, player)| draw.attach_bone.as_ref().and(player.model)))
    }
}

impl Animations {
    pub fn verify_weapon_research_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        for name in ["AmericaVehicleHumvee", "GLATankScorpion"] {
            let index = types
                .iter()
                .position(|unit| unit.name == name)
                .ok_or("research art unit absent")?;
            let unit = &types[index];
            // Impact/death weapons follow the conditional actor weapon sets;
            // they fire at the dying projectile and have no actor launch bone.
            for weapon_index in 7..unit.weapon_set_rules.next_template_index() {
                let (slot, flags) = unit.weapon_context(weapon_index);
                for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
                    let state = self
                        .firing_state(index, damage, weapon_index)
                        .ok_or("researched firing state absent")?;
                    let model = &models[state.model.ok_or("researched model absent")?];
                    let weapon = unit
                        .weapon_template(weapon_index)
                        .ok_or("researched weapon absent")?;
                    if weapon
                        .motion
                        .as_ref()
                        .is_none_or(|motion| motion.launches.iter().any(Vec::is_empty))
                    {
                        return Err(format!(
                            "{name} {} {damage:?}: researched launch binding absent",
                            weapon.name
                        )
                        .into());
                    }
                    if name == "AmericaVehicleHumvee"
                        && !state.fields.iter().any(|line| {
                            line.first().is_some_and(|key| key == "Turret")
                                && line.get(1).is_some_and(|name| name == "TurretUp01")
                        })
                    {
                        return Err("TOW does not select original upgraded turret".into());
                    }
                    if name == "GLATankScorpion" && slot == 1 {
                        let count = if flags & 4 == 0 { 1 } else { 2 };
                        let clips = [(0, 0), (count, count), (0, 0)];
                        let feedback = super::muzzle::projectile_visibility(
                            &state.fields,
                            clips,
                            self.draws[index][0].recoil.projectile_slots,
                        );
                        if feedback.len() != usize::try_from(count)?
                            || feedback.iter().any(|(name, visible)| {
                                !visible
                                    || !model.meshes.iter().any(|mesh| {
                                        mesh.name
                                            .rsplit('.')
                                            .next()
                                            .is_some_and(|short| short.eq_ignore_ascii_case(name))
                                    })
                            })
                        {
                            return Err(
                                "Scorpion original loaded-rack projectile subobjects absent".into(),
                            );
                        }
                    }
                }
            }
        }
        println!(
            "Installed researched weapon art: original Humvee TOW turret, Scorpion combined crate racks, healthy/damaged models, launch bones and projectile clip subobjects passed"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_salvage_and_research_models_bind_actor_weapons_not_impact_callbacks() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let mut data = crate::skirmish_data::load(&assets, &[]).unwrap();
        let animations = super::super::load(&assets, &mut data.models, &data.types).unwrap();
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations).unwrap();
        // SCUD authors uppercase TURRET/TURRETUP01, while Marauder uses mixed case.
        // Both must validate through the same native case-insensitive name lookup.
        animations
            .verify_salvage_art(&data.models, &data.types)
            .unwrap();
        animations
            .verify_weapon_research_art(&data.models, &data.types)
            .unwrap();
        let scorpion_index = data
            .types
            .iter()
            .position(|unit| unit.name == "GLATankScorpion")
            .unwrap();
        let gun = data.types[scorpion_index].weapon_in_slot(0).unwrap();
        assert_eq!(gun.name, "ScorpionTankGun");
        let shell = gun.motion.as_ref().unwrap().projectile_type.unwrap();
        let toxin = data.types[shell]
            .behavior
            .death_weapons
            .iter()
            .filter_map(|rules| rules.weapon.as_ref())
            .find(|weapon| weapon.name == "ToxinShellWeapon")
            .unwrap();
        let toxin_motion = toxin
            .motion
            .as_ref()
            .expect("non-projectile death weapon keeps its weapon motion");
        assert!(toxin_motion.projectile_type.is_none());
        assert!(toxin.hazard.is_some());
        assert!(
            (0..data.types[scorpion_index].weapon_template_count())
                .filter_map(|index| data.types[scorpion_index].weapon_template(index))
                .all(|weapon| weapon.name != "ToxinShellWeapon")
        );
        let scorpion = &mut data.types[scorpion_index];
        let rocket = scorpion
            .weapon_set_rules
            .templates
            .iter_mut()
            .find(|weapon| weapon.slot == 1)
            .unwrap();
        assert!(rocket.rules.motion.take().is_some());
        let error = animations
            .verify_weapon_research_art(&data.models, &data.types)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("researched launch binding absent")
        );
    }
}
