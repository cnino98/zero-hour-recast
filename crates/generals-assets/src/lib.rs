//! Bounded, read-only importers for original Generals game assets.
//!
//! These readers import a user's existing game files. They neither extract paths
//! from archives onto disk nor modify the installation.

mod archive;
mod binary;
mod heightmap;
mod refpack;
mod runtime_paths;

pub mod ani;
pub mod content;
pub mod map;
pub mod roster;
pub mod scripts;
pub mod virtual_assets;
pub mod w3d;

pub use archive::{Archive, Entry};
pub use heightmap::HeightMap;
pub use refpack::decompress;
pub use runtime_paths::{game_root, game_root_from};

use std::{fmt, io};

/// An I/O failure, missing asset, or rejected asset format.
#[derive(Debug)]
pub enum AssetError {
    /// Reading the original file failed.
    Io(io::Error),
    /// The input violates a format invariant or exceeds a resource limit.
    Invalid(&'static str),
    /// The normalized asset name was not found in the archive.
    Missing(String),
    /// A selected INI definition is malformed, ambiguous, or unsupported.
    Definition(String),
}

impl fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "asset I/O: {error}"),
            Self::Invalid(reason) => write!(formatter, "invalid asset: {reason}"),
            Self::Missing(name) => write!(formatter, "asset not found: {name}"),
            Self::Definition(message) => write!(formatter, "definition error: {message}"),
        }
    }
}

impl std::error::Error for AssetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Invalid(_) | Self::Missing(_) | Self::Definition(_) => None,
        }
    }
}

impl From<io::Error> for AssetError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> AssetError {
    AssetError::Definition(message.into())
}
