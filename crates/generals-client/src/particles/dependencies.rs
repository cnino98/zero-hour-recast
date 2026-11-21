//! Resolve selected source FX references before creating particle templates.
use crate::skirmish_data::{blocks, definition, field, scalar};
use generals_assets::{
    content::{self, ContentFile},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeSet, error::Error};

struct SourceEffects {
    files: Vec<ContentFile>,
    particles: Vec<String>,
    effects: Vec<String>,
}

pub(super) fn position_particles(
    assets: &VirtualAssets,
    effects: &BTreeSet<String>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/fxlist.ini")?)?];
    let mut names = Vec::new();
    resolve_names(
        &files,
        effects.iter().cloned().collect(),
        "ParticleSystem",
        &mut names,
    )?;
    Ok(names)
}

pub fn position_sounds(
    assets: &VirtualAssets,
    effects: &BTreeSet<String>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/fxlist.ini")?)?];
    let mut names = Vec::new();
    resolve_names(
        &files,
        effects.iter().cloned().collect(),
        "Sound",
        &mut names,
    )?;
    Ok(names)
}

pub(super) fn weapon_particles(
    assets: &VirtualAssets,
    types: &[UnitType],
) -> Result<Vec<String>, Box<dyn Error>> {
    let SourceEffects {
        files,
        mut particles,
        effects,
    } = source_effects(assets, types)?;
    resolve_names(&files, effects, "ParticleSystem", &mut particles)?;
    Ok(particles)
}

/// Only sounds reachable from the same represented source FX graph as particles.
/// Loading dependencies does not submit an audio event or consume audio RNG.
pub fn weapon_sounds(
    assets: &VirtualAssets,
    types: &[UnitType],
) -> Result<Vec<String>, Box<dyn Error>> {
    let SourceEffects { files, effects, .. } = source_effects(assets, types)?;
    let mut sounds = Vec::new();
    resolve_names(&files, effects, "Sound", &mut sounds)?;
    Ok(sounds)
}

fn resolve_names(
    files: &[ContentFile],
    effects: Vec<String>,
    kind: &str,
    names: &mut Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let mut active = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for effect in effects {
        effect_names(files, &effect, kind, &mut active, &mut visited, names)?;
    }
    Ok(())
}

fn source_effects(
    assets: &VirtualAssets,
    types: &[UnitType],
) -> Result<SourceEffects, Box<dyn Error>> {
    let mut files = ["weapon", "fxlist", "object/weaponobjects"]
        .into_iter()
        .map(|stem| content::scan(assets.read(&format!("data/ini/{stem}.ini"))?))
        .collect::<Result<Vec<_>, generals_assets::AssetError>>()?;
    crate::skirmish_data::resolve_reskins(&mut files)?;
    let mut names = Vec::new();
    let mut effects = Vec::new();
    attached_particles(types, &mut names);
    for unit in types {
        if let Some(ignition) = super::effects::ignition_fx(&files, unit)? {
            append(&mut effects, ignition);
        }
        for ability in &unit.behavior.special_abilities {
            if let Some(name) = &ability.feedback.disable_particle {
                append(&mut names, name);
            }
        }
        for weapon in (0..unit.weapon_template_count())
            .filter_map(|index| unit.weapon_template(index))
            .filter(|weapon| !weapon.name.is_empty())
        {
            let lines = &definition(&files, "Weapon", &weapon.name)?.lines;
            if let Some(line) = field(lines, 0, "LaserName") {
                laser_particles(&files, scalar(line)?, &mut names)?;
            }
            for key in ["FireFX", "ProjectileDetonationFX"] {
                if let Some(line) = field(lines, 0, key) {
                    append(&mut effects, scalar(line)?);
                }
            }
            for (key, output) in [
                ("VeterancyFireFX", &mut effects),
                ("VeterancyProjectileExhaust", &mut names),
            ] {
                if let Some(line) = field(lines, 0, key) {
                    if line.tokens.len() != 3 || line.tokens[1] != "HEROIC" {
                        return Err("unsupported veterancy FX dependency".into());
                    }
                    append(output, &line.tokens[2]);
                }
            }
            if let Some(line) = field(lines, 0, "ProjectileExhaust") {
                append(&mut names, scalar(line)?);
            }
            if let Some(line) = field(lines, 0, "ProjectileObject") {
                let name = scalar(line)?;
                if !name.eq_ignore_ascii_case("NONE") {
                    for module in blocks(&definition(&files, "Object", name)?.lines, 0, "Behavior")?
                    {
                        if let Some(line) = field(module, 1, "GarrisonHitKillFX") {
                            append(&mut effects, scalar(line)?);
                        }
                    }
                }
            }
        }
    }
    for name in super::actor_fx::names(types) {
        append(&mut effects, name);
    }
    Ok(SourceEffects {
        files,
        particles: names,
        effects,
    })
}

