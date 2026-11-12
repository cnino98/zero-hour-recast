//! Original `W3DModelDraw` state/animation playback (EA GPL-3.0-or-later).
//! Reference revision: `SuperHackers` 2bfaea60. Adaptive-delta clips remain omitted.
#[cfg(test)]
mod aircraft_tests;
mod artillery;
pub mod attachments;
#[cfg(test)]
mod cache_tests;
mod cache_verification;
#[cfg(test)]
mod civilian_props_tests;
mod clock;
mod data;
mod defenses;
mod detectors;
mod drones;
mod effects;
pub mod garrison;
#[cfg(test)]
mod helix_tests;
mod income;
mod infiltrators;
#[cfg(test)]
mod laser_snapshot_tests;
#[cfg(test)]
pub use laser_snapshot_tests::cached_gattling as laser_snapshot_fixture;
mod logical_bones;
#[cfg(test)]
mod mob_capture_tests;
mod muzzle;
pub mod native_clock;
mod performance;
mod playback;
mod point_defense;
#[cfg(test)]
mod render_object_tests;
mod render_objects;
mod salvage;
pub mod state_particles;
mod strategy;
mod tractor;
mod turret;
mod verification;
use crate::{
    model_render,
    playtest::Match,
    playtest_view::{Geometry, UnitVisual},
};
use bevy::prelude::*;
use clock::FrameTime;
pub use clock::{Clock, update_clock};
pub use data::load;
use data::{Draw, Motion};
use generals_assets::w3d::{BoneControl, MeshPose, Model, animation::Clip};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone)]
struct State {
    conditions: Vec<BTreeSet<String>>,
    name: String,
    model: Option<usize>,
    render_model: Option<usize>,
    hlod: bool,
    motions: Vec<Motion>,
    mode: String,
    flags: BTreeSet<String>,
    speed: [f32; 2],
    key: String,
    finish_key: String,
    transition: Option<(String, String)>,
    fields: Vec<Vec<String>>,
}

impl State {
    fn empty() -> Self {
        Self {
            conditions: Vec::new(),
            name: String::new(),
            model: None,
            render_model: None,
            hlod: false,
            motions: Vec::new(),
            mode: "ONCE".into(),
            flags: BTreeSet::new(),
            speed: [1.0; 2],
            key: String::new(),
            finish_key: String::new(),
            transition: None,
            fields: Vec::new(),
        }
    }
}

#[derive(Resource)]
pub struct Animations {
    logical_bones: Vec<Vec<generals_sim::skirmish::model_bones::Draw>>,
    draws: Vec<Vec<Draw>>,
    weapon_contexts: Vec<Vec<(usize, u8)>>,
    weapon_clips: Vec<Vec<u32>>,
    animate_when_disabled: Vec<bool>,
    world_draws: BTreeMap<usize, Draw>,
    clips: BTreeMap<String, Clip>,
    models: Vec<Model>,
    random: playback::Random,
    seed: u32,
    generation: Option<u32>,
    damage_thresholds: [f32; 2],
}

impl Animations {
    // InitRandom seeds the shared, non-authoritative client stream on restart.
    fn reset_random(&mut self, generation: u32) {
        if self.generation != Some(generation) {
            self.random = playback::Random::new(self.seed);
            self.generation = Some(generation);
        }
    }

    pub fn effect_random(&mut self, generation: u32) -> f32 {
        self.reset_random(generation);
        self.random.real(0.0, 1.0)
    }

    pub fn effect_real(&mut self, generation: u32, low: f32, high: f32) -> f32 {
        self.reset_random(generation);
        self.random.real(low, high)
    }

    pub fn effect_integer(&mut self, generation: u32, bounds: [u32; 2]) -> u32 {
        self.reset_random(generation);
        if bounds[0] >= bounds[1] {
            return bounds[1];
        }
        let length = u64::from(bounds[1] - bounds[0]) + 1;
        u32::try_from(
            self.random
                .index(usize::try_from(length).expect("native integer range")),
        )
        .expect("native integer result")
            + bounds[0]
    }

    pub fn effect_scorch(&mut self, generation: u32) -> i32 {
        self.reset_random(generation);
        i32::try_from(self.random.index(4)).expect("four native scorch choices")
    }

