//! Original `ParticleSys` emission, particle rates, slaves and W3D rendering.
mod actor_fx;
mod body_particles;
mod bone_fx;
mod chinook;
mod data;
mod dependencies;
mod detectors;
mod drones;
mod effects;
mod emission;
mod fire;
mod height_die;
mod inspection;
mod laser;
mod model_states;
mod object_effects;
pub mod orientation;
mod particle;
mod performance;
mod render;
pub mod smudge;
pub use smudge::SmudgePlugin;
mod spectre;
pub mod streak;
mod system;
mod verification;
pub mod view_shake;
mod volume;
mod wind;
pub use verification::verify;
#[cfg(test)]
mod terrain_tests;
#[cfg(test)]
mod tests;

use crate::model_animation::Animations;
use bevy::prelude::*;

pub use actor_fx::sounds as object_fx_sounds;
pub use data::load;
pub use data::load_with_position_effects;
pub use dependencies::position_sounds;
pub use dependencies::weapon_sounds as weapon_fx_sounds;
pub use effects::step;
pub use fire::update as update_fire;
pub use inspection::{AdditiveQuad, ParticleInspection, SmudgeFan};
pub use model_states::update as update_model_states;
pub use render::{ParticleVisual, update};

struct Range {
    low: f32,
    high: f32,
}

impl Range {
    fn sample(&self, random: &mut impl FnMut(f32, f32) -> f32) -> f32 {
        random(self.low, self.high)
    }
}

enum Volume {
    None,
    Point,
    Box([f32; 3]),
    Line([f32; 3], [f32; 3]),
    Cylinder {
        radius: f32,
        length: f32,
        hollow: bool,
    },
    Sphere {
        radius: f32,
        hollow: bool,
    },
}
enum Velocity {
    None,
    Ortho([Range; 3]),
    Outward(Range, Range),
    Spherical(Range),
    Hemispherical(Range),
    Cylindrical(Range, Range),
}
struct AlphaKey {
    value: Range,
    frame: u32,
}
struct ColorKey {
    value: [f32; 3],
    frame: u32,
}
enum Style {
    Smudge,
    Billboard,
    Streak,
    Volume(u8),
}

struct Drawing {
    style: Style,
    shader: Shader,
    ground_aligned: bool,
}

enum Shader {
    Alpha,
    Additive,
    AlphaTest,
}

impl Shader {
    const fn index(&self) -> usize {
        match self {
            Self::Alpha => 0,
            Self::Additive => 1,
            Self::AlphaTest => 2,
        }
    }
}

struct Template {
    name: String,
    texture: usize,
    drawing: Drawing,
    emit_above_ground_only: bool,
    up_towards_emitter: bool,
    lifetime: Range,
    system_lifetime: f32,
    initial_delay: Range,
    burst_delay: Range,
    burst_count: Range,
    volume: Volume,
    velocity: Velocity,
    velocity_damping: Range,
    angle: Range,
    angular_rate: Range,
    angular_damping: Range,
    gravity: f32,
    drift: [f32; 3],
    size: Range,
    start_size_rate: Range,
    size_rate: Range,
    size_rate_damping: Range,
    alpha: [AlphaKey; 8],
    color: [ColorKey; 8],
    color_scale: Range,
    slave: Option<usize>,
    attached: Option<usize>,
    slave_offset: [f32; 3],
    wind_motion: wind::Motion,
    wind_change: Range,
    wind_start: Range,
    wind_end: Range,
}

struct Particle {
    controlled: Option<u64>,
    position: [f32; 3],
    up_emitter: Option<[f32; 2]>,
    velocity: [f32; 3],
    damping: f32,
    remaining: f32,
    born: u64,
    angle: f32,
    angular_rate: f32,
    angular_damping: f32,
    size: f32,
    size_rate: f32,
    size_damping: f32,
    alpha_keys: [(f32, u32); 8],
    alpha: f32,
    alpha_rate: f32,
    alpha_target: usize,
    color_keys: [([f32; 3], u32); 8],
    color: [f32; 3],
    color_rate: [f32; 3],
    color_target: usize,
    color_scale: f32,
    personality: u32,
    wind_randomness: f32,
}

struct Lifecycle {
    destroyed: bool,
    stopped: bool,
}

struct Emitter {
    id: u64,
    template: usize,
    particles: Vec<Particle>,
    transform: Pose,
    wind: wind::State,
    wind_origin: Option<[f32; 3]>,
    radius: Option<f32>,
    previous: Option<[f32; 3]>,
    attachment: Option<u64>,
    object_attachment: Option<(usize, Pose)>,
    drawable_attachment: Option<(usize, usize)>,
    drawable_offset: Option<model_states::Attachment>,
    fire_attachment: Option<fire::Attachment>,
    control_particle: Option<(u64, u32)>,
    slave: Option<u64>,
    master: bool,
    lifecycle: Lifecycle,
    shrouded: bool,
    delay: f32,
    burst_delay: f32,
    remaining: f32,
    size_bonus: f32,
    lifetime: Option<f32>,
    personality: u32,
}

#[derive(Clone)]
struct Pose {
    position: [f32; 3],
    axes: [[f32; 3]; 3],
}
impl Pose {
    const fn at(position: [f32; 3]) -> Self {
        Self {
            position,
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
    fn rotate(&self, value: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|axis| {
            let x = self.axes[0][axis] * value[0];
            let y = self.axes[1][axis] * value[1];
            let z = self.axes[2][axis] * value[2];
            x + y + z
        })
    }
}

struct Texture {
    image: Option<Image>,
    handle: Option<Handle<Image>>,
    materials: [Option<Handle<crate::source_material::SourceMaterial>>; 3],
}

#[derive(Resource)]
pub struct Particles {
    smudges: Vec<smudge::Smudge>,
    object_emitters: object_effects::Attachments,
    templates: Vec<Template>,
    textures: Vec<Texture>,
    emitters: Vec<Emitter>,
    next_id: u64,
    generation: u32,
    frame: Option<u64>,
    processed: Option<u64>,
    phase: f32,
    scale: f32,
    bindings: Vec<Vec<Option<effects::WeaponEffects>>>,
    bone_systems: std::collections::BTreeMap<(usize, usize), Vec<u64>>,
    bodies: body_particles::Bodies,
    detectors: detectors::Bindings,
    model_states: model_states::States,
    fires: fire::Fires,
    lists: Vec<Vec<effects::Nugget>>,
    actor_fx: std::collections::BTreeMap<String, usize>,
    propaganda: Vec<[Option<usize>; 2]>,
    sounds: Vec<(String, [f32; 3])>,
    pub scorches: crate::scorch::Scorches,
    pub(crate) pulses: crate::light_pulse::Pool,
    repair_sound: String,
    shake: view_shake::Shake,
    reported_streak: std::collections::BTreeSet<usize>,
}

pub fn random(animations: &mut Animations, generation: u32) -> impl FnMut(f32, f32) -> f32 + '_ {
    move |low, high| {
        // GameClientRandomValueReal returns high without drawing for low>=high.
        if low >= high {
            high
        } else {
            let extent = high - low;
            let offset = extent * animations.effect_random(generation);
            low + offset
        }
    }
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let squares = value.map(|v| v * v);
    let length = (squares[0] + squares[1] + squares[2]).sqrt();
    if length > 0.0 {
        // Coord3D/Coord2D::normalize divide each component directly. A shared
        // reciprocal changes authored outward-velocity rounding by an ULP.
        value.map(|v| v / length)
    } else {
        value
    }
}
