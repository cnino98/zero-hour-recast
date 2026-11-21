use crate::source_texture;

mod cancellation;
mod garrison;
pub mod loading;
mod name_entry;
pub mod observer;
pub mod ocl_timer;
pub mod options;
pub mod power_shortcuts;
mod promotions;
pub mod results;
use bevy::prelude::*;
use generals_assets::{
    content::{self, ContentFile, ContentLine, Declaration},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::UnitType;
pub use promotions::{Promotions, ScienceButton};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    path::Path,
};

#[derive(Resource)]
pub struct UiData {
    pub starting_cash: Vec<u32>,
    pub default_starting_cash: u32,
    pub default_game_speed: u8,
    pub observer: observer::Data,
    pub options: options::Data,
    pub loading: loading::Data,
    pub preferences: crate::skirmish_preferences::Preferences,
    pub name_entry: name_entry::Entry,
    pub map_title: String,
    pub map_preview: String,
    pub map_extent: Vec2,
    pub map_starts: Vec<Vec2>,
    pub color_labels: Vec<String>,
    pub schemes: Vec<Scheme>,
    pub promotions: Promotions,
    pub results: results::Results,
    pub units: Vec<UnitUi>,
    pub cancel_construction: Command,
    pub ocl_timer: Option<ocl_timer::Data>,
    pub power_shortcuts: Option<power_shortcuts::Data>,
    pub windows: BTreeMap<String, Rect>,
    pub labels: BTreeMap<String, String>,
    pub factions: [String; 3],
    pub font: Option<Font>,
    pub font_handle: Handle<Font>,
    pub serif_font: Option<Font>,
    pub serif_handle: Handle<Font>,
    pub power_base: f32,
    pub power_intervals: f32,
    pub power_yellow: i16,
    pub textures: BTreeMap<String, Image>,
    pub handles: BTreeMap<String, Handle<Image>>,
    texture_sizes: BTreeMap<String, Vec2>,
    pictures: BTreeMap<String, Picture>,
}

pub struct Scheme {
    pub command_marker: Option<String>,
    pub background: String,
    pub area: Rect,
    pub money: Rect,
    pub power: Rect,
    pub options_area: Rect,
    pub options: [String; 3],
    pub worker_area: Rect,
    pub worker: [String; 4],
    pub logo: String,
    pub general_area: Rect,
    pub general: [String; 4],
    pub science_background: String,
    pub clock_color: Vec4,
}

pub struct UnitUi {
    pub structure_inventory: bool,
    pub commands: Vec<Command>,
    pub upgraded_commands: Vec<(String, Vec<Command>)>,
    pub named_commands: Vec<(String, Vec<Command>)>,
    pub portrait: String,
    pub button_image: String,
    pub name: String,
}

pub struct Command {
    pub name: String,
    pub slot: usize,
    pub action: CommandAction,
    pub accelerator: Option<char>,
    pub multi_select: bool,
    pub image: String,
    pub label: String,
    pub description: String,
    pub sciences: Vec<String>,
    pub required_upgrade: Option<RequiredUpgrade>,
}

pub enum RequiredUpgrade {
    Player(String),
    Object(String),
}

#[derive(Clone, Copy)]
pub enum CommandAction {
    Produce(usize),
    Stop,
    AttackMove,
    Guard,
    GuardFlying,
    Rally,
    CashHack,
    HackInternet,
    DisableBuilding,
    Hack(usize),
    Cleanup,
    FireWeapon(usize),
    ClearMines(usize),
    SnipeVehicle(usize),
    BurtonKnife,
    BurtonCharge(generals_sim::skirmish::special_ability::BurtonCharge),
    BurtonDetonate,
    BombTruckDisguise,
    BombTruckDetonate,
    CarBomb,
    Hijack,
    HelixBomb,
    Sabotage,
    EmergencyRepair,
    SpectreGunship,
    AirForceSpectreGunship,
    NativePower(PowerCommand),
    CancelConstruction,
    Sell,
    DetonateFake,
    Research(usize),
    Capture,
    Evacuate,
    CombatDrop,
    ExitContainer(usize),
    SwitchWeapon(usize),
    LaserLock,
    BattlePlan(generals_sim::skirmish::battle_plan::Plan),
}

#[derive(Clone, Copy)]
pub struct PowerCommand {
    pub name: &'static str,
    pub aim: PowerAim,
}

#[derive(Clone, Copy)]
pub enum PowerAim {
    Immediate,
    Location,
    Object,
    Construct(usize),
}

fn supported_power(name: &str) -> Option<&'static str> {
    [
        "SpecialPowerSpySatellite",
        "SpecialPowerSpyDrone",
        "SuperweaponCIAIntelligence",
        "SuperweaponParadropAmerica",
        "SuperweaponA10ThunderboltMissileStrike",
        "SuperweaponDaisyCutter",
        "SuperweaponLeafletDrop",
        "SuperweaponParticleUplinkCannon",
        "SuperweaponArtilleryBarrage",
        "SuperweaponClusterMines",
        "SuperweaponChinaCarpetBomb",
        "Early_SuperweaponChinaCarpetBomb",
        "SuperweaponEMPPulse",
        "SuperweaponFrenzy",
        "SuperweaponNeutronMissile",
        "SuperweaponScudStorm",
        "SuperweaponRebelAmbush",
        "SuperweaponAnthraxBomb",
        "SuperweaponGPSScrambler",
        "SuperweaponSneakAttack",
        "SpecialPowerRadarVanScan",
        "SpecialAbilityBoobyTrap",
    ]
    .into_iter()
    .find(|power| *power == name)
}

pub fn supports_radius_power(name: &str) -> bool {
    supported_power(name).is_some()
        || matches!(
            name,
            "SuperweaponEmergencyRepair"
                | "SuperweaponSpectreGunship"
                | "AirF_SuperweaponSpectreGunship"
        )
}

impl CommandAction {
    pub const fn power(self) -> Option<&'static str> {
        match self {
            Self::CashHack => Some("SuperweaponCashHack"),
            Self::EmergencyRepair => Some("SuperweaponEmergencyRepair"),
            Self::SpectreGunship => Some("SuperweaponSpectreGunship"),
            Self::AirForceSpectreGunship => Some("AirF_SuperweaponSpectreGunship"),
            Self::NativePower(power) => Some(power.name),
            _ => None,
        }
    }

    pub const fn product(self) -> Option<usize> {
        if let Self::Produce(index) = self {
            Some(index)
        } else {
            None
        }
    }

    pub const fn power_command(self) -> Option<PowerCommand> {
        if let Self::NativePower(power) = self {
            return Some(power);
        }
        if let Some(name) = self.power() {
            let aim = if matches!(self, Self::CashHack) {
                PowerAim::Object
            } else {
                PowerAim::Location
            };
            return Some(PowerCommand { name, aim });
        }
        None
    }
}

struct Picture {
    texture: String,
    rect: Rect,
    status: String,
}

impl UiData {
    pub const fn ocl_timer(&self) -> &ocl_timer::Data {
        self.ocl_timer
            .as_ref()
            .expect("match control bar OCL timer")
    }

    pub fn commands(&self, game: &generals_sim::skirmish::Skirmish, id: usize) -> &[Command] {
        let ui = &self.units[game.entities[id].type_index];
        if let Some(name) = game.entities[id].command_set_override() {
            return &ui
                .named_commands
                .iter()
                .find(|(set, _)| set == name)
                .expect("loaded source command-set override")
                .1;
        }
        ui.upgraded_commands
            .iter()
            .find(|(trigger, _)| game.object_has_upgrade(id, trigger))
            .map_or(&ui.commands, |(_, commands)| commands)
    }

