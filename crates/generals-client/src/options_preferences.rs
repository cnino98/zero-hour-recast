//! Represented `OptionPreferences` and `OptionsMenu::saveOptions`; originals stay read-only.
use bevy::prelude::Resource;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt::Write,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Values {
    pub resolution: [u32; 2],
    pub gamma: i32,
    pub music: f32,
    pub sound: f32,
    pub sound3d: f32,
    pub voice: f32,
    pub alternate_mouse: bool,
    pub scroll_factor: f32,
    pub retaliation: bool,
    pub double_click_attack_move: bool,
    pub money_transaction_volume: f32,
}

pub struct Draft {
    pub resolution: [u32; 2],
    pub gamma: i32,
    pub music: i32,
    pub sfx: i32,
    pub voice: i32,
    pub scroll: i32,
    pub alternate_mouse: bool,
    pub retaliation: bool,
    pub double_click_attack_move: bool,
}

impl Draft {
    pub fn from_values(values: &Values) -> Self {
        let scroll = (values.scroll_factor * 100.0).trunc();
        Self {
            resolution: values.resolution,
            gamma: values.gamma,
            music: slider(values.music),
            sfx: slider(values.sound.max(values.sound3d)),
            voice: slider(values.voice),
            scroll: if (10.0..=145.0).contains(&scroll) {
                num_traits::ToPrimitive::to_i32(&scroll).expect("source scroll slider")
            } else {
                0
            },
            alternate_mouse: values.alternate_mouse,
            retaliation: values.retaliation,
            double_click_attack_move: values.double_click_attack_move,
        }
    }
}

#[derive(Resource, Clone)]
pub struct Preferences {
    applied: Values,
    defaults: Values,
    relative: f32,
    fields: BTreeMap<String, String>,
    path: PathBuf,
}

impl Preferences {
    pub const fn applied(&self) -> &Values {
        &self.applied
    }
    pub fn draft(&self) -> Draft {
        Draft::from_values(&self.applied)
    }
    pub fn defaults(&self) -> Draft {
        let mut draft = Draft::from_values(&self.defaults);
        // OptionsMenu::setDefaults keeps the current resolution.
        draft.resolution = self.applied.resolution;
        draft
    }

    pub fn load(assets: &VirtualAssets, original: &Path) -> Result<Self, Box<dyn Error>> {
        let audio = content::scan(assets.read("data/ini/audiosettings.ini")?)?;
        let audio = audio
            .declarations
            .iter()
            .find(|entry| entry.kind == "AudioSettings")
            .ok_or("AudioSettings missing")?;
        let value = |key| source_number(&audio.lines, key, true);
        let defaults = default_values(assets, &audio.lines)?;
        let relative = value("Relative2DVolume")?.clamp(-1.0, 1.0);
        let root = original
            .parent()
            .and_then(Path::parent)
            .ok_or("options have no game root")?;
        let path = root.join("output/Options.ini");
        let source = match read(&path)? {
            Some(source) => source,
            None => read(original)?.unwrap_or_default(),
        };
        let fields = fields(&source);
        let applied = Values {
            resolution: resolution(field(&fields, "Resolution")).unwrap_or(defaults.resolution),
            gamma: field(&fields, "Gamma").map_or(50, integer_prefix),
            music: option_number(&fields, "MusicVolume", defaults.music * 100.0)? / 100.0,
            sound: option_number(
                &fields,
                "SFXVolume",
                defaults.sound * (1.0 + relative.min(0.0)) * 100.0,
            )? / 100.0,
            sound3d: option_number(
                &fields,
                "SFX3DVolume",
                defaults.sound3d * (1.0 - relative.max(0.0)) * 100.0,
            )? / 100.0,
            voice: option_number(&fields, "VoiceVolume", defaults.voice * 100.0)? / 100.0,
            alternate_mouse: field(&fields, "UseAlternateMouse")
                .is_some_and(|value| value.eq_ignore_ascii_case("yes")),
            scroll_factor: scroll_factor(&source, defaults.scroll_factor)?,
            retaliation: option_bool(&fields, "Retaliation", defaults.retaliation),
            double_click_attack_move: option_bool(
                &fields,
                "UseDoubleClickAttackMove",
                defaults.double_click_attack_move,
            ),
            money_transaction_volume: option_number(
                &fields,
                "MoneyTransactionVolume",
                defaults.money_transaction_volume * 100.0,
            )? / 100.0,
        };
        Ok(Self {
            applied,
            defaults,
            relative,
            fields,
            path,
        })
    }

