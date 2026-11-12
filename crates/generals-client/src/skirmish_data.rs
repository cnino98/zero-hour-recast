#[cfg(test)]
mod admission_tests;
mod aircraft;
mod aircraft_death;
mod assisted_targeting;
mod auto_find_healing;
mod battle_bus;
mod behavior_data;
#[cfg(test)]
mod body_admission_tests;
mod bone_fx;
mod bridge_behavior;
mod create_object_die;
mod creation;
mod eject_pilot;
mod helicopter_death;
mod helix;
mod hijacker;
mod map_crates;
mod minefield;
pub mod module_metadata;
mod neutron_blast;
mod object_creation;
pub mod parachute;
mod parking;
pub mod physical_ocl;
mod pilot_find_vehicle;
pub mod source_files;
pub mod source_numbers;
mod special_ability;
mod stealth_fighter;
mod sticky_bomb;
mod supply_crippling;
mod toppling;
mod veterancy_crate;
use generals_assets::{
    content::{self, ContentFile, ContentLine, Declaration},
    virtual_assets::VirtualAssets,
    w3d::Model,
};
use generals_sim::skirmish::{
    AiRules, DAMAGE_TYPE_COUNT, DamageType, EconomyRules, Faction, GroupKind, Kind,
    MovementSurface, ProductionExit, UnitAi, UnitType,
};
use std::error::Error;
mod capture;
mod car_bombs;
mod chinook;
mod cleanup;
mod command_button_hunt;
mod contamination;
mod countermeasures;
mod crushing;
mod damage_fx;
pub mod death_callbacks;
mod death_lifecycle;
mod death_weapons;
mod demo_trap;
mod deployment;
mod drones;
mod fake_buildings;
mod fire;
mod garrisons;
#[cfg(test)]
#[cfg(test)]
mod hacker_disable;
mod hazards;
mod hive_body;
mod horde;
mod income;
pub mod instant_death;
mod laser_lifetime;
mod lifetimes;
pub mod locomotor;
mod maintenance;
mod neutral_tech;
mod overlord;
mod point_defense;
mod production;
mod projectile_templates;
mod sabotage;
mod special_power;
mod spectre;
pub mod structure_death;
pub use physical_ocl::motion_rules as shell_physics_rules;
mod research;
mod reskins;
mod salvage;
mod spawn;
mod upgrades;

pub fn resolve_reskins(files: &mut [ContentFile]) -> Result<(), Box<dyn Error>> {
    reskins::resolve(files)
}
mod microwave;
mod spy_vision;
mod stealth;
mod strategy;
mod support;
pub use support::geometry;
mod reinforcement;
#[cfg(test)]
mod reinforcement_runtime_tests;
mod turret;
mod veterancy;
mod weapons;
mod weapons_scatter;

pub struct SkirmishData {
    command_buttons: generals_sim::skirmish::command_button_hunt::Definitions,
    command_sets: std::collections::BTreeMap<String, Vec<String>>,
    disabled_sounds: [String; 4],
    deliveries: Vec<generals_sim::skirmish::deliver_payload::Rules>,
    pub gravity: f32,
    pub visibility: generals_sim::skirmish::visibility::VisibilityRules,
    pub types: Vec<UnitType>,
    pub models: Vec<Model>,
    pub economy: EconomyRules,
    pub ai: AiRules,
    pub progression: generals_sim::skirmish::ProgressionRules,
}

impl SkirmishData {
    pub fn into_simulation(
        self,
        terrain: generals_sim::terrain::TerrainMap,
        layout: generals_sim::skirmish::MapLayout,
    ) -> Result<generals_sim::skirmish::Skirmish, Box<dyn Error>> {
        let mut simulation = generals_sim::skirmish::Skirmish::new(
            terrain,
            self.types,
            Faction::Usa,
            Faction::China,
            self.economy,
            self.ai,
            layout,
        )?;
        simulation.gravity = self.gravity;
        simulation.configure_delivery_lists(self.deliveries)?;
        simulation.configure_disabled_sounds(self.disabled_sounds);
        simulation.configure_command_sets(self.command_sets)?;
        simulation.configure_command_buttons(self.command_buttons);
        simulation.configure_progression(self.progression)?;
        simulation.configure_visibility(self.visibility)?;
        Ok(simulation)
    }
}

fn bind_authored_model(
    object: &Declaration,
    model: &Model,
    stats: &mut UnitType,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    stats.parking = parking::load(object)?;
    if let Some(rules) = &mut stats.death_lifecycle {
        for death in &mut rules.slow {
            if let generals_sim::skirmish::death_lifecycle::Derived::Helicopter(rule) =
                &mut death.derived
            {
                rule.blade_point = model.bone_position(&rule.blade_bone);
                if !rule.particle_bone.is_empty() {
                    rule.particle_point =
                        model.bone_position(&rule.particle_bone).unwrap_or([0.0; 3]);
                }
            }
        }
    }
    spawn::bind(object, model, stats).map_err(|error| format!("{name} spawn points: {error}"))?;
    crate::supply_data::load(object, model, stats)
        .map_err(|error| format!("{name} supply data: {error}"))?;
    if !blocks(&object.lines, 0, "Behavior")?.iter().any(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|family| family == "HelixContain")
    }) && (!stats.behavior.attachments.is_empty()
        || (blocks(&object.lines, 0, "Behavior")?.iter().any(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|family| family == "OverlordContain")
        }) && stats
            .contain
            .as_ref()
            .is_some_and(|rules| !rules.payload_templates.is_empty())))
    {
        stats.attachment_point = model
            .bone_position("FIREPOINT01")
            .ok_or("attachment carrier lacks FIREPOINT01")?;
    }
    Ok(())
}

pub fn load(
    assets: &VirtualAssets,
    records: &[generals_assets::map::MapObject],
) -> Result<SkirmishData, Box<dyn Error>> {
    let roster_files = source_files(assets)?;
    let mut types = Vec::new();
    let mut models = Vec::new();
    let mut roster: Vec<_> = ROSTER
        .iter()
        .map(|(name, faction, kind)| (String::from(*name), *faction, *kind))
        .collect();
    roster.extend(
        garrisons::templates(&roster_files, records)?
            .into_iter()
            .map(|(name, kind)| (name.to_owned(), Faction::Civilian, kind)),
    );
    loop {
        let selected: Vec<_> = roster.iter().map(|(name, _, _)| name.as_str()).collect();
        let fields = hazards::field_templates(&roster_files, &selected)?;
        if fields.is_empty() {
            break;
        }
        roster.extend(
            fields
                .into_iter()
                .map(|name| (name.to_owned(), Faction::Civilian, Kind::Effect)),
        );
    }
    let (files, metadata) = source_files::with_runtime_dependencies(assets, &mut roster)?;
    let repulsors_enabled = module_repulsors_enabled(assets)?;
    income::validate_command(&files)?;
    for (name, faction, kind) in roster {
        let object = definition(&files, "Object", &name)?;
        let mut stats = unit_type(&files, object, faction, kind)
            .and_then(|unit| finish_type(&files, object, unit))
            .map_err(|error| format!("{name}: {error}"))?;
        veterancy::load(&files, object, &mut stats)
            .map_err(|error| format!("{name} veterancy: {error}"))?;
        fake_buildings::load(&files, object, &mut stats)
            .map_err(|error| format!("{name} fake-building rules: {error}"))?;
        let model =
            normal_model(assets, object).map_err(|error| format!("{name} model: {error}"))?;
        bind_authored_model(object, &model, &mut stats, &name)?;
        stats.update_modules = metadata
            .get(&name)
            .ok_or_else(|| format!("{name}: missing ordered module metadata"))?
            .runtime_registrations(repulsors_enabled);
        stats.behavior_modules = metadata
            .get(&name)
            .ok_or_else(|| format!("{name}: missing ordered behavior metadata"))?
            .runtime_behaviors(&files)?;
        generals_sim::skirmish::module_updates::validate_registrations(&stats)?;
        types.push(stats);
        models.push(model);
    }
    aircraft_death::validate_targets(&types)?;
    helicopter_death::validate(&types)?;
    let global = &definition(&files, "GameData", "")?.lines;
    let economy = EconomyRules {
        allowed_build_height_variation: number(global, 0, "AllowedHeightVariationForBuilding")?,
        min_build_edge_distance: number(global, 0, "MinDistFromEdgeOfMapForBuild")?,
        starting_cash: unsigned(global, 0, "DefaultStartingCash")?,
        supply_value: unsigned(global, 0, "ValuePerSupplyBox")?,
        supply_build_border: number(global, 0, "SupplyBuildBorder")?,
        low_energy_penalty_modifier: number(global, 0, "LowEnergyPenaltyModifier")?,
        min_low_energy_production_speed: number(global, 0, "MinLowEnergyProductionSpeed")?,
        max_low_energy_production_speed: number(global, 0, "MaxLowEnergyProductionSpeed")?,
    };
    let selected: Vec<_> = types.iter().map(|unit| unit.name.as_str()).collect();
    let deliveries = reinforcement::lists(&files, &selected)?;
    let progression = crate::progression_data::load(assets, &files, &mut types)?;
    let disabled_sounds = disabled_sounds(&files)?;
    Ok(SkirmishData {
        deliveries,
        command_buttons: command_button_hunt::definitions(&files)?,
        disabled_sounds,
        command_sets: production::command_sets(&files)?,
        gravity: locomotor::per_frame(global, 0, "Gravity", -1.0, (1.0 / 30.0) * (1.0 / 30.0))?,
        visibility: crate::visibility_data::rules(assets, global)?,
        progression,
        types,
        models,
        economy,
        ai: ai_rules(assets)?,
    })
}

fn source_files(assets: &VirtualAssets) -> Result<Vec<ContentFile>, Box<dyn Error>> {
    let mut files = source_files::lexical(assets, false)?;
    resolve_reskins(&mut files)?;
    Ok(files)
}

pub fn feedback_templates(catalog: &[generals_sim::skirmish::UnitType]) -> Vec<&str> {
    catalog
        .iter()
        .filter(|unit| unit.laser_update.is_some())
        .map(|unit| unit.name.as_str())
        .collect()
}