    #[cfg(test)]
    pub(crate) fn effect_fixture(seed: u32) -> Self {
        let mut animations = Self::new(Vec::new(), BTreeMap::new(), [0.5, 0.25]);
        animations.retain_models(Vec::new(), seed);
        animations
    }

    /// `handleWeaponFireFX` asks the current draw module's animated bone.
    /// A missing FX bone leaves the caller's original object-position fallback.
    pub fn weapon_fire_position(
        &self,
        unit: &generals_sim::skirmish::Entity,
        slot: usize,
        barrel: usize,
        players: &Players,
    ) -> Result<Option<[f32; 3]>, generals_assets::w3d::Error> {
        let Some(draws) = self.draws.get(unit.type_index) else {
            return Ok(None);
        };
        for (draw, player) in draws.iter().zip(&players.0) {
            let Some(state) = draw.states.get(player.state) else {
                continue;
            };
            let Some(model) = state.model.and_then(|index| self.models.get(index)) else {
                continue;
            };
            let Some(bone) = muzzle::fire_bone(&state.fields, model, slot, barrel) else {
                continue;
            };
            let clip = state
                .motions
                .get(player.choice)
                .and_then(|motion| self.clips.get(&motion.name));
            let mut controls = turret::controls(
                &state.fields,
                unit.ai.turret.angle,
                unit.ai.turret.pitch,
                unit.ai.secondary_turret.as_ref(),
            );
            player.muzzle.controls(&mut controls);
            if let Some(position) =
                model.controlled_bone_position(&bone, clip, player.frame, &controls)?
            {
                return Ok(Some(std::array::from_fn(|axis| {
                    position[axis] + draw.attach_offset[axis]
                })));
            }
        }
        Ok(None)
    }

    pub fn named_bone_position(
        &self,
        unit: &generals_sim::skirmish::Entity,
        players: &Players,
        bone: &str,
    ) -> Result<Option<[f32; 3]>, generals_assets::w3d::Error> {
        let Some(draws) = self.draws.get(unit.type_index) else {
            return Ok(None);
        };
        for (draw, player) in draws.iter().zip(&players.0) {
            let Some(state) = draw.states.get(player.state) else {
                continue;
            };
            let Some(model) = state.model.and_then(|index| self.models.get(index)) else {
                continue;
            };
            let clip = state
                .motions
                .get(player.choice)
                .and_then(|motion| self.clips.get(&motion.name));
            let mut controls = turret::controls(
                &state.fields,
                unit.ai.turret.angle,
                unit.ai.turret.pitch,
                unit.ai.secondary_turret.as_ref(),
            );
            player.muzzle.controls(&mut controls);
            if let Some(position) =
                model.controlled_bone_position(bone, clip, player.frame, &controls)?
            {
                return Ok(Some(std::array::from_fn(|axis| {
                    position[axis] + draw.attach_offset[axis]
                })));
            }
        }
        Ok(None)
    }

    pub(crate) fn debris_pose(
        &self,
        model: usize,
        clip: Option<&Clip>,
        frame: f32,
    ) -> Result<Vec<MeshPose>, generals_assets::w3d::Error> {
        self.models[model].controlled_pose(clip, frame, &[])
    }

    pub fn mesh_name(&self, model: usize, mesh: usize) -> Option<&str> {
        Some(&self.models.get(model)?.meshes.get(mesh)?.name)
    }

    fn new(
        draws: Vec<Vec<Draw>>,
        clips: BTreeMap<String, Clip>,
        damage_thresholds: [f32; 2],
    ) -> Self {
        Self {
            logical_bones: Vec::new(),
            draws,
            weapon_contexts: Vec::new(),
            weapon_clips: Vec::new(),
            animate_when_disabled: Vec::new(),
            world_draws: BTreeMap::new(),
            clips,
            models: Vec::new(),
            random: playback::Random::new(0),
            seed: 0,
            generation: None,
            damage_thresholds,
        }
    }
    pub fn retain_models(&mut self, models: Vec<Model>, seed: u32) {
        self.models = models;
        self.random = playback::Random::new(seed);
        self.seed = seed;
    }