    pub fn save(&mut self, draft: &Draft) -> Result<Values, Box<dyn Error>> {
        let money_percent = option_number(
            &self.fields,
            "MoneyTransactionVolume",
            self.defaults.money_transaction_volume * 100.0,
        )?
        .trunc();
        if draft.resolution.contains(&0) {
            return Err("Options resolution has zero dimensions".into());
        }
        let mut values = Values {
            resolution: draft.resolution,
            gamma: draft.gamma,
            music: num_traits::ToPrimitive::to_f32(&draft.music).expect("source volume") / 100.0,
            sound: num_traits::ToPrimitive::to_f32(&draft.sfx).expect("source volume") / 100.0,
            sound3d: num_traits::ToPrimitive::to_f32(&draft.sfx).expect("source volume") / 100.0,
            voice: num_traits::ToPrimitive::to_f32(&draft.voice).expect("source volume") / 100.0,
            alternate_mouse: draft.alternate_mouse,
            scroll_factor: self.applied.scroll_factor,
            retaliation: draft.retaliation,
            double_click_attack_move: draft.double_click_attack_move,
            money_transaction_volume: money_percent / 100.0,
        };
        if self.relative < 0.0 {
            values.sound *= 1.0 + self.relative;
        } else {
            values.sound3d *= 1.0 - self.relative;
        }
        if draft.scroll > 0 {
            values.scroll_factor =
                num_traits::ToPrimitive::to_f32(&draft.scroll).expect("source scroll") / 100.0;
        }
        let mut fields = self.fields.clone();
        for (key, value) in [
            (
                "Resolution",
                format!("{} {}", draft.resolution[0], draft.resolution[1]),
            ),
            ("Gamma", draft.gamma.to_string()),
            ("MusicVolume", draft.music.to_string()),
            (
                "Retaliation",
                if draft.retaliation { "yes" } else { "no" }.to_owned(),
            ),
            (
                "UseDoubleClickAttackMove",
                if draft.double_click_attack_move {
                    "yes"
                } else {
                    "no"
                }
                .to_owned(),
            ),
            ("MoneyTransactionVolume", money_percent.to_string()),
            ("SFXVolume", (values.sound * 100.0).trunc().to_string()),
            ("SFX3DVolume", (values.sound3d * 100.0).trunc().to_string()),
            ("VoiceVolume", draft.voice.to_string()),
            (
                "UseAlternateMouse",
                if draft.alternate_mouse { "yes" } else { "no" }.to_owned(),
            ),
        ] {
            set(&mut fields, key, value);
        }
        if draft.scroll > 0 {
            set(&mut fields, "ScrollFactor", draft.scroll.to_string());
        }
        let mut text = String::new();
        for (key, value) in &fields {
            writeln!(text, "{key} = {value}")?;
        }
        let parent = self.path.parent().ok_or("options output has no parent")?;
        fs::create_dir_all(parent)?;
        let temporary = self.path.with_extension("ini.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, &self.path)?;
        self.fields = fields;
        self.applied = values.clone();
        Ok(values)
    }
}

// OptionPreferences uses sscanf("%d%d"); invalid dimensions cannot form a native window.
fn resolution(value: Option<&str>) -> Option<[u32; 2]> {
    let mut value = value?;
    let mut dimensions = [0; 2];
    for dimension in &mut dimensions {
        value = value.trim_start();
        let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
        let length = digits.bytes().take_while(u8::is_ascii_digit).count();
        if length == 0 {
            return None;
        }
        let end = length + usize::from(value.starts_with(['+', '-']));
        *dimension = u32::try_from(value[..end].parse::<i32>().ok()?).ok()?;
        if *dimension == 0 {
            return None;
        }
        value = &value[end..];
    }
    Some(dimensions)
}

fn integer_prefix(value: &str) -> i32 {
    let value = value.trim_start();
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    let length = digits.bytes().take_while(u8::is_ascii_digit).count();
    let end = length + usize::from(value.starts_with(['+', '-']));
    value[..end].parse().unwrap_or(0)
}

fn default_values(
    assets: &VirtualAssets,
    lines: &[content::ContentLine],
) -> Result<Values, Box<dyn Error>> {
    let value = |key| source_number(lines, key, true);
    let (scroll_factor, resolution) = game_defaults(assets)?;
    Ok(Values {
        resolution,
        gamma: 50,
        music: value("DefaultMusicVolume")?,
        sound: value("DefaultSoundVolume")?,
        sound3d: value("Default3DSoundVolume")?,
        voice: value("DefaultSpeechVolume")?,
        alternate_mouse: false,
        scroll_factor,
        retaliation: true,
        double_click_attack_move: false,
        // AudioSettings.h39–44: RTS_ZEROHOUR initializes this optional field to 0.
        money_transaction_volume: if crate::skirmish_data::field(
            lines,
            0,
            "DefaultMoneyTransactionVolume",
        )
        .is_some()
        {
            value("DefaultMoneyTransactionVolume")?
        } else {
            0.0
        },
    })
}

fn slider(value: f32) -> i32 {
    num_traits::ToPrimitive::to_i32(&(value * 100.0).trunc()).expect("volume slider bounds")
}

fn game_defaults(assets: &VirtualAssets) -> Result<(f32, [u32; 2]), Box<dyn Error>> {
    let game = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let game = game
        .declarations
        .iter()
        .find(|entry| entry.kind == "GameData")
        .ok_or("GameData missing")?;
    // GeneralsMD GlobalData.cpp1020 initializes both scroll factors to 0.5.
    let scroll = if crate::skirmish_data::field(&game.lines, 0, "KeyboardDefaultScrollSpeedFactor")
        .is_none()
    {
        0.5
    } else {
        source_number(&game.lines, "KeyboardDefaultScrollSpeedFactor", false)?
    };
    Ok((scroll, display_defaults(&game.lines)?))
}

fn display_defaults(lines: &[content::ContentLine]) -> Result<[u32; 2], Box<dyn Error>> {
    let mut dimensions = [800, 600];
    for (index, key) in ["XResolution", "YResolution"].iter().enumerate() {
        if let Some(line) = crate::skirmish_data::field(lines, 0, key) {
            let token = line
                .tokens
                .get(1)
                .ok_or("display default has no dimension")?;
            dimensions[index] = u32::try_from(token.parse::<i32>()?)?;
            if dimensions[index] == 0 {
                return Err(format!("Options default {key} has zero dimensions").into());
            }
        }
    }
    Ok(dimensions)
}

fn source_number(
    lines: &[content::ContentLine],
    key: &str,
    percent: bool,
) -> Result<f32, Box<dyn Error>> {
    let token = crate::skirmish_data::field(lines, 0, key)
        .and_then(|line| line.tokens.get(1))
        .ok_or_else(|| format!("Options default {key} missing"))?;
    let number = token.trim_end_matches('%').parse::<f32>()?;
    if !number.is_finite() {
        return Err(format!("Options default {key} is nonfinite").into());
    }
    Ok(if percent { number / 100.0 } else { number })
}

fn field<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}
fn option_bool(fields: &BTreeMap<String, String>, key: &str, fallback: bool) -> bool {
    field(fields, key).map_or(fallback, |value| value.eq_ignore_ascii_case("yes"))
}
fn option_number(
    fields: &BTreeMap<String, String>,
    key: &str,
    fallback: f32,
) -> Result<f32, Box<dyn Error>> {
    let Some(value) = field(fields, key) else {
        return Ok(fallback);
    };
    let value = value.parse::<f32>()?;
    if !value.is_finite() || num_traits::ToPrimitive::to_i32(&value).is_none() {
        return Err(format!("Options {key} is outside native integer bounds").into());
    }
    Ok(value.max(0.0))
}
pub fn scroll_factor(source: &str, fallback: f32) -> Result<f32, Box<dyn Error>> {
    scroll(&fields(source), fallback)
}

fn scroll(fields: &BTreeMap<String, String>, fallback: f32) -> Result<f32, Box<dyn Error>> {
    let Some(value) = field(fields, "ScrollFactor") else {
        return Ok(fallback);
    };
    let value = value.trim_start();
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    let length = digits.bytes().take_while(u8::is_ascii_digit).count();
    let parsed = if length == 0 {
        0
    } else {
        let sign = usize::from(value.starts_with(['+', '-']));
        value[..length + sign].parse::<i32>()?
    };
    Ok(num_traits::ToPrimitive::to_f32(&parsed.max(1)).expect("source scroll integer") / 100.0)
}

fn fields(source: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for line in source.lines() {
        if let Some((key, value)) = line.split_once('=') {
            set(&mut fields, key.trim(), value.trim().to_owned());
        }
    }
    fields
}
fn set(fields: &mut BTreeMap<String, String>, key: &str, value: String) {
    fields.retain(|existing, _| !existing.eq_ignore_ascii_case(key));
    fields.insert(key.to_owned(), value);
}
fn read(path: &Path) -> Result<Option<String>, io::Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests;