fn attached_particles(types: &[UnitType], names: &mut Vec<String>) {
    for unit in types {
        for particle in unit
            .behavior
            .bone_fx
            .iter()
            .flat_map(|rules| &rules.slots)
            .flatten()
            .filter_map(|slot| slot.particle.as_ref())
        {
            append(names, &particle.name);
        }
        for particle in unit
            .behavior
            .damage_transitions
            .iter()
            .flat_map(|rules| rules.particles.iter().flatten())
        {
            append(names, &particle.template);
        }
        for helicopter in unit.helicopter_death_rules() {
            append(names, &helicopter.particle);
        }
        for nugget in
            generals_sim::skirmish::physical_ocl::lists(unit).flat_map(|list| &list.nuggets)
        {
            append(names, &nugget.particle);
        }
    }
}

fn laser_particles(
    files: &[ContentFile],
    name: &str,
    names: &mut Vec<String>,
) -> Result<(), Box<dyn Error>> {
    if name.eq_ignore_ascii_case("NONE") {
        return Ok(());
    }
    for module in blocks(&definition(files, "Object", name)?.lines, 0, "ClientUpdate")? {
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "LaserUpdate")
        {
            for key in ["MuzzleParticleSystem", "TargetParticleSystem"] {
                if let Some(line) = field(module, 1, key) {
                    append(names, scalar(line)?);
                }
            }
        }
    }
    Ok(())
}
fn effect_names(
    files: &[ContentFile],
    name: &str,
    kind: &str,
    active: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    names: &mut Vec<String>,
) -> Result<(), Box<dyn Error>> {
    if !active.insert(name.into()) {
        return Err(format!("cyclic source FX dependency {name}").into());
    }
    if visited.insert(name.into()) {
        let fx = crate::skirmish_data::fx_definition(files, name)?
            .ok_or_else(|| format!("missing FXList {name}"))?;
        let lines = &fx.lines;
        for block in blocks(lines, 0, kind)? {
            if let Some(line) = field(block, 1, "Name") {
                append(names, scalar(line)?);
            } else if kind != "Sound" {
                return Err("FX particle lacks name".into());
            }
        }
        for block in blocks(lines, 0, "FXListAtBonePos")? {
            if let Some(line) = field(block, 1, "FX") {
                let child = scalar(line)?;
                if !child.eq_ignore_ascii_case("NONE") {
                    effect_names(files, child, kind, active, visited, names)?;
                }
            }
        }
    }
    active.remove(name);
    Ok(())
}
pub(super) fn append(names: &mut Vec<String>, name: &str) {
    if !name.eq_ignore_ascii_case("NONE") && !names.iter().any(|item| item == name) {
        names.push(name.into());
    }
}
pub(super) fn slaves(files: &[ContentFile], names: &mut Vec<String>) -> Result<(), Box<dyn Error>> {
    let mut visited = BTreeSet::new();
    let mut active = BTreeSet::new();
    for name in names.clone() {
        slave_chain(files, &name, names, &mut active, &mut visited)?;
    }
    Ok(())
}
fn slave_chain(
    files: &[ContentFile],
    name: &str,
    names: &mut Vec<String>,
    active: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Result<(), Box<dyn Error>> {
    if !files
        .iter()
        .flat_map(|file| &file.declarations)
        .any(|declaration| declaration.kind == "ParticleSystem" && declaration.name == name)
    {
        // ParticleSystemManager::createParticleSystem(null) is an empty effect.
        eprintln!("Original particle template absent: {name}");
        names.retain(|candidate| candidate != name);
        return Ok(());
    }
    if !active.insert(name.into()) {
        return Err(format!("cyclic particle-system dependency {name}").into());
    }
    if visited.insert(name.into()) {
        let lines = &definition(files, "ParticleSystem", name)?.lines;
        for key in ["SlaveSystem", "PerParticleAttachedSystem"] {
            if let Some(line) = field(lines, 0, key) {
                let child = scalar(line)?;
                if !child.eq_ignore_ascii_case("NONE") {
                    append(names, child);
                    slave_chain(files, child, names, active, visited)?;
                }
            }
        }
    }
    active.remove(name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;

    #[test]
    fn absent_original_particle_references_are_null_without_losing_present_dependencies() {
        let files = [content::scan(Asset {
            path: "particles.ini".into(), origin: "native missing SCUD child template".into(),
            bytes: b"ParticleSystem ScudLaunchSmoke\nPerParticleAttachedSystem=ScudMissleLauncherSmoke\nSlaveSystem=Present\nEnd\nParticleSystem Present\nEnd\n".to_vec(),
        }).unwrap()];
        let mut names = vec!["MissingDirect".into(), "ScudLaunchSmoke".into()];
        slaves(&files, &mut names).unwrap();
        assert_eq!(names, ["ScudLaunchSmoke", "Present"]);
    }

    #[test]
    fn laser_dependencies_allow_muzzle_only_and_keep_both_named_endpoints_in_source_order() {
        let files = [content::scan(Asset {
            path: "lasers.ini".into(), origin: "native laser dependencies".into(),
            bytes: b"Object Defense\nClientUpdate=LaserUpdate Tag\nMuzzleParticleSystem=Muzzle\nEnd\nEnd\nObject Air\nClientUpdate=LaserUpdate Tag\nMuzzleParticleSystem=Muzzle\nTargetParticleSystem=Impact\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let mut names = Vec::new();
        laser_particles(&files, "Defense", &mut names).unwrap();
        assert_eq!(names, ["Muzzle"]);
        laser_particles(&files, "Air", &mut names).unwrap();
        assert_eq!(names, ["Muzzle", "Impact"]);
    }

    #[test]
    fn attached_and_slave_dependencies_share_cycle_detection_and_preserve_native_order() {
        let files = [content::scan(Asset {
            path: "particles.ini".into(), origin: "native attached dependencies".into(),
            bytes: b"ParticleSystem Smoke\nSlaveSystem=Flame\nPerParticleAttachedSystem=Sparks\nEnd\nParticleSystem Flame\nEnd\nParticleSystem Sparks\nSlaveSystem=Flare\nEnd\nParticleSystem Flare\nEnd\nParticleSystem Loop\nPerParticleAttachedSystem=Child\nEnd\nParticleSystem Child\nSlaveSystem=Loop\nEnd\n".to_vec(),
        }).unwrap()];
        let mut names = vec!["Smoke".into()];
        slaves(&files, &mut names).unwrap();
        assert_eq!(names, ["Smoke", "Flame", "Sparks", "Flare"]);
        assert!(slaves(&files, &mut vec!["Loop".into()]).is_err());
    }

    #[test]
    fn sound_dependency_closure_follows_nested_fx_without_admitting_unreached_lists() {
        let files = [content::scan(Asset {
            path: "effects.ini".into(), origin: "native FX sound dependencies".into(),
            bytes: b"FXList Selected\nSound\nName=First\nEnd\nFXListAtBonePos\nFX=Child\nEnd\nEnd\nFXList Child\nSound\nName=Nested\nEnd\nSound\nEnd\nEnd\nFXList Unused\nSound\nName=Unreached\nEnd\nEnd\nFXList Cycle\nFXListAtBonePos\nFX=Cycle\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let mut sounds = Vec::new();
        resolve_names(
            &files,
            vec!["Selected".into(), "Child".into()],
            "Sound",
            &mut sounds,
        )
        .unwrap();
        assert_eq!(sounds, ["First", "Nested"]);
        assert!(resolve_names(&files, vec!["Cycle".into()], "Sound", &mut Vec::new()).is_err());
    }

    #[test]
    fn selected_nested_fx_and_slave_dependencies_preserve_order_and_reject_fx_cycles() {
        let files = [content::scan(Asset {
            path: "effects.ini".into(), origin: "native FX dependency fixture".into(),
            bytes: b"FXList Gun\nParticleSystem\nName=Smoke\nEnd\nFXListAtBonePos\nFX=Flash\nEnd\nEnd\nFXList Flash\nParticleSystem\nName=Flare\nEnd\nEnd\nFXList Cycle\nFXListAtBonePos\nFX=Cycle\nEnd\nEnd\nParticleSystem Smoke\nSlaveSystem=Trailing\nEnd\nParticleSystem Flare\nEnd\nParticleSystem Trailing\nEnd\nParticleSystem Loop\nSlaveSystem=Loop\nEnd\n".to_vec(),
        }).unwrap()];
        let mut names = Vec::new();
        effect_names(
            &files,
            "Gun",
            "ParticleSystem",
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut names,
        )
        .unwrap();
        slaves(&files, &mut names).unwrap();
        assert_eq!(names, ["Smoke", "Flare", "Trailing"]);
        assert!(slaves(&files, &mut vec!["Loop".into()]).is_err());
        assert!(
            effect_names(
                &files,
                "Cycle",
                "ParticleSystem",
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
                &mut names
            )
            .is_err()
        );
    }
}
