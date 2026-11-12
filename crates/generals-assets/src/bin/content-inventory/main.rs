//! Read-only installed-content census; does not claim resolved game templates.
mod players;
mod report;

use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{env, error::Error, fs, io, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = env::args().skip(1).collect();
    let [base, zero_hour, output] = arguments.as_slice() else {
        return Err(
            "usage: content-inventory BASE_DIRECTORY ZERO_HOUR_DIRECTORY OUTPUT_DIRECTORY".into(),
        );
    };
    let assets = VirtualAssets::for_zero_hour(base, zero_hour)?;
    let mut files = Vec::new();
    for path in assets.list("data/ini")? {
        if Path::new(&path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
        {
            files.push(content::scan(assets.read(&path)?)?);
        }
    }
    if files.is_empty() {
        return Err("no installed INI files found".into());
    }
    // Invalidate the prior faction result before updating any raw tables. A
    // failed metadata resolution must not leave a usable mixed-generation set.
    match fs::remove_file(Path::new(output).join("players.tsv")) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    report::write(Path::new(output), &files)?;
    players::write(Path::new(output), &files)?;
    println!(
        "Inspected {} winning INI files. Reports: {output}",
        files.len()
    );
    println!(
        "Lexical census and source-derived faction selection; inheritance and complete roster reachability remain unresolved."
    );
    Ok(())
}
