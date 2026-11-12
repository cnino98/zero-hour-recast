//! Playtest skirmish adapter using installed stats and selected source update rules.
//!
//! Source baseline: `SuperHackers` 2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed.
//! Grid routes and straight projectile travel remain incomplete translations.
//! The represented computer player reads original skirmish scripts and `AIData`.

use std::collections::VecDeque;

use crate::{Cell, Grid};

mod ai;
mod ai_goals;
pub mod aircraft;
pub mod aircraft_death;
mod assault_transport;
mod attack_view;
pub mod auto_find_healing;
pub mod battle_bus;
pub mod battle_plan;
mod behavior_instances;
mod blocking;
mod body_geometry;
pub mod body_particles;
pub mod bone_fx;
pub mod bridge_behavior;
mod bridges;
mod builder_disarm;
pub mod bunker_buster;
mod cancellation;
pub mod car_bombs;
pub mod chinook;
mod combat;
pub mod create_object_die;
mod creation;
mod crushing;
mod damage;
pub mod death_callbacks;
pub mod death_lifecycle;
pub mod death_weapons;
pub mod defenses;
pub mod eject_pilot;
pub mod neutron_blast;
mod terrain_flatten;
pub mod terrain_movement;
pub use defenses::AssistedTargeting;
pub mod command_button_hunt;
pub mod demo_trap;
pub mod deployment;
pub mod diagnostics;
pub mod drones;
pub mod helicopter_death;
pub mod helix;
pub mod hijacker;
pub mod hive_body;
pub mod instant_death;
pub mod map_crates;
pub mod microwave;
pub mod object_creation;
pub mod object_fx;
pub mod object_sounds;
pub mod parachute;
pub mod parking;
pub mod physical_ocl;
pub mod pilot_find_vehicle;
mod prone;
mod relationship;
mod retaliation;
pub mod sabotage;
pub mod salvage;
mod scatter;
pub mod special_ability;
pub mod special_power;
pub mod spectre;
pub mod spy_vision;
pub mod stealth;
pub mod sticky_bomb;
pub mod structure_death;
pub mod supply;
pub mod supply_crippling;
pub mod toppling;
pub mod turret;
mod variations;
pub mod veterancy_crate;
pub use combat::damage_fx;
pub use damage::AreaDamage;
pub mod crew;
mod economy;
pub mod fake_buildings;
mod firing;
pub use firing::WeaponBinding;
pub mod countermeasures;
pub mod flight;
mod force_event;
pub mod hacker_disable;
pub mod laser_lock;
pub mod laser_update;
pub mod minefield;
mod object_helpers;
pub mod point_defense;
mod weapons;
pub use firing::ContinuousFire;
pub use weapons::{
    ClipReload, ConditionalWeapon, Prefire, WeaponProjectile, WeaponRules, WeaponSetRules,
    WeaponTemplateSet,
};
mod maintenance;
pub use ai_dispatch::dozer::Feedback as DozerFeedback;
pub use economy::MoneyDepositEvent;
pub use maintenance::{
    ArmorUpgrade, AutoDepositUpgradeLookup, CaptureEffect, CaptureEvent, ConditionUpgrade,
    DepositBoost, DepositEvent, DepositRules, ExperienceUpgrade, HealthUpgrade, HoleExposure,
    IdleRepair, MaintenanceRules, MaxHealthChange, MovementUpgrade, OverchargeEvent,
    OverchargeRules, PowerUpgrade, RebuildRules, Regeneration, Research, ResearchEvent,
    SubObjectOverride, SubObjectUpgrade, SupplyBoost, WeaponUpgrade,
};
mod deletion;
mod healing;
pub mod height_die;
mod lifetime;
pub use healing::{AutoHeal, AutoHealTargets};
pub mod map_scripts;
mod neutral_tech;
pub use neutral_tech::{NeutralTechRules, ProductionCostModifier, RepairDock, TechBuildingRules};
mod projectiles;
pub use projectiles::{
    ClearBuilding, FlightRules, LaunchTransform, MissileRetirement, MissileRules,
    ProjectilePhysics, ShellContact, ShellRules, ShellSpeedScale, TerrainHeights, ThrustRules,
    WeaponMotion,
};
mod ground_movement;
mod group;
mod group_attack;
pub use combat::targeting::Result as CanAttackResult;
mod locomotor;
mod path_service;
mod retained_groups;
pub use locomotor::set as locomotor_set;
pub use locomotor::{GroundAppearance, GroundRules, Locomotor, LocomotorSets, LocomotorVariant};
mod ai_dispatch;
pub mod deliver_payload;
pub mod model_bones;
mod model_flags;
pub mod module_updates;
mod movement;
mod object_state;
mod occupancy;
mod opponent;
mod partition;
mod path_following;
pub mod reinforcement_timer;
mod team_membership;
pub use opponent::{
    AiArea, AiBuilding, AiCall, AiParameter, AiScript, AiScriptGroup, AiTeam, AiTeamUnit,
    AiWaypoint, OpponentDifficulty, OpponentFaction, OpponentMap, OpponentRules,
};
mod production_exit;
pub use production_exit::FactoryBib;
mod progression;
mod results;
mod rubble;
pub use progression::{
    FactionProgression, PowerDefinition, PowerEffect, PowerTarget, ProgressionRules, Rank,
    RepairBurst, Science, UnitProgression,
};
pub use results::ScoreKeeper;
pub use rubble::ConstructionClear;
mod setup;
pub mod spawn;
mod startup;
mod tunnel;
pub use bridges::RegularBridge;
pub use setup::{NeutralObject, PlayerSlot, ResolvedSlots, resolve_player_slots};
pub mod cleanup;
pub mod containment;
pub mod contamination;
pub mod fire;
pub mod hazards;
pub mod horde;
pub mod income;
pub mod overlord;
pub mod projectile_stream;
pub mod radar_update;
mod veterancy;
pub mod visibility;
pub use veterancy::{PromotionEvent, StartingVeterancy, VeterancyRules};
#[cfg(test)]
mod drawable_lifecycle_tests;
#[cfg(test)]
mod tests;

pub const DAMAGE_TYPE_COUNT: usize = 38;

pub const CELL_SIZE: f32 = 10.0;
const STEP_SECONDS: f32 = 1.0 / 30.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Faction {
    Usa,
    China,
    Gla,
    Civilian,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Crate,
    CommandCenter,
    Builder,
    SupplyCenter,
    Barracks,
    Factory,
    PowerPlant,
    TechBuilding,
    Defense,
    Infantry,
    Tank,
    Vehicle,
    Harvester,
    Effect,
}

impl Kind {
    #[must_use]
    pub const fn is_building(self) -> bool {
        matches!(
            self,
            Self::CommandCenter
                | Self::SupplyCenter
                | Self::Barracks
                | Self::Factory
                | Self::PowerPlant
                | Self::TechBuilding
                | Self::Defense
        )
    }
}

#[derive(Clone, Copy)]
pub enum DamageType {
    ArmorPiercing,
    SmallArms,
    InfantryMissile,
    ComancheVulcan,
    Gattling,
    Surrender,
    Explosion,
    Laser,
    Status,
    Crush,
    Poison,
    HazardCleanup,
    Sniper,
    Flame,
    JetMissiles,
    Radiation,
    Microwave,
    KillPilot,
    Disarm,
    AuroraBomb,
    ParticleBeam,
    Melee,
    StealthjetMissiles,
    MolotovCocktail,
    Unresistable,
    Falling,
    SubdualBuilding,
    SubdualUnresistable,
    KillGarrisoned,
    SubdualVehicle,
    SubdualMissile,
    Water,
    Healing,
    Penalty,
    Hack,
    Deploy,
    Toppling,
    LandMine,
}

/// Source lethal damage reason, separate from the unported corpse lifecycle.
#[derive(Clone, Copy)]
pub enum DeathType {
    Normal = 0,
    None = 1,
    Crushed = 2,
    Burned = 3,
    Exploded = 4,
    Poisoned = 5,
    Toppled = 6,
    Flooded = 7,
    Suicided = 8,
    Lasered = 9,
    Detonated = 10,
    Splatted = 11,
    PoisonedBeta = 12,
    Extra2 = 13,
    Extra3 = 14,
    Extra4 = 15,
    Extra5 = 16,
    Extra6 = 17,
    Extra7 = 18,
    Extra8 = 19,
    PoisonedGamma = 20,
}

pub enum MovementSurface {
    Ground,
    Air,
}

/// Source `KindOf` categories used by the two native group-column branches.
pub enum GroupKind {
    Infantry,
    Vehicle,
    Aircraft,
    Helicopter,
    Other,
}

/// Original `MaxSimultaneousOfType` accepts a ushort or the match restriction token.
pub enum TypeLimit {
    Count(u16),
    Superweapon,
}

pub enum Buildable {
    Yes,
    No,
    IgnorePrerequisites,
    OnlyByAi,
}

pub struct BuildRules {
    pub status: Buildable,
    pub objects: Vec<Vec<String>>,
    pub sciences: Vec<String>,
    pub limit: TypeLimit,
    pub linked_limit: Option<String>,
}

pub struct GeometryBounds {
    pub center_height: f32,
    pub sphere_radius: f32,
    pub max_height: f32,
}

/// Concrete Body factory inheritance, including objects without a Body module.
pub enum BodyKind {
    Absent,
    Active,
    Highlander,
    Inactive,
}

impl BodyKind {
    pub(super) const fn is_active(&self) -> bool {
        matches!(self, Self::Active | Self::Highlander)
    }
}

/// Typed authored `Behavior` module data; order remains in `behavior_modules`.
pub struct BehaviorData {
    pub create_object_die: Vec<create_object_die::Rules>,
    pub eject_pilots: Vec<eject_pilot::Rules>,
    pub bridges: Vec<bridge_behavior::Rules>,
    pub neutron_blast: Vec<neutron_blast::Rules>,
    pub bone_fx: Vec<bone_fx::Rules>,
    pub command_button_hunts: Vec<command_button_hunt::Rules>,
    pub collision_weapons: Vec<fire::collide::Rules>,
    pub toppling: Vec<toppling::Rules>,
    pub mine_generators: Vec<minefield::GeneratorRules>,
    pub minefields: Vec<minefield::MineRules>,
    pub hijackers: Vec<hijacker::UpdateRules>,
    pub sticky_bombs: Vec<sticky_bomb::Rules>,
    pub special_abilities: Vec<special_ability::Rules>,
    pub assisted_targeting: Vec<AssistedTargeting>,
    pub mobs: Vec<spawn::MobRules>,
    pub spawns: Vec<spawn::Rules>,
    pub spawn_exits: Vec<Vec<spawn::Point>>,
    pub slaves: Vec<drones::Rules>,
    pub prone: Vec<f32>,
    pub supply_crippling: Vec<supply_crippling::Rules>,
    pub granted_sciences: Vec<Option<String>>,
    pub lock_weapons: Vec<usize>,
    pub object_creation: Vec<Option<object_creation::List>>,
    pub auto_find_healing: Vec<auto_find_healing::Rules>,
    pub lifetimes: Vec<[u32; 2]>,
    pub deletions: Vec<[u32; 2]>,
    pub height_die: Vec<height_die::Rules>,
    pub ocl_update: Option<reinforcement_timer::Rules>,
    pub spy_vision: Vec<spy_vision::Rules>,
    pub radar_updates: Vec<radar_update::Rules>,
    pub radar_upgrades: Vec<radar_update::Upgrade>,
    pub radar_grants: Vec<radar_update::Grant>,
    pub money_crate: Option<map_crates::Rules>,
    pub veterancy_crates: Vec<veterancy_crate::Rules>,
    pub pilot_find_vehicle: Vec<pilot_find_vehicle::Rules>,
    pub stealth: Option<stealth::Rules>,
    pub detectors: Vec<stealth::Detector>,
    pub flammable: Vec<fire::flammable::Rules>,
    pub fire_spread: Vec<fire::flammable::SpreadRules>,
    pub car_bomb_converter: Option<car_bombs::Rules>,
    pub sabotage: Vec<sabotage::Rules>,
    pub death_weapons: Vec<death_weapons::Rules>,
    pub instant_death: Vec<instant_death::Rules>,
    pub death_fx: Vec<death_callbacks::FxRules>,
    pub crush_die: Vec<death_callbacks::CrushRules>,
    pub upgrade_die: Vec<death_callbacks::UpgradeRules>,
    pub destroy_die: Vec<death_callbacks::Conditions>,
    pub drone_upgrades: Vec<drones::Upgrade>,
    pub point_defense: Vec<point_defense::Rules>,
    pub countermeasures: Vec<countermeasures::Rules>,
    pub bunker_busters: Vec<bunker_buster::Rules>,
    pub cleanup: Vec<cleanup::Rules>,
    pub cleanup_area: Vec<cleanup::AreaRules>,
    pub fire_weapons: Vec<fire::weapon_update::Rules>,
    pub damage_weapons: Vec<fire::when_damaged::Rules>,
    pub firestorms: Vec<fire::firestorm::Rules>,
    pub contamination: Vec<contamination::Rules>,
    pub poisoned: Vec<hazards::PoisonRules>,
    pub attachments: Vec<overlord::Upgrade>,
    pub replacement_names: Vec<String>,
    pub sub_object_upgrades: Vec<SubObjectUpgrade>,
    pub propaganda: Option<overlord::Propaganda>,
    pub horde: Option<horde::Rules>,
    pub hacking: Option<income::HackerRules>,
    pub production_exit: Option<ProductionExit>,
}

