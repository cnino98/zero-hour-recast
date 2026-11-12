//! Rewrite-owned runtime content; audited selection is separate from mounting.
use super::{Asset, Layer, MAX_LISTED_FILES, MAX_LOOSE_BYTES, VirtualAssets, normalized_path};
use crate::{AssetError, invalid};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const ENGINE: &str = "2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed";
const DATA: &str = "6c7f40d6af6a4babb77a33a4502ec7656446c13e";
const PROFILE: &str = "zero-hour-superhackers-retail-data-1-v1";
const MAX_OVERLAY_BYTES: usize = 512 * 1024 * 1024;
const MANIFEST: &str = "manifest.tsv";
const HEADER: &str = "generals-content-overlay\t1\nengine\t2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed\ndata\t6c7f40d6af6a4babb77a33a4502ec7656446c13e\nprofile\tzero-hour-superhackers-retail-data-1-v1\nprecedence\toverlay,zh-loose,zh-big,base-big\n";

/// Identifies the entire selection, source revisions, and declared load profile.
pub struct OverlayIdentity {
    pub manifest_sha256: String,
    pub engine_revision: &'static str,
    pub data_revision: &'static str,
    pub profile: &'static str,
    pub asset_count: usize,
}

/// One explicitly selected, already generated runtime payload.
/// `source_path` is its normalized path within the pinned data reference.
pub struct OverlayInput {
    pub asset_path: String,
    pub payload: PathBuf,
    pub source_path: String,
}

pub(super) struct ContentOverlay {
    assets: BTreeMap<String, Asset>,
    identity: OverlayIdentity,
}

impl ContentOverlay {
    pub(super) fn read(&self, path: &str) -> Option<Asset> {
        self.assets.get(path).map(|asset| Asset {
            path: asset.path.clone(),
            origin: asset.origin.clone(),
            bytes: asset.bytes.clone(),
        })
    }

    pub(super) fn list(&self, prefix: &str, names: &mut BTreeSet<String>) {
        names.extend(
            self.assets
                .keys()
                .filter(|path| path.starts_with(prefix))
                .cloned(),
        );
    }
}

impl VirtualAssets {
    /// Build a deterministic overlay from explicit runtime payload selections.
    /// The destination must not exist. Originals are read only; selection and
    /// preprocessing are the caller's audited responsibility.
    ///
    /// # Errors
    /// Rejects unsafe/duplicate paths, oversized payloads, existing destinations,
    /// destinations overlapping mounted originals, and all I/O failures.
    pub fn build_content_overlay(
        &self,
        destination: &Path,
        inputs: &[OverlayInput],
    ) -> Result<OverlayIdentity, AssetError> {
        if !self.installation_profile {
            return Err(invalid(
                "overlay requires the Zero Hour installation precedence profile",
            ));
        }
        if self.overlay.is_some() {
            return Err(invalid(
                "build overlay requires an unpatched original baseline",
            ));
        }
        if inputs.is_empty() || inputs.len() > MAX_LISTED_FILES {
            return Err(invalid("overlay selection must contain 1..250000 assets"));
        }
        if destination.exists() {
            return Err(invalid("overlay build destination already exists"));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| invalid("overlay requires a parent directory"))?
            .canonicalize()?;
        let name = destination
            .file_name()
            .ok_or_else(|| invalid("overlay requires a directory name"))?;
        let root = parent.join(name);
        self.validate_overlay_root(&root)?;
        let mut rows = BTreeMap::new();
        let mut total_bytes = 0;
        for input in inputs {
            validate_name(&input.asset_path)?;
            validate_source(&input.source_path)?;
            let bytes = read_bounded(&input.payload)?;
            accumulate_size(&mut total_bytes, bytes.len())?;
            let baseline = self.original_hash(&input.asset_path)?;
            if rows
                .insert(
                    input.asset_path.clone(),
                    (bytes, baseline, &input.source_path),
                )
                .is_some()
            {
                return Err(invalid("duplicate overlay asset path"));
            }
        }
        let mut manifest = HEADER.to_owned();
        for (name, (bytes, baseline, source)) in &rows {
            writeln!(
                &mut manifest,
                "asset\t{name}\t{}\t{baseline}\t{source}",
                hash(bytes)
            )
            .map_err(|_| invalid("overlay manifest formatting failed"))?;
        }
        fs::create_dir(&root)?;
        fs::create_dir(root.join("payload"))?;
        for (name, (bytes, _, _)) in rows {
            let path = root.join("payload").join(name);
            fs::create_dir_all(path.parent().ok_or_else(|| invalid("asset lacks parent"))?)?;
            fs::write(path, bytes)?;
        }
        fs::write(root.join(MANIFEST), &manifest)?;
        Ok(identity(manifest.as_bytes(), inputs.len()))
    }