    pub fn decal_model(&self, players: &Players, type_index: usize) -> Option<(usize, [f32; 2])> {
        let state = self
            .draws
            .get(type_index)?
            .first()?
            .states
            .get(players.0.first()?.state)?;
        let index = state.model?;
        let bounds = &self.models.get(index)?.bounds;
        Some((
            index,
            [bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1]],
        ))
    }

    pub fn firing_model<'a>(
        &self,
        models: &'a [Model],
        type_index: usize,
        damage: Option<&str>,
        slot: usize,
    ) -> Option<&'a Model> {
        models.get(self.firing_state(type_index, damage, slot)?.model?)
    }

    pub fn receives_dynamic_lights(&self, type_index: usize) -> bool {
        self.draws[type_index]
            .iter()
            .all(|draw| draw.receives_dynamic_lights)
    }

    /// A draw without a model has no pristine launch bones.
    pub fn firing_model_absent(
        &self,
        type_index: usize,
        damage: Option<&str>,
        slot: usize,
    ) -> bool {
        self.draws.get(type_index).is_some_and(Vec::is_empty)
            || self
                .firing_state(type_index, damage, slot)
                .is_some_and(|state| state.model.is_none())
    }

    pub fn firing_fields(
        &self,
        type_index: usize,
        damage: Option<&str>,
        slot: usize,
    ) -> Option<&[Vec<String>]> {
        Some(&self.firing_state(type_index, damage, slot)?.fields)
    }

    pub fn logical_bone_position(
        &self,
        models: &[Model],
        type_index: usize,
        damage: Option<&str>,
        bone: &str,
    ) -> Option<[f32; 3]> {
        // Drawable::getPristineBonePositions visits draw modules in order.
        // Object::getSingleLogicalBonePosition uses index 0 (literal name),
        // so ExitStart01 must not substitute for an absent ExitStart.
        let flags = damage.into_iter().map(String::from).collect();
        let name = bone.to_ascii_lowercase();
        self.draws.get(type_index)?.iter().find_map(|draw| {
            let model = models.get(draw.states[draw.select(&flags)].model?)?;
            let index = *model.public_bone_indices(bone).get(&name)?;
            model.bone_position(&model.bone_names[index])
        })
    }

    pub fn garrison_model<'a>(
        &self,
        models: &'a [Model],
        type_index: usize,
        damage: Option<&str>,
    ) -> Option<&'a Model> {
        let draw = self.draws.get(type_index)?.first()?;
        let mut flags = BTreeSet::from(["GARRISONED".into()]);
        if let Some(damage) = damage {
            flags.insert(damage.into());
        }
        models.get(draw.states[draw.select(&flags)].model?)
    }

    fn firing_state(
        &self,
        type_index: usize,
        damage: Option<&str>,
        index: usize,
    ) -> Option<&State> {
        let (slot, conditions) = self
            .weapon_contexts
            .get(type_index)
            .and_then(|contexts| contexts.get(index))
            .copied()
            .unwrap_or_else(|| {
                if index < 3 {
                    (index, 0)
                } else {
                    ((index - 3) / 2, if index % 2 == 1 { 2 } else { 4 })
                }
            });
        let mut flags = BTreeSet::from(match slot {
            0 => ["FIRING_A".into(), "USING_WEAPON_A".into()],
            1 => ["FIRING_B".into(), "USING_WEAPON_B".into()],
            _ => ["FIRING_C".into(), "USING_WEAPON_C".into()],
        });
        for (mask, name) in [
            (1, "WEAPONSET_PLAYER_UPGRADE"),
            (2, "WEAPONSET_CRATEUPGRADE_ONE"),
            (4, "WEAPONSET_CRATEUPGRADE_TWO"),
            (8, "DOOR_1_WAITING_TO_CLOSE"),
        ] {
            if conditions & mask != 0 {
                flags.insert(name.into());
            }
        }
        if let Some(damage) = damage {
            flags.insert(damage.into());
        }
        let draws = self.draws.get(type_index)?;
        draws
            .iter()
            .find_map(|draw| {
                let state = &draw.states[draw.select(&flags)];
                state
                    .fields
                    .iter()
                    .any(|line| {
                        line.first().is_some_and(|key| {
                            key == "WeaponFireFXBone" || key == "WeaponLaunchBone"
                        }) && line
                            .get(1)
                            .is_some_and(|name| name == ["PRIMARY", "SECONDARY", "TERTIARY"][slot])
                    })
                    .then_some(state)
            })
            .or_else(|| draws.first().map(|draw| &draw.states[draw.select(&flags)]))
    }

    pub fn firing_offset(
        &self,
        type_index: usize,
        damage: Option<&str>,
        index: usize,
    ) -> Option<[f32; 3]> {
        let state = self.firing_state(type_index, damage, index)?;
        self.draws
            .get(type_index)?
            .iter()
            .find(|draw| {
                draw.states
                    .iter()
                    .any(|candidate| std::ptr::eq(candidate, state))
            })
            .map(|draw| draw.attach_offset)
    }

    fn advance(
        &mut self,
        player: &mut Player,
        unit: &generals_sim::skirmish::Entity,
        module: usize,
        flags: &BTreeSet<String>,
        time: &FrameTime,
        paused: bool,
    ) {
        let draw = &self.draws[unit.type_index][module];
        let desired = draw.select(flags);
        if !paused {
            playback::manual_frame(player, draw, &self.clips, unit.animation_frame());
        }
        if !paused || player.state == usize::MAX {
            playback::select(player, draw, &self.clips, desired, &mut self.random);
        }
        if !paused && unit.animation_duration() != player.duration {
            player.duration = unit.animation_duration();
            if let Some((_, frames, completion)) = player.duration {
                playback::set_duration(player, draw, &self.clips, frames, completion);
            }
        }
        let state = &draw.states[player.state];
        let disabled = (unit.hacked() || unit.paralyzed() || unit.subdued())
            && !self
                .animate_when_disabled
                .get(unit.type_index)
                .copied()
                .unwrap_or(false);
        if paused
            || disabled
            || player.manual_animation
            || draw.requires_power && flags.contains("DISABLED")
        {
            player.tick = time.tick;
            player.last_sync_ms = time.milliseconds;
            player.position = unit.position;
        } else {
            playback::update(player, state, &self.clips, unit.position, time);
        }
    }

    fn pose(
        &self,
        player: &mut Player,
        unit: &generals_sim::skirmish::Entity,
        module: usize,
        tick: u64,
        barrel: Option<(usize, usize)>,
    ) -> Option<Vec<MeshPose>> {
        let draw = &self.draws[unit.type_index][module];
        let state = &draw.states[player.state];
        let Some(model_index) = state.model else {
            player.current_bones.clear();
            return None;
        };
        let model = &self.models[model_index];
        let recoil_changed = player.muzzle.update(
            player.state,
            &state.fields,
            model,
            &draw.recoil,
            tick,
            barrel,
        );
        let projectile_clips = if draw.recoil.projectile_slots == 0 {
            [(0, 0); 3]
        } else {
            std::array::from_fn(|slot| {
                let maximum = self
                    .weapon_clips
                    .get(unit.type_index)
                    .and_then(|clips| clips.get(unit.weapon_index(slot)))
                    .copied()
                    .unwrap_or(0);
                (unit.weapon_rounds_remaining(slot), maximum)
            })
        };
        let projectile_changed =
            draw.recoil.projectile_slots != 0 && player.projectile_clips != projectile_clips;
        player.projectile_clips = projectile_clips;
        let overrides = unit.sub_object_overrides();
        let upgrade_changed = player.sub_object_overrides.as_slice() != overrides;
        if upgrade_changed {
            overrides.clone_into(&mut player.sub_object_overrides);
        }
        player.transform_revision = unit.drawable_transform_revision();
        let secondary = unit.ai.secondary_turret.as_ref();
        let sample = player.sample(
            state,
            [
                unit.ai.turret.angle,
                unit.ai.turret.pitch,
                secondary.map_or(0.0, |turret| turret.angle),
                secondary.map_or(0.0, |turret| turret.pitch),
            ],
        );
        // Playback and recoil have already run, including shared client RNG.
        // A new child set needs its initial posed mesh even if the pose is equal.
        if player.sampled == Some(sample)
            && !recoil_changed
            && !projectile_changed
            && !upgrade_changed
            && player.model == state.render_model
            && player.owner == Some(unit.owner)
        {
            return None;
        }
        let mut controls = turret::controls(
            &state.fields,
            unit.ai.turret.angle,
            unit.ai.turret.pitch,
            unit.ai.secondary_turret.as_ref(),
        );
        player.muzzle.controls(&mut controls);
        let clip = state
            .motions
            .get(player.choice)
            .and_then(|motion| self.clips.get(&motion.name));
        match model.controlled_pose_and_bones(clip, player.frame, &controls) {
            Ok((mut pose, bones)) => {
                player.current_bones = bones;
                for bone in &mut player.current_bones {
                    for (axis, offset) in bone.translation.iter_mut().zip(draw.attach_offset) {
                        *axis += offset;
                    }
                }
                Self::pose_visibility(player, draw, model, overrides, projectile_clips, &mut pose);
                player.sampled = Some(sample);
                Some(pose)
            }
            Err(error) => {
                error!("Original model animation: {error}");
                None
            }
        }
    }
    fn draw_frame(
        &self,
        player: &Player,
        game: &generals_sim::skirmish::Skirmish,
        actor: usize,
        module: usize,
        mut meshes: Option<Vec<MeshPose>>,
    ) -> Frame {
        let unit = &game.entities[actor];
        let draw = &self.draws[unit.type_index][module];
        let state = &draw.states[player.state];
        let supply = game.supply_index(actor).and_then(|source| {
            let data = draw.supply.get(&state.render_model?)?;
            let boxes = game.supplies[source].boxes;
            if let Some(pose) = &mut meshes {
                for (mesh, mesh_pose) in pose.iter_mut().enumerate() {
                    mesh_pose.visible &= data.visible(mesh, boxes);
                }
            }
            Some((source, boxes, Arc::clone(data)))
        });
        Frame {
            supply,
            meshes,
            height_offset: construction_height(
                state,
                &game.catalog[unit.type_index],
                unit.build_progress,
            ),
        }
    }

    fn pose_visibility(
        player: &Player,
        draw: &Draw,
        model: &Model,
        overrides: &[generals_sim::skirmish::SubObjectOverride],
        projectile_clips: [(u32, u32); 3],
        pose: &mut [MeshPose],
    ) {
        let state = &draw.states[player.state];
        render_objects::hide_subobjects(model, pose);
        render_objects::apply_subobject_overrides(model, pose, overrides);
        player.muzzle.apply(pose);
        for (name, visible) in muzzle::projectile_visibility(
            &state.fields,
            projectile_clips,
            draw.recoil.projectile_slots,
        ) {
            for (mesh, posed) in model.meshes.iter().zip(pose.iter_mut()) {
                if mesh
                    .name
                    .rsplit('.')
                    .next()
                    .is_some_and(|short| short.eq_ignore_ascii_case(&name))
                {
                    posed.visible = visible;
                }
            }
        }
        if draw.attach_bone.is_some() {
            for mesh in pose {
                for position in &mut mesh.positions {
                    for (axis, offset) in position.iter_mut().zip(draw.attach_offset) {
                        *axis += offset;
                    }
                }
            }
        }
    }
}

