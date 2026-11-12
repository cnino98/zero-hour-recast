//! SH `MultiPlayerLoadScreen::init`, the local `GAME_SKIRMISH` loading layout.
use super::{find, results, translate, value};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

pub struct Window {
    pub name: String,
    pub layout: results::Window,
    pub bold: bool,
    pub see_through: bool,
}

pub struct General {
    pub name: String,
    pub side: String,
    pub features: String,
    pub portrait: String,
    pub music: String,
}

pub struct Data {
    pub windows: Vec<Window>,
    pub generals: BTreeMap<String, General>,
    pub show_random_faction: bool,
    pub show_random_color: bool,
    pub show_random_start: bool,
    pub bold_font: Option<Font>,
    pub bold_handle: Handle<Font>,
    images: BTreeSet<String>,
}

impl Data {
    pub(super) fn load(
        assets: &VirtualAssets,
        labels: &BTreeMap<String, String>,
    ) -> Result<Self, Box<dyn Error>> {
        let asset = assets.read("window/menus/multiplayerloadscreen.wnd")?;
        let mut windows = Vec::new();
        let mut images = BTreeSet::new();
        for block in std::str::from_utf8(&asset.bytes)?
            .split("WINDOWTYPE = ")
            .skip(1)
        {
            let name = results::property(block, "NAME")?
                .trim_matches('"')
                .strip_prefix("MultiplayerLoadScreen.wnd:")
                .ok_or("unexpected loading window name")?
                .to_owned();
            let font = results::property(block, "FONT")?;
            let layout = results::window(block, labels)?;
            if let Some(image) = &layout.image {
                images.insert(image.clone());
            }
            windows.push(Window {
                name,
                layout,
                bold: font.contains("BOLD: 1"),
                see_through: results::property(block, "STATUS")?
                    .split('+')
                    .any(|flag| flag == "SEE_THRU"),
            });
        }
        for image in [
            "LoadingBar_DePowered",
            "LoadingBar_Progress",
            "FrameT",
            "FrameB",
            "FrameL",
            "FrameR",
            "FrameCornerUL",
            "FrameCornerUR",
            "FrameCornerLL",
            "FrameCornerLR",
            "TecBuilding",
            "Cash",
        ] {
            images.insert(image.to_owned());
        }
        for color in 0..8 {
            images.insert(format!("LoadingBar_ProgressCenter{color}"));
        }
        let generals = load_generals(assets, labels)?;
        images.extend(generals.values().map(|general| general.portrait.clone()));
        let multiplayer = content::scan(assets.read("data/ini/multiplayer.ini")?)?;
        let settings = find(&multiplayer, "MultiplayerSettings", "")?;
        let show_random = |name| -> Result<bool, Box<dyn Error>> {
            match super::optional_value(&settings.lines, name)?.unwrap_or("Yes") {
                "Yes" => Ok(true),
                "No" => Ok(false),
                _ => Err("invalid random visibility setting".into()),
            }
        };
        Ok(Self {
            windows,
            generals,
            images,
            show_random_faction: show_random("ShowRandomPlayerTemplate")?,
            show_random_color: show_random("ShowRandomColor")?,
            show_random_start: show_random("ShowRandomStartPos")?,
            bold_font: Some(crate::fonts::load("Arial", true)?),
            bold_handle: Handle::default(),
        })
    }

    pub fn music(&self, faction: generals_sim::skirmish::Faction) -> &str {
        let template = match faction {
            generals_sim::skirmish::Faction::Usa => "FactionAmerica",
            generals_sim::skirmish::Faction::China => "FactionChina",
            generals_sim::skirmish::Faction::Gla => "FactionGLA",
            generals_sim::skirmish::Faction::Civilian => {
                panic!("local loading requires a playable faction")
            }
        };
        &self.generals[template].music
    }

    pub fn window(&self, name: &str) -> &Window {
        self.windows
            .iter()
            .find(|window| window.name == name)
            .expect("original loading window")
    }

    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.images.iter().map(String::as_str)
    }
}