pub struct UnitType {
    pub command_set: String,
    pub sway_updates: usize,
    pub pristine_draws: Vec<model_bones::Draw>,
    pub combat_drop_kill_fx: Option<String>,
    pub laser_update: Option<laser_update::Rules>,
    pub behavior: BehaviorData,
    pub projectile_rules: Vec<projectiles::FlightRules>,
    pub body_damage_thresholds: [f32; 2],
    pub deliver_payload: Option<deliver_payload::TransportRules>,
    pub update_modules: Vec<module_updates::ModuleRegistration>,
    pub behavior_modules: Vec<module_updates::BehaviorRegistration>,
    pub body_kind: BodyKind,
    pub build_rules: BuildRules,
    pub neutral_tech: NeutralTechRules,
    pub minimum_health: f32,
    pub battle_bus: Option<battle_bus::Rules>,
    pub live_physics: Option<physical_ocl::LiveRules>,
    pub aircraft: Option<aircraft::Rules>,
    pub hijacker: Option<hijacker::Rules>,
    pub parachute: Option<parachute::Rules>,
    pub parking: Option<parking::Rules>,
    pub fire: Option<fire::Rules>,

    pub hive_body: Option<hive_body::Rules>,

    pub death_lifecycle: Option<death_lifecycle::Rules>,
    pub structure_death: Option<structure_death::Rules>,
    pub demo_trap: Vec<demo_trap::Rules>,
    pub spectre: Option<spectre::Rules>,
    pub spectre_deployments: Vec<spectre::DeploymentRules>,
    pub special_powers: Vec<special_power::Rules>,
    pub unpause_special_powers: Vec<Option<String>>,
    pub deployment: Option<deployment::Rules>,
    pub physical: Option<physical_ocl::Rules>,

    pub battle_plan: Option<battle_plan::Rules>,
    pub variations: Vec<String>,
    pub reskin: Option<String>,
    pub salvage: salvage::Rules,
    pub attachment_bone: Option<String>,
    pub attachment_point: [f32; 3],
    pub attachment_thresholds: [f32; 2],
    pub contain: Option<containment::Rules>,
    pub transport_slots: u32,
    pub kind_of: Vec<String>,
    pub turret: Option<turret::Rules>,
    pub secondary_turret: Option<turret::Rules>,
    pub turrets_linked: bool,
    pub enter_guard: bool,
    pub sight: visibility::Sight,
    pub maintenance: MaintenanceRules,
    pub veterancy: VeterancyRules,
    pub name: String,
    pub faction: Faction,
    pub kind: Kind,
    pub selectable: bool,
    pub scoring: SourceScoring,
    pub ai: UnitAi,
    pub progression: UnitProgression,
    pub cost: u32,
    pub build_seconds: f32,
    pub max_health: f32,
    pub initial_health: f32,
    pub speed: f32,
    pub locomotor: Locomotor,
    pub locomotor_sets: LocomotorSets,
    pub weapon: WeaponRules,
    pub secondary_weapon: Option<WeaponRules>,
    pub tertiary_weapon: Option<WeaponRules>,
    pub location_weapons: u8,
    pub weapon_set_rules: WeaponSetRules,
    pub weapon_upgrade_triggers: Vec<String>,
    pub armor: [f32; DAMAGE_TYPE_COUNT],
    pub damage_fx: Vec<damage_fx::ArmorSet>,
    pub damaged_sound: String,
    pub really_damaged_sound: String,
    pub fear_sound: String,
    pub supply: supply::CollectorRules,
    pub geometry: crate::collision_geometry::Geometry,
    pub geometry_bounds: GeometryBounds,

    pub occlusion_delay: u32,
    pub fence_width: f32,

    pub placement_angle: f32,
    pub preferred_height: f32,
    pub chinook: Option<chinook::Rules>,
    pub movement_surface: MovementSurface,
    pub group_kind: GroupKind,
    pub crush: CrushRules,
    pub production: Option<Vec<(String, u32)>>,
    pub additional_production: Vec<ProductionRules>,
    pub production_queue_limit: i32,
    pub production_disabled_types: u32,
    pub production_doors: parking::DoorRules,
    pub buildable_objects: Vec<String>,
    pub factory_bib: FactoryBib,
    pub energy: i32,
}

impl UnitType {
    #[must_use]
    pub fn no_collide(&self) -> bool {
        self.kind_of.iter().any(|kind| kind == "NO_COLLIDE")
    }

    fn avoid_supply(&self) -> bool {
        self.kind_of
            .iter()
            .any(|kind| kind == "CANNOT_BUILD_NEAR_SUPPLIES")
    }
}

/// Source crusher/crushable levels and the `SquishCollide` module predicate.
pub struct CrushRules {
    pub crusher_level: u8,
    pub crushable_level: u8,
    pub squish_module: Option<usize>,
    pub physics_module: Option<usize>,
    pub hijacker: bool,
}

pub struct SourceScoring {
    pub create: bool,
    pub destroy: bool,
    pub victory: bool,
}

/// `AIUpdateModuleData` fields used by the ground-unit order states.
pub struct UnitAi {
    pub update: UnitAiUpdate,
    pub vision_range: f32,
    pub acquisition: u8,
    pub mood_check_frames: u32,
}

pub enum UnitAiUpdate {
    Absent,
    Ordinary,
    Dozer,
    Worker,
    SupplyTruck,
    Wander,
}

impl UnitAi {
    pub const IDLE: u8 = 0x01;
    pub const STEALTHED: u8 = 0x02;
    pub const NO: u8 = 0x04;
    pub const NOT_WHILE_ATTACKING: u8 = 0x08;
    pub const BUILDINGS: u8 = 0x10;

    #[must_use]
    pub const fn auto_acquire(&self) -> bool {
        self.acquisition & Self::IDLE != 0
    }
    #[must_use]
    pub const fn acquire_stealthed(&self) -> bool {
        self.acquisition & Self::STEALTHED != 0
    }
    #[must_use]
    pub const fn auto_acquire_buildings(&self) -> bool {
        self.acquisition & Self::BUILDINGS != 0
    }
    #[must_use]
    pub const fn not_while_attacking(&self) -> bool {
        self.acquisition & Self::NOT_WHILE_ATTACKING != 0
    }
}

impl Default for UnitAi {
    fn default() -> Self {
        Self {
            update: UnitAiUpdate::Ordinary,
            vision_range: 0.0,
            acquisition: 0,
            mood_check_frames: 60,
        }
    }
}

/// Source `TAiData` constructor defaults, overridden by installed `AIData`.
pub struct AiRules {
    pub unit_damaged_threshold: f32,
    pub attack_uses_line_of_sight: bool,
    pub crushes_infantry: bool,
    pub repulsors_enabled: bool,
    pub repulsed_distance: f32,
    pub wall_height: f32,
    pub force_idle_frames: u32,
    pub guard_inner_human: f32,
    pub guard_outer_human: f32,
    pub guard_inner_ai: f32,
    pub guard_outer_ai: f32,
    pub guard_chase_frames: u32,
    pub guard_scan_frames: u32,
    pub guard_return_scan_frames: u32,
    pub max_retaliation_distance: f32,
    pub retaliation_friends_radius: f32,
    pub min_infantry_for_group: u32,
    pub min_vehicles_for_group: u32,
    pub min_distance_for_group: f32,
    pub distance_requires_group: f32,
    pub group_move_gather_factor: f32,
    pub dozer_bored_radius_modifier: f32,
}

impl Default for AiRules {
    fn default() -> Self {
        Self {
            unit_damaged_threshold: 0.0,
            attack_uses_line_of_sight: true,
            crushes_infantry: true,
            repulsors_enabled: false,
            repulsed_distance: 0.0,
            wall_height: 0.0,
            force_idle_frames: 1,
            guard_inner_human: 0.0,
            guard_outer_human: 0.0,
            guard_inner_ai: 0.0,
            guard_outer_ai: 0.0,
            guard_chase_frames: 0,
            guard_scan_frames: 15,
            guard_return_scan_frames: 30,
            max_retaliation_distance: 210.0,
            retaliation_friends_radius: 120.0,
            min_infantry_for_group: 3,
            min_vehicles_for_group: 4,
            min_distance_for_group: 100.0,
            distance_requires_group: 0.0,
            group_move_gather_factor: 1.0,
            dozer_bored_radius_modifier: 2.0,
        }
    }
}

pub struct ProductionExit {
    pub use_spawn_rally_point: bool,
    pub create: [f32; 3],
    pub rally: [f32; 3],
    pub snap_rally: bool,
    pub extend_rally: bool,
    pub queue: Option<QueueExit>,
}

pub struct QueueExit {
    pub allow_airborne_creation: bool,
    pub delay_frames: u32,
    pub initial_burst: u32,
}

impl UnitType {
    const fn footprint(&self, position: [f32; 2], angle: f32) -> Footprint {
        Footprint {
            position,
            angle,
            major: self.geometry.major,
            minor: self.geometry.minor,
            shape: self.geometry.shape,
        }
    }
}

pub struct EconomyRules {
    pub allowed_build_height_variation: f32,
    pub min_build_edge_distance: f32,
    pub starting_cash: u32,
    pub supply_value: u32,
    pub supply_build_border: f32,
    pub low_energy_penalty_modifier: f32,
    pub min_low_energy_production_speed: f32,
    pub max_low_energy_production_speed: f32,
}

pub struct Player {
    selected_objects: Vec<usize>,
    preordered: bool,
    units_should_hunt: bool,
    supply_centers: Vec<usize>,
    supply_warehouses: Vec<usize>,
    production_cost_changes: Vec<neutral_tech::ProductionCostChange>,
    battle_plans: battle_plan::Army,
    upgrades: Vec<String>,
    upgrades_in_progress: Vec<String>,
    pub faction: Faction,
    pub side: Option<String>,
    pub playable_side: bool,
    pub color: Option<u32>,
    pub statistics: ScoreKeeper,
    pub cash: u32,
    pub power: i32,
    pub rank: u32,
    pub skill_points: u32,
    pub science_points: u32,
    sciences: Vec<String>,
    acquired_sciences: Vec<String>,
    cash_bounty: f32,
    power_ready_at: Vec<Option<u64>>,
    special_power_ready: std::collections::BTreeMap<String, u32>,
    academy: results::AcademyStats,
    energy: economy::Energy,
    native_money: economy::MoneyState,
    radar: radar_update::PlayerRadar,
}

#[derive(Clone, Copy)]
pub enum Order {
    Enter(usize),
    ExitContainer,
    HackInternet,
    Idle,
    Move([f32; 2]),
    Exit([f32; 3]),
    Attack(usize),
    AttackArea,
    AttackMove([f32; 2]),
    Guard([f32; 2]),
    FollowPath,
    Build(usize),
    Repair(usize),
    RepairAt(usize),
    Harvest(usize),
    ReturnSupply,
    CleanHazard(u64),
    FireAt { position: [f32; 2], weapon: usize },
}

#[derive(Clone, Copy)]
pub enum ProductionItem {
    Unit(usize),
    Upgrade(usize),
}

pub use economy::production_instances::Rules as ProductionRules;

pub struct Production {
    quantity: u32,
    produced: u32,
    parking_door: Option<usize>,
    pub id: i32,
    pub item: ProductionItem,
    pub remaining_seconds: f32,
    pub total_seconds: f32,
    elapsed_frames: f32,
}

pub struct Entity {
    selectable: bool,
    behaviors: behavior_instances::State,
    mines: minefield::State,
    drawable: DrawableState,
    native_projectile: Option<u64>,
    projectile_flight: Option<projectiles::Flight>,
    native_hazard: Option<u64>,
    physics_control: force_event::PhysicsControl,
    pub ai: ai_dispatch::State,
    update_owner: usize,
    spy_vision: spy_vision::State,
    radar: radar_update::State,
    /// Retained native `DisabledTypes` bits; UNDERPOWERED is changed by Energy callbacks.
    disabled_status: u32,
    disabled_observed: u32,
    crew: crew::State,
    queue_exit: production_exit::QueueState,
    aircraft: Option<aircraft::State>,
    parking: Option<parking::State>,
    fire: Option<fire::State>,
    stealth: Option<Box<stealth::State>>,
    flammable: fire::flammable::ObjectState,
    car_bomb: Option<car_bombs::State>,
    death_lifecycle: Option<Box<death_lifecycle::State>>,
    structure_death: Option<Box<structure_death::State>>,
    spectre: Option<spectre::State>,
    deployment: Option<deployment::State>,
    physical: Option<Box<physical_ocl::State>>,
    helix: helix::State,
    parachute: Option<parachute::State>,
    poison: hazards::PoisonState,
    helpers: object_helpers::State,
    object_status: u64,
    private_status: u8,
    battle_plan: Option<Box<battle_plan::State>>,
    battle_bonuses: battle_plan::Bonuses,
    salvage: salvage::State,
    attachment: overlord::State,
    horde: horde::State,
    chinook: Option<Box<chinook::State>>,
    flight: Option<Box<flight::Motion>>,
    containment: containment::State,
    assault: Option<Box<assault_transport::State>>,
    delivery: Option<Box<deliver_payload::State>>,
    hacking: income::State,
    maintenance: maintenance::MaintenanceState,
    pub type_index: usize,
    pub owner: usize,
    pub position: [f32; 2],
    pub layer: u8,
    trigger_position: [i32; 2],
    shroud_range_override: Option<f32>,
    // Immutable map structures retain GameLogic's authored ground-relative Z.
    placed_height_offset: Option<f32>,
    pub facing: f32,
    transform: Option<physical_ocl::transform::Matrix>,
    pub health: f32,
    pub max_health: f32,
    pub experience: i32,
    experience_sink: Option<usize>,
    pub veterancy: u8,
    veterancy_upgrades: u8,
    pub build_progress: f32,
    pub cargo: u32,
    pub docking_flags: u8,
    dock: Option<economy::DockState>,
    pub queue: VecDeque<Production>,
    additional_production: Vec<economy::production_instances::State>,
    production_door_flags: u16,
    next_production_id: i32,
    pub rally_point: Option<[f32; 3]>,
    damage_state: combat::DamageState,
    body_geometry: Option<body_geometry::State>,
    firing: firing::FiringTracker,
    weapon_set: weapons::State,
    hacked_until: u64,
    collector_pending: bool,
    production_doors: parking::doors::State,
    health_box_offset: [f32; 3],
    battle_bus: Option<battle_bus::State>,
    live_physics: Option<physical_ocl::LiveState>,
    producer: Option<usize>,
    pub construction_percent: f32,
}

