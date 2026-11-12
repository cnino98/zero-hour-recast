//! Original `OptionsMenu` WND controls represented by the local renderer/input.
use super::window_rects;
use bevy::prelude::Rect;
use generals_assets::virtual_assets::VirtualAssets;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

pub struct Label {
    pub area: Rect,
    pub text: String,
    pub font_size: f32,
    pub serif: bool,
}

pub struct Data {
    pub windows: BTreeMap<String, Rect>,
    pub background: Rect,
    pub confirmation: BTreeMap<String, Rect>,
    pub labels: Vec<Label>,
    images: BTreeSet<String>,
}

impl Data {
    pub fn load(
        assets: &VirtualAssets,
        labels: &BTreeMap<String, String>,
    ) -> Result<Self, Box<dyn Error>> {
        let asset = assets.read("window/menus/optionsmenu.wnd")?;
        let source = std::str::from_utf8(&asset.bytes)?;
        let windows = window_rects(&asset.bytes)?;
        let confirmation = window_rects(&assets.read("window/menus/messagebox.wnd")?.bytes)?;
        let root = source
            .split("WINDOWTYPE = ")
            .nth(1)
            .ok_or("options layout has no outer window")?;
        let background = super::window_area(root)?;
        let mut text = Vec::new();
        let mut images = BTreeSet::new();
        for block in source.split("WINDOWTYPE = ").skip(1) {
            let name = block
                .split_once("NAME = \"")
                .and_then(|(_, value)| value.split_once('"'))
                .map(|(name, _)| name)
                .ok_or("options window has no name")?;
            if matches!(
                name,
                "OptionsMenu.wnd:SliderMusicVolume"
                    | "OptionsMenu.wnd:SliderSFXVolume"
                    | "OptionsMenu.wnd:SliderVoiceVolume"
                    | "OptionsMenu.wnd:SliderScrollSpeed"
                    | "OptionsMenu.wnd:SliderGamma"
                    | "OptionsMenu.wnd:ComboBoxResolution"
                    | "OptionsMenu.wnd:CheckAlternateMouse"
                    | "OptionsMenu.wnd:Retaliation"
                    | "OptionsMenu.wnd:CheckDoubleClickAttackMove"
            ) {
                images.extend(
                    block
                        .split("IMAGE: ")
                        .skip(1)
                        .filter_map(|value| value.split_once(',').map(|(image, _)| image.trim()))
                        .filter(|&image| image != "NoImage")
                        .map(str::to_owned),
                );
            }
            if !matches!(
                name,
                "OptionsMenu.wnd:MusicVolumeLabel"
                    | "OptionsMenu.wnd:SFXVolumeLabel"
                    | "OptionsMenu.wnd:VoiceVolumeLabel"
                    | "OptionsMenu.wnd:ScrollSpeedLabel"
                    | "OptionsMenu.wnd:GammaLabel"
                    | "OptionsMenu.wnd:ResolutionLabel"
            ) {
                continue;
            }
            let tag = block
                .split_once("TEXT = \"")
                .and_then(|(_, value)| value.split_once('"'))
                .map(|(tag, _)| tag)
                .ok_or("options label has no text")?;
            let font = block
                .split_once("FONT = ")
                .and_then(|(_, value)| value.split_once(';'))
                .map(|(font, _)| font)
                .ok_or("options label has no font")?;
            let size = font
                .split_once("SIZE: ")
                .and_then(|(_, value)| value.split_once(','))
                .map(|(size, _)| size)
                .ok_or("options label has no font size")?
                .parse::<f32>()?;
            text.push(Label {
                area: windows[name],
                text: labels
                    .get(&tag.to_ascii_lowercase())
                    .ok_or_else(|| format!("missing options label {tag}"))?
                    .clone(),
                font_size: size,
                serif: font.contains("Times New Roman"),
            });
        }
        Ok(Self {
            windows,
            confirmation,
            background,
            labels: text,
            images,
        })
    }

    pub fn area(&self, name: &str) -> Rect {
        self.windows[&format!("OptionsMenu.wnd:{name}")]
    }

    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.images.iter().map(String::as_str)
    }
}