fn load_generals(
    assets: &VirtualAssets,
    labels: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, General>, Box<dyn Error>> {
    let templates = content::scan(assets.read("data/ini/playertemplate.ini")?)?;
    let challenges = content::scan(assets.read("data/ini/challengemode.ini")?)?;
    let challenge = find(&challenges, "ChallengeGenerals", "")?;
    let mut personas = BTreeMap::new();
    // ChallengeGenerals supplies shared local-skirmish portraits; challenge missions stay excluded.
    for block in challenge.lines.split(|line| line.depth == Some(0)) {
        let field = |name: &str| {
            block
                .iter()
                .find(|line| line.tokens.first().is_some_and(|token| token == name))
                .and_then(|line| line.tokens.get(1))
        };
        if let Some(template) = field("PlayerTemplate") {
            let portrait = field("BioPortraitLarge").ok_or("general portrait missing")?;
            let name = field("BioNameString").ok_or("general name missing")?;
            personas.insert(template.clone(), (portrait.clone(), name.clone()));
        }
    }
    let mut generals = BTreeMap::new();
    for template in &templates.declarations {
        if template.kind != "PlayerTemplate"
            || !template
                .lines
                .iter()
                .any(|line| line.tokens.as_slice() == ["PlayableSide", "Yes"])
        {
            continue;
        }
        let side = translate(labels, value(&template.lines, 0, "DisplayName")?)?;
        let (portrait, name) = if let Some((portrait, name)) = personas.get(&template.name) {
            (portrait.clone(), translate(labels, name)?)
        } else {
            let image = match template.name.as_str() {
                "FactionAmerica" => "SAFactionLogoLg_US",
                "FactionChina" => "SNFactionLogoLg_China",
                "FactionGLA" => "SUFactionLogoLg_GLA",
                _ => return Err(format!("missing general persona {}", template.name).into()),
            };
            (image.to_owned(), side.clone())
        };
        generals.insert(
            template.name.clone(),
            General {
                name,
                side,
                portrait,
                features: translate(labels, value(&template.lines, 0, "Features")?)?,
                music: value(&template.lines, 0, "LoadScreenMusic")?.to_owned(),
            },
        );
    }
    Ok(generals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_skirmish_loading_keeps_original_layout_labels_and_general_portraits() {
        let root = generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let labels =
            super::super::strings(&assets.read("data/english/generals.csf").unwrap().bytes)
                .unwrap();
        let data = Data::load(&assets, &labels).unwrap();
        assert_eq!(
            data.window("ParentMultiplayerLoadScreen")
                .layout
                .image
                .as_deref(),
            Some("MDSkirmishLoadB")
        );
        assert_eq!(
            data.window("WinMapPreview").layout.area,
            Rect::new(529.0, 120.0, 743.0, 320.0)
        );
        assert_eq!(
            data.window("ProgressLoad0").layout.area,
            Rect::new(596.0, 363.0, 744.0, 383.0)
        );
        assert_eq!(
            data.window("StaticTextPlayer7").layout.area,
            Rect::new(62.0, 531.0, 242.0, 551.0)
        );
        assert_eq!(
            data.window("StaticTextProgressTitle").layout.text,
            labels["gui:progress"]
        );
        assert_eq!(
            data.generals["FactionAmerica"].portrait,
            "SAFactionLogoLg_US"
        );
        assert_eq!(
            data.generals["FactionChina"].portrait,
            "SNFactionLogoLg_China"
        );
        assert_eq!(data.generals["FactionGLA"].portrait, "SUFactionLogoLg_GLA");
        assert_eq!(
            data.generals["FactionAmericaAirForceGeneral"].portrait,
            "PAAirGen"
        );
        assert_eq!(
            data.generals["FactionGLAToxinGeneral"].name,
            labels["gui:bionameentry_pos1"]
        );
        assert_eq!(
            data.generals["FactionChinaTankGeneral"].features,
            labels["gui:biofeatures_pos4"]
        );
        assert!(data.show_random_faction && data.show_random_color && data.show_random_start);
        assert!(
            !data
                .windows
                .iter()
                .any(|window| window.name.contains("Cancel"))
        );
    }
}
