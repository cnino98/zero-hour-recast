use std::{collections::BTreeMap, error::Error, io::Cursor, path::Path};

use bevy::prelude::AudioSource;
use generals_assets::{content, virtual_assets::VirtualAssets};
use rodio::Source;

use super::{Program, Track};
use crate::{
    skirmish_data::{definition, duration_frames, field},
    source_audio::data,
};

type Loaded = (BTreeMap<String, Track>, [String; 3], u16, f32);

pub(super) fn load(
    assets: &VirtualAssets,
    options: &Path,
    scripts: &Program,
) -> Result<Loaded, Box<dyn Error>> {
    let templates = [content::scan(assets.read("data/ini/playertemplate.ini")?)?];
    let mut scores = Vec::new();
    for template in ["FactionAmerica", "FactionChina", "FactionGLA"] {
        let lines = &definition(&templates, "PlayerTemplate", template)?.lines;
        scores.push(
            data::value(lines, "ScoreScreenMusic")?
                .ok_or("ScoreScreenMusic missing")?
                .to_owned(),
        );
    }
    let names: Vec<_> = scripts
        .tracks()
        .chain(scores.iter().map(String::as_str))
        .collect();
    let (tracks, fade_frames, volume) = load_tracks(assets, options, &names)?;
    eprintln!(
        "Original music: {} script-selected/score recordings",
        tracks.len()
    );
    Ok((
        tracks,
        scores
            .try_into()
            .map_err(|_| "expected three score tracks")?,
        fade_frames,
        volume,
    ))
}

type LoadedTracks = (BTreeMap<String, Track>, u16, f32);

pub(super) fn load_shell(
    assets: &VirtualAssets,
    options: &Path,
) -> Result<LoadedTracks, Box<dyn Error>> {
    load_tracks(assets, options, &["Shell"])
}

pub(super) fn load_track(assets: &VirtualAssets, name: &str) -> Result<Track, Box<dyn Error>> {
    let (mut tracks, _, _) = load_tracks(assets, Path::new(""), &[name])?;
    Ok(tracks.remove(name).expect("requested original music track"))
}

fn load_tracks(
    assets: &VirtualAssets,
    options: &Path,
    names: &[&str],
) -> Result<LoadedTracks, Box<dyn Error>> {
    let settings = [content::scan(assets.read("data/ini/audiosettings.ini")?)?];
    let lines = &definition(&settings, "AudioSettings", "")?.lines;
    let fade_frames = u16::try_from(duration_frames(
        field(lines, 0, "TimeToFadeAudio").ok_or("TimeToFadeAudio missing")?,
    )?)?;
    let root = data::value(lines, "AudioRoot")?.ok_or("AudioRoot missing")?;
    let folder = data::value(lines, "MusicFolder")?.ok_or("MusicFolder missing")?;
    let mut master = data::percent(lines, "DefaultMusicVolume", 0.0)?;
    if options.is_file() {
        for line in std::fs::read_to_string(options)?.lines() {
            if let Some((key, value)) = line.split_once('=')
                && key.trim() == "MusicVolume"
            {
                master = data::signed_number(value.trim())?.max(0.0) / 100.0;
            }
        }
    }
    let definitions = definitions(assets)?;
    let mut tracks = BTreeMap::new();
    for &name in names {
        if tracks.contains_key(name) {
            continue;
        }
        let (filename, volume) = definitions
            .get(name)
            .ok_or_else(|| format!("missing music track {name}"))?;
        if filename.is_empty() {
            return Err(format!("missing filename for music track {name}").into());
        }
        let bytes = assets.read(&format!("{root}/{folder}/{filename}"))?.bytes;
        let source = AudioSource {
            bytes: bytes.into(),
        };
        let decoder = rodio::Decoder::builder()
            .with_byte_len(u64::try_from(source.bytes.len())?)
            .with_data(Cursor::new(source.clone()))
            .build()?;
        let duration = decoder
            .total_duration()
            .ok_or_else(|| format!("music track {name} has no duration"))?;
        if duration.is_zero() {
            return Err(format!("empty original music track {name}").into());
        }
        tracks.insert(
            name.to_owned(),
            Track {
                source: Some(source),
                handle: None,
                volume: volume * master,
                source_volume: *volume,
                duration,
            },
        );
    }
    Ok((tracks, fade_frames, master))
}

fn definitions(assets: &VirtualAssets) -> Result<BTreeMap<String, (String, f32)>, Box<dyn Error>> {
    let mut definitions = BTreeMap::new();
    for stem in ["default/music", "music"] {
        for file in data::files(assets, stem)? {
            for declaration in file
                .declarations
                .into_iter()
                .filter(|entry| entry.kind == "MusicTrack")
            {
                let inherited = definitions
                    .get(&declaration.name)
                    .or_else(|| definitions.get("DefaultMusicTrack"));
                let filename = data::value(&declaration.lines, "Filename")?.map_or_else(
                    || {
                        inherited.map_or_else(String::new, |(filename, _): &(String, f32)| {
                            filename.clone()
                        })
                    },
                    str::to_owned,
                );
                let volume = data::percent(
                    &declaration.lines,
                    "Volume",
                    inherited.map_or(0.0, |(_, volume)| *volume),
                )?;
                definitions.insert(declaration.name, (filename, volume));
            }
        }
    }
    Ok(definitions)
}
