//! SH `BoneFXUpdate` fixed-delay, one-shot FX/particle slots and object attachments.
use super::{Skirmish, WeaponFxEvent, physical_ocl::transform};

pub struct AtBone {
    pub bone: String,
    pub name: String,
    pub delay: i32,
}

pub struct Slot {
    pub fx: Option<AtBone>,
    pub particle: Option<AtBone>,
}

pub struct Rules {
    pub slots: [[Slot; 8]; 4],
    pub damage_fx: u64,
    pub damage_particle: u64,
}

pub struct Event {
    pub object: usize,
    pub module: usize,
    pub frame: u64,
    pub action: Action,
}

pub enum Action {
    Particle {
        slot: usize,
        template: String,
        local: [f32; 3],
        position: [f32; 3],
        basis: [[f32; 3]; 3],
        hidden: bool,
    },
    Clear,
}

pub(super) struct State {
    active: bool,
    body: usize,
    resolved: [bool; 4],
    next_fx: [[i32; 8]; 4],
    next_particle: [[i32; 8]; 4],
    fx_positions: [[[f32; 3]; 8]; 4],
    particle_positions: [[[f32; 3]; 8]; 4],
}

pub(super) fn validate(unit: &super::UnitType) -> Result<(), String> {
    let update = unit
        .behavior_modules
        .iter()
        .any(|module| module.family == "BoneFXUpdate");
    let damage = unit
        .behavior_modules
        .iter()
        .any(|module| module.family == "BoneFXDamage");
    if update && matches!(unit.body_kind, super::BodyKind::Absent) {
        return Err(format!(
            "{}: BoneFXUpdate requires a concrete Body module",
            unit.name
        ));
    }
    if update != damage {
        return Err(format!(
            "{}: BoneFXUpdate and BoneFXDamage require each other",
            unit.name
        ));
    }
    Ok(())
}

impl Skirmish {
    pub(in crate::skirmish) fn bone_fx_created(&self, id: usize) {
        let unit = &self.catalog[self.entities[id].type_index];
        assert!(
            unit.behavior_modules
                .iter()
                .any(|module| module.family == "BoneFXDamage"),
            "BoneFXUpdate requires BoneFXDamage"
        );
        assert!(
            !self.entities[id].behaviors.bone_fx.is_empty(),
            "BoneFXDamage requires BoneFXUpdate"
        );
    }