const ROSTER: &[(&str, Faction, Kind)] = &[
    ("AmericaCommandCenter", Faction::Usa, Kind::CommandCenter),
    ("AmericaVehicleDozer", Faction::Usa, Kind::Builder),
    ("AmericaSupplyCenter", Faction::Usa, Kind::SupplyCenter),
    ("AmericaBarracks", Faction::Usa, Kind::Barracks),
    ("AmericaWarFactory", Faction::Usa, Kind::Factory),
    ("AmericaAirfield", Faction::Usa, Kind::Factory),
    ("AmericaJetRaptor", Faction::Usa, Kind::Vehicle),
    ("AmericaJetStealthFighter", Faction::Usa, Kind::Vehicle),
    ("AmericaJetStealthHulk", Faction::Usa, Kind::Effect),
    ("AmericaJetAurora", Faction::Usa, Kind::Vehicle),
    ("AmericaVehicleComanche", Faction::Usa, Kind::Vehicle),
    ("AmericaPowerPlant", Faction::Usa, Kind::PowerPlant),
    ("AmericaStrategyCenter", Faction::Usa, Kind::TechBuilding),
    ("AmericaInfantryRanger", Faction::Usa, Kind::Infantry),
    ("AmericaInfantryPathfinder", Faction::Usa, Kind::Infantry),
    ("AmericaTankCrusader", Faction::Usa, Kind::Tank),
    ("AmericaTankPaladin", Faction::Usa, Kind::Tank),
    ("AmericaVehicleTomahawk", Faction::Usa, Kind::Vehicle),
    ("AmericaVehicleBattleDrone", Faction::Usa, Kind::Vehicle),
    ("AmericaVehicleHellfireDrone", Faction::Usa, Kind::Vehicle),
    ("AmericaVehicleScoutDrone", Faction::Usa, Kind::Vehicle),
    ("AmericaVehicleSentryDrone", Faction::Usa, Kind::Vehicle),
    ("AmericaVehicleSentryDroneHulk", Faction::Usa, Kind::Effect),
    ("AmericaTankMicrowave", Faction::Usa, Kind::Tank),
    ("AmericaTankAvenger", Faction::Usa, Kind::Vehicle),
    ("AmericaTankAvengerLaserTurret", Faction::Usa, Kind::Vehicle),
    ("AmericaVehicleChinook", Faction::Usa, Kind::Harvester),
    (
        "AmericaInfantryMissileDefender",
        Faction::Usa,
        Kind::Infantry,
    ),
    ("AmericaVehicleHumvee", Faction::Usa, Kind::Vehicle),
    ("AmericaVehicleHumveeDeadHull", Faction::Usa, Kind::Effect),
    ("AmericaVehicleMedic", Faction::Usa, Kind::Vehicle),
    ("ChinaCommandCenter", Faction::China, Kind::CommandCenter),
    ("ChinaVehicleDozer", Faction::China, Kind::Builder),
    ("ChinaSupplyCenter", Faction::China, Kind::SupplyCenter),
    ("ChinaBarracks", Faction::China, Kind::Barracks),
    ("ChinaWarFactory", Faction::China, Kind::Factory),
    ("ChinaAirfield", Faction::China, Kind::Factory),
    ("ChinaJetMIG", Faction::China, Kind::Vehicle),
    ("ChinaVehicleHelix", Faction::China, Kind::Vehicle),
    ("ChinaHelixGattlingCannon", Faction::China, Kind::Vehicle),
    ("ChinaHelixPropagandaTower", Faction::China, Kind::Vehicle),
    ("ChinaHelixBattleBunker", Faction::China, Kind::Vehicle),
    ("HelixRubbleHull", Faction::China, Kind::Effect),
    ("NapalmBomb", Faction::China, Kind::Effect),
    ("AmericaParachute", Faction::Usa, Kind::Effect),
    ("ChinaPowerPlant", Faction::China, Kind::PowerPlant),
    ("ChinaInfantryRedguard", Faction::China, Kind::Infantry),
    ("ChinaTankBattleMaster", Faction::China, Kind::Tank),
    ("ChinaTankDragon", Faction::China, Kind::Tank),
    ("ChinaVehicleInfernoCannon", Faction::China, Kind::Vehicle),
    ("FireWallSegment", Faction::China, Kind::Effect),
    ("FireWallSegmentUpgraded", Faction::China, Kind::Effect),
    ("FireFieldSmall", Faction::China, Kind::Effect),
    ("FireFieldUpgradedSmall", Faction::China, Kind::Effect),
    ("FirestormSmall", Faction::China, Kind::Effect),
    ("BlackNapalmFirestormSmall", Faction::China, Kind::Effect),
    ("ChinaVehicleSupplyTruck", Faction::China, Kind::Harvester),
    ("ChinaInfantryTankHunter", Faction::China, Kind::Infantry),
    ("ChinaInfantryHacker", Faction::China, Kind::Infantry),
    ("ChinaInfantryBlackLotus", Faction::China, Kind::Infantry),
    ("ChinaTankGattling", Faction::China, Kind::Tank),
    ("ChinaTankECM", Faction::China, Kind::Tank),
    ("ChinaPropagandaCenter", Faction::China, Kind::TechBuilding),
    ("ChinaTankOverlord", Faction::China, Kind::Tank),
    ("GLACommandCenter", Faction::Gla, Kind::CommandCenter),
    ("GLAInfantryWorker", Faction::Gla, Kind::Builder),
    ("GLASupplyStash", Faction::Gla, Kind::SupplyCenter),
    ("GLABarracks", Faction::Gla, Kind::Barracks),
    ("GLAArmsDealer", Faction::Gla, Kind::Factory),
    ("GLAInfantryAngryMobNexus", Faction::Gla, Kind::Infantry),
    ("GLAInfantryAngryMobPistol01", Faction::Gla, Kind::Infantry),
    ("GLAInfantryAngryMobRock02", Faction::Gla, Kind::Infantry),
    ("GLAInfantryAngryMobMolotov02", Faction::Gla, Kind::Infantry),
    ("GLAInfantryAngryMobPistol03", Faction::Gla, Kind::Infantry),
    ("GLAInfantryAngryMobRock04", Faction::Gla, Kind::Infantry),
    ("GLAInfantryAngryMobPistol05", Faction::Gla, Kind::Infantry),
    ("GLAVehicleBattleBus", Faction::Gla, Kind::Vehicle),
    ("GLAInfantryRebel", Faction::Gla, Kind::Infantry),
    ("GLAInfantryJarmenKell", Faction::Gla, Kind::Infantry),
    ("GLAInfantryTerrorist", Faction::Gla, Kind::Infantry),
    ("GLAInfantrySaboteur", Faction::Gla, Kind::Infantry),
    ("GLAInfantryHijacker", Faction::Gla, Kind::Infantry),
    ("ToxicInfantry_TerroristOnly", Faction::Gla, Kind::Infantry),
    (
        "ToxicInfantryBeta_TerroristOnly",
        Faction::Gla,
        Kind::Infantry,
    ),
    ("GLATankScorpion", Faction::Gla, Kind::Tank),
    ("GLAInfantryTunnelDefender", Faction::Gla, Kind::Infantry),
    ("GLAInfantryStingerSoldier", Faction::Gla, Kind::Infantry),
    ("GLAStingerSite", Faction::Gla, Kind::Defense),
    ("GLADemoTrap", Faction::Gla, Kind::Defense),
    ("GLAHoleStingerSite", Faction::Gla, Kind::TechBuilding),
    ("FlamingInfantry", Faction::Civilian, Kind::Infantry),
    ("ToxicInfantry", Faction::Civilian, Kind::Infantry),
    ("ToxicInfantryBeta", Faction::Civilian, Kind::Infantry),
    ("GLAVehicleQuadCannon", Faction::Gla, Kind::Vehicle),
    ("GLAVehicleRocketBuggy", Faction::Gla, Kind::Vehicle),
    ("GLAVehicleScudLauncher", Faction::Gla, Kind::Vehicle),
    ("GLAVehicleToxinTruck", Faction::Gla, Kind::Vehicle),
    ("GLATankMarauder", Faction::Gla, Kind::Tank),
    ("GLAVehicleRadarVan", Faction::Gla, Kind::Vehicle),
    ("GLAPalace", Faction::Gla, Kind::TechBuilding),
    ("GLABlackMarket", Faction::Gla, Kind::TechBuilding),
    ("FakeGLACommandCenter", Faction::Gla, Kind::TechBuilding),
    ("FakeGLABarracks", Faction::Gla, Kind::TechBuilding),
    ("FakeGLASupplyStash", Faction::Gla, Kind::TechBuilding),
    ("FakeGLAArmsDealer", Faction::Gla, Kind::TechBuilding),
    ("FakeGLABlackMarket", Faction::Gla, Kind::TechBuilding),
    ("GLAHolePalace", Faction::Gla, Kind::TechBuilding),
    ("GLAHoleBlackMarket", Faction::Gla, Kind::TechBuilding),
    ("GLAHoleCommandCenter", Faction::Gla, Kind::TechBuilding),
    ("GLAHoleBarracks", Faction::Gla, Kind::TechBuilding),
    ("GLAHoleSupplyStash", Faction::Gla, Kind::TechBuilding),
    ("GLAHoleArmsDealer", Faction::Gla, Kind::TechBuilding),
    ("TruckWork", Faction::Civilian, Kind::Vehicle),
    ("CarSport01", Faction::Civilian, Kind::Vehicle),
    ("CarCompact", Faction::Civilian, Kind::Vehicle),
    ("CarSport02", Faction::Civilian, Kind::Vehicle),
    ("TractorCombine", Faction::Civilian, Kind::Vehicle),
    ("ForkliftSmall", Faction::Civilian, Kind::Vehicle),
    ("CarTaxiCab02", Faction::Civilian, Kind::Vehicle),
    ("CarScrap", Faction::Civilian, Kind::Vehicle),
    ("TechOilDerrick", Faction::Civilian, Kind::TechBuilding),
    ("TechHospital", Faction::Civilian, Kind::TechBuilding),
    ("TechOilRefinery", Faction::Civilian, Kind::TechBuilding),
    ("TechRepairPad", Faction::Civilian, Kind::TechBuilding),
    ("TechRepairbay", Faction::Civilian, Kind::TechBuilding),
    ("TechArtilleryPlatform", Faction::Civilian, Kind::Defense),
    ("AmericaPatriotBattery", Faction::Usa, Kind::Defense),
    ("AmericaFireBase", Faction::Usa, Kind::Defense),
    ("ChinaGattlingCannon", Faction::China, Kind::Defense),
    ("ChinaBunker", Faction::China, Kind::Defense),
    ("ChinaSpeakerTower", Faction::China, Kind::Defense),
    ("ChinaInternetCenter", Faction::China, Kind::TechBuilding),
    (
        "ChinaTankOverlordGattlingCannon",
        Faction::China,
        Kind::Vehicle,
    ),
    (
        "ChinaTankOverlordPropagandaTower",
        Faction::China,
        Kind::Vehicle,
    ),
    (
        "ChinaTankOverlordBattleBunker",
        Faction::China,
        Kind::Vehicle,
    ),
    ("GLATunnelNetwork", Faction::Gla, Kind::Defense),
    ("GLAHoleTunnelNetwork", Faction::Gla, Kind::TechBuilding),
    ("GLAVehicleTechnical", Faction::Gla, Kind::Vehicle),
    ("GLAVehicleTechnicalChassisOne", Faction::Gla, Kind::Vehicle),
    ("GLAVehicleTechnicalChassisTwo", Faction::Gla, Kind::Vehicle),
    (
        "GLAVehicleTechnicalChassisThree",
        Faction::Gla,
        Kind::Vehicle,
    ),
    ("100DollarCrate", Faction::Civilian, Kind::Crate),
    ("200DollarCrate", Faction::Civilian, Kind::Crate),
    ("1000DollarCrate", Faction::Civilian, Kind::Crate),
    ("1500DollarCrate", Faction::Civilian, Kind::Crate),
    ("2500DollarCrate", Faction::Civilian, Kind::Crate),
    ("100DollarCrateForBuildings", Faction::Civilian, Kind::Crate),
    ("SupplyDropZoneCrate", Faction::Civilian, Kind::Crate),
    ("SalvageCrate", Faction::Civilian, Kind::Crate),
    ("ChinaVehicleTroopCrawler", Faction::China, Kind::Vehicle),
    ("GenericDebris", Faction::Civilian, Kind::Effect),
    ("DeadBattleBusHulk", Faction::Gla, Kind::Effect),
    ("DeadCrusaderHulk", Faction::Usa, Kind::Effect),
    ("AmericaJetRaptorHulk", Faction::Usa, Kind::Effect),
    ("AmericaJetAuroraHulk", Faction::Usa, Kind::Effect),
    ("ChinaJetMIGHulk", Faction::China, Kind::Effect),
    (
        "ChinaVehicleListeningOutpost",
        Faction::China,
        Kind::Vehicle,
    ),
    (
        "ChinaVehicleListeningOutpostDeadHull",
        Faction::China,
        Kind::Effect,
    ),
];

