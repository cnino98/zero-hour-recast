//! Installed infantry mesh checks for the public-bone/subobject fallback.
use super::{Animations, muzzle};
use crate::model_render;
use bevy::{mesh::VertexAttributeValues, prelude::*};
use generals_assets::w3d::{MeshPose, Model};
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn verify_rubble_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let flags = BTreeSet::from(["RUBBLE".to_owned()]);
        let mut count = 0;
        for (index, unit) in types
            .iter()
            .enumerate()
            .filter(|(_, unit)| unit.maintenance.rubble_height.is_some())
        {
            for draw in &self.draws[index] {
                let state = &draw.states[draw.select(&flags)];
                // Native condition selection can legally fall back when no
                // explicit RUBBLE state exists, including immediate DestroyDie.
                if let Some(model) = state.model.map(|index| &models[index]) {
                    let pose = model.controlled_pose(None, 0.0, &[])?;
                    if pose
                        .iter()
                        .flat_map(|mesh| mesh.positions.iter().flatten())
                        .any(|value| !value.is_finite())
                    {
                        return Err(
                            format!("{} original RUBBLE mesh is nonfinite", unit.name).into()
                        );
                    }
                }
            }
            count += 1;
        }
        println!(
            "Installed structure rubble-state selection: {count} templates select finite original model geometry"
        );
        Ok(())
    }

    pub fn verify_turret_articulation(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let mut checked = 0;
        let mut recoils = 0;
        let mut shared_bones = 0;
        for (type_index, unit) in types
            .iter()
            .enumerate()
            .filter(|(_, unit)| unit.turret.is_some())
        {
            for draw in &self.draws[type_index] {
                for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
                    let flags = damage.into_iter().map(str::to_owned).collect();
                    let state_index = draw.select(&flags);
                    let state = &draw.states[state_index];
                    let Some(model) = state.model.map(|index| &models[index]) else {
                        continue;
                    };
                    let controls = super::turret::controls(
                        &state.fields,
                        std::f32::consts::FRAC_PI_2,
                        // Dependency draws can bind only an elevation bone, as
                        // AVSGUNSHIP_GUN does on the Spectre's gun attachment.
                        if unit.turret.as_ref().is_some_and(|rules| rules.allows_pitch) {
                            std::f32::consts::FRAC_PI_4
                        } else {
                            0.0
                        },
                        None,
                    );
                    if !controls
                        .iter()
                        .any(|control| model.bone_position(&control.name).is_some())
                    {
                        continue;
                    }
                    let rest = model.controlled_pose(None, 0.0, &[])?;
                    let aimed = model.controlled_pose(None, 0.0, &controls)?;
                    let changed = rest
                        .iter()
                        .zip(&aimed)
                        .flat_map(|(rest, aimed)| rest.positions.iter().zip(&aimed.positions))
                        .any(|(rest, aimed)| {
                            rest.iter().zip(aimed).any(|(a, b)| (a - b).abs() > 0.01)
                        });
                    if !changed {
                        return Err(format!(
                            "{} {damage:?}: turret rotation did not move any model vertices",
                            unit.name
                        )
                        .into());
                    }
                    let (moving, overridden) =
                        verify_recoil(model, state_index, &state.fields, &draw.recoil)
                            .map_err(|error| format!("{} {damage:?}: {error}", unit.name))?;
                    recoils += moving;
                    shared_bones += overridden;
                    checked += 1;
                }
            }
            eprintln!("Installed turret articulation: {}", unit.name);
        }
        if checked == 0 || recoils == 0 {
            return Err("installed turret/recoil diagnostic checked no moving geometry".into());
        }
        eprintln!(
            "Verified {checked} installed turret condition poses rotate original vertices; {recoils} weapon recoil poses translate original barrel bones; {shared_bones} shared-bone poses preserve native later-slot capture overrides"
        );
        Ok(())
    }

    pub fn verify_weapon_visibility(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let names = [
            ("AmericaInfantryRanger", "BONE-GUN"),
            ("AmericaInfantryMissileDefender", "BONE_GUN"),
            ("ChinaInfantryRedguard", "BONE-GUN"),
            ("ChinaInfantryTankHunter", "BONE_GUN"),
            ("GLAInfantryRebel", "BONE-GUN"),
            ("GLAInfantryTunnelDefender", "BONE-GUN"),
        ];
        let mut checked = 0;
        for (name, weapon_bone) in names {
            let type_index = types
                .iter()
                .position(|unit| unit.name == name)
                .ok_or("weapon-visibility diagnostic infantry missing")?;
            let draw = &self.draws[type_index][0];
            for moving in [false, true] {
                let flags = if moving {
                    BTreeSet::from(["MOVING".to_owned()])
                } else {
                    BTreeSet::new()
                };
                let selected = draw.select(&flags);
                let state = &draw.states[selected];
                let model = &models[state.model.ok_or("infantry model missing")?];
                let motion = state.motions.first().ok_or("infantry motion missing")?;
                let clip = &self.clips[&motion.name];
                for frame in [0.0, f32::from(clip.frame_count - 1) * 0.5] {
                    let mut player = muzzle::Player::new();
                    let ticks = if matches!(name, "AmericaInfantryRanger" | "ChinaInfantryRedguard")
                    {
                        3
                    } else {
                        1
                    };
                    for tick in 0..ticks {
                        let mut pose = model.pose(clip, frame)?;
                        let body_before: Vec<_> = pose.iter().map(|mesh| mesh.visible).collect();
                        let fired = tick == 1;
                        player.update(
                            selected,
                            &state.fields,
                            model,
                            &draw.recoil,
                            tick,
                            fired.then_some((0, 0)),
                        );
                        player.apply(&mut pose);
                        check_meshes(model, &pose, &body_before, fired).map_err(|error| {
                            format!("{name} moving={moving} clip={} frame={frame} pulse-tick={tick}: {error}", clip.name)
                        })?;
                    }
                    if moving {
                        verify_rigid_weapon(model, &model.pose(clip, frame)?, weapon_bone)
                            .map_err(|error| {
                                format!("{name} clip={} frame={frame}: {error}", clip.name)
                            })?;
                    }
                    checked += 1;
                }
                eprintln!(
                    "Installed weapon visibility: {name}, moving={moving}, clip={}",
                    clip.name
                );
            }
        }
        eprintln!(
            "Verified {checked} installed rifle/launcher mesh poses retain body visibility and hide idle flashes; Ranger/Red Guard fallback flashes also show for the shot and hide next tick"
        );
        Ok(())
    }
    pub fn verify_alternate_weapon_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let ranger = types
            .iter()
            .position(|unit| unit.name == "AmericaInfantryRanger")
            .ok_or("Ranger missing from alternate art check")?;
        let draw = &self.draws[ranger][0];
        for (damage, expected) in [
            (None, "AIRngr_SKL.AIRngr_ATBB"),
            (Some("REALLYDAMAGED"), "AIRngr_SKL.AIRngr_ATB2B"),
        ] {
            let mut flags = BTreeSet::from(["USING_WEAPON_B".into(), "FIRING_B".into()]);
            flags.extend(damage.map(str::to_owned));
            let state = &draw.states[draw.select(&flags)];
            let motion = state
                .motions
                .first()
                .ok_or("Flashbang throw has no animation")?;
            if !motion.name.eq_ignore_ascii_case(expected) {
                return Err("Flashbang selected wrong source clip".into());
            }
            let clip = self
                .clips
                .get(&motion.name)
                .ok_or("Flashbang clip was not preloaded")?;
            let model = state
                .model
                .and_then(|index| models.get(index))
                .ok_or("Flashbang draw model missing")?;
            for frame in [0.0, f32::from(clip.frame_count.saturating_sub(1)) * 0.5] {
                let pose = model.pose(clip, frame)?;
                if pose
                    .iter()
                    .flat_map(|mesh| mesh.positions.iter().flatten())
                    .any(|value| !value.is_finite())
                {
                    return Err("Flashbang original throw yielded nonfinite mesh".into());
                }
            }
        }
        let defender = types
            .iter()
            .position(|unit| unit.name == "AmericaInfantryMissileDefender")
            .ok_or("Missile Defender missing from alternate art check")?;
        let rules = types[defender].behavior.special_abilities.iter()
            .find(|rules| rules.power.as_ref().is_some_and(|power|
                power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialMissileDefenderLaserGuidedMissiles))
            .ok_or("Laser Lock rules missing")?;
        let model = self
            .firing_model(models, defender, None, 1)
            .ok_or("Laser missile firing model absent")?;
        if model.bone_position(&rules.objects.bone).is_none() {
            return Err("Laser Lock authored muzzle bone absent".into());
        }
        eprintln!(
            "Alternate weapon art: both original Flashbang throw clips and Laser Lock muzzle bone verified"
        );
        Ok(())
    }
}