    pub fn image(&self, name: &str) -> ImageNode {
        let picture = &self.pictures[&name.to_ascii_lowercase()];
        assert!(
            picture.status == "NONE",
            "rotated mapped image requires the original UV material: {name}"
        );
        ImageNode {
            image: self.handles[&picture.texture].clone(),
            rect: Some(picture.rect),
            image_mode: NodeImageMode::Stretch,
            ..default()
        }
    }

    pub fn optional_image(&self, name: &str) -> Option<ImageNode> {
        let picture = self.pictures.get(&name.to_ascii_lowercase())?;
        self.handles.get(&picture.texture)?;
        Some(self.image(name))
    }

    pub fn icon(&self, name: &str) -> crate::playtest_ui::PromotionIcon {
        let picture = &self.pictures[&name.to_ascii_lowercase()];
        let texture = &self.handles[&picture.texture];
        // Mapped images use source atlas pixels; texture dimensions are captured at load.
        let size = self.texture_sizes[&picture.texture];
        crate::playtest_ui::PromotionIcon {
            uv_rect: Vec4::new(
                picture.rect.min.x / size.x,
                picture.rect.min.y / size.y,
                picture.rect.max.x / size.x,
                picture.rect.max.y / size.y,
            ),
            state: Vec4::new(
                0.0,
                1.0,
                -1.0,
                if picture.status == "ROTATED_90_CLOCKWISE" {
                    1.0
                } else {
                    0.0
                },
            ),
            clock_color: Vec4::ZERO,
            image: texture.clone(),
        }
    }

    pub fn label<'a>(&'a self, name: &'a str) -> &'a str {
        self.labels
            .get(&name.to_ascii_lowercase())
            .map_or(name, String::as_str)
    }

    pub fn image_size(&self, name: &str) -> Vec2 {
        let picture = &self.pictures[&name.to_ascii_lowercase()];
        let size = picture.rect.size();
        if picture.status == "ROTATED_90_CLOCKWISE" {
            Vec2::new(size.y, size.x)
        } else {
            size
        }
    }
}

pub fn load(
    assets: &VirtualAssets,
    catalog: &[UnitType],
    options: &Path,
) -> Result<UiData, Box<dyn Error>> {
    load_for_map(assets, catalog, options, None)
}

pub fn load_shell(assets: &VirtualAssets, options: &Path) -> Result<UiData, Box<dyn Error>> {
    load_ui(assets, None, options, None)
}

pub fn load_for_map(
    assets: &VirtualAssets,
    catalog: &[UnitType],
    options: &Path,
    map_path: Option<&str>,
) -> Result<UiData, Box<dyn Error>> {
    load_ui(assets, Some(catalog), options, map_path)
}

fn load_ui(
    assets: &VirtualAssets,
    catalog: Option<&[UnitType]>,
    options: &Path,
    map_path: Option<&str>,
) -> Result<UiData, Box<dyn Error>> {
    let labels = ui_labels(assets, map_path)?;
    let schemes = load_schemes(assets)?;
    let windows = menu_windows(assets)?;
    let (units, ocl_timer, bomb_icons) = if let Some(catalog) = catalog {
        (
            object_ui(assets, catalog, &labels)?,
            Some(ocl_timer::load(assets, catalog, &labels)?),
            Some(crate::sticky_bomb_icons::load(assets)?),
        )
    } else {
        (Vec::new(), None, None)
    };
    let cancel_construction = cancellation::load(assets, &labels)?;
    let power_shortcuts = catalog
        .map(|catalog| power_shortcuts::load(assets, catalog, &labels))
        .transpose()?;
    let promotions = load_promotions(assets, &labels, options, &windows)?;
    let pictures = mapped_images(assets)?;
    let results = results::load(assets, &labels)?;
    let observer = observer::load(assets, &labels)?;
    let options_menu = options::Data::load(assets, &labels)?;
    let name_entry = name_entry::load(assets, &labels)?;
    let loading = loading::Data::load(assets, &labels)?;
    let veterancy = crate::veterancy_feedback::load(assets)?;
    let money_crates = crate::income::map_crates::load(assets)?;
    let images = results
        .images()
        .chain(observer.images())
        .chain(options_menu.images())
        .chain(veterancy.images())
        .chain(money_crates.images())
        .chain(loading.images());
    let match_images = match_ui_images(
        power_shortcuts.as_ref(),
        bomb_icons.as_ref(),
        ocl_timer.as_ref(),
    );
    let textures = textures(
        assets,
        &schemes,
        &units,
        &promotions,
        images.chain(match_images),
        &pictures,
        &cancel_construction,
    )?;
    let font = crate::fonts::load("Arial", false)?;
    let (power_base, power_intervals, power_yellow) = power_rules(assets)?;
    let factions = [
        translate(&labels, "SIDE:America")?,
        translate(&labels, "SIDE:China")?,
        translate(&labels, "SIDE:GLA")?,
    ];
    let (starting_cash, default_starting_cash) =
        starting_money(&content::scan(assets.read("data/ini/multiplayer.ini")?)?)?;
    let default_game_speed = game_speed(assets)?;
    Ok(UiData {
        starting_cash,
        default_starting_cash,
        default_game_speed,
        observer,
        options: options_menu,
        loading,
        preferences: crate::skirmish_preferences::Preferences::load(options, default_game_speed)?,
        name_entry,
        map_title: String::new(),
        map_preview: String::new(),
        map_extent: Vec2::ZERO,
        map_starts: Vec::new(),
        color_labels: color_labels(assets, &labels)?,
        schemes,
        promotions,
        results,
        units,
        cancel_construction,
        ocl_timer,
        power_shortcuts,
        windows,
        labels: labels
            .into_iter()
            .map(|(key, value)| (key, value.replace('&', "")))
            .collect(),
        factions,
        font: Some(font),
        font_handle: Handle::default(),
        serif_font: Some(crate::fonts::load("Times New Roman", false)?),
        serif_handle: Handle::default(),
        power_base,
        power_intervals,
        power_yellow,
        texture_sizes: textures
            .iter()
            .map(|(name, image)| (name.clone(), image.size_f32()))
            .collect(),
        textures,
        handles: BTreeMap::new(),
        pictures,
    })
}

fn match_ui_images<'a>(
    shortcuts: Option<&'a power_shortcuts::Data>,
    bombs: Option<&'a crate::sticky_bomb_icons::Art>,
    ocl: Option<&'a ocl_timer::Data>,
) -> impl Iterator<Item = &'a str> {
    shortcuts
        .into_iter()
        .flat_map(power_shortcuts::Data::images)
        .chain(
            bombs
                .into_iter()
                .flat_map(crate::sticky_bomb_icons::Art::images),
        )
        .chain(
            ocl.into_iter()
                .flat_map(|timer| [timer.sell.image.as_str(), timer.rally.image.as_str()]),
        )
}

fn ui_labels(
    assets: &VirtualAssets,
    map_path: Option<&str>,
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut labels = strings(&assets.read("data/english/generals.csf")?.bytes)?;
    if let Some(path) = map_path {
        crate::world_data::map_labels(assets, path, &mut labels)?;
    }
    if !labels.contains_key("gui:addcash") {
        return Err("missing original floating income label".into());
    }
    Ok(labels)
}

fn load_promotions(
    assets: &VirtualAssets,
    labels: &BTreeMap<String, String>,
    options: &Path,
    windows: &BTreeMap<String, Rect>,
) -> Result<Promotions, Box<dyn Error>> {
    let promotions = promotions::load(assets, labels, options)?;
    for button in promotions.buttons.iter().flatten() {
        if !windows.contains_key(&button.window) {
            return Err(
                format!("promotion button has no original window {}", button.window).into(),
            );
        }
    }
    Ok(promotions)
}

fn load_schemes(assets: &VirtualAssets) -> Result<Vec<Scheme>, Box<dyn Error>> {
    let scheme_file = content::scan(assets.read("data/ini/controlbarscheme.ini")?)?;
    let mut schemes = Vec::new();
    for name in ["America8x6", "China8x6", "GLA8x6", "Observer8x6"] {
        schemes.push(
            scheme(find(&scheme_file, "ControlBarScheme", name)?)
                .map_err(|error| format!("ControlBarScheme {name}: {error}"))?,
        );
    }
    Ok(schemes)
}