    /// Enable a verified overlay above original installation layers. User maps
    /// retain their isolated namespace. Verified bytes are frozen for this mount.
    ///
    /// # Errors
    /// Rejects unsupported manifest/source/profile/precedence, unsafe paths,
    /// hashes differing from payload or original baseline, extra payload files,
    /// symlinks, duplicate mounting, and overlapping original roots.
    pub fn with_content_overlay(mut self, root: &Path) -> Result<Self, AssetError> {
        if !self.installation_profile {
            return Err(invalid(
                "overlay requires the Zero Hour installation precedence profile",
            ));
        }
        if self.overlay.is_some() {
            return Err(invalid("content overlay already mounted"));
        }
        reject_symlink(root)?;
        let root = root.canonicalize()?;
        self.validate_overlay_root(&root)?;
        let manifest_path = root.join(MANIFEST);
        reject_symlink(&manifest_path)?;
        let manifest = read_bounded(&manifest_path)?;
        let text =
            std::str::from_utf8(&manifest).map_err(|_| invalid("overlay manifest is not UTF-8"))?;
        if text.contains('\r') {
            return Err(invalid("overlay manifest requires LF line endings"));
        }
        let rows = text
            .strip_prefix(HEADER)
            .ok_or_else(|| invalid("unsupported overlay manifest/source/profile/precedence"))?;
        let mut assets = BTreeMap::new();
        let mut total_bytes = 0;
        let mut previous = None;
        for line in rows.lines() {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 5 || fields[0] != "asset" {
                return Err(invalid("malformed overlay asset row"));
            }
            let name = fields[1];
            validate_name(name)?;
            validate_source(fields[4])?;
            validate_hash(fields[2])?;
            if fields[3] != "-" {
                validate_hash(fields[3])?;
            }
            if previous.is_some_and(|previous| previous >= name) {
                return Err(invalid("overlay rows must be unique and sorted"));
            }
            previous = Some(name);
            if self.original_hash(name)? != fields[3] {
                return Err(invalid(format!(
                    "overlay original baseline hash mismatch: {name}"
                )));
            }
            let file = root.join("payload").join(name);
            let mut current = root.join("payload");
            reject_symlink(&current)?;
            for component in name.split('/') {
                current.push(component);
                reject_symlink(&current)?;
            }
            let bytes = read_bounded(&file)?;
            accumulate_size(&mut total_bytes, bytes.len())?;
            if hash(&bytes) != fields[2] {
                return Err(invalid(format!("overlay payload hash mismatch: {name}")));
            }
            assets.insert(
                name.to_owned(),
                Asset {
                    path: name.to_owned(),
                    origin: file.display().to_string(),
                    bytes,
                },
            );
            if assets.len() > MAX_LISTED_FILES {
                return Err(invalid("too many overlay assets"));
            }
        }
        if assets.is_empty() || !text.ends_with('\n') {
            return Err(invalid(
                "overlay manifest requires assets and terminal newline",
            ));
        }
        let listed = Self::from_layers(vec![Layer::Loose(root.join("payload"))]).list_payload()?;
        if listed != assets.keys().cloned().collect() {
            return Err(invalid("overlay payload tree differs from manifest"));
        }
        self.overlay = Some(ContentOverlay {
            identity: identity(&manifest, assets.len()),
            assets,
        });
        Ok(self)
    }

    /// Source identity suitable for acceptance evidence; `None` means original data.
    #[must_use]
    pub fn overlay_identity(&self) -> Option<&OverlayIdentity> {
        self.overlay.as_ref().map(|overlay| &overlay.identity)
    }

    fn original_hash(&self, name: &str) -> Result<String, AssetError> {
        match self.read(name) {
            Ok(asset) => Ok(hash(&asset.bytes)),
            Err(AssetError::Missing(_)) => Ok("-".to_owned()),
            Err(error) => Err(error),
        }
    }

    fn validate_overlay_root(&self, root: &Path) -> Result<(), AssetError> {
        let mut protected = Vec::new();
        for layer in &self.layers {
            protected.push(match layer {
                Layer::Loose(path) => path.canonicalize()?,
                Layer::Big(_, path) => path
                    .parent()
                    .ok_or_else(|| invalid("archive lacks parent"))?
                    .canonicalize()?,
            });
        }
        if let Some(path) = &self.user_maps {
            protected.push(path.canonicalize()?);
        }
        if protected
            .iter()
            .any(|original| root.starts_with(original) || original.starts_with(root))
        {
            return Err(invalid("overlay overlaps original mounted assets"));
        }
        Ok(())
    }

