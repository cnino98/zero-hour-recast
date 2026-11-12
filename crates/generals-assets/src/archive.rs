use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use crate::{
    AssetError,
    binary::{Reader, usize_from_u32},
};

const MAX_INDEX_BYTES: u32 = 16 * 1024 * 1024;
const MAX_ASSET_BYTES: u32 = 128 * 1024 * 1024;

/// A normalized entry in a BIG archive.
pub struct Entry {
    /// ASCII lowercase name with forward slashes.
    pub name: String,
    /// Stored byte length, before any compression decoding.
    pub size: u32,
}

/// A bounded index of an original BIGF/BIG4 archive.
pub struct Archive {
    path: PathBuf,
    entries: Vec<Entry>,
    locations: BTreeMap<String, (u64, u32)>,
}

impl Archive {
    /// Read and validate the index without loading archive payloads.
    ///
    /// # Errors
    /// Returns an error for I/O failures, malformed indices, duplicate normalized
    /// names, or payloads outside the file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let file_length = file.metadata()?.len();
        let mut header = [0; 16];
        file.read_exact(&mut header)?;
        let mut reader = Reader::new(&header);
        let magic = reader.take(4)?;
        if magic != b"BIGF" && magic != b"BIG4" {
            return Err(AssetError::Invalid("unsupported BIG signature"));
        }
        let declared_length = reader.le_u32()?;
        let count = reader.be_u32()?;
        let index_end = reader.be_u32()?;
        if u64::from(declared_length) != file_length {
            return Err(AssetError::Invalid("BIG file length does not match header"));
        }
        if !(16..=MAX_INDEX_BYTES).contains(&index_end) || u64::from(index_end) > file_length {
            return Err(AssetError::Invalid("invalid BIG index boundary"));
        }
        let index_length = index_end - 16;
        if count > index_length / 9 {
            return Err(AssetError::Invalid("entry count cannot fit BIG index"));
        }
        let mut index = vec![0; usize_from_u32(index_length)?];
        file.read_exact(&mut index)?;
        let (entries, locations) = parse_index(&index, count, index_end, file_length)?;
        Ok(Self {
            path,
            entries,
            locations,
        })
    }

    /// Return the validated directory in its original order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Read one stored payload, accepting either slash style and ASCII case.
    ///
    /// # Errors
    /// Returns an error if the asset is absent, larger than 128 MiB, truncated,
    /// or cannot be read. Decompression is a separate operation.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, AssetError> {
        let normalized = normalize_name(name);
        let &(offset, size) = self
            .locations
            .get(&normalized)
            .ok_or(AssetError::Missing(normalized))?;
        if size > MAX_ASSET_BYTES {
            return Err(AssetError::Invalid("asset exceeds 128 MiB reader limit"));
        }
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; usize_from_u32(size)?];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}

type Locations = BTreeMap<String, (u64, u32)>;

fn parse_index(
    bytes: &[u8],
    count: u32,
    index_end: u32,
    file_length: u64,
) -> Result<(Vec<Entry>, Locations), AssetError> {
    let mut reader = Reader::new(bytes);
    let mut entries = Vec::with_capacity(usize_from_u32(count)?);
    let mut locations = BTreeMap::new();
    for _ in 0..count {
        let offset = reader.be_u32()?;
        let size = reader.be_u32()?;
        if (size != 0 && offset < index_end) || u64::from(offset) + u64::from(size) > file_length {
            return Err(AssetError::Invalid("BIG payload lies outside data region"));
        }
        let filename = reader.nul_string()?;
        if filename.is_empty() || !filename.is_ascii() {
            return Err(AssetError::Invalid("BIG filename must be nonempty ASCII"));
        }
        let name = normalize_name(
            std::str::from_utf8(filename)
                .map_err(|_| AssetError::Invalid("invalid BIG filename"))?,
        );
        if locations
            .insert(name.clone(), (u64::from(offset), size))
            .is_some()
        {
            return Err(AssetError::Invalid("duplicate normalized BIG filename"));
        }
        entries.push(Entry { name, size });
    }
    Ok((entries, locations))
}

fn normalize_name(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::parse_index;

    fn entry(offset: u32, size: u32, name: &[u8]) -> Vec<u8> {
        let mut bytes = offset.to_be_bytes().to_vec();
        bytes.extend(size.to_be_bytes());
        bytes.extend(name);
        bytes.push(0);
        bytes
    }

    #[test]
    fn rejects_payload_outside_archive_or_inside_index() {
        assert!(parse_index(&entry(99, 2, b"a"), 1, 20, 100).is_err());
        assert!(parse_index(&entry(19, 1, b"a"), 1, 20, 100).is_err());
    }

    #[test]
    fn permits_empty_patch_directory_placeholder() {
        let (entries, _) =
            parse_index(&entry(0, 0, b"Data\\*"), 1, 99, 100).expect("empty patch marker");
        assert_eq!(entries[0].size, 0);
    }

    #[test]
    fn rejects_truncated_or_ambiguous_indices() {
        let bytes = entry(40, 4, b"A\\B");
        assert!(parse_index(&bytes[..bytes.len() - 1], 1, 20, 100).is_err());
        let mut duplicate = bytes;
        duplicate.extend(entry(45, 4, b"a/b"));
        assert!(parse_index(&duplicate, 2, 40, 100).is_err());
    }
}
