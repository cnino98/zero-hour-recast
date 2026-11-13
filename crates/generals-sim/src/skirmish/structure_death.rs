//! `StructureCollapseUpdate` / `StructureToppleUpdate`, SH 2bfaea60.
//!
//! Gameplay timing and OCL choices use the logic stream. Ordered drawable
//! callbacks consume the existing client stream without changing object position.
use super::{Entity, Skirmish, object_fx, physical_ocl, startup};

mod collapse;
mod damage;
mod topple;

#[cfg(test)]
mod tests;

pub struct Rules {
    pub conditions: super::death_callbacks::Conditions,
    pub module: Module,
    pub phases: [Phase; 4],
}

pub enum Module {
    Collapse(CollapseRules),
    Topple(ToppleRules),
}

pub const TRANSITION_SLOTS: usize = 12;

pub struct TransitionRules {
    pub suppress_water: bool,
    pub suppress_water_fx: bool,
    pub particles: [Vec<TransitionParticle>; 4],
    pub effects: [Vec<TransitionEffect>; 4],
}

pub struct TransitionParticle {
    pub slot: usize,
    pub bone: String,
    pub random_bone: bool,
    pub template: String,
    pub position: [f32; 3],
}

pub struct TransitionEffect {
    pub slot: usize,
    pub template: String,
    pub local: [f32; 3],
}

pub struct Phase {
    pub effects: Vec<String>,
    pub objects: Vec<Option<physical_ocl::List>>,
}

pub struct CollapseRules {
    pub delay: [u32; 2],
    pub burst_delay: [u32; 2],
    pub big_burst_frequency: i32,
    pub damping: f32,
    pub shudder: f32,
    pub gravity: f32,
}

pub struct ToppleRules {
    pub delay: [u32; 2],
    pub burst_delay: [u32; 2],
    pub integrity: f32,
    pub decay: f32,
    pub suppress_water_fx: bool,
    pub start_effect: String,
    pub delay_effect: String,
    pub done_effect: String,
    pub crushing_effect: String,
    pub angles: Vec<(f32, String)>,
    /// Index in the ordinary death-lifecycle weapon collection.
    pub crushing_weapon: usize,
}

pub struct Event {
    pub object: usize,
    pub frame: u64,
    pub action: VisualAction,
    pub effect: Option<object_fx::Event>,
}

pub enum VisualAction {
    PositionFx,
    TransitionClear {
        module: usize,
    },
    TransitionParticle {
        module: usize,
        template: String,
        positions: Vec<[f32; 3]>,
    },
    CollapseShudder {
        height: f32,
        amplitude: f32,
    },
    Clear,
    ToppleBegin {
        burst_delay: [u32; 2],
    },
    ToppleCheck {
        burst_delay: [u32; 2],
    },
}

pub(super) struct State {
    module: ModuleState,
}

enum ModuleState {
    Collapse(collapse::State),
    Topple(topple::State),
}

impl Rules {
    pub fn physical_lists(&self) -> impl Iterator<Item = &physical_ocl::List> {
        self.phases
            .iter()
            .flat_map(|phase| phase.objects.iter().flatten())
    }
}

impl Entity {
    pub(in crate::skirmish) fn set_structure_basis(&mut self, basis: [[f32; 3]; 3]) {
        if let Some(state) = &mut self.structure_death
            && let ModuleState::Topple(state) = &mut state.module
        {
            state.set_basis(basis);
        }
    }

    #[must_use]
    pub fn structure_basis(&self) -> Option<[[f32; 3]; 3]> {
        match &self.structure_death.as_deref()?.module {
            ModuleState::Collapse(_) => None,
            ModuleState::Topple(state) => state.basis(),
        }
    }

    #[must_use]
    pub const fn post_collapse(&self) -> bool {
        self.drawable
            .model_conditions
            .has_flag(super::ModelConditions::POST_COLLAPSE)
    }
}

pub(in crate::skirmish) fn validate(unit: &super::UnitType) -> Result<(), String> {
    let mut families = unit.behavior_modules.iter().filter(|module| {
        matches!(
            module.family.as_str(),
            "StructureCollapseUpdate" | "StructureToppleUpdate"
        )
    });
    let expected = unit
        .structure_death
        .as_ref()
        .map(|rules| match rules.module {
            Module::Collapse(_) => "StructureCollapseUpdate",
            Module::Topple(_) => "StructureToppleUpdate",
        });
    if families.next().map(|module| module.family.as_str()) != expected || families.next().is_some()
    {
        return Err(format!(
            "{}: structure death data differs from its single authored update",
            unit.name
        ));
    }
    Ok(())
}

impl Skirmish {
    fn set_post_collapse(&mut self, id: usize) {
        let conditions = &mut self.entities[id].drawable.model_conditions;
        conditions.set_flag(1 << 5, false);
        conditions.set_flag(super::ModelConditions::POST_COLLAPSE, true);
    }