fn game_speed(assets: &VirtualAssets) -> Result<u8, Box<dyn Error>> {
    let global = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    Ok(value(
        &find(&global, "GameData", "")?.lines,
        0,
        "FramesPerSecondLimit",
    )?
    .parse::<u8>()?
    .clamp(15, 61))
}

fn required_upgrade(
    button: &Declaration,
    upgrades: &ContentFile,
) -> Result<Option<RequiredUpgrade>, Box<dyn Error>> {
    if !command_option(button, "NEED_UPGRADE") {
        return Ok(None);
    }
    let name = value(&button.lines, 0, "Upgrade")?;
    let upgrade = find(upgrades, "Upgrade", name)?;
    // UpgradeTemplate defaults omitted Type to the player scope.
    Ok(Some(
        match optional_value(&upgrade.lines, "Type")?.unwrap_or("PLAYER") {
            "PLAYER" => RequiredUpgrade::Player(name.into()),
            "OBJECT" => RequiredUpgrade::Object(name.into()),
            scope => return Err(format!("unsupported required upgrade scope {scope}").into()),
        },
    ))
}

fn command_option(button: &Declaration, option: &str) -> bool {
    button.lines.iter().any(|line| {
        line.depth == Some(0)
            && line.tokens.first().is_some_and(|key| key == "Options")
            && line.tokens.iter().skip(1).any(|flag| flag == option)
    })
}

fn location_weapon_action(
    button: &Declaration,
    unit: &UnitType,
) -> Result<CommandAction, Box<dyn Error>> {
    let slot = match value(&button.lines, 0, "WeaponSlot")? {
        "PRIMARY" => 0,
        "SECONDARY" => 1,
        "TERTIARY" => 2,
        _ => return Err("unsupported location weapon slot".into()),
    };
    if command_option(button, "NEED_TARGET_POS")
        && command_option(button, "USES_MINE_CLEARING_WEAPONSET")
    {
        if !unit.weapon_set_rules.templates.iter().any(|weapon| {
            weapon.slot == slot
                && weapon.flags & 16 != 0
                && matches!(
                    weapon.rules.damage_type,
                    generals_sim::skirmish::DamageType::Disarm
                )
        }) {
            return Err("mine-clearing command lacks its original DISARM weapon set".into());
        }
        return Ok(CommandAction::ClearMines(slot));
    }
    if !command_option(button, "NEED_TARGET_POS") || unit.location_weapons & (1 << slot) == 0 {
        return Err("untranslated location weapon command".into());
    }
    Ok(CommandAction::FireWeapon(slot))
}

fn pilot_weapon_action(
    button: &Declaration,
    unit: &UnitType,
) -> Result<CommandAction, Box<dyn Error>> {
    let slot = match value(&button.lines, 0, "WeaponSlot")? {
        "PRIMARY" => 0,
        "SECONDARY" => 1,
        "TERTIARY" => 2,
        _ => return Err("unsupported pilot weapon slot".into()),
    };
    if !command_option(button, "NEED_TARGET_ENEMY_OBJECT")
        || !(unit.weapon_in_slot(slot).is_some_and(|weapon| {
            matches!(
                weapon.damage_type,
                generals_sim::skirmish::DamageType::KillPilot
            )
        }) || unit.combat_bike.is_some()
            && unit.weapon_set_rules.templates.iter().any(|weapon| {
                weapon.slot == slot
                    && matches!(
                        weapon.rules.damage_type,
                        generals_sim::skirmish::DamageType::KillPilot
                    )
            }))
    {
        return Err("untranslated pilot weapon command".into());
    }
    Ok(CommandAction::SnipeVehicle(slot))
}

fn command_action(
    command: &str,
    button: &Declaration,
    unit: &UnitType,
    catalog: &[UnitType],
) -> Result<Option<CommandAction>, Box<dyn Error>> {
    let behavior = &unit.behavior;
    let action = if matches!(command, "DOZER_CONSTRUCT" | "UNIT_BUILD") {
        let object_name = value(&button.lines, 0, "Object")?;
        let Some(index) = catalog.iter().position(|unit| unit.name == object_name) else {
            return Ok(None);
        };
        CommandAction::Produce(index)
    } else {
        match command {
            "STOP" => CommandAction::Stop,
            "COMBATDROP" if unit.chinook.is_some() => {
                if !command_option(button, "NEED_TARGET_POS")
                    || !command_option(button, "NEED_TARGET_ENEMY_OBJECT")
                    || !command_option(button, "NEED_TARGET_ALLY_OBJECT")
                    || !command_option(button, "NEED_TARGET_NEUTRAL_OBJECT")
                {
                    return Err("unsupported Combat Drop target options".into());
                }
                CommandAction::CombatDrop
            }
            "EVACUATE" if unit.contain.is_some() || !behavior.attachments.is_empty() => {
                CommandAction::Evacuate
            }
            "EXIT_CONTAINER" if unit.contain.is_some() || !behavior.attachments.is_empty() => {
                CommandAction::ExitContainer(0)
            }
            "SWITCH_WEAPON" => {
                // `INI::parseLookupList` reads the first authored token. The
                // base Neutron Warhead record terminates with an unused TERTIARY.
                let slot = field(&button.lines, 0, "WeaponSlot")?
                    .first()
                    .ok_or("missing source switch weapon slot")?;
                CommandAction::SwitchWeapon(match slot.as_str() {
                    "PRIMARY" => 0,
                    "SECONDARY" => 1,
                    "TERTIARY" => 2,
                    name => return Err(format!("unsupported command weapon slot {name}").into()),
                })
            }
            "FIRE_WEAPON" => return fire_weapon_action(button, unit),
            "HACK_INTERNET" if behavior.hacking.is_some() => CommandAction::HackInternet,
            "HIJACK_VEHICLE" if unit.hijacker.is_some() => CommandAction::Hijack,
            "CONVERT_TO_CARBOMB" if behavior.car_bomb_converter.is_some() => CommandAction::CarBomb,
            "SABOTAGE_BUILDING" if !behavior.sabotage.is_empty() => CommandAction::Sabotage,
            "SELL" => CommandAction::Sell,
            "PLAYER_UPGRADE" | "OBJECT_UPGRADE" => {
                let name = value(&button.lines, 0, "Upgrade")?;
                let Some(index) = unit
                    .maintenance
                    .research
                    .iter()
                    .position(|upgrade| upgrade.name == name)
                else {
                    return Ok(None);
                };
                CommandAction::Research(index)
            }
            "ATTACK_MOVE" => CommandAction::AttackMove,
            "GUARD" => CommandAction::Guard,
            "GUARD_FLYING_UNITS_ONLY" => CommandAction::GuardFlying,
            "SET_RALLY_POINT"
                if behavior.production_exit.is_some()
                    || unit
                        .contain
                        .as_ref()
                        .is_some_and(|rules| rules.tunnel.is_some()) =>
            {
                CommandAction::Rally
            }
            "SPECIAL_POWER" | "SPECIAL_POWER_CONSTRUCT" => {
                return special_power_action(button, unit, catalog);
            }
            _ => return Ok(None),
        }
    };
    Ok(Some(action))
}