enum ConstructionPhase {
    Approach,
    Build,
}

/// Original `SupplyCenterDockUpdate` data and pristine model-space docking bones.
pub struct SupplyDock {
    pub number_approaches: i32,
    pub approach_bones: usize,
    pub temporary_stealth_frames: u32,
    pub approaches: Vec<[f32; 3]>,
    pub enter: Option<[f32; 3]>,
    pub action: Option<[f32; 3]>,
    pub exit: Option<[f32; 3]>,
    pub allows_passthrough: bool,
}

/// Independent source docking model-condition bits, shared by actor and dock.
pub mod docking_flags {
    pub const DOCKING: u8 = 1;
    pub const BEGINNING: u8 = 2;
    pub const ACTIVE: u8 = 4;
    pub const ENDING: u8 = 8;
}

enum ConstructionState {
    Complete,
    Awaiting,
    Active,
    Interrupted,
}

impl Entity {
    #[must_use]
    pub const fn drawable_transform_revision(&self) -> u64 {
        self.drawable.transform_revision
    }
}

// Drawable.h TerrainDecalType values selected by the admitted logic consumers.
pub enum TerrainDecal {
    None,
    Horde,
    HordeNationalism,
    HordeVehicle,
    HordeFanaticism,
    ChemicalSuit,
}

// The logic-to-drawable adapter retains the last notified pose and model state.
struct DrawableState {
    present: bool,
    selected: u8,
    hidden: bool,
    hidden_by_stealth: bool,
    status: u32,
    tint_status: u32,
    decal_fade_request: Option<[f32; 2]>,
    terrain_decal: TerrainDecal,
    animation_completion: Option<(u64, u32, bool)>,
    animation_frame: Option<(u64, i32)>,
    transform_position: [f32; 3],
    transform_revision: u64,
    notified_transform: Option<([f32; 3], [[f32; 3]; 3])>,
    transform_angle: f32,
    instance_matrix: Option<[[f32; 4]; 3]>,
    safe_occlusion_frame: u32,
    slave_condition: Option<&'static str>,
    model_conditions: ModelConditions,
    sub_objects: Vec<SubObjectOverride>,
}

impl DrawableState {
    const fn for_construction(complete: bool) -> Self {
        Self {
            present: true,
            selected: 0,
            hidden: false,
            hidden_by_stealth: false,
            tint_status: 0,
            status: 2, // DRAWABLE_STATUS_DEFAULT = DRAWABLE_STATUS_SHADOWS.
            decal_fade_request: None,
            terrain_decal: TerrainDecal::None,
            animation_completion: None,
            animation_frame: None,
            transform_position: [0.0; 3],
            transform_revision: 0,
            notified_transform: None,
            transform_angle: 0.0,
            instance_matrix: None,
            safe_occlusion_frame: 0,
            slave_condition: None,
            model_conditions: ModelConditions::for_construction(complete),
            sub_objects: Vec::new(),
        }
    }
}

struct ModelConditions {
    preordered: bool,
    flags: u128,
    constructing: bool,
    production: bool,
    production_pending: Option<bool>,
    construction: ConstructionState,
}

impl ModelConditions {
    // Native ModelState.h saved flag indices.
    const TOPPLED: u128 = 1;
    const BURNED: u128 = 1 << 63;
    const PARACHUTING: u128 = 1 << 9;
    const EXPLODED_FLAILING: u128 = 1 << 89;
    const CAPTURED: u128 = 1 << 88;
    const SECOND_LIFE: u128 = 1 << 110;
    const EXPLODED_BOUNCING: u128 = 1 << 90;
    const POST_COLLAPSE: u128 = 1 << 48;
    const MOVING: u128 = 1 << 49;
    const PRONE: u128 = 1 << 54;
    const FREEFALL: u128 = 1 << 55;
    const PANICKING: u128 = 1 << 60;
    const RAPPELLING: u128 = 1 << 80;

