//! Original `RadiusDecal` template and `RadiusDecalUpdate` lifetime.
use super::{Entity, Skirmish, module_updates::FOREVER};

// OCL callbacks copy their immutable decal template into the live update module.
#[derive(Clone)]
pub struct Template {
    pub texture: String,
    pub opacity: [f32; 2],
    pub throb_frames: u32,
    pub color: [u8; 4],
    pub only_owner: bool,
    pub additive: bool,
}

pub(super) struct State {
    template: Template,
    position: [f32; 3],
    radius: f32,
    frame: u32,
    kill_after_attack: bool,
}

impl Skirmish {
    pub fn delivery_decal_templates(&self) -> impl Iterator<Item = &Template> {
        self.delivery_lists
            .iter()
            .filter_map(|list| list.decal.as_ref().map(|(_, template)| template))
    }

    pub(in crate::skirmish) fn create_attack_radius_decal(
        &mut self,
        id: usize,
        template: Template,
        radius: f32,
        position: [f32; 3],
    ) {
        if !self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .any(|module| module.family == "RadiusDecalUpdate")
        {
            return;
        }
        self.create_radius_decal(id, template, radius, position, true);
    }

    pub(in crate::skirmish) fn create_radius_decal(
        &mut self,
        id: usize,
        template: Template,
        radius: f32,
        position: [f32; 3],
        kill_after_attack: bool,
    ) {
        self.entities[id].radius_decal =
            (!template.texture.is_empty() && radius > 0.0).then_some(State {
                template,
                position,
                radius,
                frame: self.update_frame(),
                kill_after_attack,
            });
        let delay = if self.entities[id].radius_decal.is_some() {
            1
        } else {
            FOREVER
        };
        self.wake_family_update(id, "RadiusDecalUpdate", 0, delay);
    }

    pub(in crate::skirmish) fn update_radius_decal(&mut self, id: usize) -> i32 {
        if self.entities[id]
            .radius_decal
            .as_ref()
            .is_some_and(|state| state.kill_after_attack)
            && !self.entities[id].attacking()
        {
            self.entities[id].radius_decal = None;
            return i32::try_from(FOREVER).expect("native sleep range");
        }
        let now = self.update_frame();
        if let Some(state) = &mut self.entities[id].radius_decal {
            state.frame = now;
        }
        1
    }
}

impl Entity {
    #[must_use]
    pub fn radius_decal(&self) -> Option<super::spectre::DecalView<'_>> {
        self.radius_decal
            .as_ref()
            .map(|state| super::spectre::DecalView {
                style: &state.template,
                position: state.position,
                radius: state.radius,
                frame: Some(state.frame),
                enabled: true,
            })
    }
}