fn fire_weapon_action(
    button: &Declaration,
    unit: &UnitType,
) -> Result<Option<CommandAction>, Box<dyn Error>> {
    if unit.maintenance.replacement.is_some()
        && button.name == "Command_DetonateFakeBuilding"
        && value(&button.lines, 0, "WeaponSlot")? == "PRIMARY"
    {
        return Ok(Some(CommandAction::DetonateFake));
    }
    if unit.location_weapons != 0 || command_option(button, "USES_MINE_CLEARING_WEAPONSET") {
        return location_weapon_action(button, unit).map(Some);
    }
    let action = match button.name.as_str() {
        "Command_ColonelBurtonKnifeAttack" => {
            if value(&button.lines, 0, "WeaponSlot")? != "SECONDARY"
                || !command_option(button, "NEED_TARGET_ENEMY_OBJECT")
                || !command_option(button, "NEED_TARGET_NEUTRAL_OBJECT")
                || !unit.weapon_in_slot(1).is_some_and(|weapon| {
                    matches!(
                        weapon.damage_type,
                        generals_sim::skirmish::DamageType::Melee
                    )
                })
            {
                return Err("Burton knife command requires its original object weapon".into());
            }
            CommandAction::BurtonKnife
        }
        "Command_BombTruckDetonateNow" => {
            if value(&button.lines, 0, "WeaponSlot")? != "PRIMARY"
                || unit.weapon.name != "SuicideWeapon"
            {
                return Err("Bomb Truck detonation requires its original primary weapon".into());
            }
            CommandAction::BombTruckDetonate
        }
        _ if unit.name == "GLAInfantryJarmenKell"
            || unit.combat_bike.is_some()
                && button.name == "Command_GLAInfantryJarmenKellSnipeVehicleAttack" =>
        {
            pilot_weapon_action(button, unit)?
        }
        _ => return Ok(None),
    };
    Ok(Some(action))
}

fn special_power_action(
    button: &Declaration,
    unit: &UnitType,
    catalog: &[UnitType],
) -> Result<Option<CommandAction>, Box<dyn Error>> {
    use generals_sim::skirmish::special_ability::BurtonCharge;
    let behavior = &unit.behavior;
    if let Some(action) = native_power_action(button, catalog)?
        && action.power().is_some_and(|name| {
            unit.special_powers
                .iter()
                .any(|power| power.name == name && !power.script_only)
        })
    {
        return Ok(Some(action));
    }
    let action = match promotions::special_power(&button.lines)? {
        Some(
            "SpecialAbilityRangerCaptureBuilding"
            | "SpecialAbilityRedGuardCaptureBuilding"
            | "SpecialAbilityRebelCaptureBuilding"
            | "SpecialAbilityBlackLotusCaptureBuilding",
        ) => CommandAction::Capture,
        Some("SpecialAbilityColonelBurtonTimedCharges") => {
            CommandAction::BurtonCharge(BurtonCharge::Timed)
        }
        Some("SpecialAbilityDisguiseAsVehicle") => {
            if !["NEED_TARGET_ENEMY_OBJECT", "NEED_TARGET_NEUTRAL_OBJECT", "NEED_TARGET_ALLY_OBJECT"]
                .iter().all(|option| command_option(button, option))
            {
                return Err("Bomb Truck disguise requires its original object target options".into());
            }
            CommandAction::BombTruckDisguise
        }
        Some("SpecialAbilityColonelBurtonRemoteCharges") => {
            if command_option(button, "CONTEXTMODE_COMMAND") {
                CommandAction::BurtonCharge(BurtonCharge::Remote)
            } else {
                CommandAction::BurtonDetonate
            }
        }
        Some("SpecialAbilityBlackLotusDisableVehicleHack") => CommandAction::Hack(1),
        Some("SpecialAbilityBlackLotusStealCashHack") => CommandAction::Hack(2),
        Some("SpecialAbilityMissileDefenderLaserGuidedMissiles") => CommandAction::LaserLock,
        Some("SpecialAbilityHackerDisableBuilding") if behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialHackerDisableBuilding)) => {
            CommandAction::DisableBuilding
        }
        Some("SpecialAbilityHelixNapalmBomb")
            if unit
                .special_powers
                .iter()
                .any(|power| power.power_type == "SPECIAL_HELIX_NAPALM_BOMB") =>
        {
            CommandAction::HelixBomb
        }
        Some("SpecialAbilityAmbulanceCleanupArea") if !behavior.cleanup.is_empty() => {
            CommandAction::Cleanup
        }
        Some("SpecialAbilityChangeBattlePlans") if unit.battle_plan.is_some() => {
            CommandAction::BattlePlan(battle_plan_option(button)?)
        }
        Some("SuperweaponCashHack") => CommandAction::CashHack,
        Some("SuperweaponEmergencyRepair") => CommandAction::EmergencyRepair,
        Some("SuperweaponSpectreGunship")
            if unit
                .special_powers
                .iter()
                .any(|power| power.name == "SuperweaponSpectreGunship") =>
        {
            CommandAction::SpectreGunship
        }
        Some("AirF_SuperweaponSpectreGunship")
            if unit
                .special_powers
                .iter()
                .any(|power| power.name == "AirF_SuperweaponSpectreGunship") =>
        {
            CommandAction::AirForceSpectreGunship
        }
        _ => return Ok(None),
    };
    Ok(Some(action))
}

fn native_power_action(
    button: &Declaration,
    catalog: &[UnitType],
) -> Result<Option<CommandAction>, Box<dyn Error>> {
    let Some(name) = optional_value(&button.lines, "SpecialPower")?.and_then(supported_power)
    else {
        return Ok(None);
    };
    let aim = match value(&button.lines, 0, "Command")? {
        "SPECIAL_POWER_CONSTRUCT" | "SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT" => {
            let reference = value(&button.lines, 0, "Object")?;
            let Some(kind) = catalog.iter().position(|unit| unit.name == reference) else {
                return Ok(None);
            };
            PowerAim::Construct(kind)
        }
        _ if command_option(button, "NEED_TARGET_POS") => PowerAim::Location,
        _ if [
            "NEED_TARGET_ENEMY_OBJECT",
            "NEED_TARGET_ALLY_OBJECT",
            "NEED_TARGET_NEUTRAL_OBJECT",
        ]
        .into_iter()
        .any(|option| command_option(button, option)) =>
        {
            PowerAim::Object
        }
        _ => PowerAim::Immediate,
    };
    Ok(Some(CommandAction::NativePower(PowerCommand { name, aim })))
}

fn battle_plan_option(
    button: &Declaration,
) -> Result<generals_sim::skirmish::battle_plan::Plan, Box<dyn Error>> {
    use generals_sim::skirmish::battle_plan::Plan;
    let choices = [
        ("OPTION_ONE", Plan::Bombardment),
        ("OPTION_TWO", Plan::HoldTheLine),
        ("OPTION_THREE", Plan::SearchAndDestroy),
    ];
    let mut choices = choices
        .into_iter()
        .filter(|(name, _)| command_option(button, name));
    let plan = choices
        .next()
        .ok_or("battle-plan command lacks source option")?
        .1;
    if choices.next().is_some() {
        return Err("battle-plan command has multiple options".into());
    }
    Ok(plan)
}

