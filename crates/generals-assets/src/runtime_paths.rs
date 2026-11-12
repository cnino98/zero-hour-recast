//! Runtime installation discovery. Original game assets are only read.
use std::{
    env, io,
    path::{Path, PathBuf},
};

const INSTALLATIONS: [&str; 2] = [
    "GameData/Command and Conquer Generals",
    "GameData/Command and Conquer Generals Zero Hour",
];

/// Resolve `--game-root`, then `GENERALS_GAME_ROOT`, then a nearby `private` directory.
///
/// A root contains both named installations under `GameData`. `UserData`, `output`
/// and `.cache` belong beside `GameData`, outside the public source checkout.
/// Explicit selections fail immediately when invalid; discovery never masks them.
///
/// # Errors
/// Returns an error for invalid CLI selections, missing installations, unavailable
/// working/executable paths, or a root or writable namespace in the source checkout.
pub fn game_root() -> io::Result<PathBuf> {
    let mut arguments = env::args_os().skip(1);
    let mut selected = None;
    while let Some(argument) = arguments.next() {
        let value = if argument == "--game-root" {
            Some(arguments.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "--game-root requires a path")
            })?)
        } else {
            argument
                .to_str()
                .and_then(|argument| argument.strip_prefix("--game-root="))
                .map(std::ffi::OsString::from)
        };
        if let Some(value) = value
            && (value.is_empty() || selected.replace(PathBuf::from(value)).is_some())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Select exactly one nonempty --game-root path",
            ));
        }
    }
    game_root_from(selected.as_deref())
}

/// Resolve an explicitly selected installation without changing process environment.
///
/// # Errors
/// Returns an error when the selected or discovered root is missing either game
/// installation, cannot be resolved, or places writable data in the source checkout.
pub fn game_root_from(selected: Option<&Path>) -> io::Result<PathBuf> {
    let configured = env::var_os("GENERALS_GAME_ROOT").map(PathBuf::from);
    if selected.is_some() || configured.is_some() {
        return resolve(selected, configured.as_deref(), std::iter::empty());
    }
    let working = env::current_dir()?;
    let executable = env::current_exe()?;
    let directories = working
        .ancestors()
        .chain(executable.parent().into_iter().flat_map(Path::ancestors));
    resolve(None, None, directories)
}

fn resolve<'a>(
    selected: Option<&Path>,
    configured: Option<&Path>,
    directories: impl Iterator<Item = &'a Path>,
) -> io::Result<PathBuf> {
    if let Some(selected) = selected.or(configured) {
        return validate(selected);
    }
    for directory in directories {
        let candidate = directory.join("private");
        if INSTALLATIONS
            .iter()
            .all(|name| candidate.join(name).is_dir())
        {
            return validate(&candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Game files not found. Set --game-root PATH or GENERALS_GAME_ROOT to a directory containing both GameData installations, or place them in ../private/GameData.",
    ))
}

fn validate(root: &Path) -> io::Result<PathBuf> {
    let root = root.canonicalize()?;
    if !INSTALLATIONS.iter().all(|name| root.join(name).is_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "{} must contain GameData/Command and Conquer Generals and GameData/Command and Conquer Generals Zero Hour",
                root.display()
            ),
        ));
    }
    for path in [
        root.clone(),
        root.join("UserData"),
        root.join("output"),
        root.join(".cache"),
    ] {
        // Resolve existing namespace aliases as well as the installation root.
        let existing = path.ancestors().find(|part| part.exists()).unwrap_or(&root);
        let existing = existing.canonicalize()?;
        if existing.ancestors().any(|ancestor| {
            ancestor.join("Cargo.toml").is_file()
                && ancestor.join("crates/generals-assets").is_dir()
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "GameData, UserData, output and cache must be outside the public source checkout",
            ));
        }
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            // The test entrypoint sets the process temp directory outside the
            // checkout. These fixtures never need an installed retail game.
            let root = std::env::temp_dir().join(format!(
                "runtime-paths-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn installation(&self, relative: &str) -> PathBuf {
            let root = self.0.join(relative);
            for name in INSTALLATIONS {
                fs::create_dir_all(root.join(name)).unwrap();
            }
            root
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn explicit_choice_precedes_environment_and_does_not_hide_missing_retail_data() {
        let fixture = Fixture::new();
        let selected = fixture.installation("explicit choice");
        let configured = fixture.installation("environment");
        assert_eq!(
            resolve(Some(&selected), Some(&configured), std::iter::empty()).unwrap(),
            selected.canonicalize().unwrap()
        );
        fs::remove_dir(selected.join(INSTALLATIONS[1])).unwrap();
        assert_eq!(
            resolve(Some(&selected), Some(&configured), std::iter::empty())
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            resolve(None, Some(&configured), std::iter::empty()).unwrap(),
            configured.canonicalize().unwrap()
        );
    }

    #[test]
    fn discovery_uses_working_directory_then_executable_and_requires_both_games() {
        let fixture = Fixture::new();
        let working_root = fixture.installation("working/private");
        let executable_root = fixture.installation("installed/private");
        let working = fixture.0.join("working/public/crates/client");
        let executable = fixture.0.join("installed/bin");
        fs::create_dir_all(&working).unwrap();
        fs::create_dir_all(&executable).unwrap();
        // Model the two ancestor chains within this fixture. The real host may also have an installation.
        let discover = || {
            let local = |part: &&Path| part.starts_with(&fixture.0);
            resolve(
                None,
                None,
                working
                    .ancestors()
                    .take_while(local)
                    .chain(executable.ancestors().take_while(local)),
            )
        };
        assert_eq!(discover().unwrap(), working_root.canonicalize().unwrap());
        fs::remove_dir(working_root.join(INSTALLATIONS[0])).unwrap();
        assert_eq!(discover().unwrap(), executable_root.canonicalize().unwrap());
        fs::remove_dir(executable_root.join(INSTALLATIONS[1])).unwrap();
        assert_eq!(discover().unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn writable_namespaces_cannot_be_inside_a_public_checkout() {
        let fixture = Fixture::new();
        let checkout = fixture.installation("public");
        fs::write(checkout.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::create_dir_all(checkout.join("crates/generals-assets")).unwrap();
        assert_eq!(
            validate(&checkout).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[cfg(unix)]
    #[test]
    fn output_alias_into_checkout_is_rejected_and_external_root_is_canonicalized() {
        let fixture = Fixture::new();
        let root = fixture.installation("private");
        let checkout = fixture.0.join("public");
        fs::create_dir_all(checkout.join("crates/generals-assets")).unwrap();
        fs::write(checkout.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::os::unix::fs::symlink(&root, fixture.0.join("root-alias")).unwrap();
        assert_eq!(
            validate(&fixture.0.join("root-alias")).unwrap(),
            root.canonicalize().unwrap()
        );
        std::os::unix::fs::symlink(&checkout, root.join("output")).unwrap();
        assert_eq!(
            validate(&root).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