pub fn fx_definition<'a>(
    files: &'a [ContentFile],
    name: &str,
) -> Result<Option<&'a Declaration>, Box<dyn Error>> {
    if name.eq_ignore_ascii_case("NONE") {
        return Ok(None);
    }
    let mut found = None;
    for file in files {
        for declaration in &file.declarations {
            if declaration.kind != "FXList" || declaration.name != name {
                continue;
            }
            if declaration.parent.is_some()
                || declaration.lines.iter().any(|line| line.depth.is_none())
            {
                return Err(format!("unresolved FXList {name} in {}", file.path).into());
            }
            found = Some(declaration);
        }
    }
    Ok(found)
}

pub fn definition<'a>(
    files: &'a [ContentFile],
    kind: &str,
    name: &str,
) -> Result<&'a Declaration, Box<dyn Error>> {
    let mut found = None;
    for file in files {
        for declaration in &file.declarations {
            if declaration.kind == kind && declaration.name == name {
                if found.is_some()
                    || declaration.parent.is_some()
                    || declaration.lines.iter().any(|line| line.depth.is_none())
                {
                    return Err(
                        format!("ambiguous or unresolved {kind} {name} in {}", file.path).into(),
                    );
                }
                found = Some(declaration);
            }
        }
    }
    found.ok_or_else(|| format!("missing {kind} {name}").into())
}

fn unit_type(
    files: &[ContentFile],
    object: &Declaration,
    faction: Faction,
    kind: Kind,
) -> Result<UnitType, Box<dyn Error>> {
    let body = active_body(files, object, kind)?;
    let (death_lifecycle, structure_death) = death_modules(files, object)?;
    let physical = physical_ocl::rules(files, object, death_lifecycle.as_ref())?;
    Ok(UnitType {
        command_set: optional_text(&object.lines, "CommandSet")?.into(),
        pristine_draws: Vec::new(),
        sway_updates: toppling::sway_updates(object)?,
        combat_drop_kill_fx: chinook::combat_drop_fx(object)?,
        laser_update: assisted_targeting::laser_update(object)?,
        behavior: behavior_data::load(files, object, kind)?,
        projectile_rules: crate::projectile_data::template_rules(files, object)?,
        body_kind: body_kind(&object.lines)?,
        body_damage_thresholds: overlord::thresholds(files)?,
        deliver_payload: reinforcement::transport(object)?,
        update_modules: Vec::new(),
        behavior_modules: Vec::new(),
        neutral_tech: neutral_tech::rules(&object.lines)?,
        minimum_health: body_minimum(body),
        build_rules: support::build_rules(&object.lines)?,
        battle_bus: battle_bus::load(files, object)?,
        live_physics: physical_ocl::live_rules(files, object)?,
        aircraft: None,
        parking: None,
        fire: fire::rules(object)?,

        hive_body: hive_body::load(body)?,
        death_lifecycle,
        structure_death,
        demo_trap: demo_trap::load(files, object)?,
        spectre: spectre::gunship(files, object)?,
        spectre_deployments: spectre::deployments(files, object)?,
        special_powers: special_power::load(files, object)?,
        unpause_special_powers: special_power::load_unpause(files, object)?,
        deployment: deployment::load(object)?,
        hijacker: hijacker::load(object)?,
        parachute: parachute::load(files, object)?,
        physical,
        variations: salvage::words(&object.lines, 0, "BuildVariations"),
        reskin: reskins::parent(files, &object.name),
        salvage: salvage::load(files, object)?,
        attachment_bone: overlord::bone(object)?,
        attachment_point: [0.0; 3],
        attachment_thresholds: overlord::thresholds(files)?,
        battle_plan: strategy::load(files, object)?,
        contain: None,
        transport_slots: 0,
        kind_of: Vec::new(),
        turret: turret::load(object)?,
        secondary_turret: turret::load_secondary(object)?,
        turrets_linked: turret::linked(object)?,
        enter_guard: turret::enter_guard(object)?,
        sight: crate::visibility_data::sight(&object.lines)?,
        maintenance: maintenance::load(files, object)?,
        veterancy: generals_sim::skirmish::VeterancyRules::none(),
        progression: generals_sim::skirmish::UnitProgression::default(),
        name: object.name.clone(),
        faction,
        kind,
        selectable: support::selectable(&object.lines),
        scoring: support::scoring(&object.lines),
        ai: unit_ai(object)?,
        cost: support::build_cost(&object.lines)?,
        build_seconds: optional_number(&object.lines, "BuildTime", 1.0)?,
        max_health: body.map_or(Ok(0.0), |lines| body_health(lines, "MaxHealth"))?,
        initial_health: body.map_or(Ok(0.0), |lines| body_health(lines, "InitialHealth"))?,
        speed: 0.0,
        locomotor: locomotor::stationary(),
        locomotor_sets: generals_sim::skirmish::LocomotorSets::none(),
        preferred_height: 0.0,
        chinook: None,
        movement_surface: MovementSurface::Ground,
        group_kind: group_kind(&object.lines),
        crush: crushing::rules(&object.lines)?,
        production: production::quantities(object)?,
        additional_production: production::additional(object)?,
        production_queue_limit: production::queue_limit(object)?,
        production_disabled_types: production::disabled_types(object)?,
        production_doors: production::doors(object)?,
        buildable_objects: production::objects(files, object)?,
        location_weapons: production::location_weapons(files, object)?,
        factory_bib: production::factory_bib(&object.lines)?,
        weapon: generals_sim::skirmish::WeaponRules::unarmed(),
        secondary_weapon: None,
        tertiary_weapon: None,
        weapon_set_rules: generals_sim::skirmish::WeaponSetRules::independent(),
        weapon_upgrade_triggers: Vec::new(),
        armor: [1.0; DAMAGE_TYPE_COUNT],
        damage_fx: damage_fx::load(files, object)?,
        damaged_sound: optional_text(&object.lines, "SoundOnDamaged")?.into(),
        really_damaged_sound: optional_text(&object.lines, "SoundOnReallyDamaged")?.into(),
        fear_sound: optional_text(&object.lines, "VoiceFear")?.into(),
        supply: generals_sim::skirmish::supply::CollectorRules::NONE,
        geometry: physical_ocl::geometry(files, object)?,

        geometry_bounds: support::geometry_bounds(&object.lines)?,

        occlusion_delay: crate::container_data::occlusion_delay(files, object)?,
        fence_width: optional_number(&object.lines, "FenceWidth", 0.0)?,
        placement_angle: placement_angle(&object.lines)?,
        energy: support::energy(&object.lines)?,
    })
}

fn death_modules(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<
    (
        Option<generals_sim::skirmish::death_lifecycle::Rules>,
        Option<generals_sim::skirmish::structure_death::Rules>,
    ),
    Box<dyn Error>,
> {
    let mut lifecycle = death_lifecycle::load(files, object)?;
    let structure = structure_death::load(files, object, lifecycle.as_mut())?;
    Ok((lifecycle, structure))
}

fn body_minimum(body: Option<&[ContentLine]>) -> f32 {
    // ImmortalBody::internalChangeHealth retains one hit point after every delta.
    if body.is_some_and(|lines| {
        lines[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "ImmortalBody")
    }) {
        1.0
    } else {
        0.0
    }
}

fn body_kind(lines: &[ContentLine]) -> Result<generals_sim::skirmish::BodyKind, Box<dyn Error>> {
    use generals_sim::skirmish::BodyKind;
    let bodies = blocks(lines, 0, "Body")?;
    let Some(body) = bodies.last() else {
        return Ok(BodyKind::Absent);
    };
    match body[0].tokens.get(1).map(String::as_str) {
        Some(
            "ActiveBody" | "StructureBody" | "HiveStructureBody" | "ImmortalBody" | "UndeadBody",
        ) => Ok(BodyKind::Active),
        Some("InactiveBody") => Ok(BodyKind::Inactive),
        Some("HighlanderBody") => Ok(BodyKind::Highlander),
        _ => Err("Unknown concrete Body factory inheritance".into()),
    }
}

fn active_body<'a>(
    files: &'a [ContentFile],
    object: &'a Declaration,
    kind: Kind,
) -> Result<Option<&'a [ContentLine]>, Box<dyn Error>> {
    let bodies = blocks(&object.lines, 0, "Body")?;
    // Object::createModules retains every authored module; its release body
    // pointer is overwritten in module order. Map AddModule may append a body.
    let body = if bodies.is_empty()
        && (physical_ocl::OBJECTS.contains(&object.name.as_str())
            || map_crates::OBJECTS.contains(&object.name.as_str()))
    {
        // ThingFactory copies DefaultThingTemplate before parsing the new
        // object. GenericDebris retains that original inactive body module.
        let defaults = definition(files, "Object", "DefaultThingTemplate")?;
        let inherited = blocks(&defaults.lines, 0, "Body")?;
        if inherited.len() != 1
            || inherited[0][0]
                .tokens
                .get(1)
                .is_none_or(|name| name != "InactiveBody")
        {
            return Err("physical object requires the original default InactiveBody".into());
        }
        inherited.first().copied()
    } else {
        bodies.last().copied()
    };
    if body.is_none()
        && kind != Kind::Crate
        && field(&object.lines, 0, "BuildVariations").is_none_or(|line| line.tokens.len() == 1)
    {
        return Err("missing active body".into());
    }
    Ok(body)
}