fn object_ui(
    assets: &VirtualAssets,
    catalog: &[UnitType],
    labels: &BTreeMap<String, String>,
) -> Result<Vec<UnitUi>, Box<dyn Error>> {
    let buttons = content::scan(assets.read("data/ini/commandbutton.ini")?)?;
    let sets = content::scan(assets.read("data/ini/commandset.ini")?)?;
    let upgrades = content::scan(assets.read("data/ini/upgrade.ini")?)?;
    let objects = object_files(assets)?;
    let mut units = Vec::new();
    for unit in catalog {
        let object = objects
            .iter()
            .flat_map(|file| &file.declarations)
            .find(|object| object.kind == "Object" && object.name == unit.name)
            .ok_or_else(|| format!("missing UI object {}", unit.name))?;
        let [portrait, button_image] = object_art(&object.lines)?;
        // ThingTemplate's UnicodeString starts empty when DisplayName is absent.
        let name = optional_value(&object.lines, "DisplayName")?
            .map(|label| translate(labels, label))
            .transpose()?
            .unwrap_or_default();
        let set = object
            .lines
            .iter()
            .find(|line| {
                line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "CommandSet")
            })
            .map(|line| {
                find(
                    &sets,
                    "CommandSet",
                    line.tokens.get(1).ok_or("empty CommandSet")?,
                )
            })
            .transpose()?;
        let structure_inventory = set.is_none()
            && unit
                .contain
                .as_ref()
                .is_some_and(|rules| rules.garrison.is_some());
        let mut context = Vec::new();
        let specifications = garrison::command_slots(set, structure_inventory)?;
        for (slot, name) in specifications {
            let button = find(&buttons, "CommandButton", name)?;
            if let Some(command) = read_command(slot, button, unit, catalog, labels, &upgrades)? {
                context.push(command);
            }
        }
        context.sort_by_key(|command| command.slot);
        let mut inventory_slot = 0;
        for command in &mut context {
            if matches!(command.action, CommandAction::ExitContainer(_)) {
                command.action = CommandAction::ExitContainer(inventory_slot);
                inventory_slot += 1;
            }
        }
        let mut upgraded_commands = Vec::new();
        for upgrade in &unit.behavior.attachments {
            let set = find(&sets, "CommandSet", &upgrade.command_set)?;
            let mut commands = Vec::new();
            let mut inventory = 0;
            for (slot, name) in garrison::command_slots(Some(set), false)? {
                let button = find(&buttons, "CommandButton", name)?;
                if let Some(mut command) =
                    read_command(slot, button, unit, catalog, labels, &upgrades)?
                {
                    if matches!(command.action, CommandAction::ExitContainer(_)) {
                        command.action = CommandAction::ExitContainer(inventory);
                        inventory += 1;
                    }
                    commands.push(command);
                }
            }
            upgraded_commands.push((upgrade.trigger.clone(), commands));
        }
        let named_commands = named_commands(unit, catalog, labels, &sets, &buttons, &upgrades)?;
        units.push(UnitUi {
            named_commands,
            upgraded_commands,
            structure_inventory,
            commands: context,
            portrait,
            button_image,
            name,
        });
    }
    Ok(units)
}

type CommandSets = Vec<(String, Vec<Command>)>;
fn named_commands(
    unit: &UnitType,
    catalog: &[UnitType],
    labels: &BTreeMap<String, String>,
    sets: &ContentFile,
    buttons: &ContentFile,
    upgrades: &ContentFile,
) -> Result<CommandSets, Box<dyn Error>> {
    let mut named_commands = Vec::new();
    let upgrades_sets = unit.maintenance.command_sets.iter().flat_map(|upgrade| {
        std::iter::once(&upgrade.command_set)
            .chain(upgrade.alternate.as_ref().map(|(_, name)| name))
    });
    let rider_sets = unit
        .combat_bike
        .iter()
        .flat_map(|bike| bike.riders.iter().map(|rider| &rider.command_set));
    for name in upgrades_sets.chain(rider_sets) {
        if named_commands.iter().any(|(loaded, _)| loaded == name) {
            continue;
        }
        let set = find(sets, "CommandSet", name)?;
        let mut commands = Vec::new();
        for (slot, name) in garrison::command_slots(Some(set), false)? {
            let button = find(buttons, "CommandButton", name)?;
            if let Some(command) = read_command(slot, button, unit, catalog, labels, upgrades)? {
                commands.push(command);
            }
        }
        named_commands.push((name.clone(), commands));
    }
    Ok(named_commands)
}

fn read_command(
    slot: usize,
    button: &Declaration,
    unit: &UnitType,
    catalog: &[UnitType],
    labels: &BTreeMap<String, String>,
    upgrades: &ContentFile,
) -> Result<Option<Command>, Box<dyn Error>> {
    let command = value(&button.lines, 0, "Command")?;
    let Some(action) = command_action(command, button, unit, catalog)? else {
        return Ok(None);
    };
    command_details(slot, button, action, labels, upgrades).map(Some)
}

fn command_details(
    slot: usize,
    button: &Declaration,
    action: CommandAction,
    labels: &BTreeMap<String, String>,
    upgrades: &ContentFile,
) -> Result<Command, Box<dyn Error>> {
    let label_key = value(&button.lines, 0, "TextLabel")?;
    let raw_label = labels
        .get(&label_key.to_ascii_lowercase())
        .ok_or_else(|| format!("missing command label {label_key}"))?;
    Ok(Command {
        required_upgrade: required_upgrade(button, upgrades)?,
        sciences: button
            .lines
            .iter()
            .find(|line| {
                line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "Science")
            })
            .map_or_else(Vec::new, |line| line.tokens[1..].to_vec()),
        name: button.name.clone(),
        slot,
        action,
        accelerator: raw_label
            .split_once('&')
            .and_then(|(_, suffix)| suffix.chars().next()),
        multi_select: command_option(button, "OK_FOR_MULTI_SELECT"),
        image: value(&button.lines, 0, "ButtonImage")?.to_owned(),
        label: translate(labels, label_key)?,
        description: translate(labels, value(&button.lines, 0, "DescriptLabel")?)?,
    })
}

fn object_files(assets: &VirtualAssets) -> Result<Vec<content::ContentFile>, Box<dyn Error>> {
    let mut files = [
        "factionbuilding",
        "civilianbuilding",
        "civilianprop",
        "natureprop",
        "techbuildings",
        "civilianunit",
        "americaair",
        "americamiscunit",
        "chinaair",
        "chinamiscunit",
        "glaair",
        "glamiscunit",
        "system",
        "specialpowerobjects",
        "gc_chem_glasystem",
        "hulk",
        "weaponobjects",
        "americainfantry",
        "americavehicle",
        "chinainfantry",
        "chinavehicle",
        "glainfantry",
        "glavehicle",
    ]
    .into_iter()
    .map(|file| {
        Ok(content::scan(
            assets.read(&format!("data/ini/object/{file}.ini"))?,
        )?)
    })
    .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    files.insert(
        0,
        content::scan(assets.read("data/ini/default/object.ini")?)?,
    );
    files.push(content::scan(assets.read("data/ini/crate.ini")?)?);
    crate::skirmish_data::resolve_reskins(&mut files)?;
    Ok(files)
}