// One previous pure geometry input, never a cache indexed by every float frame.
type PoseSample = (usize, Option<usize>, usize, u32, [u32; 4]);

pub struct Player {
    owner: Option<usize>,
    state: usize,
    pending: Option<usize>,
    model: Option<usize>,
    choice: usize,
    frame: f32,
    direction: f32,
    speed: f32,
    tick: u64,
    last_sync_ms: u32,
    restart_clock: bool,
    duration: Option<(u64, u32, bool)>,
    manual_frame: Option<(u64, i32)>,
    manual_animation: bool,
    pending_duration: Option<f32>,
    position: [f32; 2],
    muzzle: muzzle::Player,
    projectile_clips: [(u32, u32); 3],
    sub_object_overrides: Vec<generals_sim::skirmish::SubObjectOverride>,
    sampled: Option<PoseSample>,
    current_bones: Vec<BoneControl>,
    transform_revision: u64,
}

#[derive(Component)]
pub struct Players(pub Vec<Player>);

impl Players {
    pub(crate) fn current_bone(&self, name: &str) -> Option<(&BoneControl, u64)> {
        self.0.iter().find_map(|player| {
            player
                .current_bones
                .iter()
                .find(|bone| bone.name.eq_ignore_ascii_case(name))
                .map(|bone| (bone, player.transform_revision))
        })
    }
}

