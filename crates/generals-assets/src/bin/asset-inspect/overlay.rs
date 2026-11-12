//! Build or verify an explicitly selected, rewrite-owned content overlay.
use generals_assets::virtual_assets::{OverlayIdentity, OverlayInput, VirtualAssets};
use std::{
    error::Error,
    fs,
    path::{Component, Path, PathBuf},
};

pub fn build(root: &Path, selection: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    if !matches!(
        Path::new(name).components().collect::<Vec<_>>().as_slice(),
        [Component::Normal(_)]
    ) {
        return Err("Overlay name must be one directory name".into());
    }
    let directory = overlay_directory(root)?;
    let selection = selection.canonicalize()?;
    let inputs = selected_inputs(&selection)?;
    let identity = originals(root)?.build_content_overlay(&directory.join(name), &inputs)?;
    report(&identity);
    Ok(())
}

pub fn check(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    let path = path.canonicalize()?;
    let directory = overlay_directory(root)?;
    if !path.starts_with(&directory) || path == directory {
        return Err("Overlay must be inside output/content-overlays".into());
    }
    let assets = originals(root)?.with_content_overlay(&path)?;
    report(
        assets
            .overlay_identity()
            .ok_or("Missing overlay identity")?,
    );
    Ok(())
}

fn originals(root: &Path) -> Result<VirtualAssets, Box<dyn Error>> {
    Ok(VirtualAssets::with_user_maps(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
        root.join("UserData"),
    )?)
}

fn overlay_directory(root: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut directory = root.canonicalize()?;
    for component in ["output", "content-overlays"] {
        directory.push(component);
        match fs::symlink_metadata(&directory) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir() {
                    return Err(
                        "Overlay output components must be directories without symlinks".into(),
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&directory)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(directory)
}

fn selected_inputs(selection: &Path) -> Result<Vec<OverlayInput>, Box<dyn Error>> {
    let directory = selection.parent().ok_or("Selection file has no parent")?;
    fs::read_to_string(selection)?.lines().map(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        let [asset_path, payload, source_path] = fields.as_slice() else {
            return Err("Selection requires asset path, payload path and upstream source path separated by tabs".into());
        };
        if payload.is_empty() {
            return Err("Selection payload path is empty".into());
        }
        Ok(OverlayInput {
            asset_path: (*asset_path).to_owned(),
            payload: directory.join(payload),
            source_path: (*source_path).to_owned(),
        })
    }).collect()
}

fn report(identity: &OverlayIdentity) {
    println!(
        "CONTENT_OVERLAY manifest_sha256={} engine={} data={} profile={} assets={}",
        identity.manifest_sha256,
        identity.engine_revision,
        identity.data_revision,
        identity.profile,
        identity.asset_count
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "overlay-cli-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn overlay_output_creates_only_missing_real_directories() {
        let fixture = Fixture::new();
        let expected = fixture.0.join("output/content-overlays");
        assert_eq!(overlay_directory(&fixture.0).unwrap(), expected);
        assert!(expected.is_dir());
        assert_eq!(overlay_directory(&fixture.0).unwrap(), expected);
    }

    #[cfg(unix)]
    #[test]
    fn overlay_output_rejects_symlinks_before_creating_child_directories() {
        for component in ["output", "content-overlays"] {
            let fixture = Fixture::new();
            let outside = fixture.0.join("outside");
            fs::create_dir(&outside).unwrap();
            let parent = if component == "output" {
                fixture.0.clone()
            } else {
                let output = fixture.0.join("output");
                fs::create_dir(&output).unwrap();
                output
            };
            std::os::unix::fs::symlink(&outside, parent.join(component)).unwrap();
            assert!(overlay_directory(&fixture.0).is_err());
            assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        }
    }

    #[test]
    fn overlay_output_rejects_regular_file_components() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("output"), b"retained output").unwrap();
        assert!(overlay_directory(&fixture.0).is_err());
        assert_eq!(
            fs::read(fixture.0.join("output")).unwrap(),
            b"retained output"
        );
    }
}