fn verify_recoil(
    model: &Model,
    state: usize,
    fields: &[Vec<String>],
    rules: &muzzle::Rules,
) -> Result<(usize, usize), Box<dyn Error>> {
    let mut muzzle = muzzle::Player::new();
    muzzle.update(state, fields, model, rules, 1, Some((0, 0)));
    let mut controls = Vec::new();
    muzzle.controls(&mut controls);
    let Some(fired) = controls.iter().find(|control| control.translation[0] < 0.0) else {
        return Ok((0, 0));
    };
    // handleClientRecoil loops weapon slots, and Control_Bone replaces the
    // previous capture. Quad PRIMARY and SECONDARY share the original bones.
    let effective = controls
        .iter()
        .rev()
        .find(|control| control.name.eq_ignore_ascii_case(&fired.name))
        .ok_or("fired recoil control missing")?;
    let rest = model
        .bone_position(&fired.name)
        .ok_or("installed recoil bone missing")?;
    let result = model
        .controlled_bone_position(&fired.name, None, 0.0, &controls)?
        .ok_or("installed fired recoil bone missing")?;
    let moved = rest.iter().zip(result).any(|(a, b)| (a - b).abs() >= 0.001);
    if effective.translation[0] == 0.0 {
        if moved {
            return Err("later idle weapon slot failed to replace shared recoil control".into());
        }
        Ok((0, 1))
    } else {
        if !moved {
            return Err("effective fired recoil bone stayed at rest".into());
        }
        Ok((1, 0))
    }
}