pub struct WorldDraw {
    pub template: String,
    pub module: String,
}

#[derive(Component)]
pub struct WorldPlayer {
    pub original_model: usize,
    pub source: Option<usize>,
    player: Player,
    generation: Option<u32>,
}

impl WorldPlayer {
    pub const fn new(original_model: usize, source: Option<usize>) -> Self {
        Self {
            original_model,
            source,
            player: Player::new([0.0; 2], 0),
            generation: None,
        }
    }
}

impl Player {
    fn sample(&self, state: &State, turret: [f32; 4]) -> PoseSample {
        (
            self.state,
            state.model,
            self.choice,
            self.frame.to_bits(),
            turret.map(f32::to_bits),
        )
    }

    pub const fn new(position: [f32; 2], tick: u64) -> Self {
        Self {
            owner: None,
            state: usize::MAX,
            pending: None,
            model: None,
            choice: 0,
            frame: 0.0,
            direction: 1.0,
            speed: 1.0,
            tick,
            last_sync_ms: 0,
            restart_clock: false,
            duration: None,
            manual_frame: None,
            manual_animation: false,
            pending_duration: None,
            position,
            muzzle: muzzle::Player::new(),
            projectile_clips: [(0, 0); 3],
            sub_object_overrides: Vec::new(),
            sampled: None,
            current_bones: Vec::new(),
            transform_revision: 0,
        }
    }
}

