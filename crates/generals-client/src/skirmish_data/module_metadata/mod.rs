//! Ordered native `ThingTemplate` module metadata, SH 2bfaea60, Zero Hour/CRC=0.
//! This interprets only module-affecting fields; other effective data has its own consumers.
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use std::collections::{BTreeMap, BTreeSet};
mod effective;
mod registrations;
#[path = "registry.rs"]
mod registry;
pub use effective::reconcile;

const UPDATE: u32 = 0x0001;
const BODY: u32 = 0x0020;

#[derive(Clone)]
pub struct SourceLocation {
    pub path: String,
    pub origin: String,
    pub line: u32,
}

#[derive(Clone, Copy)]
enum Category {
    Behavior,
    Draw,
    ClientUpdate,
}
impl Category {
    const fn index(self) -> usize {
        match self {
            Self::Behavior => 0,
            Self::Draw => 1,
            Self::ClientUpdate => 2,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Inheritance {
    Ordinary,
    Inheritable,
    OverrideableByLikeKind,
}

#[derive(Clone)]
pub struct ModuleInfo {
    pub family: String,
    pub tag: String,
    pub interface_mask: u32,
    pub upgrade: generals_sim::skirmish::module_updates::UpgradeData,
    pub ai_data: bool,
    pub copied_from_default: bool,
    pub inheritance: Inheritance,
    pub source: SourceLocation,
}

#[derive(Clone)]
pub struct TemplateMetadata {
    // Native has three separate ModuleInfo vectors; Body belongs to Behavior.
    pub modules: [Vec<ModuleInfo>; 3],
    pub kind_of: BTreeSet<String>,
    pub trainable: bool,
    weapon_sets: Vec<[bool; 3]>,
    weapons_copied: bool,
}
impl TemplateMetadata {
    const fn empty() -> Self {
        Self {
            modules: [Vec::new(), Vec::new(), Vec::new()],
            kind_of: BTreeSet::new(),
            trainable: false,
            weapon_sets: Vec::new(),
            weapons_copied: false,
        }
    }
    fn mark_copied(&mut self) {
        self.weapons_copied = true;
        for entries in &mut self.modules {
            for entry in entries {
                entry.copied_from_default = true;
            }
        }
    }
    fn remove(&mut self, tag: &str) -> Result<String, String> {
        // clearModuleDataWithTag removes every matching entry; selected release profile
        // does not execute debug-only duplicate-tag validation. Preserve category order.
        let mut removed = None;
        for entries in &mut self.modules {
            entries.retain(|entry| {
                if entry.tag == tag {
                    removed = Some(entry.family.clone());
                    false
                } else {
                    true
                }
            });
        }
        removed.ok_or_else(|| format!("RemoveModule/ReplaceModule tag not found: {tag}"))
    }
    fn clear_defaults(&mut self, mask: u32, family: &str) {
        const IMMUNE: &[&str] = &[
            "AIRCRAFT",
            "SHRUBBERY",
            "OPTIMIZED_TREE",
            "STRUCTURE",
            "DRAWABLE_ONLY",
            "MOB_NEXUS",
            "IGNORED_IN_GUI",
            "CLEARED_BY_BUILD",
            "DEFENSIVE_WALL",
            "BALLISTIC_MISSILE",
            "SUPPLY_SOURCE",
            "BOAT",
            "INERT",
            "BRIDGE",
            "LANDMARK_BRIDGE",
            "BRIDGE_TOWER",
        ];
        const CANDIDATE: &[&str] = &["SCORE", "VEHICLE", "INFANTRY", "PORTABLE_STRUCTURE"];
        let disallowed = IMMUNE.iter().any(|flag| self.kind_of.contains(*flag));
        let candidate = CANDIDATE.iter().any(|flag| self.kind_of.contains(*flag));
        let trainable = self.trainable;
        for entries in &mut self.modules {
            entries.retain(|entry| {
                if !entry.copied_from_default || entry.interface_mask & mask == 0 {
                    return true;
                }
                match entry.inheritance {
                    Inheritance::Inheritable => {
                        entry.tag != "ModuleTag_DefaultAutoHealBehavior" || trainable
                    }
                    Inheritance::OverrideableByLikeKind => {
                        entry.family != family && !disallowed && candidate
                    }
                    Inheritance::Ordinary => false,
                }
            });
        }
    }
}

#[derive(Clone, Copy)]
enum ParseMode {
    Ordinary,
    Inheritable,
    Overrideable,
    AddReplace,
}

/// Selected names plus their default/reskin ancestry only. Unadmitted conditional
/// families are not silently accepted or promoted by this bounded registry.
pub fn load(
    baseline: &[ContentFile],
    overrides: &[ContentFile],
    selected: &[&str],
) -> Result<BTreeMap<String, TemplateMetadata>, String> {
    let wanted = selected_ancestry(baseline, selected)?;
    let mut definitions: BTreeMap<String, TemplateMetadata> = BTreeMap::new();
    for file in baseline {
        for declaration in &file.declarations {
            if !wanted.contains(&declaration.name)
                || !matches!(declaration.kind.as_str(), "Object" | "ObjectReskin")
            {
                continue;
            }
            if definitions.contains_key(&declaration.name) {
                return Err(format!(
                    "duplicate baseline definition: {}",
                    declaration.name
                ));
            }
            let mut value = if declaration.name == "DefaultThingTemplate" {
                TemplateMetadata::empty()
            } else {
                let parent = declaration
                    .parent
                    .as_deref()
                    .unwrap_or("DefaultThingTemplate");
                let mut value = definitions
                    .get(parent)
                    .ok_or_else(|| {
                        format!("{} requires earlier parent {parent}", declaration.name)
                    })?
                    .clone();
                value.mark_copied();
                value
            };
            if declaration.kind == "ObjectReskin" {
                validate_reskin(declaration)?;
            }
            apply(
                &mut value,
                file,
                &declaration.lines,
                0,
                ParseMode::Ordinary,
                false,
                None,
            )?;
            definitions.insert(declaration.name.clone(), value);
        }
    }
    for name in &wanted {
        if !definitions.contains_key(name) {
            return Err(format!("missing admitted metadata definition: {name}"));
        }
    }
    for file in overrides {
        for declaration in &file.declarations {
            if !wanted.contains(&declaration.name) {
                continue;
            }
            if declaration.kind == "ObjectReskin" {
                return Err(
                    "map-local reskin requires its admitted native parent-copy consumer".into(),
                );
            }
            if declaration.kind != "Object" {
                continue;
            }
            let value = definitions
                .get_mut(&declaration.name)
                .ok_or("override target absent")?;
            // ThingFactory::parseObjectDefinition -> newOverride -> setCopiedFromDefault,
            // once for each declaration, not once per map or once per target file.
            value.mark_copied();
            apply(
                value,
                file,
                &declaration.lines,
                0,
                ParseMode::Ordinary,
                true,
                None,
            )?;
        }
    }
    Ok(definitions)
}

fn selected_ancestry(
    baseline: &[ContentFile],
    selected: &[&str],
) -> Result<BTreeSet<String>, String> {
    let mut wanted: BTreeSet<String> = selected.iter().map(|name| (*name).into()).collect();
    wanted.insert("DefaultThingTemplate".into());
    let mut parents = BTreeMap::new();
    for file in baseline {
        for declaration in &file.declarations {
            if declaration.kind == "ObjectReskin" {
                parents.insert(
                    declaration.name.as_str(),
                    declaration
                        .parent
                        .as_deref()
                        .ok_or("reskin parent missing")?,
                );
            }
        }
    }
    // Close ancestry before consuming definitions, while requiring source encounter
    // order when the actual copy executes. This is not recursive out-of-order parsing.
    loop {
        let before = wanted.len();
        let additions: Vec<String> = wanted
            .iter()
            .filter_map(|name| parents.get(name.as_str()).map(|parent| (*parent).into()))
            .collect();
        wanted.extend(additions);
        if wanted.len() == before {
            break;
        }
    }
    Ok(wanted)
}

fn validate_reskin(declaration: &Declaration) -> Result<(), String> {
    // Existing runtime admission represents only original Draw/geometry reskins.
    // Reject unrepresented semantic changes instead of treating reskin as Object.
    for line in &declaration.lines {
        if line.depth == Some(0)
            && !matches!(
                line.tokens.first().map(String::as_str),
                Some(
                    "Draw"
                        | "End"
                        | "Geometry"
                        | "GeometryMajorRadius"
                        | "GeometryMinorRadius"
                        | "GeometryHeight"
                        | "GeometryIsSmall"
                )
            )
        {
            return Err(format!(
                "{}: unadmitted reskin field at {}",
                declaration.name, line.line
            ));
        }
    }
    Ok(())
}

fn end_of_block(lines: &[ContentLine], start: usize, depth: u32) -> Result<usize, String> {
    (start + 1..lines.len())
        .find(|index| {
            lines[*index].depth == Some(depth)
                && lines[*index]
                    .tokens
                    .first()
                    .is_some_and(|token| token.eq_ignore_ascii_case("End"))
        })
        .ok_or_else(|| format!("unterminated metadata block at {}", lines[start].line))
}

fn parse_upgrade_fields(
    entry: &mut ModuleInfo,
    lines: &[ContentLine],
    depth: u32,
) -> Result<(), String> {
    for field in lines {
        if field.depth == Some(depth) {
            match field.tokens[0].as_str() {
                "TriggeredBy" => entry.upgrade.triggers = field.tokens[1..].to_vec(),
                "RemovesUpgrades" => entry.upgrade.removals = field.tokens[1..].to_vec(),
                "ConflictsWith" => entry.upgrade.conflicts = field.tokens[1..].to_vec(),
                "RequiresAllTriggers" => {
                    let token = field.tokens.get(1).ok_or("missing RequiresAllTriggers")?;
                    entry.upgrade.requires_all = if token.eq_ignore_ascii_case("Yes") {
                        true
                    } else if token.eq_ignore_ascii_case("No") {
                        false
                    } else {
                        return Err("invalid RequiresAllTriggers boolean".into());
                    };
                }
                "FXListUpgrade" => {
                    entry.upgrade.fx = field.tokens.get(1).cloned().unwrap_or_default();
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn apply(
    value: &mut TemplateMetadata,
    file: &ContentFile,
    lines: &[ContentLine],
    depth: u32,
    mode: ParseMode,
    overriding: bool,
    replacement: Option<(&str, &str)>,
) -> Result<(), String> {
    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index];
        let actual = line
            .depth
            .ok_or_else(|| format!("unknown INI nesting {}:{}", file.path, line.line))?;
        if actual != depth {
            index += 1;
            continue;
        }
        let key = line.tokens.first().ok_or("empty metadata source line")?;
        if key.eq_ignore_ascii_case("End") {
            break;
        }
        match key.as_str() {
            "KindOf" => parse_flags(&mut value.kind_of, &line.tokens[1..])?,
            "IsTrainable" => {
                let token = line.tokens.get(1).ok_or("IsTrainable token missing")?;
                value.trainable = if token.eq_ignore_ascii_case("YES") {
                    true
                } else if token.eq_ignore_ascii_case("NO") {
                    false
                } else {
                    return Err("invalid IsTrainable boolean".into());
                };
            }
            "InheritableModule" | "OverrideableByLikeKind" | "AddModule" | "ReplaceModule" => {
                if !matches!(mode, ParseMode::Ordinary) {
                    return Err("nested module parsing modes are invalid".into());
                }
                let end = end_of_block(lines, index, depth)?;
                let next_mode = match key.as_str() {
                    "InheritableModule" => ParseMode::Inheritable,
                    "OverrideableByLikeKind" => ParseMode::Overrideable,
                    _ => ParseMode::AddReplace,
                };
                let removed = if key == "ReplaceModule" {
                    let tag = line.tokens.get(1).ok_or("ReplaceModule tag missing")?;
                    Some((value.remove(tag)?, tag.as_str()))
                } else {
                    None
                };
                apply(
                    value,
                    file,
                    &lines[index + 1..end],
                    depth + 1,
                    next_mode,
                    overriding,
                    removed
                        .as_ref()
                        .map(|(family, tag)| (family.as_str(), *tag)),
                )?;
                index = end;
            }
            "RemoveModule" => {
                if !matches!(mode, ParseMode::Ordinary) {
                    return Err("nested RemoveModule is invalid".into());
                }
                value.remove(line.tokens.get(1).ok_or("RemoveModule tag missing")?)?;
            }
            "Behavior" | "Body" | "Draw" | "ClientUpdate" => {
                let end = end_of_block(lines, index, depth)?;
                parse_module(value, file, line, mode, overriding, replacement)?;
                let category = if key == "Draw" {
                    Category::Draw
                } else if key == "ClientUpdate" {
                    Category::ClientUpdate
                } else {
                    Category::Behavior
                };
                let entry = value.modules[category.index()]
                    .last_mut()
                    .expect("parsed module");
                parse_upgrade_fields(entry, &lines[index + 1..end], depth + 1)?;
                index = end;
            }
            "WeaponSet" => {
                let end = end_of_block(lines, index, depth)?;
                parse_weapon_set(value, &lines[index + 1..end], depth)?;
                index = end;
            }
            "ArmorSet"
            | "UnitSpecificSounds"
            | "UnitSpecificFX"
            | "Prerequisites"
            | "AttackAreaDecal"
            | "TargetingReticleDecal"
            | "GridDecalTemplate"
            | "DeliveryDecal" => {
                // Skip the complete non-metadata block, including its End at this
                // depth. Otherwise that End would terminate the enclosing Object
                // before later Body/Behavior fields (DefaultThingTemplate has
                // ArmorSet and UnitSpecificSounds before its InactiveBody).
                // These values remain the existing effective loaders' responsibility.
                index = end_of_block(lines, index, depth)?;
            }
            _ => {} // Scalar/art/body-data semantics are consumed by existing effective loaders.
        }
        index += 1;
    }
    Ok(())
}

fn parse_weapon_set(
    value: &mut TemplateMetadata,
    lines: &[ContentLine],
    depth: u32,
) -> Result<(), String> {
    if value.weapons_copied {
        value.weapon_sets.clear();
        value.weapons_copied = false;
    }
    let mut slots = [false; 3];
    for field in lines {
        if field.depth == Some(depth + 1)
            && field.tokens.first().is_some_and(|token| token == "Weapon")
        {
            let token = field.tokens.get(1).ok_or("weapon slot token missing")?;
            // WeaponTemplateSet::parseWeapon uses INI::scanIndexList,
            // whose pinned comparison is stricmp, not uppercase-only.
            let slot = if token.eq_ignore_ascii_case("PRIMARY") {
                0
            } else if token.eq_ignore_ascii_case("SECONDARY") {
                1
            } else if token.eq_ignore_ascii_case("TERTIARY") {
                2
            } else {
                return Err("unknown weapon slot".into());
            };
            slots[slot] = !field
                .tokens
                .get(2)
                .ok_or("weapon template token missing")?
                .eq_ignore_ascii_case("NONE");
        }
    }
    value.weapon_sets.push(slots);
    Ok(())
}

fn parse_module(
    value: &mut TemplateMetadata,
    file: &ContentFile,
    line: &ContentLine,
    mode: ParseMode,
    overriding: bool,
    replacement: Option<(&str, &str)>,
) -> Result<(), String> {
    let key = &line.tokens[0];
    let family = line.tokens.get(1).ok_or("module family missing")?;
    let tag = line.tokens.get(2).ok_or("module tag missing")?;
    let (mask, ai) = registry::registered(family).ok_or_else(|| {
        format!(
            "unadmitted module family {family} at {}:{}",
            file.path, line.line
        )
    })?;
    let category = match key.as_str() {
        "Behavior" | "Body" => Category::Behavior,
        "Draw" => Category::Draw,
        _ => Category::ClientUpdate,
    };
    if (key == "Body") != (mask & BODY != 0) {
        return Err("native Body interface category mismatch".into());
    }
    if matches!(category, Category::Draw) && mask & 0x0400 == 0
        || matches!(category, Category::ClientUpdate) && mask & 0x0800 == 0
        || matches!(category, Category::Behavior) && mask & (0x0400 | 0x0800) != 0
    {
        return Err(format!("module factory category mismatch for {family}"));
    }
    if overriding {
        if !matches!(mode, ParseMode::AddReplace) {
            return Err("native override requires AddModule/ReplaceModule".into());
        }
    } else {
        value.clear_defaults(mask, family);
    }
    if replacement.is_some_and(|(old_family, old_tag)| old_family != family || old_tag == tag) {
        return Err("ReplaceModule requires same family and a new tag".into());
    }
    let entries = &mut value.modules[category.index()];
    if ai {
        entries.retain(|entry| !entry.ai_data);
    }
    entries.push(ModuleInfo {
        family: family.clone(),
        tag: tag.clone(),
        interface_mask: mask,
        upgrade: generals_sim::skirmish::module_updates::UpgradeData::none(),
        ai_data: ai,
        copied_from_default: false,
        inheritance: match mode {
            ParseMode::Inheritable => Inheritance::Inheritable,
            ParseMode::Overrideable => Inheritance::OverrideableByLikeKind,
            _ => Inheritance::Ordinary,
        },
        source: SourceLocation {
            path: file.path.clone(),
            origin: file.origin.clone(),
            line: line.line,
        },
    });
    Ok(())
}

fn parse_flags(flags: &mut BTreeSet<String>, tokens: &[String]) -> Result<(), String> {
    let mut normal = false;
    let mut relative = false;
    for token in tokens {
        if token.eq_ignore_ascii_case("NONE") {
            if normal || relative {
                return Err("mixed NONE/KindOf operations".into());
            }
            flags.clear();
            break;
        }
        let (relative_token, name) = match token.as_bytes().first() {
            Some(b'+') => (Some(true), &token[1..]),
            Some(b'-') => (Some(false), &token[1..]),
            _ => (None, token.as_str()),
        };
        let name =
            registry::kind_name(name).ok_or_else(|| format!("unknown KindOf flag: {name}"))?;
        if let Some(add) = relative_token {
            if normal {
                return Err("mixed normal/relative KindOf operations".into());
            }
            relative = true;
            if add {
                flags.insert(name.to_owned());
            } else {
                flags.remove(name);
            }
        } else {
            if relative {
                return Err("mixed normal/relative KindOf operations".into());
            }
            if !normal {
                flags.clear();
            }
            normal = true;
            flags.insert(name.to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
mod diagnostic;