fn body_health(lines: &[ContentLine], field: &str) -> Result<f32, Box<dyn Error>> {
    if lines[0]
        .tokens
        .get(1)
        .is_some_and(|name| name == "InactiveBody")
    {
        Ok(0.0)
    } else {
        number(lines, 1, field)
    }
}

fn finish_type(
    files: &[ContentFile],
    object: &Declaration,
    mut stats: UnitType,
) -> Result<UnitType, Box<dyn Error>> {
    if !matches!(
        stats.ai.update,
        generals_sim::skirmish::UnitAiUpdate::Absent
    ) && !locomotor::normal_set(&object.lines)?.is_empty()
    {
        load_locomotor(&mut stats, files, object)?;
    }
    crate::container_data::load(files, object, &mut stats)?;
    weapons::load(files, object, &mut stats)?;
    car_bombs::weapon_set(files, object, &mut stats)?;
    stats.maintenance.subdual = microwave::body(active_body(files, object, stats.kind)?)?;
    weapons::global_bonuses(files, &mut stats)?;
    fire::load(files, &mut stats)?;
    hazards::load(files, &mut stats)?;
    let name = normal_set(&object.lines, "ArmorSet")?
        .map_or(Ok("NoArmor"), |set| text(set, 1, "Armor"))?;
    let armor = definition(files, "Armor", name)?;
    stats.armor = armor_coefficients(&armor.lines)?;
    supply_collector(&mut stats, object)?;
    laser_lifetime::bind(files, &mut stats)?;
    validate_health(&stats)?;
    Ok(stats)
}

fn validate_health(stats: &UnitType) -> Result<(), Box<dyn Error>> {
    if (stats.initial_health <= 0.0
        && !matches!(stats.body_kind, generals_sim::skirmish::BodyKind::Inactive)
        && !matches!(stats.kind, Kind::Crate | Kind::Effect)
        && stats.variations.is_empty())
        || stats.max_health < stats.initial_health
        || !stats.build_seconds.is_finite()
        || stats.build_seconds < 0.0
        || (stats.build_seconds <= 0.0 && stats.behavior.mobs.is_empty())
    {
        return Err("invalid health or build time".into());
    }
    Ok(())
}

fn supply_collector(stats: &mut UnitType, object: &Declaration) -> Result<(), Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0].tokens.get(1).is_some_and(|name| {
            ["ChinookAIUpdate", "SupplyTruckAIUpdate", "WorkerAIUpdate"].contains(&name.as_str())
        }) {
            // SupplyTruckAIUpdateModuleData (also the Chinook base) defaults all three to zero.
            stats.supply.capacity = field(module, 1, "MaxBoxes")
                .map(scalar)
                .transpose()?
                .unwrap_or("0")
                .parse()?;
            stats.supply.gather_seconds = field(module, 1, "SupplyWarehouseActionDelay")
                .map_or(Ok(0.0), |_| {
                    duration(module, 1, "SupplyWarehouseActionDelay")
                })?;
            stats.supply.dropoff_seconds = field(module, 1, "SupplyCenterActionDelay")
                .map_or(Ok(0.0), |_| duration(module, 1, "SupplyCenterActionDelay"))?;
        }
    }
    if stats.kind == Kind::SupplyCenter {
        validate_supply_spawn(object, stats.faction)?;
    }
    Ok(())
}

pub fn has_flag(lines: &[ContentLine], key: &str, flag: &str) -> bool {
    field(lines, 0, key).is_some_and(|line| line.tokens.iter().skip(1).any(|value| value == flag))
}

fn group_kind(lines: &[ContentLine]) -> GroupKind {
    if has_flag(lines, "KindOf", "PRODUCED_AT_HELIPAD") {
        GroupKind::Helicopter
    } else if has_flag(lines, "KindOf", "AIRCRAFT") {
        GroupKind::Aircraft
    } else if has_flag(lines, "KindOf", "INFANTRY") {
        GroupKind::Infantry
    } else if has_flag(lines, "KindOf", "VEHICLE") {
        GroupKind::Vehicle
    } else {
        GroupKind::Other
    }
}

fn load_locomotor(
    stats: &mut UnitType,
    files: &[ContentFile],
    object: &Declaration,
) -> Result<(), Box<dyn Error>> {
    let names = locomotor::normal_set(&object.lines)?;
    let name = *names
        .first()
        .ok_or("moving object lacks SET_NORMAL locomotor")?;
    let lines = &definition(files, "Locomotor", name)?.lines;
    stats.speed = number(lines, 0, "Speed")?;
    stats.locomotor = locomotor::load(
        lines,
        &definition(files, "GameData", "")?.lines,
        &object.lines,
    )?;
    for name in names.iter().skip(1) {
        let lines = &definition(files, "Locomotor", name)?.lines;
        let variant = locomotor::load(
            lines,
            &definition(files, "GameData", "")?.lines,
            &object.lines,
        )?;
        stats
            .locomotor_sets
            .normal
            .push(generals_sim::skirmish::LocomotorVariant {
                speed: number(lines, 0, "Speed")?,
                locomotor: variant,
            });
    }
    for (role, variants) in [
        ("SET_WANDER", &mut stats.locomotor_sets.wander),
        ("SET_PANIC", &mut stats.locomotor_sets.panic),
    ] {
        for name in locomotor::named_set(&object.lines, role)? {
            let lines = &definition(files, "Locomotor", name)?.lines;
            variants.push(generals_sim::skirmish::LocomotorVariant {
                speed: number(lines, 0, "Speed")?,
                locomotor: locomotor::load(
                    lines,
                    &definition(files, "GameData", "")?.lines,
                    &object.lines,
                )?,
            });
        }
    }
    stats.preferred_height = locomotor::signed_real(lines, 0, "PreferredHeight", 0.0)?;
    stats.chinook = chinook::load(files, object, lines)?;
    stats.aircraft = aircraft::load(files, object)?;
    stats.movement_surface = locomotor_surface(lines)?;
    Ok(())
}

fn locomotor_surface(lines: &[ContentLine]) -> Result<MovementSurface, Box<dyn Error>> {
    match locomotor_surface_mask(lines)? {
        1 | 17 => Ok(MovementSurface::Ground),
        8 => Ok(MovementSurface::Air),
        _ => Err("Selected locomotor uses unported movement surfaces".into()),
    }
}

fn locomotor_surface_mask(lines: &[ContentLine]) -> Result<u8, Box<dyn Error>> {
    // LocomotorTemplate initializes zero; INI::parseBitString32 applies every
    // Surfaces line sequentially, with normal replacement or +/- modification.
    // TheLocomotorSurfaceTypeNames order is fixed in LocomotorSet.h.
    const NAMES: [&str; 5] = ["GROUND", "WATER", "CLIFF", "AIR", "RUBBLE"];
    let mut mask = 0_u8;
    for line in lines.iter().filter(|line| {
        line.depth == Some(0)
            && line
                .tokens
                .first()
                .is_some_and(|token| token.eq_ignore_ascii_case("Surfaces"))
    }) {
        let mut normal = false;
        let mut editing = false;
        for token in line.tokens.iter().skip(1) {
            if token.eq_ignore_ascii_case("NONE") {
                if normal || editing {
                    return Err("Cannot mix NONE with locomotor surface flags".into());
                }
                mask = 0;
                break;
            }
            let addition = token.strip_prefix('+');
            let subtraction = token.strip_prefix('-');
            let name = addition.or(subtraction).unwrap_or(token);
            let index = NAMES
                .iter()
                .position(|flag| flag.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("Unknown locomotor surface {name}"))?;
            let bit = 1_u8 << index;
            if addition.is_some() || subtraction.is_some() {
                if normal {
                    return Err("Cannot mix replacement and edited locomotor surfaces".into());
                }
                editing = true;
                if addition.is_some() {
                    mask |= bit;
                } else {
                    mask &= !bit;
                }
            } else {
                if editing {
                    return Err("Cannot mix replacement and edited locomotor surfaces".into());
                }
                if !normal {
                    mask = 0;
                }
                normal = true;
                mask |= bit;
            }
        }
    }
    Ok(mask)
}

fn production_exit(
    object: &Declaration,
    kind: Kind,
) -> Result<Option<ProductionExit>, Box<dyn Error>> {
    let mut exit = None;
    let mut parking = false;
    for module in blocks(&object.lines, 0, "Behavior")? {
        let Some(name) = module[0].tokens.get(1) else {
            continue;
        };
        if name == "ParkingPlaceBehavior" {
            parking = true;
        }
        if !name.ends_with("ProductionExitUpdate") {
            continue;
        }
        if name == "SpawnPointProductionExitUpdate" {
            continue;
        }
        if exit.is_some() {
            return Err("multiple production exit modules".into());
        }
        let (snap_rally, extend_rally) = match name.as_str() {
            "DefaultProductionExitUpdate" => (false, true),
            "QueueProductionExitUpdate" => (true, true),
            "SupplyCenterProductionExitUpdate" => (false, false),
            _ => return Err(format!("unsupported production exit {name}").into()),
        };
        exit = Some(ProductionExit {
            use_spawn_rally_point: match field(module, 1, "UseSpawnRallyPoint")
                .map(scalar)
                .transpose()?
            {
                None => false,
                Some(value) if value.eq_ignore_ascii_case("Yes") => true,
                Some(value) if value.eq_ignore_ascii_case("No") => false,
                _ => return Err("invalid UseSpawnRallyPoint".into()),
            },
            create: exit_point(module, "UnitCreatePoint")?,
            rally: exit_point(module, "NaturalRallyPoint")?,
            snap_rally,
            extend_rally,
            queue: if name == "QueueProductionExitUpdate" {
                Some(generals_sim::skirmish::QueueExit {
                    allow_airborne_creation: match field(module, 1, "AllowAirborneCreation")
                        .map(scalar)
                        .transpose()?
                    {
                        None => false,
                        Some(value) if value.eq_ignore_ascii_case("Yes") => true,
                        Some(value) if value.eq_ignore_ascii_case("No") => false,
                        _ => return Err("invalid AllowAirborneCreation".into()),
                    },
                    delay_frames: production::exit_frames(module)?,
                    initial_burst: field(module, 1, "InitialBurst")
                        .map(scalar)
                        .transpose()?
                        .map(str::parse)
                        .transpose()?
                        .unwrap_or(0),
                })
            } else {
                None
            },
        });
    }
    if exit.is_none()
        && !parking
        && matches!(
            kind,
            Kind::CommandCenter | Kind::SupplyCenter | Kind::Barracks | Kind::Factory
        )
    {
        return Err("producing template lacks supported production exit module".into());
    }
    Ok(exit)
}