#[derive(Component)]
pub struct AnimatedPart {
    pub model: usize,
    pub binding: Arc<model_render::PartBinding>,
}

pub struct Frame {
    pub supply: Option<(usize, u32, Arc<crate::supply_presentation::SupplyDraw>)>,
    pub meshes: Option<Vec<MeshPose>>,
    /// Source world units, converted only when writing the render Transform.
    pub height_offset: f32,
}

pub struct DrawInspection {
    pub module: usize,
    pub model: usize,
    pub clip: String,
    pub mode: String,
    pub frame: f32,
    /// Bounds of visible posed geometry in the renderer's local coordinates.
    pub bounds: Option<[[f32; 3]; 2]>,
}

impl Animations {
    pub fn inspect(
        &self,
        players: &Players,
        type_index: usize,
    ) -> Result<Vec<DrawInspection>, generals_assets::w3d::Error> {
        let mut result = Vec::new();
        for (module, player) in players.0.iter().enumerate() {
            let Some(state) = self.draws[type_index][module].states.get(player.state) else {
                continue;
            };
            let Some(model) = state.model else {
                continue;
            };
            let Some(motion) = state.motions.get(player.choice) else {
                continue;
            };
            let Some(clip) = self.clips.get(&motion.name) else {
                continue;
            };
            let pose = self.models[model].pose(clip, player.frame)?;
            let mut bounds: Option<[[f32; 3]; 2]> = None;
            for point in pose
                .iter()
                .filter(|mesh| mesh.visible)
                .flat_map(|mesh| &mesh.positions)
            {
                let point = [point[0] * 0.1, point[2] * 0.1, -point[1] * 0.1];
                let bounds = bounds.get_or_insert([point; 2]);
                for axis in 0..3 {
                    bounds[0][axis] = bounds[0][axis].min(point[axis]);
                    bounds[1][axis] = bounds[1][axis].max(point[axis]);
                }
            }
            result.push(DrawInspection {
                module,
                model: state.render_model.unwrap_or(model),
                clip: motion.name.clone(),
                mode: state.mode.clone(),
                frame: player.frame,
                bounds,
            });
        }
        Ok(result)
    }
}

type AnimatedVisual = (
    Entity,
    &'static UnitVisual,
    &'static mut Players,
    Option<&'static Children>,
);

type AnimatedMesh = (
    &'static AnimatedPart,
    &'static Mesh3d,
    &'static ChildOf,
    &'static mut Visibility,
    &'static mut Transform,
    Option<&'static mut crate::sorted_view::submission::ModelSubmission>,
    Option<&'static mut crate::light_environment::Bounds>,
);