    const fn set_flag(&mut self, flag: u128, enabled: bool) {
        if enabled {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    const fn has_flag(&self, flag: u128) -> bool {
        self.flags & flag != 0
    }

    const fn for_construction(complete: bool) -> Self {
        Self {
            preordered: false,
            flags: 0,
            constructing: false,
            production: false,
            production_pending: None,
            construction: if complete {
                ConstructionState::Complete
            } else {
                ConstructionState::Awaiting
            },
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn layer(&self) -> u8 {
        self.layer
    }

    /// Native creator identity, retained after production exits and object death.
    #[must_use]
    pub const fn producer(&self) -> Option<usize> {
        self.producer
    }

    /// Source object of the last damaging hit, including a retained corpse.
    #[must_use]
    pub const fn last_attacker(&self) -> Option<usize> {
        self.damage_state.last_attacker
    }

    #[must_use]
    pub const fn death_type(&self) -> DeathType {
        self.damage_state.death_type
    }

    /// Ground move-state condition, including its turning frames.
    #[must_use]
    pub fn moving(&self) -> bool {
        self.alive()
            && self
                .drawable
                .model_conditions
                .has_flag(crate::skirmish::ModelConditions::MOVING)
    }

    #[must_use]
    pub fn actively_constructing(&self) -> bool {
        self.alive()
            && (self.drawable.model_conditions.constructing
                || self.drawable.model_conditions.production)
    }

    #[must_use]
    pub fn awaiting_construction(&self) -> bool {
        self.alive()
            && matches!(
                self.drawable.model_conditions.construction,
                ConstructionState::Awaiting
            )
    }

    #[must_use]
    pub fn partially_constructed(&self) -> bool {
        self.alive()
            && matches!(
                self.drawable.model_conditions.construction,
                ConstructionState::Active | ConstructionState::Interrupted
            )
    }

    #[must_use]
    pub fn actively_being_constructed(&self) -> bool {
        self.alive()
            && matches!(
                self.drawable.model_conditions.construction,
                ConstructionState::Active
            )
    }

    /// Requested Alt-waypoint goals, never the pathfinder's internal route cells.
    #[must_use]
    pub fn waypoint_path(&self) -> Option<(&[[f32; 3]], usize)> {
        matches!(self.ai.order, Order::FollowPath)
            .then_some((self.ai.waypoints.points.as_slice(), self.ai.waypoints.index))
    }

    #[must_use]
    pub fn alive(&self) -> bool {
        self.exists() && self.health > 0.0
    }

    #[must_use]
    pub fn complete(&self) -> bool {
        self.alive() && self.build_progress >= 1.0 && !self.selling()
    }
}

/// A `SupplyWarehouseDockUpdate` instance. Geometry and lifetime belong to its Object.
pub struct Supply {
    pub object: usize,
    pub boxes: u32,
}

pub struct PlayerStart {
    pub position: [f32; 2],
    pub rally: Option<[f32; 2]>,
}

pub struct MapLayout {
    pub height_map: Option<TerrainHeights>,
    pub map_sides: Vec<map_scripts::SideDefinition>,
    pub map_teams: Vec<map_scripts::TeamDefinition>,
    pub map_build_lists: Vec<map_scripts::BuildList>,
    pub map_template_order: Vec<map_scripts::TemplateDefinition>,
    pub scripts: Vec<map_scripts::Script>,
    pub partition_cell_size: f32,
    pub random_seed: u32,
    pub starts: Vec<PlayerStart>,
    pub regular_bridges: Vec<RegularBridge>,
    pub neutral_objects: Vec<NeutralObject>,
    pub supplies: Vec<usize>,
    pub obstacles: Vec<Footprint>,
    pub obstacle_teams: Vec<String>,
    pub world_objects: Vec<MapWorldObject>,
    pub world_object_order: Vec<MapWorldPlacement>,
    pub obstacle_objects: Vec<Option<usize>>,
    pub fences: Vec<MapFence>,
    pub obstacle_order: Vec<MapObstacle>,
}

impl MapLayout {
    fn border_extent(&self, playable_extent: [[f32; 2]; 2]) -> [[f32; 2]; 2] {
        self.height_map.as_ref().map_or(playable_extent, |map| {
            let width = f32::from(map.width) * CELL_SIZE;
            let height = f32::from(map.height) * CELL_SIZE;
            [map.origin, [map.origin[0] + width, map.origin[1] + height]]
        })
    }

    fn take_terrain(
        &mut self,
        extent: [f32; 2],
    ) -> Result<Option<projectiles::Terrain>, &'static str> {
        self.height_map
            .take()
            .map(|height_map| {
                projectiles::Terrain::new(
                    height_map.width,
                    height_map.height,
                    height_map.origin,
                    height_map.samples,
                    self.partition_cell_size,
                    extent,
                )
            })
            .transpose()
    }
}

/// Noncatalog original objects retained independently of their drawing and ground admission.
pub struct MapWorldObject {
    pub template: String,
    pub kind_of: Vec<String>,
    pub position: [f32; 3],
    pub angle: f32,
    pub geometry: crate::collision_geometry::Geometry,
    pub original_team: String,
}

/// `GameLogic::startGame` ordinary map creation order, before faction starting units.
pub enum MapWorldPlacement {
    Object(usize),
    Neutral(usize),
    Tree(usize),
}

pub struct MapFence {
    pub position: [f32; 2],
    pub angle: f32,
    pub width: f32,
    pub x_offset: f32,
}

/// Original map-file order before static footprints and live objects separate.
pub enum MapObstacle {
    Static { index: usize, transparent: bool },
    Neutral(usize),
    Fence { index: usize, transparent: bool },
}

#[derive(Clone)]
pub struct Footprint {
    pub position: [f32; 2],
    pub angle: f32,
    pub major: f32,
    pub minor: f32,
    pub shape: crate::collision_geometry::Shape,
}

impl Footprint {
    /// `GeometryInfo::isPointInFootprint` after transforming into object-local XY.
    fn contains(&self, point: [f32; 2]) -> bool {
        let x = point[0] - self.position[0];
        let y = point[1] - self.position[1];
        if matches!(self.shape, crate::collision_geometry::Shape::Box) {
            let (sine, cosine) = self.angle.sin_cos();
            let x_cosine = x * cosine;
            let y_sine = y * sine;
            let y_cosine = y * cosine;
            let x_sine = x * sine;
            (x_cosine + y_sine).abs() <= self.major && (y_cosine - x_sine).abs() <= self.minor
        } else {
            let x_squared = x * x;
            let y_squared = y * y;
            x_squared + y_squared <= self.major.powi(2)
        }
    }
}

pub struct Shot {
    pub weapon_type: usize,
    pub weapon_index: usize,
    pub weapon_slot: usize,
    pub barrel: usize,
    pub source: usize,
    pub target: Option<usize>,
    pub source_position: [f32; 3],
    pub source_facing: f32,
    pub target_position: [f32; 3],
    pub veterancy: u8,
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub owner: usize,
}

pub struct Projectile {
    pub object: Option<usize>,
    pub weapon_type: usize,
    pub weapon_index: usize,
    pub weapon_slot: usize,
    pub id: u64,
    pub position: [f32; 2],
    pub destination: [f32; 2],
    pub owner: usize,
    target: Option<usize>,
    pub hazard_target: Option<u64>,
    pub source: usize,
    pub veterancy: u8,
    damage_bonus: f32,
    damage_state: battle_plan::Damage,
    direct: Option<projectiles::DirectHit>,
}

pub enum ProjectileFxStage {
    Created,
    Ignition,
    Detonation,
    GarrisonKill,
    Lasered,
}

pub struct ProjectileFxEvent {
    pub weapon_type: usize,
    pub weapon_index: usize,
    pub object: Option<usize>,
    /// Native doFXObj retains its target geometry at the callback.
    pub object_geometry_radius: Option<f32>,
    pub weapon_slot: usize,
    pub projectile: u64,
    pub source: usize,
    pub position: [f32; 3],
    pub axes: [[f32; 3]; 3],
    pub veterancy: u8,
    pub stage: ProjectileFxStage,
}

/// Native weapon and projectile callbacks in their logic-phase invocation order.
pub enum WeaponFxEvent {
    BoneFx(usize),
    BodyParticles(usize),
    BodyParticlesClear(usize),
    TerrainMovement(usize),
    Chinook(chinook::VisualEvent),
    LaserInit(usize),
    DrawableCreated {
        object: usize,
        type_index: usize,
        validate_logic: bool,
    },
    DrawableDestroyed(usize),
    AbilityFeedback(usize),
    ObjectFx(usize),
    SpectreStrafe(usize),
    Firestorm(usize),
    SlaveWeld(usize),
    DestroyAttachedParticles(usize),
    Structure(usize),
    ObjectSound(usize),
    AttachedParticle(usize),
    Pilot(usize),
    Shot(usize),
    FiringSound {
        object: usize,
        weapon: usize,
        position: [f32; 3],
        clear_cells: u16,
    },
    StopFiringSound(usize),
    RapidFire {
        object: usize,
        position: [f32; 3],
    },
    Laser(usize),
    Projectile(ProjectileFxEvent),
}

/// Ordered presentation callbacks and the payload arrays referenced by their indices.
pub struct PresentationEvents {
    pub bone_fx: Vec<bone_fx::Event>,
    pub body_particles: Vec<body_particles::Event>,
    pub car_bomb_events: Vec<car_bombs::Event>,
    pub sabotage_events: Vec<sabotage::Event>,
    pub hijacker_events: Vec<hijacker::Event>,
    pub terrain_movement_events: Vec<terrain_movement::Event>,
    pub burning_sound_events: Vec<fire::flammable::SoundEvent>,
    pub battle_plan_events: Vec<battle_plan::Event>,
    pub research_events: Vec<ResearchEvent>,
    pub replacement_events: Vec<fake_buildings::ReplacementEvent>,
    pub promotion_events: Vec<PromotionEvent>,
    pub salvage_events: Vec<salvage::Event>,
    pub money_crate_events: Vec<map_crates::Event>,
    pub container_sounds: Vec<containment::Sound>,
    pub dozer_feedback: Vec<DozerFeedback>,
    pub hacking_events: Vec<income::Event>,
    pub deposit_events: Vec<DepositEvent>,
    pub overcharge_events: Vec<OverchargeEvent>,
    pub money_deposit_events: Vec<MoneyDepositEvent>,
    pub capture_events: Vec<CaptureEvent>,
    pub radar_events: Vec<radar_update::Event>,
    pub disable_events: Vec<hacker_disable::Event>,
    pub propaganda_events: Vec<overlord::Pulse>,
    pub assistance_events: Vec<defenses::Assistance>,
    pub stealth_events: Vec<stealth::Event>,
    pub laser_initializations: Vec<laser_update::Event>,
    pub ability_feedback: Vec<special_ability::FeedbackEvent>,
    pub attached_particles: Vec<helicopter_death::ParticleAttachment>,
    pub spectre_strafes: Vec<spectre::Strafe>,
    pub pilot_events: Vec<crew::PilotEvent>,
    pub object_fx_events: Vec<object_fx::Event>,
    pub structure_events: Vec<structure_death::Event>,
    pub object_sounds: Vec<object_sounds::Event>,
    pub firestorm_events: Vec<fire::firestorm::Event>,
    pub slave_welds: Vec<drones::WeldEvent>,
    pub shots: Vec<Shot>,
    pub laser_shots: Vec<point_defense::Shot>,
    pub weapon_fx: Vec<WeaponFxEvent>,
}

impl PresentationEvents {
    const fn new() -> Self {
        Self {
            bone_fx: Vec::new(),
            body_particles: Vec::new(),
            car_bomb_events: Vec::new(),
            sabotage_events: Vec::new(),
            hijacker_events: Vec::new(),
            terrain_movement_events: Vec::new(),
            burning_sound_events: Vec::new(),
            battle_plan_events: Vec::new(),
            research_events: Vec::new(),
            replacement_events: Vec::new(),
            promotion_events: Vec::new(),
            salvage_events: Vec::new(),
            money_crate_events: Vec::new(),
            container_sounds: Vec::new(),
            dozer_feedback: Vec::new(),
            hacking_events: Vec::new(),
            deposit_events: Vec::new(),
            overcharge_events: Vec::new(),
            money_deposit_events: Vec::new(),
            capture_events: Vec::new(),
            radar_events: Vec::new(),
            disable_events: Vec::new(),
            propaganda_events: Vec::new(),
            assistance_events: Vec::new(),
            stealth_events: Vec::new(),
            laser_initializations: Vec::new(),
            ability_feedback: Vec::new(),
            attached_particles: Vec::new(),
            spectre_strafes: Vec::new(),
            pilot_events: Vec::new(),
            object_fx_events: Vec::new(),
            structure_events: Vec::new(),
            object_sounds: Vec::new(),
            firestorm_events: Vec::new(),
            slave_welds: Vec::new(),
            shots: Vec::new(),
            laser_shots: Vec::new(),
            weapon_fx: Vec::new(),
        }
    }
}

pub struct Skirmish {
    pub rope_drawables: chinook::RopeDrawables,
    pub presentation: PresentationEvents,
    delivery_lists: Vec<deliver_payload::Rules>,
    hulk_lifetime_override: Option<u32>,
    playable_extent: [[f32; 2]; 2],
    border_extent: [[f32; 2]; 2],
    pub spectre_selections: Vec<usize>,
    pub spawn_selection_events: Vec<Vec<usize>>,
    pub deselected_objects: Vec<usize>,
    pub slow_death_scale: f32,
    pub special_power_triggers: Vec<special_power::Trigger>,
    pub draw_icon_ui: bool,
    update_scheduler: module_updates::Scheduler,
    update_objects: Vec<module_updates::Object>,
    projectile_indices: std::collections::BTreeMap<u64, usize>,
    hazard_indices: std::collections::BTreeMap<u64, usize>,
    team_membership: team_membership::Membership,
    command_buttons: command_button_hunt::Definitions,
    command_set_products: std::collections::BTreeMap<String, Vec<String>>,
    pub gravity: f32,
    fire_history: std::collections::BTreeMap<String, fire::history::History>,
    civilian_objects: std::collections::BTreeSet<usize>,
    pub construction_clears: Vec<ConstructionClear>,
    pub hazards: Vec<hazards::Field>,
    pub projectile_streams: Vec<projectile_stream::Stream>,
    hazard_serial: u64,
    disabled_sounds: [String; 4],
    pub idle_workers: Vec<(usize, usize)>,
    tunnels: Vec<tunnel::Network>,
    caves: Vec<Option<tunnel::Network>>,
    map_objects: std::collections::BTreeMap<String, usize>,
    removed_map_names: std::collections::BTreeSet<String>,
    map_scripts: map_scripts::State,
    map_side_state: map_scripts::SideState,
    bridge_objects: Vec<bridges::Binding>,
    radar_terrain_refresh_frame: u32,
    auto_deposit_upgrade: AutoDepositUpgradeLookup,
    radar_event_serial: u64,
    logic_update: bool,
    map_loading: bool,
    pub catalog: Vec<UnitType>,
    pub entities: Vec<Entity>,
    pub players: Vec<Player>,
    retaliation_enabled: Vec<bool>,
    pub supplies: Vec<Supply>,
    pub projectiles: Vec<Projectile>,
    projectile_terrain: Option<projectiles::Terrain>,
    projectile_serial: u64,
    pub tick: u64,
    pub outcome: Option<usize>,
    pub ended_at: Option<u64>,
    pub end_game_at: Option<u64>,
    pub defeated: Vec<bool>,
    pub occupied: Vec<bool>,
    pub slots: Vec<PlayerSlot>,
    pub player_colors: Vec<Option<usize>>,
    pub start_positions: Vec<Option<usize>>,
    grid: Grid,
    navigation: Grid,
    navigation_obstacles: movement::obstacles::Obstacles,
    path_service: path_service::Service,
    next_formation_id: u32,
    next_ai_group_id: u32,
    rubble_navigation: Option<Grid>,
    cliff_navigation: Option<Grid>,
    movement_terrain: crate::terrain::TerrainMap,
    occupancy: occupancy::Occupancy,
    partition: partition::Partition,
    visibility: visibility::Visibility,
    rules: EconomyRules,
    superweapon_limit: u32,
    ai_rules: AiRules,
    random: [u32; 6],
    layout: MapLayout,
    progression: Option<ProgressionRules>,
    repair_markers: Vec<progression::RepairMarker>,
    opponents: Vec<Option<opponent::Opponent>>,
    opponent_rules: Option<std::sync::Arc<OpponentRules>>,
    attack_priorities: Vec<opponent::priorities::AttackPriority>,
    trigger_state: opponent::map::TriggerState,
}

impl Skirmish {
    /// Current `ThingTemplate::calcCostToBuild`, including owned tech bonuses.
    #[must_use]
    pub fn build_cost(&self, owner: usize, product: usize) -> u32 {
        let cost = progression::integer_real(self.catalog[product].cost);
        progression::real_unsigned(cost * self.production_cost_multiplier(owner, product))
    }

    /// Creates the two-player playtest scenario on the supplied classified terrain.
    ///
    /// # Errors
    /// Rejects invalid source values, an incomplete faction catalog, or terrain without room for bases.
    pub fn new(
        terrain: crate::terrain::TerrainMap,
        catalog: Vec<UnitType>,
        player_faction: Faction,
        enemy_faction: Faction,
        rules: EconomyRules,
        ai_rules: AiRules,
        layout: MapLayout,
    ) -> Result<Self, &'static str> {
        validate_catalog(&catalog, &rules)?;
        ai::validate_rules(&catalog, &ai_rules)?;
        validate_layout(&layout)?;
        let grid = Grid::new(
            terrain.width(),
            terrain.height(),
            terrain.blocked_ground_cells(),
        )
        .map_err(|_| "Invalid movement terrain dimensions")?;
        let mut game = Self::create_world(terrain, catalog, rules, ai_rules, layout, grid)?;
        game.reset(player_faction, enemy_faction)?;
        Ok(game)
    }

    fn create_world(
        terrain: crate::terrain::TerrainMap,
        catalog: Vec<UnitType>,
        rules: EconomyRules,
        ai_rules: AiRules,
        mut layout: MapLayout,
        grid: Grid,
    ) -> Result<Self, &'static str> {
        let player_count = layout.starts.len() + 1;
        let extent = [grid.width(), grid.height()].map(|cells| f32::from(cells) * CELL_SIZE);
        Ok(Self {
            rope_drawables: chinook::RopeDrawables::new(),
            presentation: PresentationEvents::new(),
            delivery_lists: Vec::new(),
            playable_extent: [[0.0; 2], extent],
            border_extent: layout.border_extent([[0.0; 2], extent]),
            spectre_selections: Vec::new(),
            spawn_selection_events: Vec::new(),
            deselected_objects: Vec::new(),
            slow_death_scale: 1.0,
            special_power_triggers: Vec::new(),
            draw_icon_ui: true,
            update_scheduler: module_updates::Scheduler::new(),
            update_objects: Vec::new(),
            hulk_lifetime_override: None,
            projectile_indices: std::collections::BTreeMap::new(),
            hazard_indices: std::collections::BTreeMap::new(),
            team_membership: team_membership::Membership::new(),
            command_buttons: command_button_hunt::Definitions::empty(),
            command_set_products: std::collections::BTreeMap::new(),
            map_scripts: map_scripts::State::new(std::mem::take(&mut layout.scripts)),
            map_side_state: map_scripts::SideState::new(),
            bridge_objects: Vec::new(),
            radar_terrain_refresh_frame: 0,
            fire_history: std::collections::BTreeMap::new(),
            civilian_objects: std::collections::BTreeSet::new(),
            gravity: -1.0,
            construction_clears: Vec::new(),
            hazards: Vec::new(),
            projectile_streams: Vec::new(),
            hazard_serial: 0,
            tunnels: Vec::new(),
            caves: Vec::new(),
            disabled_sounds: std::array::from_fn(|_| String::new()),
            idle_workers: Vec::new(),
            map_objects: std::collections::BTreeMap::new(),
            removed_map_names: std::collections::BTreeSet::new(),
            auto_deposit_upgrade: AutoDepositUpgradeLookup::Unresolved,
            radar_event_serial: 0,
            logic_update: false,
            map_loading: false,
            movement_terrain: terrain,
            catalog,
            entities: Vec::new(),
            players: Vec::new(),
            retaliation_enabled: Vec::new(),
            slots: Vec::new(),
            player_colors: Vec::new(),
            start_positions: vec![None; player_count],
            supplies: Vec::new(),
            projectiles: Vec::new(),
            projectile_terrain: layout.take_terrain(extent)?,
            projectile_serial: 0,
            tick: 0,
            outcome: None,
            ended_at: None,
            end_game_at: None,
            defeated: vec![false; player_count],
            occupied: vec![false; player_count],
            navigation: grid.clone(),
            navigation_obstacles: movement::obstacles::Obstacles::new(),
            path_service: path_service::Service::new(),
            next_formation_id: 1,
            next_ai_group_id: 0,
            rubble_navigation: None,
            cliff_navigation: None,
            occupancy: occupancy::Occupancy::new(),
            visibility: visibility::Visibility::for_map(&layout, extent, player_count),
            partition: partition::Partition::new(layout.partition_cell_size, extent),
            grid,
            rules,
            superweapon_limit: 0,
            ai_rules,
            progression: None,
            repair_markers: Vec::new(),
            opponents: (0..player_count).map(|_| None).collect(),
            opponent_rules: None,
            attack_priorities: Vec::new(),
            trigger_state: opponent::map::TriggerState::new(),
            random: startup::seed_random(layout.random_seed),
            layout,
        })
    }

    /// Skirmish setup options retained across match resets. Zero leaves superweapons unlimited.
    pub const fn set_match_options(&mut self, starting_cash: u32, superweapon_limit: u32) {
        self.rules.starting_cash = starting_cash;
        self.superweapon_limit = superweapon_limit;
    }

    /// Install the classified terrain corresponding to the routing grid.
    ///
    /// # Errors
    /// Rejects dimensions that differ from the map used to create this match.
    pub fn configure_movement_terrain(
        &mut self,
        terrain: crate::terrain::TerrainMap,
    ) -> Result<(), &'static str> {
        if terrain.width() != self.grid.width() || terrain.height() != self.grid.height() {
            return Err("Movement terrain dimensions do not match the routing grid");
        }
        let wall_pieces: Vec<_> = self.movement_terrain.wall_piece_ids().collect();
        self.movement_terrain = terrain;
        for id in wall_pieces {
            self.movement_terrain.add_wall_piece(id);
        }
        self.allocate_map_wall_layer();
        self.bind_map_bridges();
        self.reclassify_navigation_obstacles();
        Ok(())
    }