fn exit_point(lines: &[ContentLine], key: &str) -> Result<[f32; 3], Box<dyn Error>> {
    let Some(line) = field(lines, 1, key) else {
        // DefaultProductionExitUpdateModuleData initializes each Coord3D to zero.
        return Ok([0.0; 3]);
    };
    let component = |label: &str| -> Result<f32, Box<dyn Error>> {
        for (index, token) in line.tokens.iter().enumerate().skip(1) {
            if let Some(value) = token.strip_prefix(label) {
                let value = if value.is_empty() {
                    line.tokens
                        .get(index + 1)
                        .map(String::as_str)
                        .ok_or("coordinate lacks value")?
                } else {
                    value
                };
                let value = value.parse::<f32>()?;
                if !value.is_finite() {
                    return Err("nonfinite production exit coordinate".into());
                }
                return Ok(value);
            }
        }
        Err(format!("{key} lacks {label}").into())
    };
    Ok([component("X:")?, component("Y:")?, component("Z:")?])
}

fn validate_supply_spawn(object: &Declaration, faction: Faction) -> Result<(), Box<dyn Error>> {
    let expected = match faction {
        Faction::Usa => "AmericaVehicleChinook",
        Faction::China => "ChinaVehicleSupplyTruck",
        Faction::Gla => "GLAInfantryWorker",
        Faction::Civilian => return Err("neutral supply creation is unsupported".into()),
    };
    let mut found = false;
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "SpawnBehavior")
        {
            if found
                || unsigned(module, 1, "SpawnNumber")? != 1
                || text(module, 1, "SpawnTemplateName")? != expected
                || !text(module, 1, "OneShot")?.eq_ignore_ascii_case("Yes")
            {
                return Err("unsupported supply-center collector spawn".into());
            }
            found = true;
        }
    }
    if !found {
        return Err("supply center lacks its collector spawn".into());
    }
    Ok(())
}

fn weapon_aim_delta(lines: &[ContentLine]) -> Result<f32, Box<dyn Error>> {
    let Some(line) = field(lines, 0, "AcceptableAimDelta") else {
        return Ok(0.0);
    };
    // INI::parseAngleReal accepts signed scanReal, then multiplies by PI/180.
    let source = scalar(line)?;
    let angle: f32 = source.strip_suffix(['f', 'F']).unwrap_or(source).parse()?;
    let radians = angle * (std::f32::consts::PI / 180.0);
    if !radians.is_finite() {
        return Err("non-finite AcceptableAimDelta".into());
    }
    Ok(radians)
}

fn armor_coefficients(lines: &[ContentLine]) -> Result<[f32; DAMAGE_TYPE_COUNT], Box<dyn Error>> {
    // ArmorTemplate clears every coefficient to 1; DEFAULT resets all sequentially.
    let mut armor = [1.0; DAMAGE_TYPE_COUNT];
    for line in lines {
        if line.tokens.first().is_some_and(|key| key == "Armor") {
            let [_, damage, percent] = line.tokens.as_slice() else {
                return Err("malformed Armor coefficient".into());
            };
            let coefficient = finite_number(
                percent
                    .strip_suffix('%')
                    .ok_or("Armor coefficient lacks %")?,
            )? / 100.0;
            if damage.eq_ignore_ascii_case("DEFAULT") {
                armor.fill(coefficient);
            } else if let Some(kind) = damage_type(damage)
                && let Some(value) = armor.get_mut(kind as usize)
            {
                *value = coefficient;
            }
        }
    }
    Ok(armor)
}

fn damage_type(name: &str) -> Option<DamageType> {
    match name.to_ascii_uppercase().as_str() {
        "ARMOR_PIERCING" => Some(DamageType::ArmorPiercing),
        "SMALL_ARMS" => Some(DamageType::SmallArms),
        "INFANTRY_MISSILE" => Some(DamageType::InfantryMissile),
        "COMANCHE_VULCAN" => Some(DamageType::ComancheVulcan),
        "GATTLING" => Some(DamageType::Gattling),
        "SURRENDER" => Some(DamageType::Surrender),
        "EXPLOSION" => Some(DamageType::Explosion),
        "LASER" => Some(DamageType::Laser),
        "STATUS" => Some(DamageType::Status),
        "CRUSH" => Some(DamageType::Crush),
        "HAZARD_CLEANUP" => Some(DamageType::HazardCleanup),
        "POISON" => Some(DamageType::Poison),
        "SNIPER" => Some(DamageType::Sniper),
        "FLAME" => Some(DamageType::Flame),
        "JET_MISSILES" => Some(DamageType::JetMissiles),
        "RADIATION" => Some(DamageType::Radiation),
        "MICROWAVE" => Some(DamageType::Microwave),
        "KILL_PILOT" => Some(DamageType::KillPilot),
        "DISARM" => Some(DamageType::Disarm),
        "AURORA_BOMB" => Some(DamageType::AuroraBomb),
        "PARTICLE_BEAM" => Some(DamageType::ParticleBeam),
        "MELEE" => Some(DamageType::Melee),
        "STEALTHJET_MISSILES" => Some(DamageType::StealthjetMissiles),
        "MOLOTOV_COCKTAIL" => Some(DamageType::MolotovCocktail),
        "UNRESISTABLE" => Some(DamageType::Unresistable),
        "FALLING" => Some(DamageType::Falling),
        "WATER" => Some(DamageType::Water),
        "HEALING" => Some(DamageType::Healing),
        "PENALTY" => Some(DamageType::Penalty),
        "HACK" => Some(DamageType::Hack),
        "DEPLOY" => Some(DamageType::Deploy),
        "TOPPLING" => Some(DamageType::Toppling),
        "LAND_MINE" => Some(DamageType::LandMine),
        "SUBDUAL_BUILDING" => Some(DamageType::SubdualBuilding),
        "SUBDUAL_VEHICLE" => Some(DamageType::SubdualVehicle),
        "SUBDUAL_MISSILE" => Some(DamageType::SubdualMissile),
        "SUBDUAL_UNRESISTABLE" => Some(DamageType::SubdualUnresistable),
        "KILL_GARRISONED" => Some(DamageType::KillGarrisoned),
        _ => None,
    }
}

pub fn ai_module(name: &str) -> bool {
    matches!(
        name,
        "WanderAIUpdate"
            | "MissileAIUpdate"
            | "AIUpdateInterface"
            | "TransportAIUpdate"
            | "AssaultTransportAIUpdate"
            | "DozerAIUpdate"
            | "WorkerAIUpdate"
            | "SupplyTruckAIUpdate"
            | "ChinookAIUpdate"
            | "JetAIUpdate"
            | "DeliverPayloadAIUpdate"
            | "DeployStyleAIUpdate"
            | "HackInternetAIUpdate"
    )
}

fn unit_ai(object: &Declaration) -> Result<UnitAi, Box<dyn Error>> {
    // ThingTemplate and AIUpdateModuleData constructor defaults.
    let mut ai = UnitAi {
        update: generals_sim::skirmish::UnitAiUpdate::Absent,
        vision_range: optional_number(&object.lines, "VisionRange", 0.0)?,
        acquisition: 0,
        mood_check_frames: 60,
    };
    for module in blocks(&object.lines, 0, "Behavior")? {
        let Some(name) = module[0].tokens.get(1) else {
            continue;
        };
        if !ai_module(name) {
            continue;
        }
        ai.update = match name.as_str() {
            "WanderAIUpdate" => generals_sim::skirmish::UnitAiUpdate::Wander,
            "DozerAIUpdate" => generals_sim::skirmish::UnitAiUpdate::Dozer,
            "WorkerAIUpdate" => generals_sim::skirmish::UnitAiUpdate::Worker,
            "SupplyTruckAIUpdate" | "ChinookAIUpdate" => {
                generals_sim::skirmish::UnitAiUpdate::SupplyTruck
            }
            _ => generals_sim::skirmish::UnitAiUpdate::Ordinary,
        };
        if let Some(line) = field(module, 1, "MoodAttackCheckRate") {
            ai.mood_check_frames = duration_frames(line)?;
        }
        if let Some(line) = field(module, 1, "AutoAcquireEnemiesWhenIdle") {
            for flag in &line.tokens[1..] {
                match flag.to_ascii_uppercase().as_str() {
                    "YES" => ai.acquisition |= UnitAi::IDLE,
                    "ATTACK_BUILDINGS" => ai.acquisition |= UnitAi::BUILDINGS,
                    "NOTWHILEATTACKING" => ai.acquisition |= UnitAi::NOT_WHILE_ATTACKING,
                    "STEALTHED" => ai.acquisition |= UnitAi::STEALTHED,
                    "NO" => ai.acquisition |= UnitAi::NO,
                    _ => return Err(format!("unknown auto-acquire flag {flag}").into()),
                }
            }
        }
    }
    Ok(ai)
}

pub fn duration_frames(line: &ContentLine) -> Result<u32, Box<dyn Error>> {
    // All active AI millisecond fields are integer-valued in the installed INIs.
    let milliseconds: u32 = scalar(line)?.parse()?;
    Ok(u32::try_from(
        (u64::from(milliseconds) * 30).div_ceil(1000),
    )?)
}