#[derive(bevy::ecs::system::SystemParam)]
pub struct AnimationResources<'w> {
    animations: ResMut<'w, Animations>,
    clock: Res<'w, Clock>,
    geometry: Res<'w, Geometry>,
    meshes: ResMut<'w, Assets<Mesh>>,
    timings: Option<Res<'w, crate::render_performance::Timings>>,
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    resources: AnimationResources,
    mut visuals: Query<AnimatedVisual>,
    mut parts: Query<AnimatedMesh>,
) {
    let AnimationResources {
        mut animations,
        clock,
        geometry,
        mut meshes,
        timings,
    } = resources;
    let _span = timings.as_deref().map(|timings| timings.span("animation"));
    let battle = battle.into_inner();
    let geometry = geometry.into_inner();
    if !battle.started {
        return;
    }
    // SkirmishGameOptionsMenu::startGame and QuitMenu restart reseed both
    // logic and client streams through InitRandom(the selected game seed).
    animations.reset_random(battle.generation);
    let mut poses = BTreeMap::new();
    for (entity, visual, mut players, children) in &mut visuals {
        if visual.generation != battle.generation {
            continue;
        }
        if !matches!(
            battle.simulation.visibility_status(0, visual.index),
            generals_sim::skirmish::visibility::Status::Clear
                | generals_sim::skirmish::visibility::Status::Partial
        ) {
            continue;
        }
        let unit = &battle.simulation.entities[visual.index];
        if !crate::playtest_view::drawable_present(&battle.simulation, visual.index) {
            continue;
        }
        let color = battle
            .simulation
            .player_colors
            .get(unit.owner)
            .copied()
            .flatten();
        let flags = object_conditions(battle, unit, animations.damage_thresholds);
        if players.0.is_empty() {
            players.0 = (0..animations.draws[unit.type_index].len())
                .map(|_| Player::new(unit.position, battle.simulation.tick))
                .collect();
        }
        for (module, player) in players.0.iter_mut().enumerate() {
            animations.advance(
                player,
                unit,
                module,
                &flags,
                &clock.time(battle.simulation.tick),
                battle.paused,
            );
            let barrel = battle
                .simulation
                .presentation
                .shots
                .iter()
                .find(|shot| shot.source == visual.index)
                .map(|shot| (shot.weapon_slot, shot.barrel));
            let pose = animations.pose(player, unit, module, battle.simulation.tick, barrel);
            let frame =
                animations.draw_frame(player, &battle.simulation, visual.index, module, pose);
            let state = &animations.draws[unit.type_index][module].states[player.state];
            if player.model != state.render_model || player.owner != Some(unit.owner) {
                if let Some(children) = children {
                    for child in children {
                        if parts
                            .get(*child)
                            .is_ok_and(|(part, ..)| Some(part.model) == player.model)
                        {
                            commands.entity(*child).despawn();
                        }
                    }
                }
                player.model = state.render_model;
                player.owner = Some(unit.owner);
                if let Some(model) = state.render_model {
                    geometry.spawn_unit_parts(
                        &mut commands,
                        entity,
                        model,
                        color,
                        &mut meshes,
                        &frame,
                    );
                }
                // Deferred old children keep their old model key, so they cannot
                // receive another draw module's new pose in this frame.
            } else if let Some(model) = state.render_model {
                poses.insert((entity, model), frame);
            }
        }
    }
    apply_poses(&mut meshes, &mut parts, &poses);
}

fn object_conditions(
    battle: &Match,
    unit: &generals_sim::skirmish::Entity,
    thresholds: [f32; 2],
) -> BTreeSet<String> {
    battle.simulation.model_flags(unit, thresholds)
}

fn construction_height(
    state: &State,
    unit: &generals_sim::skirmish::UnitType,
    progress: f32,
) -> f32 {
    if state
        .flags
        .contains("ADJUST_HEIGHT_BY_CONSTRUCTION_PERCENT")
    {
        let height = unit.geometry_bounds.max_height;
        let constructed = height * progress;
        -height + constructed
    } else {
        0.0
    }
}