fn mapped_images(assets: &VirtualAssets) -> Result<BTreeMap<String, Picture>, Box<dyn Error>> {
    // ImageCollection::load loads TextureSize_512 first, then HandCreated overrides.
    let mut pictures = BTreeMap::new();
    for directory in [
        "data/ini/mappedimages/texturesize_512",
        "data/ini/mappedimages/handcreated",
    ] {
        for path in assets.list(directory)? {
            if !std::path::Path::new(&path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
            {
                continue;
            }
            for declaration in content::scan(assets.read(&path)?)?.declarations {
                if declaration.kind != "MappedImage" {
                    continue;
                }
                pictures.insert(
                    declaration.name.to_ascii_lowercase(),
                    picture(&declaration).map_err(|error| {
                        format!("{path}: MappedImage {}: {error}", declaration.name)
                    })?,
                );
            }
        }
    }
    Ok(pictures)
}

fn textures<'a>(
    assets: &VirtualAssets,
    schemes: &[Scheme],
    units: &[UnitUi],
    promotions: &Promotions,
    window_images: impl Iterator<Item = &'a str>,
    pictures: &BTreeMap<String, Picture>,
    cancel_construction: &Command,
) -> Result<BTreeMap<String, Image>, Box<dyn Error>> {
    let mut needed = BTreeSet::new();
    needed.insert(cancel_construction.image.clone());
    needed.extend(window_images.map(str::to_owned));
    for scheme in schemes {
        needed.extend(scheme.command_marker.iter().cloned());
        needed.extend([scheme.background.clone(), scheme.logo.clone()]);
        needed.extend(scheme.options.iter().cloned());
        needed.extend(scheme.worker.iter().cloned());
        needed.extend(scheme.general.iter().cloned());
        needed.insert(scheme.science_background.clone());
    }
    for unit in units {
        needed.insert(unit.portrait.clone());
        needed.insert(unit.button_image.clone());
        needed.extend(
            unit.commands
                .iter()
                .chain(
                    unit.upgraded_commands
                        .iter()
                        .flat_map(|(_, commands)| commands),
                )
                .chain(
                    unit.named_commands
                        .iter()
                        .flat_map(|(_, commands)| commands),
                )
                .map(|command| command.image.clone()),
        );
    }
    needed.remove("");
    for button in promotions.buttons.iter().flatten() {
        needed.insert(button.image.clone());
    }
    needed.extend(menu_images().map(str::to_owned));
    for state in ["", "-HiLite", "-Pushed", "-Disabled"] {
        for part in ["Left", "Middle", "Right"] {
            needed.insert(format!("Buttons{state}-{part}"));
        }
    }
    let mut textures = BTreeMap::new();
    for name in needed {
        let picture = pictures
            .get(&name.to_ascii_lowercase())
            .ok_or_else(|| format!("missing mapped image {name}"))?;
        if !matches!(picture.status.as_str(), "NONE" | "ROTATED_90_CLOCKWISE") {
            return Err(format!("unsupported image status {} for {name}", picture.status).into());
        }
        if !textures.contains_key(&picture.texture) {
            textures.insert(
                picture.texture.clone(),
                source_texture::load(assets, &picture.texture)?,
            );
        }
    }
    // positionStartSpots permits an absent original UnknownMap image.
    if let Some(picture) = pictures.get("unknownmap")
        && !textures.contains_key(&picture.texture)
        && let Ok(image) = source_texture::load(assets, &picture.texture)
    {
        textures.insert(picture.texture.clone(), image);
    }
    Ok(textures)
}

fn power_rules(assets: &VirtualAssets) -> Result<(f32, f32, i16), Box<dyn Error>> {
    let global = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let lines = &find(&global, "GameData", "")?.lines;
    // GlobalData constructor defaults, overridden only when installed GameData specifies them.
    let base = optional_value(lines, "PowerBarBase")?.map_or(Ok(7.0), str::parse::<f32>)?;
    let intervals =
        optional_value(lines, "PowerBarIntervals")?.map_or(Ok(3.0), str::parse::<f32>)?;
    let yellow = optional_value(lines, "PowerBarYellowRange")?.map_or(Ok(5), str::parse::<i16>)?;
    if !base.is_finite() || base <= 1.0 || !intervals.is_finite() || intervals <= 0.0 {
        return Err("invalid source power display scale".into());
    }
    Ok((base, intervals, yellow))
}

fn object_art(lines: &[ContentLine]) -> Result<[String; 2], Box<dyn Error>> {
    // ThingTemplate's INI::parseAsciiString assignments take the latest line.
    let read = |key| {
        crate::skirmish_data::field(lines, 0, key)
            .map(crate::skirmish_data::scalar)
            .transpose()
            .map(|value| value.unwrap_or_default().to_owned())
    };
    Ok([read("SelectPortrait")?, read("ButtonImage")?])
}

fn optional_value<'a>(
    lines: &'a [ContentLine],
    key: &str,
) -> Result<Option<&'a str>, Box<dyn Error>> {
    if lines
        .iter()
        .any(|line| line.depth == Some(0) && line.tokens.first().is_some_and(|token| token == key))
    {
        Ok(Some(value(lines, 0, key)?))
    } else {
        Ok(None)
    }
}

fn find<'a>(
    file: &'a ContentFile,
    kind: &str,
    name: &str,
) -> Result<&'a Declaration, Box<dyn Error>> {
    let mut matches = file
        .declarations
        .iter()
        .filter(|item| item.kind == kind && item.name == name);
    let item = matches
        .next()
        .ok_or_else(|| format!("missing {kind} {name}"))?;
    if matches.next().is_some() {
        return Err(format!("ambiguous {kind} {name}").into());
    }
    Ok(item)
}

fn field<'a>(
    lines: &'a [ContentLine],
    depth: u32,
    key: &str,
) -> Result<&'a [String], Box<dyn Error>> {
    let mut matches = lines.iter().filter(|line| {
        line.depth == Some(depth) && line.tokens.first().is_some_and(|name| name == key)
    });
    let line = matches
        .next()
        .ok_or_else(|| format!("missing UI field {key}"))?;
    if matches.next().is_some() {
        return Err(format!("ambiguous UI field {key}").into());
    }
    Ok(&line.tokens[1..])
}

fn value<'a>(lines: &'a [ContentLine], depth: u32, key: &str) -> Result<&'a str, Box<dyn Error>> {
    let [value] = field(lines, depth, key)? else {
        return Err(format!("invalid UI scalar {key}").into());
    };
    Ok(value)
}

fn coord(tokens: &[String], key: &str) -> Result<f32, Box<dyn Error>> {
    let prefix = format!("{key}:");
    // INI::getNextSubToken permits whitespace after the colon.
    let (index, number) = tokens
        .iter()
        .enumerate()
        .find_map(|(index, token)| token.strip_prefix(&prefix).map(|value| (index, value)))
        .ok_or_else(|| format!("missing UI coordinate {key}"))?;
    let number = if number.is_empty() {
        tokens
            .get(index + 1)
            .map(String::as_str)
            .ok_or_else(|| format!("missing UI coordinate value {key}"))?
    } else {
        number
    };
    Ok(f32::from(number.parse::<u16>().map_err(|error| {
        format!("invalid UI coordinate {key}:{number}: {error}")
    })?))
}

fn point(tokens: &[String]) -> Result<Vec2, Box<dyn Error>> {
    Ok(Vec2::new(coord(tokens, "X")?, coord(tokens, "Y")?))
}

fn area(lines: &[ContentLine], name: &str) -> Result<Rect, Box<dyn Error>> {
    Ok(Rect::from_corners(
        point(field(lines, 0, &format!("{name}UL"))?)?,
        point(field(lines, 0, &format!("{name}LR"))?)?,
    ))
}

fn states(lines: &[ContentLine], name: &str) -> Result<[String; 3], Box<dyn Error>> {
    Ok([
        value(lines, 0, &format!("{name}Enable"))?.into(),
        value(lines, 0, &format!("{name}Hightlited"))?.into(),
        value(lines, 0, &format!("{name}Pushed"))?.into(),
    ])
}

fn scheme(declaration: &Declaration) -> Result<Scheme, Box<dyn Error>> {
    let lines = &declaration.lines;
    if point(field(lines, 0, "ScreenCreationRes")?)? != Vec2::new(800.0, 600.0) {
        return Err("unsupported control bar source resolution".into());
    }
    let position = point(field(lines, 1, "Position")?)?;
    let size = point(field(lines, 1, "Size")?)?;
    Ok(Scheme {
        command_marker: command_marker(lines)?,
        background: value(lines, 1, "ImageName")?.into(),
        area: Rect::from_corners(position, position + size),
        money: area(lines, "Money")?,
        power: area(lines, "PowerBar")?,
        options_area: area(lines, "Options")?,
        options: states(lines, "OptionsButton")?,
        worker_area: area(lines, "Worker")?,
        worker: {
            let [enabled, highlighted, pushed] = states(lines, "IdleWorkerButton")?;
            [
                enabled,
                highlighted,
                pushed,
                value(lines, 0, "IdleWorkerButtonDisabled")?.into(),
            ]
        },
        logo: value(lines, 0, "RightHUDImage")?.into(),
        general_area: area(lines, "General")?,
        general: {
            let [enabled, highlighted, pushed] = states(lines, "GeneralButton")?;
            [
                enabled,
                highlighted,
                pushed,
                value(lines, 0, "GeneralButtonDisabled")?.into(),
            ]
        },
        science_background: value(lines, 0, "PowerPurchaseImage")?.into(),
        clock_color: {
            let color = field(lines, 0, "BuildUpClockColor")?;
            Vec4::new(
                coord(color, "R")?,
                coord(color, "G")?,
                coord(color, "B")?,
                coord(color, "A")?,
            ) / 255.0
        },
    })
}