    fn list_payload(&self) -> Result<BTreeSet<String>, AssetError> {
        let mut names = BTreeSet::new();
        let Layer::Loose(root) = &self.layers[0] else {
            return Err(invalid("expected loose overlay payload"));
        };
        super::list_loose(root, root, "payload", &mut names)?;
        Ok(names
            .into_iter()
            .map(|name| name["payload/".len()..].to_owned())
            .collect())
    }
}

fn validate_name(name: &str) -> Result<(), AssetError> {
    if name
        .split('/')
        .any(|component| matches!(component, "." | ".."))
        || normalized_path(name)? != name
        || !name.is_ascii()
        || name.chars().any(char::is_control)
        || name == "user-maps"
        || name.starts_with("user-maps/")
    {
        return Err(invalid(
            "overlay asset path must be canonical ASCII outside user-maps",
        ));
    }
    Ok(())
}

fn validate_source(path: &str) -> Result<(), AssetError> {
    validate_name(path)?;
    if !path.starts_with("patch104pzh/") {
        return Err(invalid(
            "overlay source must identify pinned Zero Hour patch input",
        ));
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), AssetError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("overlay requires lowercase SHA-256"));
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), AssetError> {
    if path.symlink_metadata()?.file_type().is_symlink() {
        return Err(invalid("overlay symlinks unsupported"));
    }
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, AssetError> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_LOOSE_BYTES {
        return Err(invalid("overlay file must be regular and at most 128 MiB"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_LOOSE_BYTES + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_err(|_| invalid("overlay size overflow"))? > MAX_LOOSE_BYTES {
        return Err(invalid("overlay grew beyond size limit"));
    }
    Ok(bytes)
}

fn accumulate_size(total: &mut usize, bytes: usize) -> Result<(), AssetError> {
    *total = total
        .checked_add(bytes)
        .ok_or_else(|| invalid("overlay byte count overflow"))?;
    if *total > MAX_OVERLAY_BYTES {
        return Err(invalid("overlay exceeds 512 MiB verified payload limit"));
    }
    Ok(())
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn identity(bytes: &[u8], asset_count: usize) -> OverlayIdentity {
    OverlayIdentity {
        manifest_sha256: hash(bytes),
        engine_revision: ENGINE,
        data_revision: DATA,
        profile: PROFILE,
        asset_count,
    }
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
                "overlay-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("zh/data")).unwrap();
            fs::create_dir_all(root.join("base")).unwrap();
            fs::write(root.join("zh/data/rules.ini"), b"original").unwrap();
            fs::write(root.join("replacement.ini"), b"replacement").unwrap();
            fs::write(root.join("new.ini"), b"new").unwrap();
            Self(root)
        }
        fn assets(&self) -> VirtualAssets {
            VirtualAssets::for_zero_hour(self.0.join("base"), self.0.join("zh")).unwrap()
        }
        fn inputs(&self) -> Vec<OverlayInput> {
            vec![
                OverlayInput {
                    asset_path: "data/rules.ini".to_owned(),
                    payload: self.0.join("replacement.ini"),
                    source_path: "patch104pzh/gamefilesedited/data/ini/rules.ini".to_owned(),
                },
                OverlayInput {
                    asset_path: "data/new.ini".to_owned(),
                    payload: self.0.join("new.ini"),
                    source_path: "patch104pzh/gamefilesedited/data/ini/new.ini".to_owned(),
                },
            ]
        }
        fn build(&self) -> PathBuf {
            let root = self.0.join("overlay");
            self.assets()
                .build_content_overlay(&root, &self.inputs())
                .unwrap();
            root
        }
        fn mutate_manifest(&self, before: &str, after: &str) {
            let path = self.0.join("overlay/manifest.tsv");
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.contains(before));
            fs::write(path, text.replace(before, after)).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn overlay_precedence_new_assets_frozen_bytes_and_original_baseline() {
        let fixture = Fixture::new();
        let root = fixture.build();
        let assets = fixture.assets().with_content_overlay(&root).unwrap();
        assert_eq!(
            assets.read("DATA\\RULES.INI").unwrap().bytes,
            b"replacement"
        );
        assert!(
            std::path::Path::new(&assets.read("data/rules.ini").unwrap().origin)
                .ends_with("overlay/payload/data/rules.ini")
        );
        assert_eq!(assets.read("data/new.ini").unwrap().bytes, b"new");
        assert_eq!(
            assets.list("data").unwrap(),
            ["data/new.ini", "data/rules.ini"]
        );
        let identity = assets.overlay_identity().unwrap();
        assert_eq!(identity.engine_revision, ENGINE);
        assert_eq!(identity.data_revision, DATA);
        assert_eq!(identity.asset_count, 2);
        assert_eq!(
            identity.manifest_sha256,
            hash(&fs::read(root.join(MANIFEST)).unwrap())
        );
        fs::write(root.join("payload/data/rules.ini"), b"changed after mount").unwrap();
        assert_eq!(assets.read("data/rules.ini").unwrap().bytes, b"replacement");
        assert!(fixture.assets().with_content_overlay(&root).is_err());
        let original = fixture.assets();
        assert!(original.overlay_identity().is_none());
        assert_eq!(original.read("data/rules.ini").unwrap().bytes, b"original");
        assert!(original.read("data/new.ini").is_err());
        assert_eq!(
            fs::read(fixture.0.join("zh/data/rules.ini")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn overlay_build_manifest_is_reproducible_and_selection_order_independent() {
        let fixture = Fixture::new();
        let first = fixture.build();
        let mut inputs = fixture.inputs();
        inputs.reverse();
        let second = fixture.0.join("second");
        fixture
            .assets()
            .build_content_overlay(&second, &inputs)
            .unwrap();
        assert_eq!(
            fs::read(first.join(MANIFEST)).unwrap(),
            fs::read(second.join(MANIFEST)).unwrap()
        );
        assert!(
            fixture
                .assets()
                .build_content_overlay(&second, &inputs)
                .is_err()
        );
        assert!(
            fixture
                .assets()
                .build_content_overlay(&fixture.0.join("zh/overlay"), &inputs)
                .is_err()
        );
        for alias in [
            "../outside.ini",
            "data/./new.ini",
            "data/new.ini/.",
            "data/sub/../new.ini",
        ] {
            inputs[0].asset_path = alias.to_owned();
            let invalid = fixture.0.join("invalid");
            assert!(
                fixture
                    .assets()
                    .build_content_overlay(&invalid, &inputs)
                    .is_err()
            );
            assert!(!invalid.exists());
        }
    }

    #[test]
    fn overlay_rejects_baseline_drift_and_unmanifested_payloads() {
        let fixture = Fixture::new();
        let root = fixture.build();
        fs::write(
            fixture.0.join("zh/data/rules.ini"),
            b"different installation",
        )
        .unwrap();
        assert!(fixture.assets().with_content_overlay(&root).is_err());
        fs::write(fixture.0.join("zh/data/rules.ini"), b"original").unwrap();
        fs::write(root.join("payload/data/unlisted.ini"), b"unlisted").unwrap();
        assert!(fixture.assets().with_content_overlay(&root).is_err());
    }

    #[test]
    fn overlay_rejects_unsupported_identity_and_corrupt_paths_hashes_and_order() {
        for (before, after) in [
            (ENGINE, "0000000000000000000000000000000000000000"),
            (DATA, "0000000000000000000000000000000000000000"),
            (PROFILE, "unsupported"),
            ("overlay,zh-loose,zh-big,base-big", "zh-big,overlay"),
            ("data/new.ini", "../new.ini"),
            ("data/new.ini", "user-maps/new.ini"),
            ("data/new.ini", "data/Rules.ini"),
            ("asset\tdata/new.ini", "unknown\tdata/new.ini"),
        ] {
            let fixture = Fixture::new();
            let root = fixture.build();
            fixture.mutate_manifest(before, after);
            assert!(
                fixture.assets().with_content_overlay(&root).is_err(),
                "{before}"
            );
        }
        let fixture = Fixture::new();
        let root = fixture.build();
        fixture.mutate_manifest(&hash(b"new"), &"0".repeat(64));
        assert!(fixture.assets().with_content_overlay(&root).is_err());
        let fixture = Fixture::new();
        let root = fixture.build();
        let path = root.join(MANIFEST);
        let text = fs::read_to_string(&path).unwrap();
        let mut lines: Vec<_> = text.lines().collect();
        lines.swap(5, 6);
        fs::write(path, format!("{}\n", lines.join("\n"))).unwrap();
        assert!(fixture.assets().with_content_overlay(&root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn overlay_rejects_symlinked_payload_even_when_hash_matches() {
        let fixture = Fixture::new();
        let root = fixture.build();
        let file = root.join("payload/data/rules.ini");
        fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(fixture.0.join("replacement.ini"), file).unwrap();
        assert!(fixture.assets().with_content_overlay(&root).is_err());
    }
}
