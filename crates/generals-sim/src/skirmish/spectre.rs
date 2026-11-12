//! SH 2bfaea60 `SpectreGunshipUpdate` / `SpectreGunshipDeploymentUpdate`, CRC=0.
use super::{Entity, Skirmish, WeaponRules, module_updates::FOREVER};

mod deployment;
mod runtime;
#[cfg(test)]
mod tests;

pub struct Decal {
    pub texture: String,
    pub opacity: [f32; 2],
    pub throb_frames: u32,
    pub color: [u8; 4],
    pub only_owner: bool,
}

pub struct Rules {
    pub power: String,
    pub gunner: String,
    pub howitzer: Option<WeaponRules>,
    pub orbit_frames: u32,
    pub firing_rate: u32,
    pub follow_lag: u32,
    pub attack_radius: f32,
    pub reticle_radius: f32,
    pub orbit_radius: f32,
    pub strafe_increment: f32,
    pub insertion_slope: f32,
    pub howitzer_offset: f32,
    pub strafe_particle: String,
    pub decals: [Decal; 2],
    pub afterburner_sound: String,
    pub howitzer_sound: String,
    pub attack_voice: String,
}

pub enum CreationEdge {
    NearSource,
    FarSource,
    NearTarget,
    FarTarget,
}

pub struct DeploymentRules {
    pub power: String,
    pub required_science: String,
    pub gunship: String,
    pub attack_radius: f32,
    pub edge: CreationEdge,
}

#[derive(Clone, Copy)]
enum Status {
    Inserting,
    Orbiting,
    Departing,
    Idle,
}

pub(in crate::skirmish) struct State {
    initial_target: [f32; 3],
    override_target: [f32; 3],
    satellite: [f32; 3],
    gunner_target: [f32; 3],
    shoot_at: [f32; 3],
    status: Status,
    howitzer_counter: u32,
    escape_frame: u32,
    gunner: Option<usize>,
    afterburners: bool,
    doors_open: bool,
    decals: Decals,
}

struct Decals {
    allocated: bool,
    positions: [[f32; 3]; 2],
    frame: Option<u32>,
    draw_ui: bool,
}

pub(in crate::skirmish) struct DeploymentState {
    target: [f32; 3],
    gunship: Option<usize>,
}

pub struct Strafe {
    pub ship: usize,
    pub position: [f32; 3],
    pub particle: String,
    pub visible_players: u16,
}

pub struct DecalView<'a> {
    pub style: &'a Decal,
    pub position: [f32; 3],
    pub radius: f32,
    pub frame: Option<u32>,
    pub enabled: bool,
}

impl Entity {
    #[must_use]
    pub fn spectre_afterburners(&self) -> bool {
        self.spectre
            .as_ref()
            .is_some_and(|state| state.afterburners)
    }

    #[must_use]
    pub fn spectre_doors_open(&self) -> Option<bool> {
        self.spectre.as_ref().map(|state| state.doors_open)
    }

    #[must_use]
    pub fn spectre_retarget_active(&self) -> bool {
        self.spectre
            .as_ref()
            .is_some_and(|state| matches!(state.status, Status::Inserting | Status::Orbiting))
    }

