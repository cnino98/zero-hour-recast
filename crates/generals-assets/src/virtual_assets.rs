//! Explicit SAGE asset lookup precedence for the supported local installation.
mod content_overlay;
pub use content_overlay::{OverlayIdentity, OverlayInput};

use crate::{Archive, AssetError, invalid};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

const MAX_LOOSE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_LISTED_FILES: usize = 250_000;
const MAX_DIRECTORY_DEPTH: usize = 32;

/// Payload plus the winning physical source for diagnostics and rule provenance.
pub struct Asset {
    pub path: String,
    pub origin: String,
    pub bytes: Vec<u8>,
}

/// An explicitly ordered source: earlier entries win.
pub enum Layer {
    Loose(PathBuf),
    Big(Archive, PathBuf),
}

/// Read-only ordered lookup. No silent merging of duplicate file contents.
pub struct VirtualAssets {
    map_ini: BTreeMap<String, Asset>,
    map_ini_sources: Vec<Asset>,
    layers: Vec<Layer>,
    user_maps: Option<PathBuf>,
    overlay: Option<content_overlay::ContentOverlay>,
    installation_profile: bool,
}

impl VirtualAssets {
    /// Construct an explicit high-to-low priority source list.
    #[must_use]
    pub const fn from_layers(layers: Vec<Layer>) -> Self {
        Self {
            map_ini: BTreeMap::new(),
            map_ini_sources: Vec::new(),
            layers,
            user_maps: None,
            overlay: None,
            installation_profile: false,
        }
    }

    /// Use local loose files, then ZH archives, then original-game archives.
    ///
    /// Within each archive directory, case-insensitive alphabetical order wins
    /// first, matching EA's `FilenameList` + overwrite=false archive loading.
    /// Explicit mod archives and nested archive scans are outside this profile.
    ///
    /// # Errors
    /// Rejects unreadable directories, ambiguous case-folded archive filenames,
    /// and malformed archives rather than silently changing lookup precedence.
    pub fn for_zero_hour(
        base_dir: impl AsRef<Path>,
        zh_dir: impl AsRef<Path>,
    ) -> Result<Self, AssetError> {
        let zh_dir = zh_dir.as_ref().canonicalize()?;
        let base_dir = base_dir.as_ref().canonicalize()?;
        let mut layers = vec![Layer::Loose(zh_dir.clone())];
        append_archives(&mut layers, &zh_dir)?;
        append_archives(&mut layers, &base_dir)?;
        Ok(Self {
            map_ini: BTreeMap::new(),
            map_ini_sources: Vec::new(),
            layers,
            user_maps: None,
            overlay: None,
            installation_profile: true,
        })
    }

    /// Mount only the original user-map directory under `user-maps/`.
    /// Installed game assets keep their original precedence.
    ///
    /// # Errors
    /// Rejects an unreadable installation or an unsafe user-map directory.
    pub fn with_user_maps(
        base_dir: impl AsRef<Path>,
        zh_dir: impl AsRef<Path>,
        user_data: impl AsRef<Path>,
    ) -> Result<Self, AssetError> {
        let mut assets = Self::for_zero_hour(base_dir, zh_dir)?;
        if user_data.as_ref().is_dir()
            && let Some(directory) = resolve_loose_path(user_data.as_ref(), "maps")?
        {
            if !directory.is_dir() {
                return Err(invalid("user-map root must be a directory"));
            }
            assets.user_maps = Some(directory);
        }
        Ok(assets)
    }

    /// Return the highest-priority matching payload, with physical provenance.
    ///
    /// # Errors
    /// Rejects missing assets, unsafe relative paths, ambiguous loose-file case,
    /// symlinks escaping the loose root, and read/size failures.
    pub fn read(&self, name: &str) -> Result<Asset, AssetError> {
        let path = normalized_path(name)?;
        if let Some(asset) = self.map_ini.get(&path) {
            return Ok(Asset {
                path,
                origin: asset.origin.clone(),
                bytes: asset.bytes.clone(),
            });
        }
        self.read_base(path)
    }