fn verify_rigid_weapon(
    model: &Model,
    pose: &[MeshPose],
    weapon_bone: &str,
) -> Result<(), Box<dyn Error>> {
    let bone = model
        .bone_names
        .iter()
        .position(|name| name == weapon_bone)
        .ok_or_else(|| format!("original infantry {weapon_bone} is missing"))?;
    let mut vertices_checked = 0;
    let mut maximum_relative_error = 0.0_f32;
    for (index, mesh) in model.meshes.iter().enumerate() {
        let Some(deformation) = &mesh.deformation else {
            continue;
        };
        let vertices: Vec<_> = (0..mesh.positions.len())
            .filter(|&vertex| deformation.vertex_bone(vertex) == Some(bone))
            .collect();
        for (offset, &first) in vertices.iter().enumerate() {
            for &second in &vertices[offset + 1..] {
                let original = Vec3::from_array(mesh.positions[first])
                    .distance(Vec3::from_array(mesh.positions[second]));
                let animated = Vec3::from_array(pose[index].positions[first])
                    .distance(Vec3::from_array(pose[index].positions[second]));
                // This is a diagnostic tolerance, not a geometry correction.
                // Source Fast_Slerp uses interpolated 1024-entry trig tables;
                // chained f32 rotations need not retain unit length exactly.
                let error = (animated - original).abs() / original.max(1.0);
                maximum_relative_error = maximum_relative_error.max(error);
                if error > 0.001 {
                    return Err(format!(
                        "{} rigid gun edge changed from {original} to {animated}",
                        mesh.name
                    )
                    .into());
                }
            }
        }
        vertices_checked += vertices.len();
        if !vertices.is_empty() {
            verify_weapon_material(model, pose, index)?;
        }
    }
    if vertices_checked < 3 {
        return Err("original rifle rigid vertex group is missing".into());
    }
    eprintln!(
        "Original rigid infantry weapon: {vertices_checked} {weapon_bone} vertices preserve pairwise lengths (maximum scaled error {maximum_relative_error}); original opaque material and UVs retained"
    );
    Ok(())
}

