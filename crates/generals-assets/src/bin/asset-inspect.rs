//! Read-only inspection of locally installed BIG archives and supported assets.
use generals_assets::{Archive, HeightMap, w3d::Model};
use std::{env, error::Error, path::Path};
#[path = "asset-inspect/overlay.rs"]
mod overlay;

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = env::args().skip(1).collect();
    match arguments.as_slice() {
        [command, root, selection, name] if command == "--build-overlay" => {
            overlay::build(Path::new(root), Path::new(selection), name)?;
        }
        [command, root, path] if command == "--check-overlay" => {
            overlay::check(Path::new(root), Path::new(path))?;
        }
        [path] => {
            for entry in Archive::open(path)?.entries() {
                println!("{}\t{}", entry.size, entry.name);
            }
        }
        [path, name] if name == "--maps" => inspect_maps(&Archive::open(path)?)?,
        [path, name] => inspect_asset(&Archive::open(path)?, name)?,
        _ => return Err("usage: asset-inspect ARCHIVE.big [ASSET_NAME | --maps]\n       asset-inspect --build-overlay GENERALS_ROOT SELECTION.tsv NAME\n       asset-inspect --check-overlay GENERALS_ROOT OVERLAY_DIR".into()),
    }
    Ok(())
}

fn inspect_maps(archive: &Archive) -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for entry in archive.entries().iter().filter(|entry| {
        Path::new(&entry.name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("map"))
    }) {
        inspect_asset(archive, &entry.name)?;
        count += 1;
    }
    println!("validated {count} maps");
    Ok(())
}

fn inspect_asset(archive: &Archive, name: &str) -> Result<(), Box<dyn Error>> {
    let bytes = archive.read(name)?;
    if name.to_ascii_lowercase().ends_with(".map") {
        let map = HeightMap::parse(&bytes)?;
        let minimum = map.heights.iter().min().ok_or("empty heightmap")?;
        let maximum = map.heights.iter().max().ok_or("empty heightmap")?;
        println!(
            "{name}: {}x{} border={} heights={minimum}..{maximum}",
            map.width, map.height, map.border
        );
    } else if name.to_ascii_lowercase().ends_with(".w3d") {
        let model = Model::parse(&bytes)?;
        let vertices: usize = model.meshes.iter().map(|mesh| mesh.positions.len()).sum();
        let triangles: usize = model.meshes.iter().map(|mesh| mesh.indices.len() / 3).sum();
        println!(
            "{name}: {} meshes, {vertices} vertices, {triangles} triangles",
            model.meshes.len()
        );
    } else {
        return Err("supported inspection types: .map and .w3d".into());
    }
    Ok(())
}