fn command_marker(lines: &[ContentLine]) -> Result<Option<String>, Box<dyn Error>> {
    let Some(line) = lines.iter().find(|line| {
        line.depth == Some(0)
            && line
                .tokens
                .first()
                .is_some_and(|key| key == "CommandMarkerImage")
    }) else {
        // ControlBarScheme starts with a null image; updateSlotExitImage then
        // leaves the command's own image intact.
        return Ok(None);
    };
    let [_, name] = line.tokens.as_slice() else {
        return Err("invalid original CommandMarkerImage".into());
    };
    Ok((!name.eq_ignore_ascii_case("None")).then(|| name.clone()))
}

fn picture(declaration: &Declaration) -> Result<Picture, Box<dyn Error>> {
    let lines = &declaration.lines;
    let coords = field(lines, 0, "Coords")?;
    let rect = Rect::from_corners(
        Vec2::new(coord(coords, "Left")?, coord(coords, "Top")?),
        Vec2::new(coord(coords, "Right")?, coord(coords, "Bottom")?),
    );
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Err("empty mapped image".into());
    }
    Ok(Picture {
        texture: value(lines, 0, "Texture")?.to_ascii_lowercase(),
        rect,
        status: value(lines, 0, "Status")?.into(),
    })
}

fn window_rects(bytes: &[u8]) -> Result<BTreeMap<String, Rect>, Box<dyn Error>> {
    let text = std::str::from_utf8(bytes)?;
    let mut windows = BTreeMap::new();
    for block in text.split("WINDOWTYPE = ").skip(1) {
        let name = block
            .split_once("NAME = \"")
            .and_then(|(_, value)| value.split_once('"'))
            .map(|(name, _)| name)
            .ok_or("WND window has no name")?;
        windows.insert(name.to_owned(), window_area(block)?);
    }
    Ok(windows)
}

fn window_area(block: &str) -> Result<Rect, Box<dyn Error>> {
    let coords = block
        .split_once("SCREENRECT = ")
        .and_then(|(_, value)| value.split_once(';'))
        .map(|(coords, _)| coords)
        .ok_or("WND window has no rectangle")?;
    let numbers: Vec<u16> = coords
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let [x1, y1, x2, y2, 800, 600] = numbers.as_slice() else {
        return Err("unsupported WND creation resolution".into());
    };
    Ok(Rect::from_corners(
        Vec2::new(f32::from(*x1), f32::from(*y1)),
        Vec2::new(f32::from(*x2), f32::from(*y2)),
    ))
}

fn translate(strings: &BTreeMap<String, String>, key: &str) -> Result<String, Box<dyn Error>> {
    strings
        .get(&key.to_ascii_lowercase())
        .map(|value| value.replace('&', ""))
        .ok_or_else(|| format!("missing original UI text {key}").into())
}

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], Box<dyn Error>> {
    let (head, rest) = bytes.split_at_checked(count).ok_or("truncated CSF")?;
    *bytes = rest;
    Ok(head)
}

fn integer(bytes: &mut &[u8]) -> Result<usize, Box<dyn Error>> {
    Ok(usize::try_from(u32::from_le_bytes(
        take(bytes, 4)?.try_into()?,
    ))?)
}

pub fn strings(mut bytes: &[u8]) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    // GameTextManager::parseCSF: first string per label, inverted UTF-16LE.
    if take(&mut bytes, 4)? != b" FSC" || integer(&mut bytes)? != 3 {
        return Err("unsupported CSF header".into());
    }
    let count = integer(&mut bytes)?;
    if count > 100_000 {
        return Err("excessive CSF labels".into());
    }
    take(&mut bytes, 12)?;
    let mut result = BTreeMap::new();
    for _ in 0..count {
        if take(&mut bytes, 4)? != b" LBL" {
            return Err("invalid CSF label".into());
        }
        let variants = integer(&mut bytes)?;
        if variants > 64 {
            return Err("excessive CSF variants".into());
        }
        let length = integer(&mut bytes)?;
        let label = std::str::from_utf8(take(&mut bytes, length)?)?.to_ascii_lowercase();
        for variant in 0..variants {
            let kind = take(&mut bytes, 4)?;
            if kind != b" RTS" && kind != b"WRTS" {
                return Err("invalid CSF string".into());
            }
            let length = integer(&mut bytes)?
                .checked_mul(2)
                .ok_or("CSF length overflow")?;
            let encoded = take(&mut bytes, length)?;
            if variant == 0 {
                let units: Vec<_> = encoded
                    .chunks_exact(2)
                    .map(|pair| !u16::from_le_bytes([pair[0], pair[1]]))
                    .collect();
                result.insert(label.clone(), String::from_utf16(&units)?);
            }
            if kind == b"WRTS" {
                let length = integer(&mut bytes)?;
                take(&mut bytes, length)?;
            }
        }
    }
    Ok(result)
}

fn menu_images() -> impl Iterator<Item = &'static str> {
    [
        "Active-Unchecked",
        "Active-Checked",
        "Active-HiLighted",
        "Disabled-Unchecked",
        "Disabled-Checked",
        "hilightedbox",
        "dehilightedbox",
        "linebox",
        "arrow",
        "ExperienceBar",
        "Cameo_hilited",
        "Cameo_push",
        "SSChevron1L",
        "SSChevron2L",
        "SSChevron3L",
        "MainMenuBackdrop",
        "MainMenuRuler",
        "GeneralsLogo",
        "PowerPointG",
        "PowerPointY",
        "PowerPointR",
        "PowerBarSlider",
        "TextEntryEnabledLeftEnd",
        "TextEntryEnabledRightEnd",
        "TextEntryEnabledRepeatingCenter",
        "TextEntryEnabledSmallRepeatingCenter",
        "TextEntryHiliteLeftEnd",
        "TextEntryHiliteRightEnd",
        "TextEntryHiliteRepeatingCenter",
        "TextEntryHiliteSmallRepeatingCenter",
        "VSliderDownButtonEnabled",
        "VSliderDownButtonHilite",
        "VSliderDownButtonHiliteSelected",
        "VSliderDownButtonDisabled",
        "TextEntryDisabledLeftEnd",
        "TextEntryDisabledRightEnd",
        "TextEntryDisabledRepeatingCenter",
        "Cash",
        "TecBuilding",
        "ScrollBarThumbEnabled",
        "ScrollBarThumbDisabled",
        "ScrollBarThumbHilite",
        "ScrollBarThumbHiliteSelected",
        "BlackSquare",
        "PlayerStart",
        "PlayerStartDisabled",
        "VSliderUpButtonEnabled",
        "VSliderUpButtonHilite",
        "VSliderUpButtonHiliteSelected",
        "VSliderUpButtonDisabled",
        "RadioButtonEnabledLeft",
        "RadioButtonEnabledMiddle",
        "RadioButtonEnabledRight",
        "RadioButtonHilightedLeft",
        "RadioButtonHilightedMiddle",
        "RadioButtonHilightedRight",
        "PlayerStartHilite",
        "PlayerStartHilitePushed",
        "ListBoxHiliteItemLeftEnd",
        "ListBoxHiliteItemRightEnd",
        "ListBoxHiliteItemRepeatingCenter",
        "ListBoxHiliteSelectedItemLeftEnd",
        "ListBoxHiliteSelectedItemRightEnd",
        "ListBoxHiliteSelectedItemRepeatingCenter",
    ]
    .into_iter()
}