fn ai_rules(assets: &VirtualAssets) -> Result<AiRules, Box<dyn Error>> {
    let mut rules = AiRules::default();
    // AI::init loads the defaults first, then the installation's override file.
    for path in ["data/ini/default/aidata.ini", "data/ini/aidata.ini"] {
        let file = content::scan(assets.read(path)?)?;
        if file.declarations.is_empty()
            && file.unclassified.is_empty()
            && file.diagnostics.is_empty()
        {
            continue;
        }
        let data = definition(std::slice::from_ref(&file), "AIData", "")?;
        if let Some(line) = field(&data.lines, 0, "EnableRepulsors") {
            rules.repulsors_enabled = match scalar(line)?.to_ascii_lowercase().as_str() {
                "yes" => true,
                "no" => false,
                _ => return Err("invalid AIData EnableRepulsors".into()),
            };
        }
        if let Some(line) = field(&data.lines, 0, "WallHeight") {
            rules.wall_height = finite_number(scalar(line)?)?;
        }
        if let Some(line) = field(&data.lines, 0, "RepulsedDistance") {
            rules.repulsed_distance = finite_number(scalar(line)?)?;
        }
        if let Some(line) = field(&data.lines, 0, "AICrushesInfantry") {
            rules.crushes_infantry = match scalar(line)?.to_ascii_lowercase().as_str() {
                "yes" => true,
                "no" => false,
                _ => return Err("invalid AIData AICrushesInfantry".into()),
            };
        }
        if let Some(line) = field(&data.lines, 0, "AttackUsesLineOfSight") {
            rules.attack_uses_line_of_sight = match scalar(line)?.to_ascii_lowercase().as_str() {
                "yes" => true,
                "no" => false,
                _ => return Err("invalid AIData AttackUsesLineOfSight".into()),
            };
        }
        for (name, value) in [
            ("ForceIdleMSEC", &mut rules.force_idle_frames),
            ("GuardChaseUnitsDuration", &mut rules.guard_chase_frames),
            ("GuardEnemyScanRate", &mut rules.guard_scan_frames),
            (
                "GuardEnemyReturnScanRate",
                &mut rules.guard_return_scan_frames,
            ),
        ] {
            if let Some(line) = field(&data.lines, 0, name) {
                *value = duration_frames(line)?;
            }
        }
        for (name, value) in [
            (
                "MaxRetaliationDistance",
                &mut rules.max_retaliation_distance,
            ),
            (
                "RetaliationFriendsRadius",
                &mut rules.retaliation_friends_radius,
            ),
            ("GuardInnerModifierHuman", &mut rules.guard_inner_human),
            ("GuardOuterModifierHuman", &mut rules.guard_outer_human),
            ("GuardInnerModifierAI", &mut rules.guard_inner_ai),
            ("GuardOuterModifierAI", &mut rules.guard_outer_ai),
            ("MinDistanceForGroup", &mut rules.min_distance_for_group),
            ("DistanceRequiresGroup", &mut rules.distance_requires_group),
            (
                "AIDozerBoredRadiusModifier",
                &mut rules.dozer_bored_radius_modifier,
            ),
        ] {
            if let Some(line) = field(&data.lines, 0, name) {
                *value = finite_number(scalar(line)?)?;
            }
        }
        for (name, value) in [
            ("MinInfantryForGroup", &mut rules.min_infantry_for_group),
            ("MinVehiclesForGroup", &mut rules.min_vehicles_for_group),
        ] {
            if let Some(line) = field(&data.lines, 0, name) {
                *value = scalar(line)?.parse()?;
            }
        }
    }
    let game = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let data = definition(std::slice::from_ref(&game), "GameData", "")?;
    rules.unit_damaged_threshold = number(&data.lines, 0, "UnitDamagedThreshold")?;
    rules.group_move_gather_factor = optional_number(
        &data.lines,
        "GroupMoveClickToGatherAreaFactor",
        rules.group_move_gather_factor,
    )?;
    Ok(rules)
}

pub fn normal_model(assets: &VirtualAssets, object: &Declaration) -> Result<Model, Box<dyn Error>> {
    // The normal state also supplies the placement preview and neutral map model.
    let draws = blocks(&object.lines, 0, "Draw")?;
    let draw = draws.first().ok_or("object has no Draw module")?;
    let debris = object.name == "GenericDebris"
        && draw[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "W3DDebrisDraw");
    let default_draw = draw[0]
        .tokens
        .get(1)
        .is_some_and(|name| name == "W3DDefaultDraw");
    if default_draw && draw.len() != 2 {
        return Err("W3DDefaultDraw requires its original empty draw module".into());
    }
    let laser = draw[0]
        .tokens
        .get(1)
        .is_some_and(|family| family == "W3DLaserDraw");
    if default_draw
        || laser
        || fire::OBJECTS.contains(&object.name.as_str())
        || (draw[0]
            .tokens
            .get(1)
            .is_some_and(|family| family == "W3DModelDraw")
            && field(normal_state(draw)?, 2, "Model")
                .map(scalar)
                .transpose()?
                .is_some_and(|name| name.eq_ignore_ascii_case("NONE")))
        || object.name == "GLAInfantryAngryMobNexus"
        || debris
    {
        if draws.len() != 1
            || (!debris
                && !default_draw
                && !laser
                && !text(normal_state(draw)?, 2, "Model")?.eq_ignore_ascii_case("NONE"))
        {
            return Err("fire object must retain original invisible draw".into());
        }
        return Ok(Model {
            hlod: false,
            emitter: None,
            scene_sphere: None,
            scene_bounds: generals_assets::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            meshes: Vec::new(),
            bone_names: Vec::new(),
            hierarchy: None,
            bounds: generals_assets::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
        });
    }
    let model = normal_draw_model(assets, draw)?;
    if model.emitter.is_some() {
        crate::w3d_emitter::validate_object(object)?;
    }
    Ok(model)
}

pub fn normal_draw_model(
    assets: &VirtualAssets,
    draw: &[ContentLine],
) -> Result<Model, Box<dyn Error>> {
    let tree = draw[0]
        .tokens
        .get(1)
        .is_some_and(|module| module == "W3DTreeDraw");
    let direct = tree
        || draw[0]
            .tokens
            .get(1)
            .is_some_and(|module| module == "W3DPropDraw");
    let state = if direct { draw } else { normal_state(draw)? };
    let name = if direct {
        text(draw, 1, "ModelName")?
    } else {
        text(state, 2, "Model")?
    };
    let mut model = state_model(assets, name, state)?;
    if let Some(definition) = &model.emitter {
        crate::w3d_emitter::validate_draw(draw, definition)?;
    }
    hide_idle_effects(&mut model, state);
    if tree {
        let texture = text(draw, 1, "TextureName")?;
        for mesh in &mut model.meshes {
            if let Some(materials) = &mut mesh.materials {
                for entry in &mut materials.textures {
                    texture.clone_into(&mut entry.name);
                }
            }
        }
    }
    Ok(model)
}

pub fn state_model(
    assets: &VirtualAssets,
    name: &str,
    state: &[ContentLine],
) -> Result<Model, Box<dyn Error>> {
    if name.eq_ignore_ascii_case("None") {
        return Err("primary normal model is invisible".into());
    }
    // WW3DAssetManager::Find_Prototype always supplies the built-in NULL object.
    if name.eq_ignore_ascii_case("NULL") {
        return Ok(Model::null());
    }
    let mut model = Model::load(assets, name)?;
    if let Some(definition) = &model.emitter {
        crate::w3d_emitter::validate(definition)?;
    }
    for (name, hidden) in subobject_visibility(state) {
        model.set_subobject_hidden(name, hidden);
    }
    for mesh in &mut model.meshes {
        if mesh.name.contains("HEADLIGHT") {
            mesh.attributes |= 0x1000;
        }
    }
    Ok(model)
}

fn subobject_visibility(state: &[ContentLine]) -> Vec<(&str, bool)> {
    let mut entries: Vec<(&str, bool)> = Vec::new();
    // W3DModelDraw::parseShowHideSubObject updates duplicate names in place;
    // first insertion order controls later parent/child visibility overrides.
    for line in state {
        let Some(key) = line.tokens.first() else {
            continue;
        };
        if !matches!(key.as_str(), "HideSubObject" | "ShowSubObject") {
            continue;
        }
        if line
            .tokens
            .get(1)
            .is_some_and(|name| name.eq_ignore_ascii_case("NONE"))
        {
            entries.clear();
            continue;
        }
        for name in &line.tokens[1..] {
            let hidden = key == "HideSubObject";
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| entry.0.eq_ignore_ascii_case(name))
            {
                entry.1 = hidden;
            } else {
                entries.push((name, hidden));
            }
        }
    }
    entries
}

// W3DModelDraw::validateWeaponBarrelInfo/hideAllMuzzleFlashes and
// hideAllHeadlights at the pinned SuperHackers revision. Placement and neutral
// models hide flashes at load; animated units retain them for per-shot visibility.
fn hide_idle_effects(model: &mut Model, state: &[ContentLine]) {
    let mut hidden = Vec::new();
    for slot in ["PRIMARY", "SECONDARY", "TERTIARY"] {
        let names = [
            "WeaponFireFXBone",
            "WeaponRecoilBone",
            "WeaponMuzzleFlash",
            "WeaponLaunchBone",
        ]
        .map(|key| {
            state
                .iter()
                .rev()
                .find(|line| {
                    line.depth == Some(2)
                        && line.tokens.first().is_some_and(|value| value == key)
                        && line
                            .tokens
                            .get(1)
                            .is_some_and(|value| value.eq_ignore_ascii_case(slot))
                })
                .and_then(|line| line.tokens.get(2))
        });
        let public = names.map(|name| name.map(|name| model.public_bone_indices(name)));
        let find_bone = |role: usize, name: &str, numbered: bool| {
            let index = *public[role].as_ref()?.get(&name.to_ascii_lowercase())?;
            (index != 0 || (!numbered && role == 3)).then(|| &model.bone_names[index])
        };
        let mut numbered = false;
        for index in 1..=99 {
            let bones: [Option<&String>; 4] = std::array::from_fn(|role| {
                names[role].and_then(|name| find_bone(role, &format!("{name}{index:02}"), true))
            });
            if bones.iter().all(Option::is_none) {
                break;
            }
            numbered = true;
            if let Some(bone) = bones[2] {
                hidden.push(bone.as_str());
            }
        }
        if !numbered && let Some(bone) = names[2].and_then(|name| find_bone(2, name, false)) {
            hidden.push(bone.as_str());
        }
    }
    for bone in hidden {
        if let Some(mesh) = model
            .meshes
            .iter_mut()
            .find(|mesh| mesh.bone_name.as_deref() == Some(bone))
        {
            mesh.attributes |= 0x1000;
        }
    }
    for mesh in &mut model.meshes {
        if mesh.name.contains("HEADLIGHT") {
            mesh.attributes |= 0x1000;
        }
    }
}

fn normal_state(draw: &[ContentLine]) -> Result<&[ContentLine], Box<dyn Error>> {
    if let Some(default) = blocks(draw, 1, "DefaultConditionState")?.first() {
        return Ok(*default);
    }
    blocks(draw, 1, "ConditionState")?
        .into_iter()
        .find(|state| {
            state[0]
                .tokens
                .get(1)
                .is_some_and(|value| value.eq_ignore_ascii_case("NONE"))
        })
        .ok_or_else(|| "primary Draw has no normal condition state".into())
}

pub fn blocks<'a>(
    lines: &'a [ContentLine],
    depth: u32,
    key: &str,
) -> Result<Vec<&'a [ContentLine]>, Box<dyn Error>> {
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.depth == Some(depth) && line.tokens.first().is_some_and(|token| token == key) {
            let end = lines[index + 1..]
                .iter()
                .position(|line| {
                    line.depth == Some(depth)
                        && line
                            .tokens
                            .first()
                            .is_some_and(|token| token.eq_ignore_ascii_case("End"))
                })
                .ok_or_else(|| format!("unclosed {key} block at line {}", line.line))?;
            found.push(&lines[index..=index + 1 + end]);
        }
    }
    Ok(found)
}