    /// Restarts without reloading the catalog or changing the selected source values.
    ///
    /// # Errors
    /// Returns an error if either faction lacks starting types or no legal spawn space exists.
    pub fn reset(
        &mut self,
        player_faction: Faction,
        enemy_faction: Faction,
    ) -> Result<(), &'static str> {
        let slots = self.legacy_slots(player_faction, Some(enemy_faction));
        self.reset_slots(slots)
    }

    /// Starts the source single-player sandbox with no occupied opponent slot.
    /// # Errors
    /// Rejects missing starting types or terrain without space for the player.
    pub fn reset_sandbox(&mut self, faction: Faction) -> Result<(), &'static str> {
        let slots = self.legacy_slots(faction, None);
        self.reset_slots(slots)
    }

    fn clear_match_state(&mut self) {
        self.draw_icon_ui = true;
        self.special_power_triggers.clear();
        self.fire_history.clear();
        self.presentation.deposit_events.clear();
        self.presentation.overcharge_events.clear();
        self.presentation.money_deposit_events.clear();
        for actor in &mut self.entities {
            actor.set_effectively_dead(true);
        }
        for object in (0..self.update_objects.len()).rev() {
            self.destroy_maintenance_object(self.update_objects[object].entity);
        }
        self.process_update_destruction();
        self.logic_update = false;
        self.update_scheduler.reset();
        self.update_objects.clear();
        self.projectile_indices.clear();
        self.hazard_indices.clear();
        self.team_membership = team_membership::Membership::new();
        self.entities.clear();
        self.rope_drawables = chinook::RopeDrawables::new();
        self.idle_workers.clear();
        self.presentation.dozer_feedback.clear();
        self.navigation_obstacles.clear();
        self.hazards.clear();
        self.projectile_streams.clear();
        self.presentation.burning_sound_events.clear();
        self.presentation.firestorm_events.clear();
        self.presentation.slave_welds.clear();
        self.hulk_lifetime_override = None;
        self.hazard_serial = 0;
        self.path_service = path_service::Service::new();
        self.next_formation_id = 1;
        self.next_ai_group_id = 0;
        self.presentation.promotion_events.clear();
        self.presentation.car_bomb_events.clear();
        self.presentation.sabotage_events.clear();
        self.presentation.hijacker_events.clear();
        self.presentation.bone_fx.clear();
        self.presentation.body_particles.clear();
        self.presentation.object_fx_events.clear();
        self.presentation.structure_events.clear();
        self.presentation.object_sounds.clear();
        self.civilian_objects.clear();
        self.construction_clears.clear();
        self.presentation.terrain_movement_events.clear();
        self.presentation.battle_plan_events.clear();
        self.presentation.research_events.clear();
        self.presentation.replacement_events.clear();
        self.presentation.salvage_events.clear();
        self.presentation.money_crate_events.clear();
        self.presentation.container_sounds.clear();
        self.map_objects.clear();
        self.bridge_objects.clear();
        self.radar_terrain_refresh_frame = 0;
        self.caves.clear();
        self.removed_map_names.clear();
        self.map_scripts.reset(&mut self.random);
        self.rubble_navigation = None;
        self.cliff_navigation = None;
        self.movement_terrain.reset_path_classification();
        self.movement_terrain.reset_bridges();
        self.grid.restore_ground_terrain(&self.movement_terrain);
        self.presentation.hacking_events.clear();
        self.presentation.capture_events.clear();
        self.presentation.radar_events.clear();
        self.radar_event_serial = 0;
        self.presentation.disable_events.clear();
        self.occupancy.clear();
        self.partition.clear();
        self.supplies.clear();
        self.presentation.shots.clear();
        self.presentation.laser_shots.clear();
        self.presentation.propaganda_events.clear();
        self.presentation.assistance_events.clear();
        self.presentation.weapon_fx.clear();
        self.presentation
            .slave_welds
            .retain(|event| event.weld.frame + 1 >= self.tick);
        self.presentation.spectre_strafes.clear();
        self.presentation.attached_particles.clear();
        self.presentation.laser_initializations.clear();
        self.presentation.ability_feedback.clear();
        self.spectre_selections.clear();
        self.spawn_selection_events.clear();
        self.deselected_objects.clear();
        self.projectiles.clear();
        self.tick = 0;
        self.outcome = None;
        self.ended_at = None;
        self.end_game_at = None;
        self.defeated.fill(false);
        self.rebuild_navigation();
    }

    fn reset_players(&mut self, factions: Vec<Faction>) -> Result<(), &'static str> {
        // Native map startup runs inside GameLogic::update, even though this
        // adapter constructs the initial scene outside an ordinary simulation step.
        self.map_loading = true;
        let result = self.create_map_players(factions);
        self.map_loading = false;
        result
    }

    fn create_map_players(&mut self, factions: Vec<Faction>) -> Result<(), &'static str> {
        self.clear_match_state();
        self.players = factions
            .into_iter()
            .map(|faction| new_player(faction, self.rules.starting_cash))
            .collect();
        self.players.push(new_player(Faction::Civilian, 0));
        self.initialize_map_side_players()?;
        self.reset_player_identities()?;
        self.occupied.resize(self.players.len(), false);
        self.defeated.resize(self.players.len(), false);
        self.retaliation_enabled = vec![false; self.players.len()];
        self.tunnels = (0..self.players.len())
            .map(|_| tunnel::Network::new())
            .collect();
        self.visibility.reset(self.players.len());
        self.reset_progression();
        self.spawn_map_bridges()?;
        self.spawn_neutral_objects()?;
        for owner in 0..self.slots.len() {
            if !self.occupied[owner] {
                continue;
            }
            let center = self
                .type_for(owner, Kind::CommandCenter)
                .ok_or("Faction has no command center")?;
            let builder = self
                .type_for(owner, Kind::Builder)
                .ok_or("Faction has no builder")?;
            // GameLogic::placeNetworkBuildingsForPlayer places the command center
            // at Player_N_Start; it does not relocate it to a generated base site.
            let start = self.start_positions[owner].ok_or("Occupied player has no map start")?;
            let position = self.layout.starts[start].position;
            if self.cell_at(position).is_none() {
                return Err("Player start is outside the map");
            }
            let command_center = self.spawn(center, owner, position, true);
            self.native_build_complete(command_center);
            self.record_built(command_center);
            self.players[owner].statistics.money_spent += self.catalog[center].cost;
            self.update_ground_contacts();
            self.rebuild_navigation();
            let geometry = &self.catalog[center];
            let sphere_radius = if matches!(
                geometry.geometry.shape,
                crate::collision_geometry::Shape::Box
            ) {
                let major_squared = geometry.geometry.major.powi(2);
                let minor_squared = geometry.geometry.minor.powi(2);
                let height_squared = (geometry.geometry.height * 0.5).powi(2);
                (major_squared + minor_squared + height_squared).sqrt()
            } else {
                geometry.geometry.major.max(geometry.geometry.height * 0.5)
            };
            let wanted = self.layout.starts[start]
                .rally
                .unwrap_or([position[0], position[1] - sphere_radius * 0.5]);
            let mut random = self.random;
            let exit = self.starting_builder_position(builder, wanted, sphere_radius, &mut random);
            self.random = random;
            let exit = exit.ok_or("No builder spawn space")?;
            let builder = self.spawn(builder, owner, exit, true);
            self.native_build_complete(builder);
            self.record_built(builder);
        }
        self.update_ground_contacts();
        self.reset_opponent();
        self.update_power();
        self.update_visibility();
        Ok(())
    }

    #[must_use]
    pub fn type_for(&self, owner: usize, kind: Kind) -> Option<usize> {
        let faction = self.players.get(owner)?.faction;
        self.catalog
            .iter()
            .position(|unit| unit.faction == faction && unit.kind == kind)
    }

    /// # Errors
    /// Rejects dead/unfinished/immobile actors and blocked or unreachable destinations.
    pub fn move_to(&mut self, id: usize, position: [f32; 2]) -> Result<(), &'static str> {
        self.move_to_world(id, [position[0], position[1], self.ground_height(position)])
    }

    /// Issue the original picked world-space destination including bridge height.
    /// # Errors
    /// Rejects unavailable actors and invalid destinations.
    pub fn move_to_world(&mut self, id: usize, position: [f32; 3]) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if position.iter().any(|value| !value.is_finite()) {
            return Err("Destination is not finite");
        }
        if self.native_subclass_command(
            id,
            ai_dispatch::stored_command::Command {
                source: ai_goals::COMMAND_FROM_PLAYER,
                kind: ai_dispatch::stored_command::Kind::Move(position),
            },
        ) {
            return Ok(());
        }
        self.dozer_player_command(id);
        self.set_last_command_source(id, ai_goals::COMMAND_FROM_PLAYER);
        let result = self.move_to_ordinary_world(id, position);
        self.finish_dozer_player_command(id);
        result
    }

    fn move_to_ordinary_world(
        &mut self,
        id: usize,
        position: [f32; 3],
    ) -> Result<(), &'static str> {
        self.refresh_occupancy();
        let adjusted = self
            .adjusted_destination_world(id, position)
            .ok_or("Destination is blocked or unreachable")?;
        self.begin_move_world(id, adjusted)?;
        self.assault_command(id, Order::Move([adjusted[0], adjusted[1]]));
        Ok(())
    }

    fn begin_move_world(&mut self, id: usize, position: [f32; 3]) -> Result<(), &'static str> {
        self.active_actor(id)?;
        self.begin_move_ordinary_world(id, position)
    }

    fn begin_move_ordinary_world(
        &mut self,
        id: usize,
        world: [f32; 3],
    ) -> Result<(), &'static str> {
        if self.native_subclass_command(
            id,
            ai_dispatch::stored_command::Command {
                source: self.entities[id].ai.command.last_command_source,
                kind: ai_dispatch::stored_command::Kind::Move(world),
            },
        ) {
            return Ok(());
        }
        let position = [world[0], world[1]];
        let actor = self.active_actor(id)?;
        if self.catalog[actor.type_index].speed <= 0.0 || actor.disabled_held() {
            return Err("This object cannot move");
        }
        let locomotor = actor.active_locomotor(&self.catalog[actor.type_index]);
        if locomotor.ground.is_some() || locomotor.flight.is_some() {
            self.set_order_state(id, Order::Move(position), true, Some(world));
            if self.move_destination_adjusts(id) {
                let layer = self.movement_goal_layer(id, position);
                self.reserve_goal_on(id, position, layer);
            }
            return Ok(());
        }
        let mut path = self
            .movement_path(id, actor.position, position)
            .ok_or("Destination is blocked or unreachable")?;
        let goal = *path.optimized.back().ok_or("Path has no destination")?;
        path.optimized.pop_front();
        // Keep the requested float goal separately. Ordinary ground paths end
        // at their native grid node; quick paths retain the requested endpoint.
        self.set_order(id, Order::Move(position));
        self.set_ai_goal_position(id, world);
        self.entities[id].ai.command.move_to.position = world;
        let actor = &mut self.entities[id];
        // Replacing an exit state ends movement; source onExit/friend_endingMove
        // releases its ignored obstacle at the next logic update.
        actor.ai.ignored_producer = None;
        actor.ai.path = path;
        actor.ai.path_goal = Some(goal.cell);
        actor.ai.path_endpoint = Some(position);
        actor.ai.order = Order::Move(position);
        actor.ai.action_frames = 0.0;
        self.install_ground_path(id);
        self.reserve_goal(id, position);
        Ok(())
    }

    /// # Errors
    /// Rejects noncombatants, friendly/dead targets and invalid object IDs.
    pub fn attack(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.attack_command(id, target, false, false)
    }

    /// Orders a source force attack, including nonenemy targets.
    /// # Errors
    /// Rejects invalid actors, hidden/dead targets, or unavailable weapons.
    pub fn force_attack(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.attack_command(id, target, false, true)
    }

    pub(in crate::skirmish) fn attack_from_ai(
        &mut self,
        id: usize,
        target: usize,
    ) -> Result<(), &'static str> {
        self.active_actor(id)?;
        self.entities[id].ai.command.from_ai = true;
        self.set_last_command_source(id, ai_goals::COMMAND_FROM_AI);
        self.entities[id].ai.attack_context.forced = false;
        self.attack_object(id, target)
    }

    fn attack_command(
        &mut self,
        id: usize,
        target: usize,
        from_ai: bool,
        forced: bool,
    ) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        let victim = self.entities.get(target).ok_or("Target does not exist")?;
        if id == target
            || !victim.alive()
            || victim.masked()
            || !(from_ai || forced || self.player_attack_relationship(id, target))
        {
            return Err("Target relationship does not permit this attack");
        }
        if !self.attack_target_reachable_mode(id, target, forced) {
            return Err("This weapon set cannot reach that target");
        }
        let source = if from_ai {
            ai_goals::COMMAND_FROM_AI
        } else {
            ai_goals::COMMAND_FROM_PLAYER
        };
        if !from_ai {
            self.dozer_player_command(id);
        }
        if self.entities[id].chinook.is_some()
            || self.entities[id].aircraft.is_some()
            || self.catalog[self.entities[id].type_index]
                .behavior
                .hacking
                .is_some()
        {
            self.native_attack_goal_from(
                id,
                combat::attack::Goal::Object(target),
                i32::MAX.unsigned_abs(),
                source,
                forced,
            );
            if !from_ai {
                self.finish_dozer_player_command(id);
            }
            return Ok(());
        }
        self.entities[id].ai.command.from_ai = from_ai;
        self.set_last_command_source(id, source);
        self.entities[id].ai.attack_context.forced = forced;
        if !from_ai && !self.entities[id].behaviors.spawns.is_empty() {
            let slaves = self.group_spawn_attack(id, target, source);
            if self.entities[id].ai.registration.is_some() {
                self.native_attack_goal_from(
                    id,
                    combat::attack::Goal::Object(target),
                    i32::MAX.unsigned_abs(),
                    source,
                    forced,
                );
            }
            self.finish_dozer_player_command(id);
            return if slaves || self.entities[id].ai.attack.is_some() {
                Ok(())
            } else {
                Err("No spawned weapon can reach that target")
            };
        }
        if self.transport_ai(id) || self.entities[id].chinook.is_some() {
            self.native_attack_goal_from(
                id,
                combat::attack::Goal::Object(target),
                i32::MAX.unsigned_abs(),
                source,
                forced,
            );
            if !from_ai {
                self.finish_dozer_player_command(id);
            }
            return Ok(());
        }
        let passengers = self.order_container_attack(id, target);
        self.assault_command_from(id, Order::Attack(target), None, source, forced);
        let result = self.attack_object(id, target);
        if result.is_ok() {
            self.entities[id].ai.command.from_ai = from_ai;
        }
        if from_ai {
            self.worker_drop_mine_clearing_cargo(id);
        } else {
            self.finish_dozer_player_command(id);
        }
        if passengers { Ok(()) } else { result }
    }

    fn attack_object(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.active_actor(id)?;
        let victim = self.entities.get(target).ok_or("Target does not exist")?;
        if id == target
            || !victim.alive()
            || victim.masked()
            || !self.attack_relationship(id, target)
        {
            return Err("Choose a living enemy target");
        }
        // WeaponSet tests every allocated damage weapon before selecting a slot.
        if !(0..3).any(|slot| self.weapon_available(id, slot)) {
            return Err("This object has no weapon");
        }
        if !self.weapon_target_reachable(id, target) {
            return Err("This weapon set cannot reach that target");
        }
        self.set_order(id, Order::Attack(target));
        Ok(())
    }

    /// Pays once when a valid building foundation is placed.
    ///
    /// # Errors
    /// Rejects missing prerequisites, insufficient cash, obstructed placement, and unreachable sites.
    pub fn build(
        &mut self,
        worker: usize,
        type_index: usize,
        position: [f32; 2],
    ) -> Result<usize, &'static str> {
        let angle = self
            .catalog
            .get(type_index)
            .ok_or("Unknown building type")?
            .placement_angle;
        self.build_at_angle(worker, type_index, position, angle, false)
    }

    /// Places a building using the facing supplied by the original placement gesture.
    ///
    /// # Errors
    /// Rejects invalid builders, unavailable construction or an illegal rotated site.
    pub fn build_facing(
        &mut self,
        worker: usize,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
    ) -> Result<usize, &'static str> {
        self.build_at_angle(worker, type_index, position, angle, false)
    }

    fn build_at_angle(
        &mut self,
        worker: usize,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
        computer: bool,
    ) -> Result<usize, &'static str> {
        let actor = if computer {
            self.active_actor(worker)?
        } else {
            self.ready_actor(worker)?
        };
        let owner = actor.owner;
        if self.catalog[actor.type_index].kind != Kind::Builder {
            return Err("Select a builder first");
        }
        let unit = self
            .catalog
            .get(type_index)
            .ok_or("Unknown building type")?;
        if !unit.kind.is_building() || unit.faction != self.catalog[actor.type_index].faction {
            return Err("Builder cannot construct this type");
        }
        if !self.production_possible(worker, type_index) {
            return Err("Required construction command or prerequisite is unavailable");
        }
        if !angle.is_finite() || !position.iter().all(|value| value.is_finite()) {
            return Err("Building site is obstructed or outside the map");
        }
        let cost = self.build_cost(owner, type_index);
        self.clear_rubble_for_construction(type_index, position, angle);
        let world = [position[0], position[1], self.ground_height(position)];
        if !self.move_objects_for_construction(owner, type_index, world, angle) && !computer {
            return Err("Construction area contains objects that cannot move");
        }
        // buildObjectNow sends aiIdle before Dozer/Worker::construct validates.
        self.set_ai_order(worker, Order::Idle);
        self.clear_ai_goal(worker);
        self.entities[worker].ai.command.from_ai = true;
        if !computer {
            self.check_prerequisite(owner, type_index)?;
            if self.players[owner].cash < cost {
                return Err("Not enough money");
            }
            if !self.can_place_for_builder(type_index, position, angle, false, Some(worker)) {
                return Err("Building site is obstructed or outside the map");
            }
            if !self.build_site_reachable(worker, position) {
                return Err("Builder cannot reach this site");
            }
        }
        self.set_dozer_rebuilding(worker, false);
        let building = self.spawn(type_index, owner, position, false);
        self.entities[building].producer = Some(worker);
        self.entities[building].maintenance.builder = Some(worker);
        self.worker_leaving_supply(worker);
        self.players[owner].cash = self.players[owner].cash.saturating_sub(cost);
        self.set_object_orientation(building, angle);
        self.rebuild_navigation();
        self.entities[building].producer = Some(worker);
        self.entities[building].maintenance.builder = Some(worker);
        if self.entities[worker].ai.dozer.is_some() {
            self.new_dozer_task(worker, ai_dispatch::dozer::BUILD, building);
            return Ok(building);
        }
        self.set_order(worker, Order::Build(building));
        let dock = self.construction_approach(worker, building);
        self.entities[worker].ai.construction_dock = dock;
        Ok(building)
    }

    /// # Errors
    /// Rejects invalid builders, enemy structures or completed/dead construction sites.
    pub fn assist_build(&mut self, worker: usize, building: usize) -> Result<(), &'static str> {
        self.ready_actor(worker)?;
        self.resume_build(worker, building)
    }

    fn resume_build(&mut self, worker: usize, building: usize) -> Result<(), &'static str> {
        let actor = self.active_actor(worker)?;
        let target = self
            .entities
            .get(building)
            .ok_or("Building does not exist")?;
        if self.catalog[actor.type_index].kind != Kind::Builder
            || target.owner != actor.owner
            || !target.alive()
            || target.build_progress >= 1.0
            || !self.catalog[target.type_index].kind.is_building()
        {
            return Err("Choose an unfinished friendly building and a builder");
        }
        if self.entities[worker].ai.dozer.is_some() {
            return self.dozer_resume_command(worker, building);
        }
        self.set_order(worker, Order::Build(building));
        let dock = self.construction_approach(worker, building);
        self.entities[worker].ai.construction_dock = dock;
        Ok(())
    }

    /// # Errors
    /// Rejects incompatible production facilities, unavailable types, a full queue or insufficient cash.
    pub fn train(&mut self, producer: usize, type_index: usize) -> Result<(), &'static str> {
        self.ready_actor(producer)?;
        self.queue_unit(producer, type_index)
    }

    fn queue_unit(&mut self, producer: usize, type_index: usize) -> Result<(), &'static str> {
        let actor = self
            .entities
            .get(producer)
            .filter(|actor| actor.exists())
            .ok_or("Production object is missing")?;
        let owner = actor.owner;
        let unit = self.catalog.get(type_index).ok_or("Unknown unit type")?;
        if self.catalog[actor.type_index].production.is_none()
            || (self.catalog[actor.type_index]
                .behavior
                .production_exit
                .is_none()
                && self.catalog[actor.type_index].parking.is_none())
        {
            return Err("This building cannot produce that unit");
        }
        if !self.production_queue_available(producer) {
            return Err("Production queue is full");
        }
        self.check_production_prerequisite(producer, type_index)?;
        let cost = self.build_cost(owner, type_index);
        if self.players[owner].cash < cost {
            return Err("Not enough money");
        }
        let id = self.entities[producer].production_next_id(0);
        let next_id = id.checked_add(1).ok_or("Production IDs exhausted")?;
        let seconds = unit.build_seconds;
        let quantity = self.catalog[actor.type_index]
            .production
            .iter()
            .flatten()
            .find(|(name, _)| {
                self.catalog
                    .iter()
                    .find(|candidate| &candidate.name == name)
                    .is_some_and(|candidate| candidate.equivalent_to(unit))
            })
            .map_or(1, |(_, quantity)| *quantity);
        let parking_door = self.queue_parking_door(producer, type_index)?;
        self.players[owner].cash -= cost;
        self.entities[producer].next_production_id = next_id;
        self.entities[producer].queue.push_back(Production {
            quantity,
            produced: 0,
            parking_door,
            id,
            item: ProductionItem::Unit(type_index),
            remaining_seconds: seconds,
            total_seconds: seconds,
            elapsed_frames: 0.0,
        });
        self.entities[producer].production_added();
        Ok(())
    }

    /// # Errors
    /// Rejects noncollectors and unknown or empty depots.
    pub fn harvest(&mut self, id: usize, supply: usize) -> Result<(), &'static str> {
        let actor = self.ready_actor(id)?;
        if self.catalog[actor.type_index].supply.capacity == 0 {
            return Err("This unit cannot collect supplies");
        }
        if self
            .supplies
            .get(supply)
            .is_none_or(|depot| depot.boxes == 0)
        {
            return Err("Supply depot is empty or missing");
        }
        let object = self.supplies[supply].object;
        if !self.can_transfer_supplies(id, object) {
            return Err("Supply depot is unavailable");
        }
        self.ai_dock(
            id,
            economy::DockTarget::Warehouse(supply),
            ai_goals::COMMAND_FROM_PLAYER,
        );
        Ok(())
    }

    /// # Errors
    /// Rejects objects that cannot accept an order.
    pub fn stop(&mut self, id: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        self.dozer_player_command(id);
        self.set_last_command_source(id, ai_goals::COMMAND_FROM_PLAYER);
        self.stop_garrison(id);
        let result = self.stop_actor(id);
        self.finish_dozer_player_command(id);
        self.group_spawn_idle(id, ai_goals::COMMAND_FROM_PLAYER);
        result
    }

    fn stop_actor(&mut self, id: usize) -> Result<(), &'static str> {
        self.active_actor(id)?;
        if self.entities[id].chinook.is_some()
            || self.entities[id].aircraft.is_some()
            || self.catalog[self.entities[id].type_index]
                .behavior
                .hacking
                .is_some()
        {
            self.native_idle_from(id, self.entities[id].ai.command.last_command_source);
            return Ok(());
        }
        self.assault_command(id, Order::Idle);
        if self.entities[id].ai.command.last_command_source == ai_goals::COMMAND_FROM_PLAYER
            && self.entities[id].ai.supply.constructed()
            && !self.worker_interface(id)
        {
            self.entities[id].ai.supply.force_busy = true;
        }
        self.set_order(id, Order::Idle);
        self.clear_ai_goal(id);
        Ok(())
    }

    /// Advances one 30 Hz logic frame, independent of rendering speed.
    pub fn step(&mut self) {
        self.clear_frame_events();
        if self.end_game_at.is_some_and(|frame| self.tick >= frame) {
            return;
        }
        self.logic_update = true;
        self.tick = self.tick.saturating_add(1);
        self.presentation
            .radar_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.expire_crates();
        self.presentation
            .hacking_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .capture_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .disable_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.refresh_occupancy();
        self.update_bridge_states();
        self.update_map_scripts();
        self.update_opponent_scripts();
        self.update_repair_markers();
        self.update_sales();
        self.update_power();
        self.refresh_occupancy();
        let physics_count = self.entities.len();
        self.update_physics(physics_count);
        self.update_attachments();
        // AIUpdate runs PHASE_INITIAL; AutoHealBehavior uses PHASE_NORMAL.
        let mut spawner = 0;
        while spawner < self.entities.len() {
            self.update_free_collector(spawner);
            spawner += 1;
        }
        self.update_projectile_streams();
        self.update_deaths(physics_count);
        self.run_object_updates();
        self.update_ground_contacts();
        self.update_deaths(self.entities.len());
        // GameLogic finishes object update phases before AI::update services
        // queued paths, then PlayerList updates the computer players.
        self.process_path_service();
        self.update_map_build_lists();
        self.update_opponent();
        self.update_native_money_buckets();
        self.update_visibility();
        self.process_update_destruction();
        self.update_direct_weapon_damage();
        self.update_victory();
        self.expire_hacked();
        self.expire_paralysis();
        self.logic_update = false;
    }

    fn update_physics(&mut self, count: usize) {
        // Source update phases run every AI before any PhysicsBehavior.
        for id in 0..count {
            // Native projectile controls and Physics are owned by the shared heap.
            if self.registered_physics(id) || self.entities[id].projectile_flight.is_some() {
                continue;
            }
            self.update_dead_ground(id);
        }
        for id in 0..count {
            // Native projectile controls and Physics are owned by the shared heap.
            if self.registered_physics(id) || self.entities[id].projectile_flight.is_some() {
                continue;
            }
            let actor = &mut self.entities[id];
            let unit = &self.catalog[actor.type_index];
            if actor.complete()
                && actor.contained_by().is_none()
                && actor.active_locomotor(unit).ground.is_some()
                && !actor.live_physics_active()
                && !actor.disabled_held()
            {
                locomotor::integrate_ground(actor, unit, self.tick);
                self.update_position_claim(id);
                self.observe_ground_partition(id);
            }
        }
        for id in 0..count {
            // Native projectile controls and Physics are owned by the shared heap.
            if self.registered_physics(id) || self.entities[id].projectile_flight.is_some() {
                continue;
            }
            if self.entities[id].complete() && self.entities[id].contained_by().is_none() {
                self.integrate_flight(id);
                self.integrate_aircraft_taxi_height(id);
                self.integrate_chinook(id);
                self.clip_chinook(id);
                if self.entities[id].chinook.is_some() || self.entities[id].flight.is_some() {
                    self.update_position_claim(id);
                    self.observe_ground_partition(id);
                }
            }
        }
        for id in 0..count {
            // Native projectile controls and Physics are owned by the shared heap.
            if self.registered_physics(id) || self.entities[id].projectile_flight.is_some() {
                continue;
            }
            self.integrate_death_body(id);
            self.integrate_live_physics(id);
            self.integrate_physical_object(id);
        }
    }

    fn clear_frame_events(&mut self) {
        self.presentation.terrain_movement_events.clear();
        self.presentation.money_crate_events.clear();
        self.presentation.pilot_events.clear();
        self.presentation.stealth_events.clear();
        self.presentation.bone_fx.clear();
        self.presentation.body_particles.clear();
        self.presentation.object_fx_events.clear();
        self.presentation.structure_events.clear();
        self.presentation.object_sounds.clear();
        self.presentation
            .car_bomb_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .hijacker_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .sabotage_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .laser_shots
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation.shots.clear();
        self.presentation
            .battle_plan_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .research_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .replacement_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .promotion_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .salvage_events
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation
            .container_sounds
            .retain(|event| event.frame + 1 >= self.tick);
        self.presentation.propaganda_events.clear();
        self.presentation.assistance_events.clear();
        self.presentation.weapon_fx.clear();
        self.presentation
            .slave_welds
            .retain(|event| event.weld.frame + 1 >= self.tick);
        self.presentation.spectre_strafes.clear();
        self.presentation.attached_particles.clear();
        self.presentation.laser_initializations.clear();
        self.presentation.ability_feedback.clear();
        self.spectre_selections.clear();
        self.spawn_selection_events.clear();
        self.deselected_objects.clear();
    }

    fn update_order(&mut self, id: usize) {
        if self.entities[id].ai_busy() {
            return;
        }
        if self.entities[id].unmanned() {
            return;
        }

        match self.entities[id].ai.order {
            Order::Enter(carrier) => self.update_enter(id, carrier),
            Order::ExitContainer => self.update_container_exit(id),
            Order::HackInternet => self.update_hacking(id),
            Order::Idle => self.acquire_target(id),
            Order::Move(goal) => {
                let locomotor =
                    self.entities[id].active_locomotor(&self.catalog[self.entities[id].type_index]);
                let complete = if locomotor.ground.is_some() || locomotor.flight.is_some() {
                    !matches!(
                        self.update_primary_ground_move(id),
                        ground_movement::MoveResult::Continue
                    )
                } else {
                    self.travel_to(id, goal, 0.1)
                };
                if complete {
                    self.entities[id].ai.ignored_producer = None;
                    if self.entities[id].cleaning_area().is_some() {
                        self.cleanup_order(id, Order::Idle);
                    } else {
                        // AIStates.cpp686 returns to Idle without a new command;
                        // independent SpecialAbility state survives this move.
                        self.set_ai_order(id, Order::Idle);
                    }
                }
            }
            Order::Exit(_) | Order::FollowPath => self.update_native_follow_path(id),
            Order::CleanHazard(target) => self.update_cleanup_attack(id, target),
            Order::FireAt { .. } => self.update_fire_position(id),
            Order::Attack(target) => {
                if !self.update_attack(id, target) {
                    self.set_order(id, Order::Idle);
                }
            }
            Order::AttackMove(goal) => self.update_attack_move(id, goal),
            Order::AttackArea => self.update_attack_area(id),
            Order::Guard(goal) => self.update_guard(id, goal),
            Order::Build(building) => self.update_construction(id, building),
            Order::Repair(building) => self.update_repair(id, building),
            Order::RepairAt(pad) => self.update_repair_pad(id, pad),
            Order::Harvest(supply) => self.update_harvest(id, supply),
            Order::ReturnSupply => self.update_dropoff(id),
        }
    }

    fn update_ground_locomotor(&mut self, id: usize) -> u32 {
        if self.catalog[self.entities[id].type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "IMMOBILE")
        {
            return module_updates::FOREVER;
        }
        self.select_surface_locomotor(id);
        let actor = &self.entities[id];
        if actor.is_ai_dead()
            && !actor
                .active_locomotor(&self.catalog[actor.type_index])
                .works_when_dead
        {
            return 1;
        }
        if self.entities[id].ai.motion.follows_path() {
            if self.entities[id].ai.ground_path.is_none() {
                // POSITION_ON_PATH cannot generate forces while waiting for its path.
                return module_updates::FOREVER;
            }
            self.sample_ground_path_force(id);
        }
        self.sample_climb_terrain(id);
        let terrain = physical_ocl::transform::TerrainContext {
            terrain: self.projectile_terrain.as_ref(),
            layers: &self.movement_terrain,
            height: self.object_transform(id).0[2],
        };
        let mass = self.object_physics_mass(id);
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if !actor.complete() {
            return 1;
        }
        if actor.active_locomotor(unit).ground.is_some() {
            match actor.ai.motion.take_request() {
                Some(_) if actor.physics_stunned() => {}
                Some(locomotor::GroundRequest::Path(_)) if actor.disabled_held() => {
                    locomotor::maintain_ground(actor, self.tick);
                }
                Some(locomotor::GroundRequest::Path(goal)) => {
                    let surfaces = actor.active_locomotor(unit).legal_surfaces;
                    let layer = actor.layer;
                    let mass = mass.unwrap_or_else(|| {
                        actor
                            .active_locomotor(unit)
                            .ground
                            .as_ref()
                            .expect("ground rules")
                            .mass
                    });
                    locomotor::move_ground(
                        actor,
                        unit,
                        &goal,
                        mass,
                        self.tick,
                        &terrain,
                        |point| {
                            self.movement_terrain
                                .valid_movement_on(layer, point, surfaces)
                        },
                    );
                }
                Some(locomotor::GroundRequest::Angle(angle)) => {
                    locomotor::turn_ground_angle(actor, unit, angle, self.tick, &terrain);
                }
                None => locomotor::maintain_ground(actor, self.tick),
            }
        }
        if actor.active_locomotor(unit).wander_length.is_some() {
            let threshold = actor.active_locomotor(unit).airborne_targeting_height;
            let airborne = self.height_above_ground(id) > threshold;
            self.entities[id].set_airborne_target(airborne);
        }
        self.drain_ai_physics_force_wake(id);
        let actor = &self.entities[id];
        if actor
            .active_locomotor(&self.catalog[actor.type_index])
            .ground
            .is_some()
            && !actor.ai.motion.has_goal()
            && !actor.ai.blocking.blocked
        {
            module_updates::FOREVER
        } else {
            1
        }
    }

    fn ready_actor(&self, id: usize) -> Result<&Entity, &'static str> {
        if self.end_game_at.is_some() {
            return Err("Match has ended; restart to play again");
        }
        self.active_actor(id)
    }

    // ScriptActions disables human input at victory/defeat; GameLogic still
    // updates objects and player AI until ScriptEngine's end-game timer expires.
    fn active_actor(&self, id: usize) -> Result<&Entity, &'static str> {
        self.entities
            .get(id)
            .filter(|entity| {
                entity.complete() && !entity.unmanned() && entity.contained_by().is_none()
            })
            .ok_or("Object is dead, unfinished or missing")
    }

    fn set_order(&mut self, id: usize, order: Order) {
        self.set_order_state(id, order, true, None);
    }

    fn set_ai_order(&mut self, id: usize, order: Order) {
        self.set_order_state(id, order, false, None);
    }

    fn set_order_state(
        &mut self,
        id: usize,
        order: Order,
        cancel_abilities: bool,
        goal_position: Option<[f32; 3]>,
    ) {
        self.set_order_state_from(id, order, cancel_abilities, goal_position, None);
    }

    fn set_order_state_from(
        &mut self,
        id: usize,
        order: Order,
        cancel_abilities: bool,
        goal_position: Option<[f32; 3]>,
        command_source: Option<i32>,
    ) {
        if let Some(ground_move) =
            self.prepare_order_state(id, order, cancel_abilities, goal_position, command_source)
        {
            self.enter_order_state(id, order, ground_move);
        }
    }

    fn prepare_order_state(
        &mut self,
        id: usize,
        order: Order,
        cancel_abilities: bool,
        goal_position: Option<[f32; 3]>,
        command_source: Option<i32>,
    ) -> Option<bool> {
        if self.entities[id].ai.command.locked
            || matches!(
                self.entities[id].ai.command.main_state,
                ai_goals::MainState::Dead
            )
        {
            return None;
        }
        self.cancel_hacking(id);
        self.exit_hunt(id);
        self.exit_rappel(id);
        self.exit_native_follow_path(id);
        self.exit_chinook_phase(id);
        self.exit_aircraft_phase(id);
        if self.entities[id].ai.dozer.is_some() {
            self.entities[id].drawable.model_conditions.constructing = false;
        }
        self.exit_waypoint_state(id);
        self.exit_repulsor_state(id);
        self.exit_attack_move(id);
        self.exit_native_attack(id);
        self.entities[id].ai.command_revision = self.entities[id]
            .ai
            .command_revision
            .checked_add(1)
            .expect("AI command revision overflow");
        self.entities[id].ai.primary.sleep_until = 0;
        if !matches!(order, Order::Attack(_)) {
            self.entities[id].ai.attack_context.forced = false;
        }
        if !matches!(order, Order::AttackArea) {
            self.entities[id].ai.command.attack_area = None;
        }
        self.record_ai_order_goal(id, order);
        if let Some(position) = goal_position {
            self.set_ai_goal_position(id, position);
            if matches!(order, Order::Move(_)) {
                self.entities[id].ai.command.move_to.position = position;
            }
        }
        self.entities[id].ai.guard.retaliating = false;
        let previous = self.entities[id].ai.order;
        self.entities[id]
            .maintenance
            .ordered(self.tick, previous, order);
        let locomotor =
            self.entities[id].active_locomotor(&self.catalog[self.entities[id].type_index]);
        let ground_move = (locomotor.ground.is_some() || locomotor.flight.is_some())
            && (matches!(previous, Order::Move(_)) || matches!(order, Order::Move(_)));
        if !ground_move {
            self.entities[id].ai.path_request.cancel();
        }
        self.entities[id].ai.command.attack_approach = None;
        if let Some(flight) = &mut self.entities[id].flight {
            flight.ordered();
        }
        self.entities[id].ai.command.ability_goal = None;
        if let Some(source) = command_source {
            self.set_last_command_source(id, source);
            self.entities[id].ai.command.from_ai = source == ai_goals::COMMAND_FROM_AI;
        }
        self.container_notifications(id, order);
        self.reset_garrison_aim(id);
        if cancel_abilities {
            self.cancel_order_abilities(id, order);
        }
        self.clear_temporary(id);
        self.cancel_docking(id);
        self.entities[id].reset_order_state(order, ground_move);
        Some(ground_move)
    }

    fn cancel_order_abilities(&mut self, id: usize, order: Order) {
        if !matches!(order, Order::Attack(_) | Order::FireAt { .. })
            && matches!(self.entities[id].weapon_set.lock, weapons::Lock::Temporary)
        {
            self.entities[id].weapon_set.lock = weapons::Lock::None;
        }
    }

    fn enter_order_state(&mut self, id: usize, order: Order, ground_move: bool) {
        if !ground_move {
            self.reserve_goal(id, self.entities[id].position);
        }
        self.ai_state_changed(id);
        if ground_move
            && matches!(order, Order::Move(_))
            && matches!(
                self.enter_primary_ground_move(id),
                ground_movement::MoveResult::Failure
            )
        {
            self.entities[id].ai.order = Order::Idle;
            self.enter_primary_idle(id);
        }
        match order {
            Order::Idle => self.enter_primary_idle(id),
            Order::Harvest(source) => {
                self.enter_docking(id, economy::DockTarget::Warehouse(source));
            }
            Order::ReturnSupply => {
                if let Some(object) = self.entities[id].ai.command.goal_object {
                    self.enter_docking(id, economy::DockTarget::Center(object));
                } else {
                    self.set_ai_order(id, Order::Idle);
                }
            }
            Order::RepairAt(object) => self.enter_docking(id, economy::DockTarget::Repair(object)),
            Order::Enter(carrier) => {
                if !self.enter_native_enter(id, carrier) {
                    self.set_ai_order(id, Order::Idle);
                }
            }
            Order::AttackMove(_) => {
                if !self.enter_attack_move(id) {
                    self.set_ai_order(id, Order::Idle);
                }
            }
            Order::Attack(target) => {
                if !self.enter_native_attack(id, combat::attack::Goal::Object(target)) {
                    self.entities[id].ai.order = Order::Idle;
                    self.enter_primary_idle(id);
                }
            }
            Order::FireAt { .. } => {
                let position = self.entities[id].ai.command.goal_position;
                if !self.enter_native_attack(id, combat::attack::Goal::Position(position)) {
                    self.entities[id].ai.order = Order::Idle;
                    self.enter_primary_idle(id);
                }
            }
            _ => {}
        }
    }

    fn spawn(
        &mut self,
        type_index: usize,
        owner: usize,
        position: [f32; 2],
        complete: bool,
    ) -> usize {
        let id = self.new_object(type_index, owner, complete);
        let unit = &self.catalog[self.entities[id].type_index];
        let height = self.ground_height(position) + unit.preferred_height;
        let facing = unit.placement_angle;
        self.set_object_position(id, [position[0], position[1], height]);
        self.set_object_orientation(id, facing);
        self.observe_ground_partition(id);
        id
    }

    fn new_object_on_team(
        &mut self,
        type_index: usize,
        owner: usize,
        team: Option<usize>,
    ) -> usize {
        let id = self.allocate_object(type_index, owner, true);
        self.map_unit_produced(None, id);
        self.team_membership.set_team(id, team);
        self.register_object_updates(id);
        id
    }

    // ThingFactory::newObject: constructors and CREATE see Thing's identity
    // transform. The caller places the object only after the factory returns.
    fn new_object(&mut self, type_index: usize, owner: usize, complete: bool) -> usize {
        let id = self.allocate_object(type_index, owner, complete);
        self.map_unit_produced(None, id);
        self.register_object_updates(id);
        id
    }

    fn allocate_object(&mut self, type_index: usize, owner: usize, complete: bool) -> usize {
        let type_index = self.resolve_variation(type_index);
        let entity = self.create_entity(&self.catalog[type_index], type_index, owner, complete);
        let id = self.entities.len();
        self.entities.push(entity);
        self.update_objects.push(module_updates::Object::new(id));
        id
    }

    fn spawn_native_projectile(
        &mut self,
        type_index: usize,
        owner: usize,
        position: [f32; 3],
        projectile_id: u64,
    ) -> usize {
        let id = self.allocate_object(type_index, owner, true);
        self.entities[id].native_projectile = Some(projectile_id);
        self.map_unit_produced(None, id);
        self.register_object_updates(id);
        self.set_object_position(id, position);
        id
    }

    fn spawn_native_hazard(
        &mut self,
        type_index: usize,
        owner: usize,
        position: [f32; 3],
        hazard_id: u64,
    ) -> usize {
        let id = self.allocate_object(type_index, owner, true);
        self.entities[id].native_hazard = Some(hazard_id);
        self.map_unit_produced(None, id);
        self.register_object_updates(id);
        self.set_object_position(id, position);
        id
    }

    fn create_entity(
        &self,
        unit: &UnitType,
        type_index: usize,
        owner: usize,
        complete: bool,
    ) -> Entity {
        let position = [0.0; 2];
        Entity {
            behaviors: behavior_instances::State::new(),
            mines: minefield::State::new(),
            selectable: unit.selectable,
            drawable: DrawableState::for_construction(complete),
            native_projectile: None,
            projectile_flight: None,
            native_hazard: None,
            physics_control: force_event::PhysicsControl::new(),
            ai: ai_dispatch::State::new(unit, self.tick, position, &self.ai_rules),
            update_owner: self.update_objects.len(),
            spy_vision: spy_vision::State::new(),
            radar: radar_update::State::new(),
            disabled_status: 0,
            disabled_observed: 0,
            crew: crew::State::new(),
            queue_exit: production_exit::QueueState::new(unit.behavior.production_exit.as_ref()),
            aircraft: None,
            parking: None,
            fire: None,
            stealth: stealth::State::for_unit(unit, self.tick),
            flammable: fire::flammable::ObjectState::new(),
            car_bomb: None,
            death_lifecycle: None,
            structure_death: None,
            spectre: None,
            deployment: None,
            physical: None,
            helix: helix::State::new(),
            parachute: None,
            poison: hazards::PoisonState::new(),
            battle_plan: battle_plan::State::for_unit(unit),
            battle_bonuses: battle_plan::Bonuses::new(),
            salvage: salvage::State::new(unit.behavior.money_crate.is_some()),
            attachment: overlord::State::new(),
            helpers: object_helpers::State::new(),
            object_status: 0,
            private_status: u8::from(matches!(unit.body_kind, BodyKind::Inactive)),
            horde: horde::State::new(),
            flight: None,
            chinook: None,
            containment: containment::State::new(),
            assault: None,
            delivery: None,
            hacking: income::State::new(),
            maintenance: maintenance::MaintenanceState::new(self.tick),
            type_index,
            owner,
            position,
            shroud_range_override: None,
            placed_height_offset: Some(-self.ground_height(position)),
            facing: 0.0,
            transform: None,
            layer: 1,
            trigger_position: [0; 2],
            // DozerAIUpdate::constructObject creates a foundation at exactly one HP.
            health: if complete { unit.initial_health } else { 1.0 },
            max_health: unit.max_health,
            experience: 0,
            experience_sink: None,
            veterancy: 0,
            veterancy_upgrades: 0,
            build_progress: if complete { 1.0 } else { 0.0 },
            cargo: 0,
            docking_flags: 0,
            dock: None,
            queue: VecDeque::new(),
            additional_production: Vec::new(),
            production_door_flags: 0,
            next_production_id: 1,
            rally_point: None,
            damage_state: combat::DamageState::for_unit(unit, complete),
            body_geometry: None,
            firing: firing::FiringTracker::new(),
            weapon_set: weapons::State::new(unit),
            hacked_until: 0,
            collector_pending: complete && unit.kind == Kind::SupplyCenter,
            production_doors: parking::doors::State::new(),
            health_box_offset: [0.0; 3],
            battle_bus: unit.battle_bus.as_ref().map(|_| battle_bus::State::new()),
            live_physics: None,
            producer: None,
            construction_percent: if complete { 100.0 } else { 0.0 },
        }
    }
    fn initialize_created_object(&mut self, id: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let height = self.object_position(id)[2];
        let flight = unit
            .locomotor
            .flight
            .as_ref()
            .map(|_| Box::new(flight::Motion::new(height)));
        self.entities[id].flight = flight;
        // Unregistered legacy AI remains outside the native dispatcher. The
        // registered AI constructor already selected NORMAL at its declaration.
        if self.entities[id].ai.registration.is_none() {
            self.initialize_locomotor_set(id);
        }
        if self.entities[id].aircraft.is_some() {
            self.choose_locomotor_set(id, locomotor::set::TAXI);
        }
        self.initialize_fire(id);
        self.initialize_death_lifecycle(id);
        self.set_mine_clearing(id, true);
        self.initialize_physical_object(id);
        self.initialize_battle_bonuses(id);
        if self.entities[id].battle_plan.is_some() {
            self.entities[id].ai.turret.enabled = false;
            self.entities[id].weapon_set.lock = weapons::Lock::Temporary;
        }
        self.initialize_crate(id);
        self.activate_upgrade_modules(id);
        self.initialize_progression_modules(id);
        self.observe_ground_partition(id);
    }
}