fn menu_windows(assets: &VirtualAssets) -> Result<BTreeMap<String, Rect>, Box<dyn Error>> {
    let mut windows = BTreeMap::new();
    for path in [
        "window/controlbar.wnd",
        "window/generalsexppoints.wnd",
        "window/controlbarpopupdescription.wnd",
        "window/menus/mainmenu.wnd",
        "window/menus/skirmishgameoptionsmenu.wnd",
        "window/menus/skirmishmapselectmenu.wnd",
        "window/menus/quitmenu.wnd",
        "window/menus/messagebox.wnd",
        "window/menus/quitmessagebox.wnd",
    ] {
        windows.extend(
            window_rects(&assets.read(path)?.bytes).map_err(|error| format!("{path}: {error}"))?,
        );
    }
    Ok(windows)
}

fn starting_money(file: &ContentFile) -> Result<(Vec<u32>, u32), Box<dyn Error>> {
    // MultiplayerSettings::addStartingMoneyChoice keeps declaration order.
    let mut choices = Vec::new();
    let mut default = 0;
    for choice in file
        .declarations
        .iter()
        .filter(|item| item.kind == "MultiplayerStartingMoneyChoice")
    {
        let amount = value(&choice.lines, 0, "Value")?.parse()?;
        choices.push(amount);
        if optional_value(&choice.lines, "Default")?
            .is_some_and(|value| value.eq_ignore_ascii_case("Yes"))
        {
            default = amount;
        }
    }
    Ok((choices, default))
}

fn color_labels(
    assets: &VirtualAssets,
    labels: &BTreeMap<String, String>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/multiplayer.ini")?)?;
    file.declarations
        .iter()
        .filter(|item| item.kind == "MultiplayerColor")
        .map(|item| translate(labels, value(&item.lines, 0, "TooltipName")?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{coord, strings};

    #[test]
    fn object_art_uses_the_last_authored_portrait_and_button_assignments() {
        let file = super::content::scan(generals_assets::virtual_assets::Asset {
            path: "FactionBuilding.ini".into(),
            origin: "Repeated original ThingTemplate art fields".into(),
            bytes: b"Object Launcher\nSelectPortrait=FirstPortrait\nButtonImage=FirstIcon\nSelectPortrait=FinalPortrait\nButtonImage=FinalIcon\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            super::object_art(&file.declarations[0].lines).unwrap(),
            ["FinalPortrait", "FinalIcon"]
        );
    }

    #[test]
    fn starting_money_preserves_authored_order_and_default() {
        let file = super::content::scan(generals_assets::virtual_assets::Asset {
            path: "multiplayer.ini".into(),
            origin: "source money choices".into(),
            bytes: b"MultiplayerStartingMoneyChoice\nValue=5000\nEnd\nMultiplayerStartingMoneyChoice\nValue=10000\nDefault=Yes\nEnd\nMultiplayerStartingMoneyChoice\nValue=20000\nEnd\n".to_vec(),
        }).unwrap();
        assert_eq!(
            super::starting_money(&file).unwrap(),
            (vec![5000, 10000, 20000], 10000)
        );
    }

    #[test]
    fn inventory_marker_reads_faction_scheme_and_retains_native_null_default() {
        for marker in ["SAEmptyFrame", "SNEmptyFrame", "SUEmptyFrame", "None", ""] {
            let field = if marker.is_empty() {
                String::new()
            } else {
                format!("CommandMarkerImage {marker}\n")
            };
            let file = super::content::scan(generals_assets::virtual_assets::Asset {
                path: "ControlBarScheme.ini".into(),
                origin: "Original inventory marker initialization".into(),
                bytes: format!("ControlBarScheme Fixture\n{field}End\n").into_bytes(),
            })
            .unwrap();
            assert_eq!(
                super::command_marker(&file.declarations[0].lines)
                    .unwrap()
                    .as_deref(),
                (!marker.is_empty() && marker != "None").then_some(marker)
            );
        }
    }

    #[test]
    fn command_upgrade_requirement_retains_player_versus_object_scope() {
        for (scope, type_line) in [
            ("PLAYER", "Type = PLAYER"),
            ("OBJECT", "Type = OBJECT"),
            ("PLAYER", ""),
        ] {
            let bytes = format!(
                "CommandButton Mode\n Options = CHECK_LIKE NEED_UPGRADE\n Upgrade = Research\nEnd\nUpgrade Research\n {type_line}\nEnd\n"
            ).into_bytes();
            let file = super::content::scan(generals_assets::virtual_assets::Asset {
                path: "CommandButton.ini".into(),
                origin: "Original NEED_UPGRADE scope fixture".into(),
                bytes,
            })
            .unwrap();
            let upgrade = super::required_upgrade(&file.declarations[0], &file)
                .unwrap()
                .unwrap();
            match upgrade {
                super::RequiredUpgrade::Player(name) => {
                    assert_eq!(scope, "PLAYER");
                    assert_eq!(name, "Research");
                }
                super::RequiredUpgrade::Object(name) => {
                    assert_eq!(scope, "OBJECT");
                    assert_eq!(name, "Research");
                }
            }
        }
    }

    #[test]
    fn coordinate_accepts_source_whitespace_and_rejects_missing_value() {
        let tokens = ["Left:85", "Top:", "424", "Right:109", "Bottom:", "446"].map(str::to_owned);
        assert!((coord(&tokens, "Top").expect("source coordinate") - 424.0).abs() < f32::EPSILON);
        assert!(coord(&["Top:".into(), "Right:109".into()], "Top").is_err());
    }

    #[test]
    fn csf_decodes_unicode_and_uses_first_variant() {
        let mut bytes = b" FSC".to_vec();
        for value in [3_u32, 1, 2, 0, 0] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(b" LBL");
        bytes.extend(2_u32.to_le_bytes());
        bytes.extend(1_u32.to_le_bytes());
        bytes.push(b'x');
        bytes.extend(b" RTS");
        bytes.extend(2_u32.to_le_bytes());
        bytes.extend([0xbe, 0xff, 0x56, 0xfc]);
        bytes.extend(b" RTS");
        bytes.extend(1_u32.to_le_bytes());
        bytes.extend([0xbd, 0xff]);
        assert_eq!(strings(&bytes).expect("valid CSF")["x"], "AΩ");
        bytes.pop();
        assert!(strings(&bytes).is_err());
    }

    #[test]
    fn csf_rejects_truncated_declared_payload_without_allocating_it() {
        let mut bytes = b" FSC".to_vec();
        for value in [3_u32, 1, 1, 0, 0] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(b" LBL");
        bytes.extend(1_u32.to_le_bytes());
        bytes.extend(1_u32.to_le_bytes());
        bytes.push(b'x');
        bytes.extend(b" RTS");
        bytes.extend(u32::MAX.to_le_bytes());
        assert!(strings(&bytes).is_err());
    }
    #[test]
    fn strategy_command_options_select_exactly_one_original_plan() {
        use generals_sim::skirmish::battle_plan::Plan;
        for (option, expected) in [
            ("OPTION_ONE", Some(Plan::Bombardment)),
            ("OPTION_TWO", Some(Plan::HoldTheLine)),
            ("OPTION_THREE", Some(Plan::SearchAndDestroy)),
            ("OPTION_ONE OPTION_TWO", None),
            ("", None),
        ] {
            let file = super::content::scan(generals_assets::virtual_assets::Asset {
                path: "CommandButton.ini".into(), origin: "source strategy command fixture".into(),
                bytes: format!("CommandButton Plan\nCommand=SPECIAL_POWER\nSpecialPower=SpecialAbilityChangeBattlePlans\nOptions=NEED_SPECIAL_POWER_SCIENCE CHECK_LIKE {option}\nEnd\n").into_bytes(),
            }).unwrap();
            assert_eq!(
                super::promotions::special_power(&file.declarations[0].lines).unwrap(),
                Some("SpecialAbilityChangeBattlePlans")
            );
            assert!(super::battle_plan_option(&file.declarations[0]).ok() == expected);
        }
    }
}