fn normal_set<'a>(
    lines: &'a [ContentLine],
    key: &str,
) -> Result<Option<&'a [ContentLine]>, Box<dyn Error>> {
    let mut found = None;
    for block in blocks(lines, 0, key)? {
        // ArmorTemplateSet/WeaponTemplateSet::clear start with no condition bits.
        if field(block, 1, "Conditions").is_none_or(|conditions| {
            conditions.tokens[1..]
                .iter()
                .all(|value| value.eq_ignore_ascii_case("NONE"))
        }) {
            if found.is_some() {
                return Err(format!("multiple normal {key} blocks").into());
            }
            found = Some(block);
        }
    }
    Ok(found)
}

fn selected_pair<'a>(
    lines: &'a [ContentLine],
    depth: u32,
    key: &str,
    selection: &str,
) -> Result<Option<&'a str>, Box<dyn Error>> {
    let mut found = None;
    for line in lines {
        if line.depth == Some(depth)
            && line.tokens.first().is_some_and(|token| token == key)
            && line.tokens.get(1).is_some_and(|token| token == selection)
        {
            if found.is_some() || line.tokens.len() != 3 {
                return Err(format!("ambiguous {key} {selection}").into());
            }
            found = Some(line.tokens[2].as_str());
        }
    }
    Ok(found)
}

pub fn field<'a>(lines: &'a [ContentLine], depth: u32, key: &str) -> Option<&'a ContentLine> {
    // INI::initFromINI dispatches fields sequentially; scalar parsers overwrite prior values.
    lines.iter().rev().find(|line| {
        line.depth == Some(depth) && line.tokens.first().is_some_and(|token| token == key)
    })
}

pub fn scalar(line: &ContentLine) -> Result<&str, Box<dyn Error>> {
    let [_, value] = line.tokens.as_slice() else {
        return Err(format!("expected scalar at line {}: {:?}", line.line, line.tokens).into());
    };
    Ok(value)
}

pub fn text<'a>(
    lines: &'a [ContentLine],
    depth: u32,
    key: &str,
) -> Result<&'a str, Box<dyn Error>> {
    scalar(field(lines, depth, key).ok_or_else(|| format!("missing {key}"))?)
}

pub fn unsigned(lines: &[ContentLine], depth: u32, key: &str) -> Result<u32, Box<dyn Error>> {
    Ok(text(lines, depth, key)?.parse()?)
}

pub fn finite_number(text: &str) -> Result<f32, Box<dyn Error>> {
    let value = source_real(text)?;
    if value < 0.0 {
        return Err(format!("invalid nonnegative source number {text}").into());
    }
    Ok(value)
}

pub fn source_real(text: &str) -> Result<f32, Box<dyn Error>> {
    // Pinned C++20 INI::scanReal/scanType ignores from_chars' trailing-text pointer.
    let token = text.strip_prefix('+').unwrap_or(text);
    let bytes = token.as_bytes();
    let mut end = usize::from(bytes.first() == Some(&b'-'));
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    let mantissa_end = end;
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent_end = end + 1;
        if matches!(bytes.get(exponent_end), Some(b'+' | b'-')) {
            exponent_end += 1;
        }
        let digits_start = exponent_end;
        while bytes.get(exponent_end).is_some_and(u8::is_ascii_digit) {
            exponent_end += 1;
        }
        // from_chars leaves an incomplete exponent outside the consumed prefix.
        if exponent_end > digits_start {
            end = exponent_end;
        }
    }
    let value: f32 = token[..end].parse()?;
    let underflow = value == 0.0
        && bytes[..mantissa_end]
            .iter()
            .any(|byte| matches!(byte, b'1'..=b'9'));
    if !value.is_finite() || underflow {
        return Err(format!("invalid finite source number {text}").into());
    }
    Ok(value)
}

pub fn number(lines: &[ContentLine], depth: u32, key: &str) -> Result<f32, Box<dyn Error>> {
    finite_number(text(lines, depth, key)?)
}

fn duration(lines: &[ContentLine], depth: u32, key: &str) -> Result<f32, Box<dyn Error>> {
    // INI::parseDurationUnsignedInt and WeaponTemplate::parseShotDelay round up to logic frames.
    Ok((number(lines, depth, key)? * 30.0 / 1000.0).ceil() / 30.0)
}

fn optional_text<'a>(lines: &'a [ContentLine], key: &str) -> Result<&'a str, Box<dyn Error>> {
    field(lines, 0, key).map_or(Ok(""), scalar)
}

pub fn optional_number(
    lines: &[ContentLine],
    key: &str,
    default: f32,
) -> Result<f32, Box<dyn Error>> {
    field(lines, 0, key).map_or(Ok(default), |line| finite_number(scalar(line)?))
}

fn placement_angle(lines: &[ContentLine]) -> Result<f32, Box<dyn Error>> {
    let Some(line) = field(lines, 0, "PlacementViewAngle") else {
        return Ok(0.0);
    };
    let angle: f32 = scalar(line)?.parse()?;
    if !angle.is_finite() {
        return Err("non-finite PlacementViewAngle".into());
    }
    Ok(angle.to_radians())
}

// AI::init reads defaults, then installed/map-effective AIData. TAiData defaults false.
fn module_repulsors_enabled(assets: &VirtualAssets) -> Result<bool, Box<dyn Error>> {
    let mut enabled = false;
    for path in ["data/ini/default/aidata.ini", "data/ini/aidata.ini"] {
        let file = content::scan(assets.read(path)?)?;
        for declaration in &file.declarations {
            if declaration.kind != "AIData" {
                continue;
            }
            if let Some(line) = field(&declaration.lines, 0, "EnableRepulsors") {
                enabled = match scalar(line)?.to_ascii_lowercase().as_str() {
                    "yes" => true,
                    "no" => false,
                    _ => return Err("invalid AIData EnableRepulsors".into()),
                };
            }
        }
    }
    Ok(enabled)
}

fn disabled_sounds(files: &[ContentFile]) -> Result<[String; 4], Box<dyn Error>> {
    let misc = definition(files, "MiscAudio", "")?;
    let keys = [
        "BuildingDisabled",
        "BuildingReenabled",
        "VehicleDisabled",
        "VehicleReenabled",
    ];
    let values = keys.map(|key| {
        field(&misc.lines, 0, key)
            .map(scalar)
            .transpose()
            .map(|value| value.unwrap_or_default().to_owned())
    });
    let [
        building_disabled,
        building_enabled,
        vehicle_disabled,
        vehicle_enabled,
    ] = values;
    Ok([
        building_disabled?,
        building_enabled?,
        vehicle_disabled?,
        vehicle_enabled?,
    ])
}

#[cfg(test)]
mod tests {
    use super::{
        DamageType, armor_coefficients, definition, finite_number, placement_angle, unit_ai,
    };
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn omitted_projectile_armor_conditions_keep_the_native_empty_mask() {
        let source = "Object GenericTankShell\nArmorSet\nArmor=ProjectileArmor\nEnd\nArmorSet\nConditions=PLAYER_UPGRADE\nArmor=UpgradedArmor\nEnd\nEnd\n";
        let parse = |source: &str| {
            content::scan(Asset {
                path: "projectile.ini".into(),
                origin: "original projectile armor contract".into(),
                bytes: source.as_bytes().to_vec(),
            })
            .unwrap()
        };
        let file = parse(source);
        let set = super::normal_set(&file.declarations[0].lines, "ArmorSet")
            .unwrap()
            .unwrap();
        assert_eq!(super::text(set, 1, "Armor").unwrap(), "ProjectileArmor");
        let ambiguous = parse(&source.replace("Conditions=PLAYER_UPGRADE", "Conditions=None"));
        assert!(super::normal_set(&ambiguous.declarations[0].lines, "ArmorSet").is_err());
    }

    #[test]
    fn subobject_visibility_preserves_first_insertion_order_and_none_reset() {
        let parse = |source: &str| {
            content::scan(Asset {
                path: "draw.ini".into(),
                origin: "fixture".into(),
                bytes: format!("Object Test\n{source}\nEnd\n").into_bytes(),
            })
            .unwrap()
        };
        let file =
            parse("HideSubObject=Chassis\nHideSubObject=Barrel MuzzleFX\nShowSubObject=CHASSIS");
        assert_eq!(
            super::subobject_visibility(&file.declarations[0].lines),
            [("Chassis", false), ("Barrel", true), ("MuzzleFX", true)]
        );
        let file = parse("HideSubObject=Chassis\nShowSubObject=None\nHideSubObject=Barrel");
        assert_eq!(
            super::subobject_visibility(&file.declarations[0].lines),
            [("Barrel", true)]
        );
    }