    /// Read the installation and content overlay before per-map INI overrides.
    ///
    /// # Errors
    /// Rejects the same missing, unsafe or unreadable assets as `read`.
    pub fn read_without_map_ini(&self, name: &str) -> Result<Asset, AssetError> {
        self.read_base(normalized_path(name)?)
    }

    /// Inspect an effective map INI file without reading installation layers.
    ///
    /// # Errors
    /// Rejects unsafe relative names.
    pub fn map_ini_asset(&self, name: &str) -> Result<Option<&Asset>, AssetError> {
        Ok(self.map_ini.get(&normalized_path(name)?))
    }

    /// Original map.ini then solo.ini payloads, before effective-file flattening.
    #[must_use]
    pub fn map_ini_sources(&self) -> &[Asset] {
        &self.map_ini_sources
    }

    /// Mount effective files together with their ordered original override sources.
    /// Sources have already passed the map-local scanner and admission boundary.
    ///
    /// # Errors
    /// Rejects the same invalid or repeated effective mount as `with_map_ini`.
    pub fn with_map_ini_provenance(
        self,
        files: Vec<Asset>,
        sources: Vec<Asset>,
    ) -> Result<Self, AssetError> {
        let mut result = self.with_map_ini(files)?;
        result.map_ini_sources = sources;
        Ok(result)
    }

    fn read_base(&self, path: String) -> Result<Asset, AssetError> {
        if let Some(relative) = path.strip_prefix("user-maps/") {
            let root = self
                .user_maps
                .as_ref()
                .ok_or_else(|| AssetError::Missing(path.clone()))?;
            let mount = Self::from_layers(vec![Layer::Loose(root.clone())]);
            let mut asset = mount.read(relative)?;
            asset.path = path;
            return Ok(asset);
        }
        if let Some(overlay) = &self.overlay
            && let Some(asset) = overlay.read(&path)
        {
            return Ok(asset);
        }
        for layer in &self.layers {
            match layer {
                Layer::Loose(root) => {
                    if let Some(file) = resolve_loose(root, &path)? {
                        if file.metadata()?.len() > MAX_LOOSE_BYTES {
                            return Err(invalid("loose asset exceeds 128 MiB limit"));
                        }
                        return Ok(Asset {
                            path,
                            origin: file.display().to_string(),
                            bytes: fs::read(file)?,
                        });
                    }
                }
                Layer::Big(archive, source) => {
                    if archive.entries().iter().any(|entry| entry.name == path) {
                        return Ok(Asset {
                            bytes: archive.read(&path)?,
                            path,
                            origin: source.display().to_string(),
                        });
                    }
                }
            }
        }
        Err(AssetError::Missing(path))
    }