    #[must_use]
    pub fn spectre_decals<'a>(&self, rules: &'a Rules) -> Option<[DecalView<'a>; 2]> {
        let state = self
            .spectre
            .as_ref()
            .filter(|state| state.decals.allocated)?;
        Some([
            DecalView {
                style: &rules.decals[0],
                position: state.decals.positions[0],
                radius: rules.attack_radius,
                frame: state.decals.frame,
                enabled: state.decals.draw_ui,
            },
            DecalView {
                style: &rules.decals[1],
                position: state.decals.positions[1],
                radius: rules.reticle_radius,
                frame: state.decals.frame,
                enabled: state.decals.draw_ui,
            },
        ])
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_spectre(&mut self, id: usize) {
        self.entities[id].spectre = Some(State {
            initial_target: [0.0; 3],
            override_target: [0.0; 3],
            satellite: [0.0; 3],
            gunner_target: [0.0; 3],
            shoot_at: [0.0; 3],
            status: Status::Idle,
            howitzer_counter: 0,
            escape_frame: 0,
            gunner: None,
            afterburners: false,
            doors_open: false,
            decals: Decals {
                allocated: false,
                positions: [[0.0; 3]; 2],
                frame: None,
                draw_ui: true,
            },
        });
    }

    pub(in crate::skirmish) fn create_spectre(&mut self, id: usize) {
        let position = self.object_position(id);
        self.entities[id]
            .spectre
            .as_mut()
            .expect("Spectre constructor")
            .satellite = position;
    }

    pub(in crate::skirmish) fn construct_spectre_deployment(&mut self, id: usize) {
        self.entities[id]
            .behaviors
            .spectre_deployments
            .push(DeploymentState {
                target: [0.0; 3],
                gunship: None,
            });
    }

    pub(in crate::skirmish) fn update_spectre_deployment(&self, id: usize, _ordinal: usize) -> i32 {
        let actor = &self.entities[id];
        if actor.selling() || actor.under_construction() || actor.is_effectively_dead() {
            i32::try_from(FOREVER).expect("native sleep")
        } else {
            1
        }
    }

    pub(in crate::skirmish) fn initiate_spectre(
        &mut self,
        id: usize,
        power: &str,
        target: [f32; 3],
        script: bool,
    ) -> bool {
        let rules = self.catalog[self.entities[id].type_index]
            .spectre
            .as_ref()
            .expect("Spectre rules");
        if rules.power != power {
            return false;
        }
        let gunner_name = rules.gunner.clone();
        if script {
            self.set_special_power_ready_frame(id, power, self.update_frame());
        }
        let state = self.entities[id]
            .spectre
            .as_mut()
            .expect("Spectre constructor");
        state.initial_target = target;
        if !script {
            state.override_target = target;
            state.gunner_target = target;
        }
        state.status = Status::Inserting;
        self.set_flight_locomotor(id, true);
        self.set_flight_precision(id, false, true);
        self.entities[id]
            .spectre
            .as_mut()
            .expect("Spectre state")
            .doors_open = false;
        self.spectre_afterburner(id, true);
        if self.container_rules(id).is_some() {
            // Source drops the remembered ID without destroying an old gunner.
            self.entities[id]
                .spectre
                .as_mut()
                .expect("Spectre state")
                .gunner = None;
            if let Some(kind) = self
                .catalog
                .iter()
                .position(|unit| unit.name == gunner_name)
            {
                let owner = self.entities[id].owner;
                let gunner = self.new_object(kind, owner, true);
                self.add_to_contain(gunner, id);
                self.entities[id]
                    .spectre
                    .as_mut()
                    .expect("Spectre state")
                    .gunner = Some(gunner);
                self.entities[gunner].disabled_status |= 1 << 2;
                self.observe_spy_vision_disabled(gunner);
            }
        }
        let position = self.object_position(id);
        let state = self.entities[id].spectre.as_mut().expect("Spectre state");
        state.decals.allocated = true;
        state.decals.positions = [position; 2];
        state.decals.frame = None;
        state.decals.draw_ui = true;
        self.mark_special_power_triggered(id, power, Some(target));
        true
    }

    #[must_use]
    pub fn can_override_special_power_destination(&self, id: usize, position: [f32; 3]) -> bool {
        self.entities.get(id).is_some_and(|object| {
            object.exists()
                && object.spectre_retarget_active()
                && !matches!(
                    self.visibility_cell(object.owner, [position[0], position[1]]),
                    super::visibility::Status::Shrouded
                )
        })
    }

    /// `ActionManager` destination admission followed by the source group's
    /// active update callback. Disabled Spectres accept the command but ignore it.
    /// # Panics
    /// An active Spectre must retain its authored gunship rules.
    pub fn override_special_power_destination(
        &mut self,
        id: usize,
        position: [f32; 3],
        local_player: usize,
    ) -> bool {
        if !self.can_override_special_power_destination(id, position) {
            return false;
        }
        let owner = self.entities[id].owner;
        self.set_spectre_target(id, position);
        if owner == local_player && !self.object_disabled(id) {
            let voice = self.catalog[self.entities[id].type_index]
                .spectre
                .as_ref()
                .expect("active Spectre")
                .attack_voice
                .clone();
            self.object_sound(id, voice, super::object_sounds::Action::Play);
        }
        true
    }

    pub(in crate::skirmish) fn set_spectre_target(&mut self, id: usize, position: [f32; 3]) {
        if self.object_disabled(id) {
            return;
        }
        self.entities[id]
            .spectre
            .as_mut()
            .expect("Spectre target override")
            .override_target = position;
    }

    pub(in crate::skirmish) fn delete_spectre(&mut self, id: usize) {
        // Destructor clears only the decals; gunner removal is normal containment deletion.
        if let Some(state) = &mut self.entities[id].spectre {
            state.decals.allocated = false;
        }
    }

    fn spectre_cleanup(&mut self, id: usize) {
        let state = self.entities[id].spectre.as_mut().expect("Spectre state");
        state.decals.allocated = false;
        let gunner = state.gunner;
        if let Some(gunner) = gunner.filter(|&other| self.entities[other].exists()) {
            self.destroy_maintenance_object(gunner);
        }
    }

    fn spectre_afterburner(&mut self, id: usize, enabled: bool) {
        let state = self.entities[id].spectre.as_mut().expect("Spectre state");
        let changed = state.afterburners != enabled;
        state.afterburners = enabled;
        if changed {
            let sound = self.catalog[self.entities[id].type_index]
                .spectre
                .as_ref()
                .expect("Spectre rules")
                .afterburner_sound
                .clone();
            self.object_sound(
                id,
                sound,
                if enabled {
                    super::object_sounds::Action::Play
                } else {
                    super::object_sounds::Action::Stop
                },
            );
        }
    }
}

fn length(vector: [f32; 3]) -> f32 {
    let squares = vector.map(|component| component * component);
    (squares[0] + squares[1] + squares[2]).sqrt()
}

fn normalized(mut vector: [f32; 3]) -> [f32; 3] {
    let magnitude = length(vector);
    if magnitude != 0.0 {
        let inverse = 1.0 / magnitude;
        for value in &mut vector {
            *value *= inverse;
        }
    }
    vector
}
