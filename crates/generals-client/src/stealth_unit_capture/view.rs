use super::{Animation, Check, Control, Error, Match, SystemParam, require};
use crate::{
    control_bar::{CommandAction, UiData},
    model_animation::{AnimatedPart, Animations, Players},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::UnitVisual,
    source_material::SourceMaterial,
    stealth_view::HeatPart,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};

#[derive(SystemParam)]
pub(super) struct View<'w, 's> {
    data: Res<'w, UiData>,
    framing: crate::capture_framing::AircraftFrame<'w, 's>,
    icons: Res<'w, Assets<PromotionIcon>>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static ComputedNode,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    roots: Query<'w, 's, (&'static UnitVisual, &'static Players, &'static Children)>,
    parts: Query<
        'w,
        's,
        (
            &'static AnimatedPart,
            &'static ViewVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
    >,
    heat: Query<
        'w,
        's,
        (
            &'static ViewVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
        With<HeatPart>,
    >,
    materials: Res<'w, Assets<SourceMaterial>>,
    animations: Res<'w, Animations>,
}

impl View<'_, '_> {
    pub(super) fn progress(
        self,
        check: &mut Check,
        control: &mut Control,
    ) -> Result<(), Box<dyn Error>> {
        if check.scout {
            super::scout::progress(check, &self, control)
        } else {
            super::pathfinder::progress(check, &self, control)
        }
    }

    pub(super) fn command(
        &self,
        battle: &Match,
        slot: usize,
    ) -> Result<CommandAction, Box<dyn Error>> {
        let command =
            selected_command(battle, &self.data, slot).ok_or("original command slot missing")?;
        let expected = self.data.icon(&command.image);
        require(
            self.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(index) if *index == slot)
                    && node.size().min_element() > 0.0
                    && self.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
            }),
            "original command icon is not painted",
        )?;
        Ok(command.action)
    }

    pub(super) fn focus_aircraft(
        &self,
        camera: &mut crate::playtest::CameraRig,
        position: [f32; 3],
    ) {
        self.framing.focus(camera, position);
    }

    pub(super) fn scout_state(&self, battle: &Match, child: usize, target: usize) -> String {
        let frame = self
            .framing
            .visibility_diagnostic(battle, child)
            .unwrap_or_else(|error| error.to_string());
        format!(
            "Scout {frame}, target_position={:?} target_detected={} heat_visible={}",
            battle.simulation.object_position(target),
            battle.simulation.entities[target].detected(),
            self.detected(battle, target)
        )
    }

    pub(super) fn framed_aircraft(
        &self,
        battle: &Match,
        id: usize,
    ) -> Result<bool, Box<dyn Error>> {
        self.framing.visible(battle, id)
    }

    pub(super) fn visible(&self, battle: &Match, id: usize, cloaked: bool) -> bool {
        self.children(battle, id).is_some_and(|children| {
            children.iter().any(|child| {
                self.parts.get(child).is_ok_and(|(_, visible, material)| {
                    visible.get()
                        && self.materials.get(&material.0).is_some_and(|material| {
                            let opacity = material.parameters.opacity;
                            opacity.is_finite()
                                && opacity > 0.0
                                && (!cloaked
                                    || (opacity < 1.0 && material.alpha_mode == AlphaMode::Blend))
                        })
                })
            })
        })
    }

    pub(super) fn detected(&self, battle: &Match, id: usize) -> bool {
        self.children(battle, id).is_some_and(|children| {
            children.iter().any(|child| {
                self.heat.get(child).is_ok_and(|(visible, material)| {
                    visible.get()
                        && self.materials.get(&material.0).is_some_and(|material| {
                            material.alpha_mode == AlphaMode::Add
                                && material.parameters.emissive.max_element() > 0.0
                        })
                })
            })
        })
    }

    fn children(&self, battle: &Match, id: usize) -> Option<&Children> {
        self.roots.iter().find_map(|(visual, _, children)| {
            (visual.index == id && visual.generation == battle.generation).then_some(children)
        })
    }

    pub(super) fn animation(
        &self,
        battle: &Match,
        id: usize,
        clip: &str,
        state: &mut Animation,
    ) -> Result<(), Box<dyn Error>> {
        let Some((_, players, _)) = self
            .roots
            .iter()
            .find(|(visual, _, _)| visual.index == id && visual.generation == battle.generation)
        else {
            return Ok(());
        };
        for sample in self
            .animations
            .inspect(players, battle.simulation.entities[id].type_index)?
        {
            if sample.clip == clip {
                state.advanced |= state
                    .previous
                    .is_some_and(|frame| frame.to_bits() != sample.frame.to_bits());
                state.previous = Some(sample.frame);
            }
        }
        Ok(())
    }
}