    /// Mount effective INI files after the map loader has applied native overrides.
    /// Each new map starts with a fresh installation view; original files stay read-only.
    ///
    /// # Errors
    /// Rejects repeated mounts, duplicate or non-INI targets, and oversized payloads.
    pub fn with_map_ini(mut self, files: Vec<Asset>) -> Result<Self, AssetError> {
        if !self.map_ini.is_empty() {
            return Err(invalid("map INI already configured"));
        }
        let mut total = 0_u64;
        for mut asset in files {
            asset.path = normalized_path(&asset.path)?;
            if !asset.path.starts_with("data/ini/")
                || !Path::new(&asset.path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
            {
                return Err(invalid("map INI target must be under data/ini"));
            }
            let size =
                u64::try_from(asset.bytes.len()).map_err(|_| invalid("map INI size overflow"))?;
            total = total
                .checked_add(size)
                .ok_or_else(|| invalid("map INI size overflow"))?;
            if size > MAX_LOOSE_BYTES || total > 512 * 1024 * 1024 {
                return Err(invalid("map INI payload exceeds size limit"));
            }
            if self.map_ini.insert(asset.path.clone(), asset).is_some() {
                return Err(invalid("duplicate map INI target"));
            }
            check_list_size(self.map_ini.len())?;
        }
        Ok(self)
    }

    /// List normalized winning names inside one directory, including descendants.
    ///
    /// Direct files come first, then all descendant files sorted by normalized
    /// full name. This is the two-pass order of the pinned `SuperHackers`
    /// `INI::loadDirectory`, not a recursive breadth-first directory traversal.
    /// Names occur once; `read` supplies the winning layer's bytes and origin.
    /// Listing does not imply that the game loads every listed file. In
    /// particular `loadFileDirectory` loads `Name.ini` before `Name/` separately.
    ///
    /// # Errors
    /// Rejects unsafe directory prefixes, ambiguous loose names, directory
    /// symlinks, escaping file symlinks, I/O errors, and excessive tree sizes.
    pub fn list(&self, prefix: &str) -> Result<Vec<String>, AssetError> {
        self.list_layers(prefix, true)
    }

    /// List installation and content-overlay names before per-map INI overrides.
    ///
    /// # Errors
    /// Rejects the same unsafe or unreadable directory trees as `list`.
    pub fn list_without_map_ini(&self, prefix: &str) -> Result<Vec<String>, AssetError> {
        self.list_layers(prefix, false)
    }

    fn list_layers(&self, prefix: &str, include_map_ini: bool) -> Result<Vec<String>, AssetError> {
        let directory = normalized_path(prefix.trim_end_matches(['/', '\\']))?;
        if directory == "user-maps" || directory.starts_with("user-maps/") {
            let Some(root) = &self.user_maps else {
                return Ok(Vec::new());
            };
            let relative = directory.strip_prefix("user-maps/");
            let mut names = BTreeSet::new();
            if let Some(relative) = relative {
                if let Some(start) = resolve_loose_path(root, relative)?
                    && start.is_dir()
                {
                    list_loose(root, &start, relative, &mut names)?;
                }
            } else {
                list_loose(root, root, "user-maps", &mut names)?;
                return Ok(names.into_iter().collect());
            }
            return Ok(names
                .into_iter()
                .map(|name| format!("user-maps/{name}"))
                .collect());
        }
        let prefix = format!("{directory}/");
        let mut names = BTreeSet::new();
        if include_map_ini {
            names.extend(
                self.map_ini
                    .keys()
                    .filter(|name| name.starts_with(&prefix))
                    .cloned(),
            );
        }
        if let Some(overlay) = &self.overlay {
            overlay.list(&prefix, &mut names);
        }
        for layer in &self.layers {
            match layer {
                Layer::Loose(root) => {
                    if let Some(start) = resolve_loose_path(root, &directory)?
                        && start.is_dir()
                    {
                        list_loose(root, &start, &directory, &mut names)?;
                    }
                }
                Layer::Big(archive, _) => {
                    for entry in archive.entries() {
                        if entry.name.starts_with(&prefix) {
                            names.insert(entry.name.clone());
                            check_list_size(names.len())?;
                        }
                    }
                }
            }
        }
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort_by(|left, right| {
            let left_child = left[prefix.len()..].contains('/');
            let right_child = right[prefix.len()..].contains('/');
            left_child.cmp(&right_child).then_with(|| left.cmp(right))
        });
        Ok(names)
    }
}

fn check_list_size(count: usize) -> Result<(), AssetError> {
    if count > MAX_LISTED_FILES {
        return Err(invalid("asset listing exceeds 250000 entries"));
    }
    Ok(())
}

fn list_loose(
    root: &Path,
    start: &Path,
    prefix: &str,
    names: &mut BTreeSet<String>,
) -> Result<(), AssetError> {
    let canonical_root = root.canonicalize()?;
    let mut pending = vec![(start.to_path_buf(), prefix.to_owned(), 0)];
    let mut count = 0;
    while let Some((directory, prefix, depth)) = pending.pop() {
        if depth > MAX_DIRECTORY_DEPTH {
            return Err(invalid("loose asset directory depth exceeds 32"));
        }
        let mut local_names = BTreeSet::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            count += 1;
            check_list_size(count)?;
            let file_name = entry.file_name();
            let component = file_name
                .to_str()
                .ok_or_else(|| invalid("non-UTF-8 loose asset filename"))?
                .to_ascii_lowercase();
            if !local_names.insert(component.clone()) {
                return Err(invalid("ambiguous case-folded loose asset"));
            }
            let name = normalized_path(&format!("{prefix}/{component}"))?;
            let resolved = entry.path().canonicalize()?;
            if !resolved.starts_with(&canonical_root) {
                return Err(invalid("loose asset escapes its installation root"));
            }
            if resolved.is_dir() {
                if entry.file_type()?.is_symlink() {
                    return Err(invalid("directory symlinks unsupported in asset listing"));
                }
                pending.push((resolved, name, depth + 1));
            } else if resolved.is_file() {
                names.insert(name);
                check_list_size(names.len())?;
            }
        }
    }
    Ok(())
}