fn validate_layout(layout: &MapLayout) -> Result<(), &'static str> {
    if !layout.partition_cell_size.is_finite() || layout.partition_cell_size < 1.0 {
        return Err("Invalid partition cell size");
    }
    if !(2..=8).contains(&layout.starts.len()) {
        return Err("Map requires between two and eight player starts");
    }
    Ok(())
}

fn validate_catalog(catalog: &[UnitType], rules: &EconomyRules) -> Result<(), &'static str> {
    validate_economy(rules)?;
    tunnel::validate(catalog)?;
    containment::validate(catalog)?;
    countermeasures::validate(catalog)?;
    if catalog.is_empty() {
        return Err("Empty unit catalog");
    }
    for unit in catalog {
        battle_plan::validate(unit)?;
        salvage::validate(unit, catalog)?;
        maintenance::validate(unit, catalog)?;
        minefield::validate(unit, catalog)?;
        overlord::validate(unit, catalog)?;
        horde::validate(unit)?;
        eject_pilot::validate(unit, catalog)?;
        create_object_die::validate(unit, catalog)?;
        spy_vision::validate(&unit.behavior.spy_vision)?;
        microwave::validate(unit.maintenance.subdual.as_ref())?;
        point_defense::validate(unit)?;
        drones::validate(unit, catalog)?;
        spawn::validate(unit, catalog)?;
        if let Some(rules) = &unit.behavior.hacking {
            income::validate(rules)?;
        }

        veterancy::validate(unit)?;
        locomotor::validate(&unit.locomotor)?;
        for variant in unit
            .locomotor_sets
            .normal
            .iter()
            .chain(&unit.locomotor_sets.wander)
            .chain(&unit.locomotor_sets.panic)
        {
            locomotor::validate(&variant.locomotor)?;
            if !variant.speed.is_finite() || variant.speed <= 0.0 {
                return Err("Invalid surface locomotor speed");
            }
        }
        weapons::validate(unit)?;
        validate_unit_values(unit)?;
        if unit.behavior.production_exit.as_ref().is_some_and(|exit| {
            exit.create
                .iter()
                .chain(&exit.rally)
                .any(|value| !value.is_finite())
        }) {
            return Err("Invalid production exit coordinates");
        }
        if unit.speed > 0.0 && unit.locomotor.turn_rate <= 0.0 {
            return Err("Moving unit has no turn rate");
        }
        validate_supply(unit)?;
        healing::validate(unit)?;
    }
    Ok(())
}