    pub(in crate::skirmish) fn construct_bone_fx(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.bone_fx.len(), ordinal);
        self.entities[id].behaviors.bone_fx.push(State {
            active: false,
            body: 0,
            resolved: [false; 4],
            next_fx: [[-1; 8]; 4],
            next_particle: [[-1; 8]; 4],
            fx_positions: [[[0.0; 3]; 8]; 4],
            particle_positions: [[[0.0; 3]; 8]; 4],
        });
    }

    pub(in crate::skirmish) fn update_bone_fx(&mut self, id: usize, ordinal: usize) -> i32 {
        if !self.entities[id].behaviors.bone_fx[ordinal].active {
            self.initialize_bone_times(id, ordinal);
            self.entities[id].behaviors.bone_fx[ordinal].active = true;
        }
        let now = i32::from_ne_bytes(self.update_frame().to_ne_bytes());
        for slot in 0..8 {
            let state = &self.entities[id].behaviors.bone_fx[ordinal];
            let body = state.body;
            let next = state.next_fx[body][slot];
            if next != -1 && next <= now {
                self.resolve_bone_fx(id, ordinal);
                let kind = self.entities[id].type_index;
                let rules = &self.catalog[kind].behavior.bone_fx[ordinal];
                if self.bone_damage_allowed(id, rules.damage_fx) {
                    let effect = rules.slots[body][slot]
                        .fx
                        .as_ref()
                        .expect("scheduled FX")
                        .name
                        .clone();
                    let local =
                        self.entities[id].behaviors.bone_fx[ordinal].fx_positions[body][slot];
                    let (origin, basis) = self.object_transform(id);
                    let offset = transform::vector(basis, local);
                    let position = std::array::from_fn(|axis| origin[axis] + offset[axis]);
                    if !effect.is_empty() && !effect.eq_ignore_ascii_case("NONE") {
                        self.positional_fx_with_radius(id, effect, position, 0.0);
                    }
                }
                self.entities[id].behaviors.bone_fx[ordinal].next_fx[body][slot] = -1;
            }
            let next = self.entities[id].behaviors.bone_fx[ordinal].next_particle[body][slot];
            if next != -1 && next <= now {
                self.emit_bone_particle(id, ordinal, slot);
                self.entities[id].behaviors.bone_fx[ordinal].next_particle[body][slot] = -1;
            }
        }
        1
    }

    fn initialize_bone_times(&mut self, id: usize, ordinal: usize) {
        let kind = self.entities[id].type_index;
        let now = i32::from_ne_bytes(self.update_frame().to_ne_bytes());
        let state = &mut self.entities[id].behaviors.bone_fx[ordinal];
        let body = state.body;
        for (slot, rules) in self.catalog[kind].behavior.bone_fx[ordinal].slots[body]
            .iter()
            .enumerate()
        {
            // Fixed UNIFORM ranges return the bound without advancing either RNG.
            state.next_fx[body][slot] = rules
                .fx
                .as_ref()
                .map_or(-1, |fx| now.wrapping_add(fx.delay));
            state.next_particle[body][slot] = rules
                .particle
                .as_ref()
                .map_or(-1, |particle| now.wrapping_add(particle.delay));
        }
    }

    fn resolve_bone_fx(&mut self, id: usize, ordinal: usize) {
        let state = &self.entities[id].behaviors.bone_fx[ordinal];
        let body = state.body;
        if state.resolved[body] || !self.entities[id].drawable.present {
            return;
        }
        let kind = self.entities[id].type_index;
        let positions = std::array::from_fn::<_, 8, _>(|slot| {
            let rules = &self.catalog[kind].behavior.bone_fx[ordinal].slots[body][slot];
            [rules.fx.as_ref(), rules.particle.as_ref()].map(|entry| {
                entry
                    .and_then(|entry| self.logical_bone_position(id, &entry.bone))
                    .unwrap_or([0.0; 3])
            })
        });
        let state = &mut self.entities[id].behaviors.bone_fx[ordinal];
        for (slot, pair) in positions.into_iter().enumerate() {
            state.fx_positions[body][slot] = pair[0];
            state.particle_positions[body][slot] = pair[1];
        }
        state.resolved[body] = true;
    }

    fn bone_damage_allowed(&self, id: usize, mask: u64) -> bool {
        let actor = &self.entities[id];
        matches!(
            self.catalog[actor.type_index].body_kind,
            super::BodyKind::Inactive
        ) || mask & (1_u64 << actor.damage_state.damage_type as u32) != 0
    }

    fn emit_bone_particle(&mut self, id: usize, ordinal: usize, slot: usize) {
        self.resolve_bone_fx(id, ordinal);
        let state = &self.entities[id].behaviors.bone_fx[ordinal];
        let body = state.body;
        let kind = self.entities[id].type_index;
        let rules = &self.catalog[kind].behavior.bone_fx[ordinal];
        if !self.bone_damage_allowed(id, rules.damage_particle) {
            return;
        }
        let template = &rules.slots[body][slot]
            .particle
            .as_ref()
            .expect("scheduled PS")
            .name;
        if template.is_empty() || template.eq_ignore_ascii_case("NONE") {
            return;
        }
        let local = state.particle_positions[body][slot];
        let template = template.clone();
        let (position, basis) = self.object_transform(id);
        let hidden = self.entities[id].drawable_effectively_hidden();
        self.bone_event(
            id,
            ordinal,
            Action::Particle {
                slot,
                template,
                local,
                position,
                basis,
                hidden,
            },
        );
    }

    // BoneFXDamage deliberately finds the FIRST update module, not a paired ordinal.
    pub(in crate::skirmish) fn bone_body_changed(&mut self, id: usize, body: usize) {
        assert!(
            !self.entities[id].behaviors.bone_fx.is_empty(),
            "BoneFXDamage requires BoneFXUpdate"
        );
        self.entities[id].behaviors.bone_fx[0].body = body;
        self.bone_event(id, 0, Action::Clear);
        self.initialize_bone_times(id, 0);
    }

    /// `BoneFXUpdate::stopAllBoneFX` cancels timers on the first found update.
    pub(in crate::skirmish) fn stop_first_bone_fx(&mut self, id: usize) {
        let Some(state) = self.entities[id].behaviors.bone_fx.first_mut() else {
            return;
        };
        state.next_fx = [[-1; 8]; 4];
        state.next_particle = [[-1; 8]; 4];
        self.bone_event(id, 0, Action::Clear);
    }

    pub(in crate::skirmish) fn destroy_bone_fx(&mut self, id: usize, ordinal: usize) {
        self.bone_event(id, ordinal, Action::Clear);
    }

    fn bone_event(&mut self, object: usize, module: usize, action: Action) {
        let index = self.presentation.bone_fx.len();
        self.presentation.bone_fx.push(Event {
            object,
            module,
            frame: self.tick,
            action,
        });
        self.presentation
            .weapon_fx
            .push(WeaponFxEvent::BoneFx(index));
    }
}

#[cfg(test)]
mod tests;