fn apply_poses(
    meshes: &mut Assets<Mesh>,
    parts: &mut Query<AnimatedMesh>,
    poses: &BTreeMap<(Entity, usize), Frame>,
) {
    for (part, mesh, parent, mut visibility, mut transform, submission, bounds) in parts {
        let Some(frame) = poses.get(&(parent.parent(), part.model)) else {
            continue;
        };
        let view_height = frame.height_offset * 0.1;
        if transform.translation.y.to_bits() != view_height.to_bits() {
            transform.translation.y = view_height;
        }
        if let Some(mut bounds) = bounds {
            bounds.native_height = frame.height_offset;
        }
        let Some(pose) = &frame.meshes else {
            continue;
        };
        if let Some(mut submission) = submission
            && let Some(mesh_pose) = pose.get(part.binding.mesh)
        {
            submission.set_pose(mesh_pose);
        }
        let Some(current) = meshes.get(&mesh.0) else {
            continue;
        };
        let result =
            model_render::deformation_matches(current, &part.binding, pose).and_then(|unchanged| {
                // MeshClass::Render rejects hidden subobjects before skin upload.
                // Keep sampling the original pose, but leave its last uploaded
                // vertices until a visible pose compares against this Mesh.
                // Validation above still reports malformed hidden bindings.
                if unchanged || !pose[part.binding.mesh].visible {
                    Ok(pose[part.binding.mesh].visible)
                } else {
                    let mut mesh = meshes.get_mut(&mesh.0).expect("existing animated mesh");
                    model_render::deform(&mut mesh, &part.binding, pose)
                }
            });
        match result {
            Ok(visible) => {
                visibility.set_if_neq(if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            Err(error) => error!("Original animated mesh: {error}"),
        }
    }
}

type WorldVisual = (
    Entity,
    &'static mut WorldPlayer,
    Option<&'static Children>,
    &'static mut Visibility,
);

pub fn update_world(
    mut commands: Commands,
    battle: Res<Match>,
    resources: AnimationResources,
    mut visuals: Query<WorldVisual, Without<AnimatedPart>>,
    mut parts: Query<AnimatedMesh>,
) {
    let AnimationResources {
        mut animations,
        clock,
        geometry,
        mut meshes,
        timings,
    } = resources;
    let _span = timings
        .as_deref()
        .map(|timings| timings.span("world_animation"));
    let battle = battle.into_inner();
    let geometry = geometry.into_inner();
    if !battle.started {
        return;
    }
    let Animations {
        world_draws,
        clips,
        models,
        random,
        ..
    } = &mut *animations;
    let mut poses = BTreeMap::new();
    for (entity, mut visual, children, mut visibility) in &mut visuals {
        if visual.generation != Some(battle.generation) {
            visual.player = Player::new([0.0; 2], battle.simulation.tick);
            visual.generation = Some(battle.generation);
        }
        let visible = visual.source.is_none_or(|index| {
            let supply = &battle.simulation.supplies[index];
            battle.simulation.entities[supply.object].exists()
        });
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let draw = &world_draws[&visual.original_model];
        if !battle.paused || visual.player.state == usize::MAX {
            playback::select(&mut visual.player, draw, clips, 0, random);
        }
        let state = &draw.states[visual.player.state];
        if battle.paused {
            visual.player.last_sync_ms = clock.time(battle.simulation.tick).milliseconds;
        } else {
            playback::update(
                &mut visual.player,
                state,
                clips,
                [0.0; 2],
                &clock.time(battle.simulation.tick),
            );
        }
        let pose = world_pose(&mut visual.player, state, models, clips);
        let frame = Frame {
            supply: None,
            meshes: pose,
            height_offset: 0.0,
        };
        if visual.player.model != state.model {
            if let Some(children) = children {
                for child in children {
                    commands.entity(*child).despawn();
                }
            }
            visual.player.model = state.model;
            if let Some(model) = state.model {
                geometry.spawn_unit_parts(&mut commands, entity, model, None, &mut meshes, &frame);
            }
        } else if let Some(model) = state.model {
            poses.insert((entity, model), frame);
        }
    }
    apply_poses(&mut meshes, &mut parts, &poses);
}

fn world_pose(
    player: &mut Player,
    state: &State,
    models: &[Model],
    clips: &BTreeMap<String, Clip>,
) -> Option<Vec<MeshPose>> {
    let sample = player.sample(state, [0.0; 4]);
    if player.sampled == Some(sample) && player.model == state.model {
        return None;
    }
    let model = state.model?;
    let clip = state
        .motions
        .get(player.choice)
        .and_then(|motion| clips.get(&motion.name));
    match models[model].controlled_pose(clip, player.frame, &[]) {
        Ok(pose) => {
            player.sampled = Some(sample);
            Some(pose)
        }
        Err(error) => {
            error!("Original world animation: {error}");
            None
        }
    }
}