fn verify_weapon_material(
    model: &Model,
    pose: &[MeshPose],
    index: usize,
) -> Result<(), Box<dyn Error>> {
    for mut part in model_render::parts(model)? {
        let binding = part.binding.as_ref().ok_or("weapon part binding missing")?;
        if binding.mesh != index {
            continue;
        }
        let material = model.meshes[index]
            .materials
            .as_ref()
            .ok_or("weapon source material missing")?;
        if !matches!(part.material.alpha_mode, AlphaMode::Opaque)
            || material
                .shaders
                .iter()
                .any(|shader| shader[7] != 1 || shader[3] != 0 || shader[12] != 0)
            || part.texture.as_ref().is_none_or(|(name, _)| {
                !material
                    .textures
                    .iter()
                    .any(|texture| texture.name.eq_ignore_ascii_case(name))
            })
        {
            return Err("original rifle opaque material/texture mapping changed".into());
        }
        let Some(VertexAttributeValues::Float32x2(before)) =
            part.mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            return Err("original rifle UVs missing".into());
        };
        let before: Vec<_> = before.iter().map(|uv| uv.map(f32::to_bits)).collect();
        model_render::deform(&mut part.mesh, binding, pose)?;
        let Some(VertexAttributeValues::Float32x2(after)) =
            part.mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            return Err("rifle deformation removed original UVs".into());
        };
        if !before
            .iter()
            .copied()
            .eq(after.iter().map(|uv| uv.map(f32::to_bits)))
        {
            return Err("rifle deformation changed original UVs".into());
        }
    }
    Ok(())
}

fn check_meshes(
    model: &Model,
    pose: &[MeshPose],
    before: &[bool],
    shown: bool,
) -> Result<(), Box<dyn Error>> {
    let mut flashes = 0;
    let mut shown_flashes = 0;
    for mut part in model_render::parts(model)? {
        let binding = part
            .binding
            .as_ref()
            .ok_or("original part binding missing")?;
        let mesh = &model.meshes[binding.mesh];
        let flash = mesh
            .name
            .split_once('.')
            .is_some_and(|(_, short)| short.eq_ignore_ascii_case("MUZZLEFX01"));
        let expected = (!flash || shown) && before[binding.mesh];
        let visible = model_render::deform(&mut part.mesh, binding, pose)?;
        if visible != expected {
            return Err(format!("{} visibility={visible}, expected={expected}", mesh.name).into());
        }
        flashes += usize::from(flash);
        shown_flashes += usize::from(flash && visible);
    }
    if flashes == 0 || (shown && shown_flashes == 0) {
        return Err(
            "diagnostic found no installed visible muzzle geometry for the expected pulse".into(),
        );
    }
    Ok(())
}

impl Animations {
    pub fn verify_upgrade_art(&self, types: &[UnitType]) -> Result<(), Box<dyn Error>> {
        for (name, condition, expected) in [
            ("AmericaPowerPlant", "POWER_PLANT_UPGRADED", "ABPWRPLANT_A1"),
            ("GLAPalace", "USER_1", "UBPalaceEG"),
            ("GLABlackMarket", "GARRISONED", "UBBlkMktEG"),
        ] {
            let index = types
                .iter()
                .position(|unit| unit.name == name)
                .ok_or("upgrade art type is missing")?;
            let flags = BTreeSet::from([condition.to_owned()]);
            let found = self.draws[index].iter().any(|draw| {
                let state = &draw.states[draw.select(&flags)];
                state.name.eq_ignore_ascii_case(expected) && state.model.is_some()
            });
            if !found {
                return Err(format!("{name} {condition} did not preload/select original model {expected} in any original draw module").into());
            }
        }
        println!(
            "Installed upgrade art: original Control Rods, Fortified Palace and Fortified Black Market models are loaded and selected"
        );
        Ok(())
    }
}