fn append_archives(layers: &mut Vec<Layer>, directory: &Path) -> Result<(), AssetError> {
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| {
        path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("big"))
    });
    paths.sort_by_cached_key(|path| path.file_name().unwrap_or_default().to_ascii_lowercase());
    for pair in paths.windows(2) {
        if pair[0]
            .file_name()
            .unwrap_or_default()
            .eq_ignore_ascii_case(pair[1].file_name().unwrap_or_default())
        {
            return Err(invalid("ambiguous case-folded BIG filenames"));
        }
    }
    for path in paths {
        layers.push(Layer::Big(Archive::open(&path)?, path));
    }
    Ok(())
}

fn normalized_path(name: &str) -> Result<String, AssetError> {
    let path = name.replace('\\', "/").to_ascii_lowercase();
    if path.is_empty()
        || path.contains(':')
        || path.split('/').any(str::is_empty)
        || Path::new(&path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid("asset lookup requires a normalized relative path"));
    }
    Ok(path)
}

fn resolve_loose(root: &Path, name: &str) -> Result<Option<PathBuf>, AssetError> {
    Ok(resolve_loose_path(root, name)?.filter(|path| path.is_file()))
}

fn resolve_loose_path(root: &Path, name: &str) -> Result<Option<PathBuf>, AssetError> {
    let canonical_root = root.canonicalize()?;
    let mut current = canonical_root.clone();
    for component in name.split('/') {
        if !current.is_dir() {
            return Ok(None);
        }
        let mut matching = None;
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            if entry.file_name().eq_ignore_ascii_case(component) {
                if matching.is_some() {
                    return Err(invalid("ambiguous case-folded loose asset"));
                }
                matching = Some(entry.path());
            }
        }
        let Some(found) = matching else {
            return Ok(None);
        };
        current = found.canonicalize()?;
        if !current.starts_with(&canonical_root) {
            return Err(invalid("loose asset escapes its installation root"));
        }
        if current.is_dir() && found.symlink_metadata()?.file_type().is_symlink() {
            return Err(invalid("directory symlinks unsupported in asset lookup"));
        }
    }
    Ok(Some(current))
}

