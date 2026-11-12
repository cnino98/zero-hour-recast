//! GameLogic.cpp1826–1918; GameLOD.cpp347,394–419,584–605; `OptionPreferences`.
use super::{Error, VirtualAssets, content, value};
use std::{collections::BTreeMap, path::Path};

pub(super) struct Settings {
    pub(super) use_trees: bool,
    pub(super) force_fluff_to_prop: bool,
}

pub(super) fn creates_object(kind: &[String], fence_width: f32, settings: &Settings) -> bool {
    !(kind
        .iter()
        .any(|flag| flag == "OPTIMIZED_TREE" || flag == "PROP")
        || (settings.force_fluff_to_prop
            && fence_width == 0.0
            && kind.iter().any(|flag| flag == "CLEARED_BY_BUILD")))
}

pub(super) fn installed(assets: &VirtualAssets, root: &Path) -> Result<Settings, Box<dyn Error>> {
    let text = match std::fs::read_to_string(root.join("UserData/Options.ini")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let options: BTreeMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect();
    let globals = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let global = globals
        .declarations
        .iter()
        .find(|item| item.kind == "GameData")
        .ok_or("GameData missing")?;
    let level = options.get("StaticGameLOD").copied().ok_or(
        "StaticGameLOD is Unknown: native hardware recommendation is an unresolved P07 dependency",
    )?;
    let memory_bytes = if level.eq_ignore_ascii_case("Custom") {
        0
    } else {
        physical_memory()?
    };
    from_options(
        level,
        &options,
        memory_bytes,
        value(global, "UseTrees").is_some_and(|value| value.eq_ignore_ascii_case("Yes")),
        value(global, "UseShadowVolumes").is_some_and(|value| value.eq_ignore_ascii_case("Yes")),
    )
}

fn from_options(
    level: &str,
    options: &BTreeMap<&str, &str>,
    memory_bytes: u64,
    default_trees: bool,
    default_shadow_volumes: bool,
) -> Result<Settings, Box<dyn Error>> {
    let ordinal = ["Low", "Medium", "High", "VeryHigh", "Custom"].iter()
        .position(|name| level.eq_ignore_ascii_case(name)).ok_or(
            "StaticGameLOD is Unknown: native hardware recommendation is an unresolved P07 dependency",
        )?;
    let custom = ordinal == 4;
    let shadow_volumes = options
        .get("UseShadowVolumes")
        .map_or(default_shadow_volumes, |value| {
            value.eq_ignore_ascii_case("Yes")
        });
    let use_trees = if custom {
        options
            .get("ShowTrees")
            .map_or(default_trees, |value| value.eq_ignore_ascii_case("Yes"))
    } else {
        let bytes = num_traits::ToPrimitive::to_f32(&memory_bytes)
            .ok_or("Physical memory is not representable")?;
        bytes / 268_435_456.0 >= 0.94
    };
    let force_fluff_to_prop = !(custom && shadow_volumes) && ordinal < 2;
    Ok(Settings {
        use_trees,
        force_fluff_to_prop,
    })
}

fn physical_memory() -> Result<u64, Box<dyn Error>> {
    let mut system = sysinfo::System::new()?;
    system.refresh_memory();
    let bytes = system.total_memory();
    if bytes == 0 {
        return Err("Physical memory query returned no memory".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optimized_trees_and_props_have_no_logic_but_fences_keep_their_modules_at_low_lod() {
        let low = Settings {
            use_trees: true,
            force_fluff_to_prop: true,
        };
        let kind = |flags: &str| {
            flags
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert!(!creates_object(
            &kind("SHRUBBERY OPTIMIZED_TREE"),
            0.0,
            &low
        ));
        assert!(!creates_object(&kind("PROP"), 0.0, &low));
        assert!(!creates_object(
            &kind("CLEARED_BY_BUILD IMMOBILE"),
            0.0,
            &low
        ));
        assert!(creates_object(
            &kind("CLEARED_BY_BUILD IMMOBILE"),
            10.0,
            &low
        ));
        let high = Settings {
            use_trees: true,
            force_fluff_to_prop: false,
        };
        assert!(creates_object(
            &kind("CLEARED_BY_BUILD IMMOBILE"),
            0.0,
            &high
        ));
        assert!(creates_object(&kind("SHRUBBERY IMMOBILE"), 0.0, &high));
    }

    #[test]
    fn noncustom_tree_admission_uses_ram_and_high_keeps_logic_fluff() {
        let options = BTreeMap::from([("ShowTrees", "No")]);
        let high = from_options("High", &options, 8_589_934_592, false, false).unwrap();
        assert!(high.use_trees && !high.force_fluff_to_prop);
        let low = from_options("Low", &options, 134_217_728, true, true).unwrap();
        assert!(!low.use_trees && low.force_fluff_to_prop);
    }
    #[test]
    fn custom_tree_preference_and_unknown_hardware_branch_are_explicit() {
        let options = BTreeMap::from([("ShowTrees", "No"), ("UseShadowVolumes", "Yes")]);
        let custom = from_options("Custom", &options, 8_589_934_592, true, false).unwrap();
        assert!(!custom.use_trees && !custom.force_fluff_to_prop);
        assert!(from_options("Unknown", &options, 8_589_934_592, true, true).is_err());
    }
}