fn validate_unit_values(unit: &UnitType) -> Result<(), &'static str> {
    let values = [
        unit.build_seconds,
        unit.max_health,
        unit.initial_health,
        unit.speed,
        unit.locomotor.speed_damaged,
        unit.locomotor.turn_rate,
        unit.locomotor.turn_rate_damaged,
        unit.supply.gather_seconds,
        unit.supply.dropoff_seconds,
        unit.supply.scan_distance,
        unit.geometry.major,
        unit.geometry.minor,
        unit.geometry.height,
        unit.geometry_bounds.center_height,
        unit.geometry_bounds.sphere_radius,
        unit.geometry_bounds.max_height,
        unit.fence_width,
        unit.factory_bib.exit_width,
        unit.factory_bib.extra_width,
    ];
    if values
        .iter()
        .chain(&unit.armor)
        .any(|value| !value.is_finite() || *value < 0.0)
        || (unit.build_seconds <= 0.0 && unit.behavior.mobs.is_empty())
        || unit.build_seconds > 3600.0
        || (unit.initial_health <= 0.0
            && !matches!(unit.body_kind, BodyKind::Inactive)
            && unit.kind != Kind::Effect
            && unit.kind != Kind::Crate
            && unit.variations.is_empty())
        || unit.initial_health > unit.max_health
        || !unit.placement_angle.is_finite()
        || unit.speed > 1000.0
        || !unit.locomotor.turn_pivot_offset.is_finite()
        || !unit.locomotor.damage_health_threshold.is_finite()
        || !(0.0..=1.0).contains(&unit.locomotor.damage_health_threshold)
        || !(0..=2).contains(&unit.locomotor.move_priority)
        || i16::try_from(unit.energy).is_err()
    {
        eprintln!(
            "Invalid source values for {}: build {}, health {}/{}, speed {}, geometry [{}, {}, {}]",
            unit.name,
            unit.build_seconds,
            unit.initial_health,
            unit.max_health,
            unit.speed,
            unit.geometry.major,
            unit.geometry.minor,
            unit.geometry.height
        );
        return Err("Invalid unit source values");
    }
    Ok(())
}