#[cfg(test)]
mod tests {
    use super::{Asset, Layer, VirtualAssets, normalized_path};
    use crate::Archive;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn map_ini_is_a_per_map_snapshot_and_preserves_installed_files() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("Data/INI")).unwrap();
        let original = fixture.0.join("Data/INI/Weapon.ini");
        fs::write(&original, b"original").unwrap();
        let assets = VirtualAssets::from_layers(vec![Layer::Loose(fixture.0.clone())])
            .with_map_ini(vec![Asset {
                path: "DATA\\INI\\WEAPON.INI".into(),
                origin: "maps/fixture/map.ini over original".into(),
                bytes: b"effective".to_vec(),
            }])
            .unwrap();
        let selected = assets.read("data/ini/weapon.ini").unwrap();
        assert_eq!(selected.bytes, b"effective");
        assert_eq!(selected.origin, "maps/fixture/map.ini over original");
        assert_eq!(
            assets
                .read_without_map_ini("data/ini/weapon.ini")
                .unwrap()
                .bytes,
            b"original"
        );
        assert_eq!(assets.list("data/ini").unwrap(), ["data/ini/weapon.ini"]);
        assert_eq!(fs::read(&original).unwrap(), b"original");
        let next_map = VirtualAssets::from_layers(vec![Layer::Loose(fixture.0.clone())]);
        assert_eq!(
            next_map.read("data/ini/weapon.ini").unwrap().bytes,
            b"original"
        );
    }

    #[test]
    fn baseline_listing_excludes_map_only_files_without_changing_effective_listing() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("Data/INI/Object")).unwrap();
        fs::write(fixture.0.join("Data/INI/Object/Original.ini"), b"original").unwrap();
        let assets = VirtualAssets::from_layers(vec![Layer::Loose(fixture.0.clone())])
            .with_map_ini(vec![Asset {
                path: "data/ini/object/maponly.ini".into(),
                origin: "maps/custom/map.ini".into(),
                bytes: b"effective".to_vec(),
            }])
            .unwrap();
        assert_eq!(
            assets.list("data/ini/object").unwrap(),
            [
                "data/ini/object/maponly.ini",
                "data/ini/object/original.ini"
            ]
        );
        assert_eq!(
            assets.list_without_map_ini("data/ini/object").unwrap(),
            ["data/ini/object/original.ini"]
        );
        assert!(
            assets
                .map_ini_asset("data/ini/object/maponly.ini")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn map_ini_rejects_duplicate_and_non_ini_targets() {
        let make = |path: &str| Asset {
            path: path.into(),
            origin: "map.ini".into(),
            bytes: Vec::new(),
        };
        for name in ["user-maps/a/map.ini", "data/art/mesh.w3d", "../weapon.ini"] {
            assert!(
                VirtualAssets::from_layers(Vec::new())
                    .with_map_ini(vec![make(name)])
                    .is_err()
            );
        }
        assert!(
            VirtualAssets::from_layers(Vec::new())
                .with_map_ini(vec![
                    make("Data/INI/Weapon.ini"),
                    make("data/ini/weapon.ini")
                ])
                .is_err()
        );
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "generals-assets-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("test directory");
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove fixture");
        }
    }

    fn big(path: &std::path::Path, contents: &[u8]) {
        let name = b"data/rules.ini\0";
        let offset = u32::try_from(24 + name.len()).expect("small fixture");
        let size = u32::try_from(contents.len()).expect("small fixture");
        let mut bytes = b"BIGF".to_vec();
        bytes.extend((offset + size).to_le_bytes());
        bytes.extend(1_u32.to_be_bytes());
        bytes.extend(offset.to_be_bytes());
        bytes.extend(offset.to_be_bytes());
        bytes.extend(size.to_be_bytes());
        bytes.extend(name);
        bytes.extend(contents);
        fs::write(path, bytes).expect("fixture archive");
    }

    #[test]
    fn winning_layer_controls_payload_and_provenance() {
        let fixture = Fixture::new();
        let high = fixture.0.join("a.big");
        let low = fixture.0.join("b.big");
        big(&high, b"first");
        big(&low, b"second");
        let layers = vec![
            Layer::Loose(fixture.0.clone()),
            Layer::Big(Archive::open(&high).expect("first archive"), high.clone()),
            Layer::Big(Archive::open(&low).expect("second archive"), low),
        ];
        let assets = VirtualAssets::from_layers(layers);
        let first = assets.read("DATA\\Rules.INI").expect("archive lookup");
        assert_eq!(first.bytes, b"first");
        assert_eq!(first.origin, high.display().to_string());
        fs::create_dir(fixture.0.join("Data")).expect("loose directory");
        fs::write(fixture.0.join("Data/Rules.ini"), b"loose").expect("loose file");
        let loose = assets.read("data/rules.ini").expect("loose lookup");
        assert_eq!(loose.bytes, b"loose");
        assert!(std::path::Path::new(&loose.origin).ends_with("Data/Rules.ini"));
        assert_eq!(
            assets.list("data/").expect("winning names"),
            ["data/rules.ini"]
        );
    }

    #[test]
    fn installation_uses_zh_before_base_and_first_alphabetical_archive() {
        let fixture = Fixture::new();
        let base = fixture.0.join("base");
        let zh = fixture.0.join("zh");
        fs::create_dir(&base).expect("base directory");
        fs::create_dir(&zh).expect("ZH directory");
        big(&base.join("a.big"), b"base");
        big(&zh.join("B.big"), b"later");
        big(&zh.join("a.big"), b"first");
        let assets = VirtualAssets::for_zero_hour(&base, &zh).expect("installation layers");
        let asset = assets.read("data/rules.ini").expect("winning source");
        assert_eq!(asset.bytes, b"first");
        assert!(std::path::Path::new(&asset.origin).ends_with("zh/a.big"));
    }

    #[test]
    fn rejects_parent_and_absolute_paths() {
        assert!(normalized_path("../secret").is_err());
        assert!(normalized_path("/absolute").is_err());
        assert!(normalized_path("C:\\Windows").is_err());
        assert!(normalized_path("data//rules.ini").is_err());
    }

    #[test]
    fn listing_loads_direct_files_then_sorted_descendants() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("Data/A/0")).expect("directories");
        for name in [
            "Data/Z.ini",
            "Data/A/A.ini",
            "Data/A/Z.ini",
            "Data/A/0/Deep.ini",
        ] {
            fs::write(fixture.0.join(name), b"data").expect("loose file");
        }
        let assets = VirtualAssets::from_layers(vec![Layer::Loose(fixture.0.clone())]);
        assert_eq!(
            assets.list("DATA").expect("listing"),
            [
                "data/z.ini",
                "data/a/0/deep.ini",
                "data/a/a.ini",
                "data/a/z.ini",
            ]
        );
        assert!(
            assets
                .list("data/missing")
                .expect("missing directory")
                .is_empty()
        );
        assert!(assets.list("../data").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn listing_rejects_directory_cycles_and_escaping_file_links() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let root = fixture.0.join("root");
        fs::create_dir_all(root.join("Data")).expect("directories");
        let assets = VirtualAssets::from_layers(vec![Layer::Loose(root.clone())]);
        symlink(root.join("Data"), root.join("Data/loop")).expect("cycle link");
        assert!(assets.list("data").is_err());
        fs::remove_file(root.join("Data/loop")).expect("remove cycle link");
        fs::write(fixture.0.join("outside.ini"), b"outside").expect("outside file");
        symlink(fixture.0.join("outside.ini"), root.join("Data/escape.ini"))
            .expect("escaping link");
        assert!(assets.list("data").is_err());
    }
}

