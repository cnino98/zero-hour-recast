//! Explicit local content selection; original installation and user maps stay read-only.
use bevy::prelude::Resource;
use generals_assets::virtual_assets::VirtualAssets;
use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Resource, Clone)]
pub struct ContentOverlay {
    path: PathBuf,
    identity: String,
}

impl ContentOverlay {
    pub fn load(root: &Path, path: &Path) -> Result<Self, Box<dyn Error>> {
        let path = path.canonicalize()?;
        let expected = root.canonicalize()?.join("output/content-overlays");
        let allowed = expected.canonicalize()?;
        if allowed != expected {
            return Err("Content overlay output namespace cannot contain symlink aliases".into());
        }
        if !path.starts_with(&allowed) || path == allowed {
            return Err("Content overlay must be inside output/content-overlays".into());
        }
        let assets = original_assets(root)?.with_content_overlay(&path)?;
        let identity = assets
            .overlay_identity()
            .ok_or("Content overlay identity missing")?;
        println!(
            "CONTENT_OVERLAY manifest_sha256={} engine={} data={} profile={} assets={}",
            identity.manifest_sha256,
            identity.engine_revision,
            identity.data_revision,
            identity.profile,
            identity.asset_count
        );
        Ok(Self {
            path,
            identity: identity.manifest_sha256.clone(),
        })
    }
}

pub fn assets(
    root: &Path,
    overlay: Option<&ContentOverlay>,
) -> Result<VirtualAssets, Box<dyn Error>> {
    let assets = original_assets(root)?;
    let Some(overlay) = overlay else {
        return Ok(assets);
    };
    let assets = assets.with_content_overlay(&overlay.path)?;
    if assets
        .overlay_identity()
        .is_none_or(|identity| identity.manifest_sha256 != overlay.identity)
    {
        return Err("Selected content overlay changed during this game session".into());
    }
    Ok(assets)
}

fn original_assets(root: &Path) -> Result<VirtualAssets, Box<dyn Error>> {
    Ok(VirtualAssets::with_user_maps(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
        root.join("UserData"),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::OverlayInput;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join("generals-client-tests")
                .join(format!(
                    "client-overlay-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(root.join("GameData/Command and Conquer Generals")).unwrap();
            fs::create_dir_all(root.join("GameData/Command and Conquer Generals Zero Hour/data"))
                .unwrap();
            fs::create_dir_all(root.join("UserData/Maps/Custom")).unwrap();
            fs::create_dir_all(root.join("output/content-overlays")).unwrap();
            fs::write(
                root.join("GameData/Command and Conquer Generals Zero Hour/data/rules.ini"),
                b"original rules",
            )
            .unwrap();
            fs::write(root.join("UserData/Maps/Custom/custom.map"), b"user map").unwrap();
            fs::write(root.join("replacement.ini"), b"selected rules").unwrap();
            Self(root)
        }

        fn build(&self) -> PathBuf {
            let path = self.0.join("output/content-overlays/selected");
            original_assets(&self.0)
                .unwrap()
                .build_content_overlay(
                    &path,
                    &[OverlayInput {
                        asset_path: "data/rules.ini".to_owned(),
                        payload: self.0.join("replacement.ini"),
                        source_path: "patch104pzh/gamefilesedited/data/ini/rules.ini".to_owned(),
                    }],
                )
                .unwrap();
            path
        }

        fn assert_original(&self) {
            assert_eq!(
                fs::read(
                    self.0
                        .join("GameData/Command and Conquer Generals Zero Hour/data/rules.ini")
                )
                .unwrap(),
                b"original rules"
            );
            assert_eq!(
                fs::read(self.0.join("UserData/Maps/Custom/custom.map")).unwrap(),
                b"user map"
            );
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn selected_overlay_survives_reload_without_changing_originals_or_user_maps() {
        let fixture = Fixture::new();
        let path = fixture.build();
        let selected = ContentOverlay::load(&fixture.0, &path).unwrap();
        let original = assets(&fixture.0, None).unwrap();
        assert!(original.overlay_identity().is_none());
        assert_eq!(
            original.read("data/rules.ini").unwrap().bytes,
            b"original rules"
        );
        for _ in 0..2 {
            let mounted = assets(&fixture.0, Some(&selected)).unwrap();
            assert_eq!(
                mounted.overlay_identity().unwrap().manifest_sha256,
                selected.identity
            );
            assert_eq!(
                mounted.read("data/rules.ini").unwrap().bytes,
                b"selected rules"
            );
            assert_eq!(
                mounted.read("user-maps/custom/custom.map").unwrap().bytes,
                b"user map"
            );
        }
        fixture.assert_original();
    }

    #[test]
    fn selected_overlay_reload_rejects_changed_manifest_or_payload() {
        let fixture = Fixture::new();
        let path = fixture.build();
        let selected = ContentOverlay::load(&fixture.0, &path).unwrap();
        let manifest_path = path.join("manifest.tsv");
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        // A structurally valid provenance edit changes session identity even
        // though all payload and baseline hashes still match.
        fs::write(
            &manifest_path,
            manifest.replace(
                "patch104pzh/gamefilesedited/data/ini/rules.ini",
                "patch104pzh/gamefilesedited/data/ini/different-rules.ini",
            ),
        )
        .unwrap();
        let independently_mounted = original_assets(&fixture.0)
            .unwrap()
            .with_content_overlay(&path)
            .unwrap();
        assert_ne!(
            independently_mounted
                .overlay_identity()
                .unwrap()
                .manifest_sha256,
            selected.identity,
        );
        assert_eq!(
            assets(&fixture.0, Some(&selected))
                .err()
                .unwrap()
                .to_string(),
            "Selected content overlay changed during this game session",
        );
        fs::write(&manifest_path, manifest).unwrap();
        fs::write(path.join("payload/data/rules.ini"), b"changed rules").unwrap();
        assert!(assets(&fixture.0, Some(&selected)).is_err());
        fixture.assert_original();
    }

    #[cfg(unix)]
    #[test]
    fn selected_overlay_rejects_output_namespace_symlink_aliases() {
        for component in ["output", "output/content-overlays"] {
            let fixture = Fixture::new();
            fixture.build();
            let alias = fixture.0.join(component);
            let relocated = fixture.0.join("relocated");
            fs::rename(&alias, &relocated).unwrap();
            std::os::unix::fs::symlink(&relocated, &alias).unwrap();
            assert!(
                ContentOverlay::load(
                    &fixture.0,
                    &fixture.0.join("output/content-overlays/selected")
                )
                .is_err()
            );
            fixture.assert_original();
        }
    }
}
