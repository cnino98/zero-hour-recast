//! Represented native `Skirmish.ini` settings; the original installation is read-only.
mod name;
mod slots;
#[cfg(test)]
mod tests;

use crate::playtest_ui::OpponentSlot;
use generals_sim::skirmish::Faction;
use std::{
    collections::BTreeMap,
    error::Error,
    fmt::Write,
    fs, io,
    path::{Path, PathBuf},
};

pub struct Slot {
    pub faction: Option<Faction>,
    pub opponent: OpponentSlot,
    pub color: Option<usize>,
    pub start: Option<usize>,
    pub team: Option<u8>,
}

impl Slot {
    const fn new(opponent: OpponentSlot) -> Self {
        Self {
            faction: None,
            opponent,
            color: None,
            start: None,
            team: None,
        }
    }
}

pub struct Preferences {
    pub name: String,
    pub slots: Vec<Slot>,
    pub starting_cash: Option<u32>,
    pub superweapon_restricted: bool,
    pub game_speed: u8,
    path: PathBuf,
    fields: BTreeMap<String, String>,
}

impl Preferences {
    pub fn map_path(&self) -> Option<&str> {
        self.fields.get("Map").map(String::as_str)
    }
    pub fn load(options: &Path, default_game_speed: u8) -> Result<Self, Box<dyn Error>> {
        let root = options
            .parent()
            .and_then(Path::parent)
            .ok_or("options have no game root")?;
        let path = root.join("output/Skirmish.ini");
        let fallback = options.with_file_name("Skirmish.ini");
        let source = match read(&path)? {
            Some(source) => source,
            None => read(&fallback)?.unwrap_or_default(),
        };
        let hostname = hostname::get()?
            .into_string()
            .map_err(|_| "local machine name is not Unicode")?;
        Ok(Self::parse(
            path,
            &source,
            hostname.trim_end_matches(['\r', '\n']),
            default_game_speed,
        ))
    }

    fn parse(path: PathBuf, source: &str, hostname: &str, default_game_speed: u8) -> Self {
        let fields: BTreeMap<_, _> = source
            .lines()
            .filter_map(|line| {
                let (key, value) = line.split_once('=')?;
                Some((key.trim().to_owned(), value.trim().to_owned()))
            })
            .collect();
        let starting_cash = fields
            .get("StartingCash")
            .and_then(|value| match value.parse() {
                Ok(value) => Some(value),
                Err(error) => {
                    eprintln!("Ignoring invalid Skirmish StartingCash: {error}");
                    None
                }
            });
        let superweapon_restricted = fields
            .get("SuperweaponRestrict")
            .is_some_and(|value| value.eq_ignore_ascii_case("yes"));
        let game_speed = fields
            .get("FPS")
            .and_then(|value| match value.parse::<i32>() {
                Ok(value) => Some(value),
                Err(error) => {
                    eprintln!("Ignoring invalid Skirmish FPS: {error}");
                    None
                }
            })
            .unwrap_or_else(|| i32::from(default_game_speed))
            .clamp(15, 61);
        let game_speed = u8::try_from(game_speed).expect("source FPS slider bounds");
        let name = fields
            .get("UserName")
            .and_then(|value| match name::decode_name(value) {
                Ok(name) => Some(name.trim().to_owned()),
                Err(error) => {
                    eprintln!("Ignoring invalid Skirmish UserName: {error}");
                    None
                }
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| hostname.to_owned());
        let faction = fields
            .get("PlayerTemplate")
            .and_then(|value| faction(value).ok())
            .flatten();
        let color = fields
            .get("Color")
            .and_then(|value| index(value, 8).ok())
            .flatten();
        let mut slots: Vec<_> = (0..8)
            .map(|index| {
                Slot::new(if index == 1 {
                    OpponentSlot::Easy
                } else {
                    OpponentSlot::Open
                })
            })
            .collect();
        // The native fresh AI slot copies the local player's preferred side/color.
        for slot in &mut slots[..2] {
            slot.faction = faction;
            slot.color = color;
        }
        if let Some(value) = fields.get("SlotList") {
            match slots::parse(value) {
                Ok((loaded, loaded_name)) => {
                    slots = loaded;
                    return Self {
                        name: loaded_name,
                        slots,
                        starting_cash,
                        superweapon_restricted,
                        game_speed,
                        path,
                        fields,
                    };
                }
                Err(error) => eprintln!("Ignoring invalid Skirmish SlotList: {error}"),
            }
        }
        Self {
            name,
            slots,
            starting_cash,
            superweapon_restricted,
            game_speed,
            path,
            fields,
        }
    }

    pub fn save(
        &self,
        selections: &[Slot],
        map: &str,
        seed: u32,
        starting_cash: u32,
        superweapon_restricted: bool,
        game_speed: u8,
    ) -> Result<(), Box<dyn Error>> {
        let mut fields = self.fields.clone();
        fields.insert("StartingCash".into(), starting_cash.to_string());
        fields.insert(
            "SuperweaponRestrict".into(),
            if superweapon_restricted { "Yes" } else { "No" }.into(),
        );
        fields.insert("FPS".into(), game_speed.to_string());
        fields.insert("UserName".into(), name::encode(&self.name));
        fields.insert("Color".into(), number(selections[0].color));
        fields.insert(
            "PlayerTemplate".into(),
            faction_number(selections[0].faction).to_string(),
        );
        fields.insert("Map".into(), map.to_owned());
        fields.insert(
            "SlotList".into(),
            slots::encode(
                selections,
                &self.name,
                map,
                seed,
                starting_cash,
                superweapon_restricted,
            ),
        );
        let mut output = String::new();
        for (key, value) in fields {
            writeln!(output, "{key} = {value}")?;
        }
        let temporary = self.path.with_extension("ini.tmp");
        fs::write(&temporary, output)?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

fn read(path: &Path) -> Result<Option<String>, io::Error> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            eprintln!(
                "Ignoring invalid skirmish preferences {}: {error}",
                path.display()
            );
            Ok(Some(String::new()))
        }
        Err(error) => Err(error),
    }
}

fn index(value: &str, maximum: usize) -> Result<Option<usize>, &'static str> {
    let value = value
        .parse::<i32>()
        .map_err(|_| "invalid selection number")?;
    if value == -1 {
        return Ok(None);
    }
    let index = usize::try_from(value).map_err(|_| "negative selection")?;
    if index >= maximum {
        return Err("selection exceeds original bounds");
    }
    Ok(Some(index))
}

fn faction(value: &str) -> Result<Option<Faction>, &'static str> {
    match value {
        "-1" => Ok(None),
        "2" => Ok(Some(Faction::Usa)),
        "3" => Ok(Some(Faction::China)),
        "4" => Ok(Some(Faction::Gla)),
        _ => Err("unrepresented player template"),
    }
}

fn faction_number(faction: Option<Faction>) -> i32 {
    match faction {
        None => -1,
        Some(Faction::Usa) => 2,
        Some(Faction::China) => 3,
        Some(Faction::Gla) => 4,
        Some(Faction::Civilian) => unreachable!("neutral is not selectable"),
    }
}

fn number(index: Option<usize>) -> String {
    index.map_or_else(|| "-1".into(), |value| value.to_string())
}
