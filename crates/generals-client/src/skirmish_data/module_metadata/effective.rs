use super::{ModuleInfo, TemplateMetadata};
use generals_assets::content::{ContentFile, ContentLine};
use std::collections::BTreeMap;

fn copy(line: &ContentLine, depth: u32) -> ContentLine {
    ContentLine {
        line: line.line,
        tokens: line.tokens.clone(),
        raw: line.raw.clone(),
        assigned: line.assigned,
        depth: Some(depth),
    }
}

fn module_lines(
    entry: &ModuleInfo,
    baseline: &[ContentFile],
    overrides: &[ContentFile],
) -> Result<Vec<ContentLine>, String> {
    let file = baseline
        .iter()
        .chain(overrides)
        .find(|file| file.path == entry.source.path && file.origin == entry.source.origin)
        .ok_or("effective module source file missing")?;
    let lines = file
        .declarations
        .iter()
        .find_map(|declaration| {
            declaration
                .lines
                .iter()
                .position(|line| line.line == entry.source.line)
                .map(|index| &declaration.lines[index..])
        })
        .ok_or("effective module source body missing")?;
    let depth = lines[0]
        .depth
        .ok_or("effective module source depth missing")?;
    let end = super::end_of_block(lines, 0, depth)?;
    lines[..=end]
        .iter()
        .map(|line| {
            let relative = line
                .depth
                .ok_or("effective module source depth missing")?
                .checked_sub(depth)
                .ok_or("effective module body crosses source block")?;
            Ok(copy(line, relative))
        })
        .collect()
}

pub fn reconcile(
    files: &mut [ContentFile],
    baseline: &[ContentFile],
    overrides: &[ContentFile],
    metadata: &BTreeMap<String, TemplateMetadata>,
) -> Result<(), String> {
    for file in files {
        for declaration in &mut file.declarations {
            if declaration.kind != "Object" {
                continue;
            }
            let Some(value) = metadata.get(&declaration.name) else {
                continue;
            };
            let mut lines = Vec::new();
            let mut index = 0;
            // Only the last End closes the object. Retained WeaponSet,
            // ArmorSet and other non-module blocks also end at depth zero.
            while index + 1 < declaration.lines.len() {
                let line = &declaration.lines[index];
                if line.depth == Some(0) {
                    let key = line.tokens.first().ok_or("empty effective object line")?;
                    if matches!(
                        key.as_str(),
                        "Body"
                            | "Behavior"
                            | "Draw"
                            | "ClientUpdate"
                            | "InheritableModule"
                            | "OverrideableByLikeKind"
                    ) {
                        index = super::end_of_block(&declaration.lines, index, 0)? + 1;
                        continue;
                    }
                    if matches!(key.as_str(), "KindOf" | "IsTrainable") {
                        index += 1;
                        continue;
                    }
                }
                lines.push(copy(
                    line,
                    line.depth.ok_or("effective object depth missing")?,
                ));
                index += 1;
            }
            for entries in &value.modules {
                for entry in entries {
                    lines.extend(module_lines(entry, baseline, overrides)?);
                }
            }
            for (key, tokens) in [
                (
                    "KindOf",
                    if value.kind_of.is_empty() {
                        vec!["NONE".into()]
                    } else {
                        value.kind_of.iter().cloned().collect()
                    },
                ),
                (
                    "IsTrainable",
                    vec![if value.trainable {
                        "Yes".into()
                    } else {
                        "No".into()
                    }],
                ),
            ] {
                let raw = format!("{key} = {}", tokens.join(" "));
                let mut fields = vec![key.into()];
                fields.extend(tokens);
                lines.push(ContentLine {
                    line: declaration.line,
                    tokens: fields,
                    raw,
                    assigned: true,
                    depth: Some(0),
                });
            }
            let end = declaration
                .lines
                .last()
                .ok_or("effective object End missing")?;
            lines.push(copy(end, 0));
            declaration.lines = lines;
        }
    }
    Ok(())
}
