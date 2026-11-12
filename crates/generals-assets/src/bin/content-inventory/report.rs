use generals_assets::content::{ContentFile, Declaration};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::Path,
};

pub fn table(directory: &Path, name: &str, header: &[&str]) -> io::Result<BufWriter<File>> {
    let mut writer = BufWriter::new(File::create(directory.join(name))?);
    row(&mut writer, header)?;
    Ok(writer)
}

pub fn row(writer: &mut impl Write, columns: &[&str]) -> io::Result<()> {
    for (index, column) in columns.iter().enumerate() {
        if index != 0 {
            write!(writer, "\t")?;
        }
        for character in column.chars() {
            match character {
                '\\' => write!(writer, "\\\\")?,
                '\t' => write!(writer, "\\t")?,
                '\n' => write!(writer, "\\n")?,
                '\r' => write!(writer, "\\r")?,
                other => write!(writer, "{other}")?,
            }
        }
    }
    writeln!(writer)
}

pub fn write(directory: &Path, files: &[ContentFile]) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let mut sources = table(
        directory,
        "files.tsv",
        &[
            "path",
            "origin",
            "declarations",
            "diagnostics",
            "unclassified_lines",
        ],
    )?;
    let mut declarations = table(
        directory,
        "declarations.tsv",
        &["id", "kind", "name", "parent", "path", "line", "origin"],
    )?;
    let mut fields = table(
        directory,
        "fields.tsv",
        &["declaration", "line", "depth", "assigned", "key", "values"],
    )?;
    let mut modules = table(
        directory,
        "module-usage.tsv",
        &["declaration", "line", "kind", "module", "tag", "depth"],
    )?;
    let mut diagnostics = table(
        directory,
        "diagnostics.tsv",
        &["path", "line", "code", "message"],
    )?;
    let mut totals = BTreeMap::<String, usize>::new();
    for file in files {
        row(
            &mut sources,
            &[
                &file.path,
                &file.origin,
                &file.declarations.len().to_string(),
                &file.diagnostics.len().to_string(),
                &file.unclassified.len().to_string(),
            ],
        )?;
        for diagnostic in &file.diagnostics {
            row(
                &mut diagnostics,
                &[
                    &file.path,
                    &diagnostic.line.to_string(),
                    &diagnostic.code,
                    &diagnostic.message,
                ],
            )?;
        }
        for line in &file.unclassified {
            row(
                &mut diagnostics,
                &[
                    &file.path,
                    &line.line.to_string(),
                    "unclassified",
                    &line.raw,
                ],
            )?;
        }
        for declaration in &file.declarations {
            *totals.entry(declaration.kind.clone()).or_default() += 1;
            let id = format!("{}:{}", file.path, declaration.line);
            row(
                &mut declarations,
                &[
                    &id,
                    &declaration.kind,
                    &declaration.name,
                    declaration.parent.as_deref().unwrap_or(""),
                    &file.path,
                    &declaration.line.to_string(),
                    &file.origin,
                ],
            )?;
            write_fields(&id, declaration, &mut fields, &mut modules)?;
        }
    }
    let mut counts = table(directory, "counts.tsv", &["kind", "declarations"])?;
    for (kind, count) in totals {
        row(&mut counts, &[&kind, &count.to_string()])?;
    }
    for writer in [
        &mut sources,
        &mut declarations,
        &mut fields,
        &mut modules,
        &mut diagnostics,
        &mut counts,
    ] {
        writer.flush()?;
    }
    Ok(())
}

fn write_fields(
    id: &str,
    declaration: &Declaration,
    fields: &mut impl Write,
    modules: &mut impl Write,
) -> io::Result<()> {
    for line in &declaration.lines {
        let Some(key) = line.tokens.first() else {
            continue;
        };
        let depth = line
            .depth
            .map_or_else(|| "unknown".into(), |value| value.to_string());
        row(
            fields,
            &[
                id,
                &line.line.to_string(),
                &depth,
                if line.assigned { "yes" } else { "no" },
                key,
                &line.tokens[1..].join(" "),
            ],
        )?;
        if ["Behavior", "Body", "Draw", "ClientUpdate"]
            .iter()
            .any(|kind| key.eq_ignore_ascii_case(kind))
        {
            row(
                modules,
                &[
                    id,
                    &line.line.to_string(),
                    key,
                    line.tokens.get(1).map_or("", String::as_str),
                    line.tokens.get(2).map_or("", String::as_str),
                    &depth,
                ],
            )?;
        }
    }
    Ok(())
}
