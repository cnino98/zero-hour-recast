//! Map-local `GameTextManager::parseMapStringFile` and global-first `fetch`.
use generals_assets::{AssetError, virtual_assets::VirtualAssets};
use std::{collections::BTreeMap, error::Error};

pub(super) fn load(
    assets: &VirtualAssets,
    map_path: &str,
    labels: &mut BTreeMap<String, String>,
) -> Result<(), Box<dyn Error>> {
    let Some((directory, _)) = map_path.rsplit_once('/') else {
        return Ok(());
    };
    match assets.read(&format!("{directory}/map.str")) {
        Ok(asset) => {
            for (label, text) in parse(&asset.bytes)? {
                labels.entry(label).or_insert(text);
            }
        }
        Err(AssetError::Missing(_)) => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn parse(bytes: &[u8]) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut cursor = 0;
    let mut labels = BTreeMap::new();
    while let Some(raw_line) = line(bytes, &mut cursor) {
        let label = trim(raw_line);
        if label.is_empty() || label.starts_with(b"//") {
            continue;
        }
        let label = latin1(label).to_ascii_lowercase();
        let mut text = None;
        loop {
            let value = trim(line(bytes, &mut cursor).ok_or("Unterminated map string label")?);
            if value.eq_ignore_ascii_case(b"END") {
                break;
            }
            if value.first() != Some(&b'"') {
                continue;
            }
            if text.is_some() {
                return Err("Map string label contains more than one string".into());
            }
            let mut quoted = value[1..].to_vec();
            let mut escaped = false;
            let mut index = 0;
            loop {
                if index == quoted.len() {
                    quoted.push(b'\n');
                    let continuation =
                        line(bytes, &mut cursor).ok_or("Unterminated map string quote")?;
                    quoted.extend_from_slice(continuation);
                }
                let character = quoted[index];
                if character == b'"' && !escaped {
                    break;
                }
                escaped = character == b'\\' && !escaped;
                if character.is_ascii_whitespace() {
                    quoted[index] = b' ';
                }
                index += 1;
            }
            text = Some(translate(&quoted[..index]));
        }
        let text = text.ok_or("Map string label has no string")?;
        if labels.insert(label, text).is_some() {
            return Err("Map string label is multiply defined".into());
        }
    }
    Ok(labels)
}

fn line<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    if *cursor == bytes.len() {
        return None;
    }
    let start = *cursor;
    let end = bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start + offset);
    *cursor = if end < bytes.len() { end + 1 } else { end };
    Some(&bytes[start..end])
}

fn trim(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

fn translate(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut escaped = false;
    for byte in bytes {
        if escaped {
            result.push(match byte {
                b'n' => '\n',
                b't' => '\t',
                byte => char::from(*byte),
            });
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else {
            result.push(char::from(*byte));
        }
    }
    result.trim_matches(char::is_whitespace).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_global_labels_dominate_local_text_case_insensitively() {
        let mut global = BTreeMap::from([("map:title".into(), "Original global title".into())]);
        for (label, text) in
            parse(b"MAP:TITLE\n\"Local title\"\nEND\nMap:OnlyLocal\n\"Local text\"\nEND\n").unwrap()
        {
            global.entry(label).or_insert(text);
        }
        assert_eq!(global["map:title"], "Original global title");
        assert_eq!(global["map:onlylocal"], "Local text");
    }

    #[test]
    fn map_string_reader_preserves_native_byte_text_escapes_and_multiline_spaces() {
        let labels = parse(b"// original local strings\nMap:Title\n\"  caf\xe9\\nLine\\tTwo \\\"quoted\\\"\n   continuation  \" = Voice123\nEND\nMap:Other\n\"\\\\\\?\\q\"\nEND\n").unwrap();
        assert_eq!(
            labels["map:title"],
            "caf\u{e9}\nLine\tTwo \"quoted\"    continuation"
        );
        assert_eq!(labels["map:other"], "\\?q");
        assert!(parse(b"MAP:A\n\"one\"\nEND\nmap:a\n\"two\"\nEND\n").is_err());
        assert!(parse(b"MAP:A\n\"one\n").is_err());
    }
}
