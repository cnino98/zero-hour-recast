//! `JetAIUpdate` / `HeliAIStateMachine`, pinned SH 2bfaea60, `CRC=0/XFER_SAVE=0`.
use super::Locomotor;

mod commands;
mod eligibility;
mod landing;
mod lifecycle;
pub mod lockon;
mod motion;
mod presentation;
mod rearm;
mod states;
mod taxi;
#[cfg(test)]
mod tests;
mod update;

pub struct LocomotorSet {
    /// Native `LocomotorSetType` identity, independent of attack/return use.
    pub role: u8,
    pub locomotor: Locomotor,
    pub speed: f32,
}

pub struct Rules {
    pub taxi: LocomotorSet,
    pub attacking: Option<LocomotorSet>,
    pub returning: Option<LocomotorSet>,
    pub needs_runway: bool,
    pub keeps_space: bool,
    pub helipad: bool,
    pub takeoff_pause: u32,
    pub idle_return: u32,
    pub out_of_ammo_damage: f32,
    pub min_height: f32,
    pub parking_offset: f32,
    pub attack_locomotor_frames: u32,
    pub attackers_miss_frames: u32,
    pub sneaky_offset: f32,
    pub lockon: lockon::Rules,
    pub sounds: Sounds,
}

pub struct Sounds {
    pub afterburner: String,
    pub low_fuel: String,
    pub wheel_screech: String,
}

// State IDs are copied at the source dispatcher before onExit/onEnter can change them.
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    TaxiFromHangar,
    AwaitTakeoff,
    TaxiTakeoff,
    Pause,
    Takeoff,
    Return,
    AwaitLanding,
    Landing,
    TaxiFromLanding,
    Orient,
    Reload,
    ReturnDead,
    CircleDead,
}

const ALLOW_AIR: u16 = 1;
const TAKEOFF: u16 = 2;
const LANDING: u16 = 4;
const TAXI: u16 = 8;
const CIRCLING: u16 = 16;
const SPECIAL_RETURN: u16 = 32;
const RESUME_RELOAD: u16 = 64;
const AFTERBURNER: u16 = 128;
const EXHAUST: u16 = 256;
const ENGINE: u16 = 512;
const LANDING_SOUND_PLAYED: u16 = 1024;

struct Pause {
    takeoff: u32,
    transfer: u32,
    waited_for: Option<usize>,
    can_turn: bool,
}

impl Pause {
    const fn new() -> Self {
        Self {
            takeoff: 0,
            transfer: 0,
            waited_for: None,
            can_turn: false,
        }
    }
}

pub(super) struct State {
    phase: Phase,
    takeoff_lift: f32,
    flags: u16,
    pending: Option<super::ai_dispatch::stored_command::Command>,
    recent: Option<super::ai_dispatch::stored_command::Command>,
    producer_position: Option<[f32; 3]>,
    landing_position: [f32; 3],
    endpoint: [f32; 3],
    heli_points: [[f32; 3]; 2],
    heli_index: usize,
    pause: Pause,
    rearm: rearm::Reload,
    return_at: u32,
    attack_until: u32,
    miss_until: u32,
    check_airfield: u32,
    draw_height: f32,
    targeters: Vec<usize>,
    untargetable_expire: u32,
    lockon_drawable: Option<lockon::Drawable>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            takeoff_lift: 0.0,
            flags: ENGINE,
            pending: None,
            recent: None,
            producer_position: None,
            landing_position: [0.0; 3],
            endpoint: [0.0; 3],
            heli_points: [[0.0; 3]; 2],
            heli_index: 0,
            pause: Pause::new(),
            rearm: rearm::Reload::new(),
            return_at: 0,
            attack_until: 0,
            miss_until: 0,
            check_airfield: 0,
            draw_height: 0.0,
            targeters: Vec::new(),
            untargetable_expire: 0,
            lockon_drawable: None,
        }
    }

    const fn flag(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    const fn set(&mut self, flag: u16, value: bool) {
        if value {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }
}

#[cfg(test)]
mod targeting_tests;