#[cfg(test)]
mod user_map_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn user_map_mount_keeps_installation_precedence_and_rejects_unsafe_paths() {
        let directory = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "user-assets-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let base = directory.join("base");
        let zh = directory.join("zh");
        let user = directory.join("UserData");
        fs::create_dir_all(base.join("maps/same")).unwrap();
        fs::create_dir_all(zh.join("maps/same")).unwrap();
        fs::create_dir_all(user.join("mApS/Same")).unwrap();
        fs::write(zh.join("maps/same/same.map"), b"official").unwrap();
        fs::write(user.join("mApS/Same/Same.map"), b"custom").unwrap();
        fs::write(user.join("mApS/Same/map.ini"), b"local").unwrap();
        let assets = VirtualAssets::with_user_maps(&base, &zh, &user).unwrap();
        assert_eq!(
            assets.read("maps/same/same.map").unwrap().bytes,
            b"official"
        );
        assert_eq!(
            assets.read("USER-MAPS\\SAME\\SAME.MAP").unwrap().bytes,
            b"custom"
        );
        assert_eq!(
            assets.read("user-maps/same/map.ini").unwrap().bytes,
            b"local"
        );
        assert_eq!(
            assets.list("user-maps").unwrap(),
            ["user-maps/same/map.ini", "user-maps/same/same.map"]
        );
        for path in [
            "user-maps/../maps/same/same.map",
            "user-maps/C:/same.map",
            "/user-maps/same/same.map",
        ] {
            assert!(assets.read(path).is_err());
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                zh.join("maps/same/same.map"),
                user.join("mApS/Same/escape.map"),
            )
            .unwrap();
            assert!(assets.read("user-maps/same/escape.map").is_err());
            assert!(assets.list("user-maps").is_err());
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