fn validate_economy(rules: &EconomyRules) -> Result<(), &'static str> {
    if rules.supply_value == 0
        || !rules.supply_build_border.is_finite()
        || rules.supply_build_border < 0.0
        || !rules.low_energy_penalty_modifier.is_finite()
        || rules.low_energy_penalty_modifier < 0.0
        || !rules.min_low_energy_production_speed.is_finite()
        || !rules.max_low_energy_production_speed.is_finite()
        || rules.min_low_energy_production_speed <= 0.0
        || rules.max_low_energy_production_speed < rules.min_low_energy_production_speed
        || rules.max_low_energy_production_speed > 1.0
    {
        return Err("Invalid economy source values");
    }
    Ok(())
}

fn validate_supply(unit: &UnitType) -> Result<(), &'static str> {
    if unit.supply.capacity > 100
        || (unit.supply.capacity > 0
            && (unit.supply.gather_seconds <= 0.0
                || unit.supply.dropoff_seconds <= 0.0
                || unit.supply.scan_distance <= 0.0))
    {
        return Err("Invalid supply collector source values");
    }
    if unit.supply.dock.as_ref().is_some_and(|dock| {
        dock.approaches.len() > 64
            || dock
                .approaches
                .iter()
                .chain(&dock.enter)
                .chain(&dock.action)
                .chain(&dock.exit)
                .flatten()
                .any(|value| !value.is_finite())
    }) {
        return Err("Invalid supply docking bones");
    }
    Ok(())
}

const fn new_player(faction: Faction, cash: u32) -> Player {
    Player {
        selected_objects: Vec::new(),
        preordered: false,
        units_should_hunt: false,
        supply_centers: Vec::new(),
        supply_warehouses: Vec::new(),
        production_cost_changes: Vec::new(),
        battle_plans: battle_plan::Army::new(),
        upgrades: Vec::new(),
        upgrades_in_progress: Vec::new(),
        faction,
        side: None,
        playable_side: false,
        color: None,
        statistics: ScoreKeeper::new(),
        cash,
        power: 0,
        rank: 1,
        skill_points: 0,
        science_points: 0,
        sciences: Vec::new(),
        acquired_sciences: Vec::new(),
        cash_bounty: 0.0,
        power_ready_at: Vec::new(),
        special_power_ready: std::collections::BTreeMap::new(),
        academy: results::AcademyStats {
            special_powers_used: 0,
            units_entered_tunnel: 0,
            mines_cleared: 0,
            researched_radar: false,
            buildings_captured: 0,
        },
        energy: economy::Energy::new(),
        native_money: economy::MoneyState::new(),
        radar: radar_update::PlayerRadar::new(),
    }
}

impl Entity {
    fn reset_order_state(&mut self, order: Order, ground_move: bool) {
        let actor = self;
        if !ground_move {
            actor.ai.ignored_producer = None;
        }
        if actor.ai.motion.is_moving() {
            actor.ai.motion.end_move();
        }
        actor.ai.motion.movement_active = false;
        if !ground_move {
            actor.ai.motion.clear_goal();
        }
        actor.ai.blocking.retry_at = None;
        actor
            .drawable
            .model_conditions
            .set_flag(crate::skirmish::ModelConditions::MOVING, false);
        actor.drawable.model_conditions.constructing = false;
        actor.ai.order = order;
        actor.ai.command.main_state = ai_goals::MainState::Active;
        actor.weapon_set.weapons[actor.weapon_set.current].max_shots = None;
        actor.ai.guard.area = None;
        if !ground_move {
            actor.ai.path.clear();
            actor.ai.path_goal = None;
            actor.ai.path_endpoint = None;
            actor.ai.ground_path = None;
        }
        actor.ai.command.move_to.waiting = actor.ai.path_request.pending.is_some();
        actor.ai.command.path_extra_distance = 0.0;
        actor.ai.construction_dock = None;
        actor.ai.construction_phase = ConstructionPhase::Approach;
        actor.ai.action_frames = 0.0;
        actor.ai.docking = None;
        actor.ai.ai_target = None;
        let forced = actor.ai.attack_context.forced;
        actor.ai.attack_context = builder_disarm::Attack::new();
        actor.ai.attack_context.source = match actor.ai.command.last_command_source {
            0 => builder_disarm::Source::Player,
            1 => builder_disarm::Source::Script,
            2 => builder_disarm::Source::Ai,
            3 => builder_disarm::Source::Dozer,
            _ => unreachable!("native CommandSourceType"),
        };
        actor.ai.attack_context.forced = forced;
        actor.ai.waypoints.points.clear();
        actor.ai.waypoints.index = 0;
        actor.ai.waypoints.exit_quick_pending = false;
    }
}