    #[test]
    fn production_exit_omitted_coordinates_keep_native_zero_defaults_without_creating_a_module() {
        use generals_sim::skirmish::Kind;
        let parse = |fields: &str| {
            content::scan(Asset { path: "exit.ini".into(), origin: "DefaultProductionExitUpdateModuleData contract".into(),
            bytes: format!("Object Factory\nBehavior = DefaultProductionExitUpdate Exit\n{fields}End\nEnd\n").into_bytes() }).unwrap()
        };
        for (fields, create, rally) in [
            ("", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            (
                "NaturalRallyPoint=X:0 Y:-60 Z:0\n",
                [0.0, 0.0, 0.0],
                [0.0, -60.0, 0.0],
            ),
            (
                "UnitCreatePoint=X:-18 Y:40 Z:0\n",
                [-18.0, 40.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ] {
            let file = parse(fields);
            let exit = super::production_exit(&file.declarations[0], Kind::Factory)
                .unwrap()
                .unwrap();
            assert_eq!(exit.create.map(f32::to_bits), create.map(f32::to_bits));
            assert_eq!(exit.rally.map(f32::to_bits), rally.map(f32::to_bits));
        }
        for field in [
            "NaturalRallyPoint=X:3 Y:4 Z:12\n",
            "UnitCreatePoint=X:0 Y:0 Z:12\n",
        ] {
            let file = parse(field);
            let exit = super::production_exit(&file.declarations[0], Kind::Factory)
                .unwrap()
                .unwrap();
            assert_eq!(
                if field.starts_with("Natural") {
                    exit.rally[2]
                } else {
                    exit.create[2]
                }
                .to_bits(),
                12.0_f32.to_bits()
            );
        }
        let malformed = parse("UnitCreatePoint=X:0 Y:0\n");
        assert!(super::production_exit(&malformed.declarations[0], Kind::Factory).is_err());
        let file = content::scan(Asset {
            path: "no-exit.ini".into(),
            origin: "Absent exit module".into(),
            bytes: b"Object Factory\nEnd\n".to_vec(),
        })
        .unwrap();
        assert!(super::production_exit(&file.declarations[0], Kind::Factory).is_err());
        assert!(
            super::production_exit(&file.declarations[0], Kind::Vehicle)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn weapon_aim_angle_preserves_signed_degrees_and_rejects_nonfinite_values() {
        for (source, expected) in [
            ("", 0.0),
            (
                "AcceptableAimDelta=-30f",
                -30.0 * (std::f32::consts::PI / 180.0),
            ),
            (
                "AcceptableAimDelta=90",
                90.0 * (std::f32::consts::PI / 180.0),
            ),
        ] {
            let file = content::scan(Asset {
                path: "weapon.ini".into(),
                origin: "fixture".into(),
                bytes: format!("Weapon Test\n{source}\nEnd\n").into_bytes(),
            })
            .unwrap();
            let actual = super::weapon_aim_delta(&file.declarations[0].lines).unwrap();
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        let file = content::scan(Asset {
            path: "weapon.ini".into(),
            origin: "fixture".into(),
            bytes: b"Weapon Test\nAcceptableAimDelta=NaN\nEnd\n".to_vec(),
        })
        .unwrap();
        assert!(super::weapon_aim_delta(&file.declarations[0].lines).is_err());
    }

    #[test]
    fn locomotor_surface_lists_preserve_source_bitmask_updates() {
        let parse = |fields: &str| {
            let file = content::scan(Asset {
                path: "locomotor.ini".into(),
                origin: "fixture".into(),
                bytes: format!("Locomotor Test\n{fields}\nEnd\n").into_bytes(),
            })
            .unwrap();
            super::locomotor_surface(&file.declarations[0].lines)
        };
        for source in [
            "Surfaces = GROUND",
            "Surfaces = GROUND RUBBLE",
            "Surfaces = GROUND RUBBLE\nSurfaces = -RUBBLE",
        ] {
            assert!(matches!(parse(source), Ok(super::MovementSurface::Ground)));
        }
        assert!(matches!(
            parse("Surfaces = GROUND RUBBLE\nSurfaces = AIR"),
            Ok(super::MovementSurface::Air)
        ));
        assert!(matches!(
            parse("Surfaces = GROUND\nSurfaces = -GROUND +AIR"),
            Ok(super::MovementSurface::Air)
        ));
        for source in [
            "Surfaces = GROUND AIR",
            "Surfaces = GROUND WATER",
            "Surfaces = GROUND CLIFF",
            "Surfaces = RUBBLE",
            "Surfaces = GROUND +AIR",
            "Surfaces = NONE AIR",
            "Surfaces = UNKNOWN",
        ] {
            assert!(parse(source).is_err(), "unexpectedly accepted {source}");
        }
    }

    #[test]
    fn normal_state_hides_attached_muzzle_geometry_and_day_headlights() {
        use generals_assets::w3d::{Bounds, Mesh, Model};
        let file = content::scan(Asset {
            path: "object.ini".into(), origin: "fixture".into(),
            bytes: b"Object Vehicle\nDraw=W3DTruckDraw Test\nConditionState=NONE\nWeaponMuzzleFlash=PRIMARY Flash\nEnd\nEnd\nEnd\n".to_vec(),
        }).expect("source state");
        let draw = super::blocks(&file.declarations[0].lines, 0, "Draw").unwrap()[0];
        let state = super::normal_state(draw).unwrap();
        let mesh = |name: &str, bone: &str| Mesh {
            name: name.into(),
            bone_name: Some(bone.into()),
            attributes: 0,
            sort_level: 0,
            subobject_order: 0,
            bounds: generals_assets::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            sphere_center: [0.0; 3],
            sphere_radius: 1.0,
            bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
            positions: vec![[0.0; 3]],
            indices: Vec::new(),
            normals: None,
            materials: None,
            deformation: None,
        };
        let mut model = Model {
            hlod: false,
            emitter: None,
            scene_sphere: None,
            scene_bounds: generals_assets::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            meshes: vec![
                mesh("VEHICLE.EFFECT", "Flash01"),
                mesh("VEHICLE.Flash01", "Turret"),
                mesh("VEHICLE.HEADLIGHTS", "Light"),
            ],
            bone_names: vec![
                "ROOT".into(),
                "Flash01".into(),
                "Turret".into(),
                "Light".into(),
            ],
            bounds: Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            hierarchy: None,
        };
        super::hide_idle_effects(&mut model, state);
        assert_ne!(model.meshes[0].attributes & 0x1000, 0);
        assert_eq!(model.meshes[1].attributes & 0x1000, 0);
        assert_ne!(model.meshes[2].attributes & 0x1000, 0);
    }

    #[test]
    fn later_scalar_assignment_overwrites_original_value() {
        let file = content::scan(Asset {
            path: "object.ini".into(),
            origin: "fixture".into(),
            bytes: b"Object HQ\nPlacementViewAngle=0\nPlacementViewAngle=90\nEnd\n".to_vec(),
        })
        .expect("object fixture");
        let angle = placement_angle(&file.declarations[0].lines).expect("repeated scalar is valid");
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < f32::EPSILON);
    }

    #[test]
    fn armor_default_resets_earlier_specialized_coefficients() {
        let file = content::scan(Asset { path: "armor.ini".into(), origin: "fixture".into(), bytes: b"Armor Tank\nArmor=ARMOR_PIERCING 50%\nArmor=DEFAULT 100%\nArmor=SMALL_ARMS 10%\nEnd\n".to_vec() }).expect("armor fixture");
        let armor = armor_coefficients(&file.declarations[0].lines).expect("ordered coefficients");
        assert!((armor[0] - 1.0).abs() < f32::EPSILON);
        assert!((armor[1] - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn armor_defaults_and_overrides_cover_special_damage_types_in_source_order() {
        let file = content::scan(Asset {
            path: "armor.ini".into(), origin: "source armor fixture".into(),
            bytes: b"Armor Tested\nArmor=POISON 0%\nArmor=Default 75%\nArmor=LASER 20%\nArmor=HAZARD_CLEANUP 0%\nArmor=FLAME 50%\nArmor=JET_MISSILES 25%\nArmor=RADIATION 10%\nArmor=MICROWAVE 15%\nArmor=WATER 40%\nEnd\n".to_vec(),
        }).unwrap();
        let armor = armor_coefficients(&file.declarations[0].lines).unwrap();
        for (kind, expected) in [
            (DamageType::Poison, 0.75_f32),
            (DamageType::Laser, 0.2),
            (DamageType::HazardCleanup, 0.0),
            (DamageType::Flame, 0.5),
            (DamageType::JetMissiles, 0.25),
            (DamageType::Radiation, 0.1),
            (DamageType::Microwave, 0.15),
            (DamageType::Water, 0.4),
        ] {
            assert_eq!(armor[kind as usize].to_bits(), expected.to_bits());
        }
    }

    #[test]
    fn dozer_interface_identity_requires_the_authored_dozer_or_worker_ai_module() {
        for (module, dozer, collector) in [
            ("DozerAIUpdate", true, false),
            ("WorkerAIUpdate", true, true),
            ("SupplyTruckAIUpdate", false, true),
            ("ChinookAIUpdate", false, true),
            ("AIUpdateInterface", false, false),
        ] {
            let file = content::scan(Asset {
                path: "object.ini".into(),
                origin: "native dozer interface fixture".into(),
                bytes: format!(
                    "Object Builder\nKindOf=DOZER\nBehavior={module} ModuleTag_AI\nEnd\nEnd\n"
                )
                .into_bytes(),
            })
            .unwrap();
            let ai = unit_ai(&file.declarations[0]).unwrap();
            assert_eq!(
                matches!(
                    ai.update,
                    generals_sim::skirmish::UnitAiUpdate::Dozer
                        | generals_sim::skirmish::UnitAiUpdate::Worker
                ),
                dozer
            );
            assert_eq!(
                matches!(
                    ai.update,
                    generals_sim::skirmish::UnitAiUpdate::SupplyTruck
                        | generals_sim::skirmish::UnitAiUpdate::Worker
                ),
                collector
            );
        }
    }

    #[test]
    fn ai_flags_keep_yes_with_no_and_round_millisecond_checks_up() {
        let file = content::scan(Asset {
            path: "object.ini".into(), origin: "fixture".into(),
            bytes: b"Object Tank\nVisionRange=150\nBehavior=AIUpdateInterface ModuleTag_AI\nAutoAcquireEnemiesWhenIdle=Yes No ATTACK_BUILDINGS NotWhileAttacking\nMoodAttackCheckRate=67\nEnd\nEnd\n".to_vec(),
        }).expect("AI fixture");
        let ai = unit_ai(&file.declarations[0]).expect("native AI settings");
        assert!(ai.auto_acquire() && ai.auto_acquire_buildings() && ai.not_while_attacking());
        assert_eq!(ai.mood_check_frames, 3);
    }

    #[test]
    fn refuses_ambiguous_definitions_and_nonfinite_source_stats() {
        let files = [content::scan(Asset {
            path: "object.ini".into(),
            origin: "fixture".into(),
            bytes: b"Object Tank\nEnd\nObject Tank\nEnd\n".to_vec(),
        })
        .expect("duplicated fixture")];
        assert!(definition(&files, "Object", "Tank").is_err());
        assert!(finite_number("NaN").is_err());
        assert!(finite_number("inf").is_err());
        assert!(finite_number("-1").is_err());
    }

    #[test]
    fn source_real_prefix_preserves_installed_mogadishu_house_geometry() {
        // Authored dimensions isolate the invalid suffix seen in installed geometry.
        let file = content::scan(Asset {
            path: "numeric-prefix.ini".into(),
            origin: "authored numeric-prefix geometry".into(),
            bytes: b"Object PrefixBox\nGeometry=BOX\nGeometryMajorRadius=5.0\nGeometryMinorRadius=12.0\nGeometryHeight=40v.0\nEnd\n".to_vec(),
        })
        .expect("authored geometry fields");
        let bounds = super::support::geometry_bounds(&file.declarations[0].lines)
            .expect("source numeric prefix");
        assert_eq!(bounds.center_height.to_bits(), 20.0_f32.to_bits());
        assert_eq!(bounds.sphere_radius.to_bits(), 569.0_f32.sqrt().to_bits());
        assert_eq!(bounds.max_height.to_bits(), 40.0_f32.to_bits());
    }

    #[test]
    fn source_real_prefix_keeps_sign_decimal_and_complete_exponent() {
        for (token, expected) in [
            ("100.0f", 100.0_f32),
            ("+.5tail", 0.5),
            ("2.e+2tail", 200.0),
            ("40e-", 40.0),
            ("40e+v", 40.0),
            ("40v.0", 40.0),
            ("+-0tail", -0.0),
            ("-0e-999tail", -0.0),
        ] {
            assert_eq!(finite_number(token).unwrap().to_bits(), expected.to_bits());
        }
        for token in [
            "v40",
            "+",
            "++1",
            ".",
            "e2",
            "-1tail",
            "1e100tail",
            "1e-999",
            "-1e-999",
        ] {
            assert!(finite_number(token).is_err(), "{token}");
        }
    }
}

#[cfg(test)]
mod model_loading_tests;