    pub(super) fn initialize_structure_death(&mut self, id: usize) {
        let unit = &self.catalog[self.entities[id].type_index];
        let Some(rules) = &unit.structure_death else {
            return;
        };
        let height = if matches!(
            unit.geometry.shape,
            crate::collision_geometry::Shape::Sphere
        ) {
            unit.geometry.major
        } else {
            unit.geometry.height
        };
        let state = match rules.module {
            Module::Collapse(_) => ModuleState::Collapse(collapse::State::new(height)),
            Module::Topple(_) => ModuleState::Topple(topple::State::new(height)),
        };
        self.entities[id].structure_death = Some(Box::new(State { module: state }));
    }

    pub(super) fn begin_structure_death(&mut self, id: usize) {
        let rules = self.catalog[self.entities[id].type_index]
            .structure_death
            .as_ref()
            .expect("structure module");
        if !self.die_conditions_match(id, &rules.conditions) {
            return;
        }
        let family = match rules.module {
            Module::Collapse(_) => "StructureCollapseUpdate",
            Module::Topple(_) => "StructureToppleUpdate",
        };
        if !matches!(
            self.catalog[self.entities[id].type_index].ai.update,
            super::UnitAiUpdate::Absent
        ) {
            self.mark_ai_dead(id);
        }
        self.deselect_dead_object(id);
        let mut state = self.entities[id]
            .structure_death
            .take()
            .expect("structure state");
        match &mut state.module {
            ModuleState::Collapse(state) => self.begin_collapse(id, state),
            ModuleState::Topple(state) => self.begin_topple(id, state),
        }
        self.entities[id].structure_death = Some(state);
        self.wake_family_update(id, family, 0, 1);
    }

    pub(super) fn update_structure_death(&mut self, id: usize) -> i32 {
        let mut state = self.entities[id]
            .structure_death
            .take()
            .expect("constructed structure update");
        match &mut state.module {
            ModuleState::Collapse(state) => self.update_collapse(id, state),
            ModuleState::Topple(state) => self.update_topple(id, state),
        }
        let sleeping = match &state.module {
            ModuleState::Collapse(state) => state.sleeping(),
            ModuleState::Topple(state) => state.sleeping(),
        };
        self.entities[id].structure_death = Some(state);
        if sleeping {
            i32::from_ne_bytes(super::module_updates::FOREVER.to_ne_bytes())
        } else {
            1
        }
    }

    fn structure_visual(
        &mut self,
        id: usize,
        action: VisualAction,
        effect: Option<object_fx::Event>,
    ) {
        self.presentation
            .weapon_fx
            .push(super::WeaponFxEvent::Structure(
                self.presentation.structure_events.len(),
            ));
        self.presentation.structure_events.push(Event {
            object: id,
            frame: self.tick,
            action,
            effect,
        });
    }

    fn structure_position_fx(&mut self, id: usize, effect: String, position: [f32; 3]) {
        if effect.is_empty() || effect.eq_ignore_ascii_case("NONE") {
            return;
        }
        let event =
            self.snapshot_object_fx(id, effect, position, physical_ocl::transform::yaw(0.0));
        self.structure_visual(id, VisualAction::PositionFx, Some(event));
    }

    fn structure_phase(&mut self, id: usize, phase: usize, position: [f32; 3]) {
        // Each native phase count defaults to one and has no INI count field.
        // FX choice is a logic draw, then FX executes, then the OCL choice.
        let rules = self.catalog[self.entities[id].type_index]
            .structure_death
            .as_ref()
            .expect("structure module");
        let effects = &rules.phases[phase].effects;
        if !effects.is_empty() {
            let index = startup::random_between(
                &mut self.random,
                u32::try_from(effects.len() - 1).expect("FX choices"),
            );
            let effect = effects[usize::try_from(index).expect("FX choice")].clone();
            self.structure_position_fx(id, effect, position);
        }
        let rules = self.catalog[self.entities[id].type_index]
            .structure_death
            .as_ref()
            .expect("structure module");
        let objects = &rules.phases[phase].objects;
        if !objects.is_empty() {
            let index = startup::random_between(
                &mut self.random,
                u32::try_from(objects.len() - 1).expect("OCL choices"),
            );
            let list = objects[usize::try_from(index).expect("OCL choice")].clone();
            if let Some(list) = list {
                self.create_physical_ocl_at(id, &list, position)
                    .expect("validated collapse OCL");
            }
        }
    }
}

fn random_delay(random: &mut [u32; 6], bounds: [u32; 2]) -> u32 {
    if bounds[0] >= bounds[1] {
        bounds[1]
    } else {
        startup::random_between(random, bounds[1] - bounds[0]) + bounds[0]
    }
}
