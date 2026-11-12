use super::{Check, Control, Error, Match, Stage, SystemParam, require};
use crate::{
    control_bar::UiData,
    input::{PickTarget, picking},
    model_animation::{Animations, Players},
    particles::ParticleInspection,
    playtest_ui::{Action, Picture, PromotionIcon, selected_command},
    playtest_view::UnitVisual,
    source_material::SourceMaterial,
};
use bevy::{
    prelude::*,
    ui_render::ui_material::MaterialNode,
    window::{CursorOptions, PrimaryWindow},
};

#[derive(SystemParam)]
pub(super) struct View<'w, 's> {
    pub(super) data: Res<'w, UiData>,
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
    portraits: Query<'w, 's, (&'static Picture, &'static ImageNode, &'static ComputedNode)>,
    roots: Query<'w, 's, (&'static UnitVisual, &'static Players, &'static Children)>,
    parts: Query<
        'w,
        's,
        (
            &'static ViewVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
    >,
    materials: Res<'w, Assets<SourceMaterial>>,
    animations: Res<'w, Animations>,
    pub(super) particles: ParticleInspection<'w, 's>,
    pub(super) scorch: crate::scorch::Inspection<'w, 's>,
    picking: picking::View<'w, 's>,
    cursor: Query<'w, 's, &'static Node, With<crate::attack_cursor::AttackPointer>>,
    window: Single<'w, 's, &'static CursorOptions, With<PrimaryWindow>>,
    radius: Res<'w, crate::radius_cursor::RadiusCursor>,
    radius_visual: Query<'w, 's, &'static ViewVisibility, With<crate::radius_cursor::RadiusVisual>>,
}

impl View<'_, '_> {
    pub(super) fn mine_radius(&self, radius: f32) -> bool {
        self.radius.mine_clearing_radius(radius)
            && self.radius_visual.iter().any(|visibility| visibility.get())
    }

    pub(super) fn ground_pointer(&self, point: [f32; 2]) -> Option<Vec2> {
        let ground = Vec2::from_array(point) * 0.1;
        self.picking
            .camera
            .0
            .world_to_viewport(
                self.picking.camera.1,
                Vec3::new(ground.x, self.picking.terrain.sample(ground), -ground.y),
            )
            .ok()
    }

    pub(super) fn mine_button(
        &self,
        battle: &Match,
    ) -> Result<crate::control_bar::CommandAction, Box<dyn Error>> {
        let command = selected_command(battle, &self.data, 14)
            .ok_or("original mine-clearing button missing")?;
        require(
            command.name == "Command_DisarmMinesAtPosition",
            "wrong original area control",
        )?;
        let expected = self.data.icon(&command.image);
        require(
            self.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(14))
                    && node.size().min_element() > 0.0
                    && self.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
            }),
            "mine-clearing slot14 icon is not painted",
        )?;
        Ok(command.action)
    }

    pub(super) fn progress(
        self,
        check: &mut Check,
        control: &mut Control,
    ) -> Result<(), Box<dyn Error>> {
        self.animation(&control.battle, check)?;
        super::stages::progress(check, &self, control)
    }

    pub(super) fn body(&self, battle: &Match, actor: usize) -> bool {
        self.roots.iter().any(|(root, _, children)| {
            root.index == actor
                && root.generation == battle.generation
                && children.iter().any(|child| {
                    self.parts.get(child).is_ok_and(|(visible, material)| {
                        visible.get()
                            && self
                                .materials
                                .get(&material.0)
                                .is_some_and(|material| material.parameters.opacity > 0.0)
                    })
                })
        })
    }

    pub(super) fn controls(&self, battle: &Match, actor: usize) -> Result<(), Box<dyn Error>> {
        let command = selected_command(battle, &self.data, 1)
            .ok_or("Dozer original construction control missing")?;
        require(
            command.name == "Command_ConstructAmericaPowerPlant",
            "Dozer original build command changed",
        )?;
        let expected = self.data.icon(&command.image);
        require(
            self.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(1))
                    && node.size().min_element() > 0.0
                    && self.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
            }),
            "Dozer original build icon is not painted",
        )?;
        let portrait = self
            .data
            .image(&self.data.units[battle.simulation.entities[actor].type_index].portrait);
        require(
            self.portraits.iter().any(|(picture, image, node)| {
                matches!(picture, Picture::Portrait)
                    && node.size().min_element() > 0.0
                    && image.image == portrait.image
                    && image.rect == portrait.rect
            }),
            "Dozer original portrait is not painted",
        )
    }

    pub(super) fn cursor_visible(&self) -> bool {
        !self.window.visible && self.cursor.iter().any(|node| node.display != Display::None)
    }

    fn animation(&self, battle: &Match, check: &mut Check) -> Result<(), Box<dyn Error>> {
        if !matches!(check.stage, Stage::Prime | Stage::Dig) {
            return Ok(());
        }
        for (root, players, _) in &self.roots {
            if root.index != check.setup.builder || root.generation != battle.generation {
                continue;
            }
            for draw in self
                .animations
                .inspect(players, battle.simulation.entities[root.index].type_index)?
            {
                if draw.clip.eq_ignore_ascii_case("AVCONSTDOZ_A.AVCONSTDOZ_A")
                    && draw.bounds.is_some()
                {
                    check.animated |= check
                        .animation
                        .is_some_and(|prior| prior.to_bits() != draw.frame.to_bits());
                    check.animation = Some(draw.frame);
                }
            }
        }
        Ok(())
    }

    pub(super) fn target_pointer(&self, battle: &Match, target: usize) -> Option<Vec2> {
        let point = battle.simulation.object_position(target);
        let height = battle.simulation.catalog[battle.simulation.entities[target].type_index]
            .geometry
            .height
            * 0.5;
        let world = Vec3::new(point[0] * 0.1, (point[2] + height) * 0.1, -point[1] * 0.1);
        let center = self
            .picking
            .camera
            .0
            .world_to_viewport(self.picking.camera.1, world)
            .ok()?;
        for x in [0.0, -5.0, 5.0, -10.0, 10.0] {
            for y in [0.0, -5.0, 5.0, -10.0, 10.0] {
                let point = center + Vec2::new(x, y);
                if matches!(self.picking.pick(battle, point), Some(PickTarget::Unit(id)) if id == target)
                {
                    return Some(point);
                }
            }
        }
        None
    }
}
