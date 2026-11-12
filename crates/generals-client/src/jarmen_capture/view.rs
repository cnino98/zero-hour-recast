use super::{Check, Control, Error, Match, SystemParam, require};
use crate::{
    control_bar::{CommandAction, UiData},
    model_animation::{AnimatedPart, Animations, Players},
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::UnitVisual,
    source_material::SourceMaterial,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};

#[derive(SystemParam)]
pub(super) struct View<'w, 's> {
    data: Res<'w, UiData>,
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
            &'static Mesh3d,
        ),
    >,
    materials: Res<'w, Assets<SourceMaterial>>,
    meshes: Res<'w, Assets<Mesh>>,
    animations: Res<'w, Animations>,
}

impl View<'_, '_> {
    pub(super) fn progress(
        self,
        check: &mut Check,
        control: &mut Control,
    ) -> Result<(), Box<dyn Error>> {
        super::stages::progress(check, &self, control)
    }

    pub(super) fn command(&self, battle: &Match, reloading: bool) -> Result<(), Box<dyn Error>> {
        let command =
            selected_command(battle, &self.data, 1).ok_or("original Jarmen slot one missing")?;
        require(
            matches!(command.action, CommandAction::SnipeVehicle(1)),
            "Jarmen pilot command differs",
        )?;
        let expected = self.data.icon(&command.image);
        require(
            self.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(1))
                    && node.size().min_element() > 0.0
                    && self.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image
                            && icon.uv_rect == expected.uv_rect
                            && (!reloading || (0.0..100.0).contains(&icon.state.z))
                    })
            }),
            "original pilot command icon/reload clock is not painted",
        )
    }

    pub(super) fn visible(&self, battle: &Match, actor: usize, cloaked: bool) -> bool {
        self.roots
            .iter()
            .find_map(|(visual, _, children)| {
                (visual.index == actor && visual.generation == battle.generation)
                    .then_some(children)
            })
            .is_some_and(|children| {
                children.iter().any(|child| {
                    self.parts
                        .get(child)
                        .is_ok_and(|(_, visible, material, _)| {
                            visible.get()
                                && self.materials.get(&material.0).is_some_and(|material| {
                                    let opacity = material.parameters.opacity;
                                    opacity.is_finite()
                                        && opacity > 0.0
                                        && (!cloaked
                                            || (opacity < 1.0
                                                && material.alpha_mode == AlphaMode::Blend))
                                })
                        })
                })
            })
    }

    pub(super) fn animation(
        &self,
        battle: &Match,
        setup: &super::Setup,
        proof: &mut super::animation::Proof,
    ) -> Result<(), Box<dyn Error>> {
        let sniper = setup.sniper;
        let Some((_, players, children)) = self.roots.iter().find(|(visual, _, _)| {
            visual.index == sniper && visual.generation == battle.generation
        }) else {
            return Ok(());
        };
        let game = &battle.simulation;
        let actor = &game.entities[sniper];
        let firing = actor.weapon_condition() == Some("FIRING_A")
            && game.presentation.shots.iter().any(|shot| {
                shot.source == sniper
                    && shot.target == Some(setup.infantry)
                    && shot.weapon_slot == 0
            });
        for sample in self.animations.inspect(players, actor.type_index)? {
            if let Some(pose) = self.pose(children, sample.model) {
                proof.observe(&sample.clip, sample.frame, pose, actor.moving(), firing);
            }
        }
        Ok(())
    }

    fn pose(&self, children: &Children, model: usize) -> Option<super::animation::Pose> {
        super::animation::Pose::from_meshes(children.iter().filter_map(|entity| {
            let (part, visible, material, mesh) = self.parts.get(entity).ok()?;
            let opacity = self.materials.get(&material.0)?.parameters.opacity;
            (part.model == model && visible.get() && opacity.is_finite() && opacity > 0.0)
                .then(|| self.meshes.get(&mesh.0).map(|mesh| (entity, mesh)))
                .flatten()
        }))
    }
}
