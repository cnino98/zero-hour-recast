use super::{Check, Control, Error, Match, SystemParam, require};
use crate::{
    control_bar::{CommandAction, UiData},
    input::{PickTarget, picking},
    model_animation::{Animations, Players},
    particles::ParticleInspection,
    playtest_ui::{Action, PromotionIcon, selected_command},
    playtest_view::UnitVisual,
    projectile_render::ProjectileVisual,
    source_material::SourceMaterial,
    waypoint_display::RouteInspection,
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
    roots: Query<
        'w,
        's,
        (
            &'static UnitVisual,
            &'static Visibility,
            Option<&'static Children>,
            Option<&'static Players>,
        ),
    >,
    parts: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static ViewVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
    >,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    materials: Res<'w, Assets<SourceMaterial>>,
    animations: Res<'w, Animations>,
    particles: ParticleInspection<'w, 's>,
    route: RouteInspection<'w, 's>,
    picking: picking::View<'w, 's>,
}

impl View<'_, '_> {
    pub(super) fn progress(
        self,
        check: &mut Check,
        control: &mut Control,
    ) -> Result<(), Box<dyn Error>> {
        self.animation(&control.battle, check)?;
        super::stages::progress(check, &self, control)
    }

    pub(super) fn body(&self, battle: &Match, id: usize, cloak: bool) -> bool {
        self.roots.iter().any(|(root, _, children, _)| {
            root.index == id
                && root.generation == battle.generation
                && children.is_some_and(|children| {
                    children.iter().any(|child| {
                        self.parts.get(child).is_ok_and(|(_, visible, material)| {
                            visible.get()
                                && self.materials.get(&material.0).is_some_and(|material| {
                                    !cloak
                                        || (material.alpha_mode == AlphaMode::Blend
                                            && material.parameters.opacity > 0.0
                                            && material.parameters.opacity < 1.0)
                                })
                        })
                    })
                })
        })
    }

    pub(super) fn inventory(&self, battle: &Match, check: &Check) -> Result<(), Box<dyn Error>> {
        for (slot, name) in [
            (1, "Command_TransportExit"),
            (2, "Command_TransportExit"),
            (11, "Command_AttackMove"),
            (12, "Command_EmptyCrawler"),
            (13, "Command_Guard"),
            (14, "Command_Stop"),
        ] {
            let command = selected_command(battle, &self.data, slot)
                .ok_or("Outpost original command slot absent")?;
            require(
                command.name == name,
                "Outpost command set differs from installed source",
            )?;
            let image = if slot <= 2 {
                &self.data.units
                    [battle.simulation.entities[check.setup.riders[slot - 1]].type_index]
                    .button_image
            } else {
                &command.image
            };
            self.icon(slot, image)?;
        }
        require(
            check.setup.riders.iter().all(|&id| {
                self.roots.iter().any(|(root, visibility, _, _)| {
                    root.index == id
                        && root.generation == battle.generation
                        && *visibility == Visibility::Hidden
                })
            }),
            "Outpost contained riders remained drawn",
        )
    }

    pub(super) fn command(
        &self,
        battle: &Match,
        slot: usize,
    ) -> Result<CommandAction, Box<dyn Error>> {
        let command = selected_command(battle, &self.data, slot)
            .ok_or("Outpost original command slot absent")?;
        Ok(command.action)
    }

    fn icon(&self, slot: usize, image: &str) -> Result<(), Box<dyn Error>> {
        let expected = self.data.icon(image);
        require(
            self.buttons.iter().any(|(action, node, material)| {
                matches!(action, Action::Slot(index) if *index == slot)
                    && node.size().min_element() > 0.0
                    && self.icons.get(&material.0).is_some_and(|icon| {
                        icon.image == expected.image && icon.uv_rect == expected.uv_rect
                    })
            }),
            "Outpost original command/inventory icon is not painted",
        )
    }

    fn animation(&self, battle: &Match, check: &mut Check) -> Result<(), Box<dyn Error>> {
        for (root, _, _, players) in &self.roots {
            if root.index != check.setup.outpost || root.generation != battle.generation {
                continue;
            }
            let Some(players) = players else { continue };
            let kind = battle.simulation.entities[root.index].type_index;
            for state in self.animations.inspect(players, kind)? {
                if state.clip.eq_ignore_ascii_case("NVLOUTPOST_B.NVLOUTPOST_B") {
                    check.animated |= check
                        .animation
                        .is_some_and(|frame| frame.to_bits() != state.frame.to_bits());
                    check.animation = Some(state.frame);
                }
            }
        }
        Ok(())
    }

    pub(super) fn scan(&self, generation: u32) -> Result<bool, Box<dyn Error>> {
        self.particles.incendiary("IRLenzflare", generation)
    }

    pub(super) fn pick_enemy(&self, battle: &Match, enemy: usize) -> Option<Vec2> {
        let position = battle.simulation.object_position(enemy);
        let height = battle.simulation.catalog[battle.simulation.entities[enemy].type_index]
            .geometry
            .height
            * 0.5;
        let center = Vec3::new(
            position[0] * 0.1,
            (position[2] + height) * 0.1,
            -position[1] * 0.1,
        );
        let center = self
            .picking
            .camera
            .0
            .world_to_viewport(self.picking.camera.1, center)
            .ok()?;
        for x in [0.0, -6.0, 6.0, -12.0, 12.0] {
            for y in [0.0, -6.0, 6.0, -12.0, 12.0] {
                let point = center + Vec2::new(x, y);
                if matches!(self.picking.pick(battle, point), Some(PickTarget::Unit(id)) if id == enemy)
                {
                    return Some(point);
                }
            }
        }
        None
    }

    pub(super) fn route(&self) -> bool {
        self.route.revealed()
    }

    pub(super) fn missile(&self, battle: &Match, riders: &[usize]) -> Result<bool, Box<dyn Error>> {
        for (root, children) in &self.projectiles {
            if root.generation == battle.generation
                && battle.simulation.projectiles.iter().any(|projectile| {
                    projectile.id == root.id && riders.contains(&projectile.source)
                })
                && children.iter().any(|child| {
                    self.parts
                        .get(child)
                        .is_ok_and(|(_, visible, _)| visible.get())
                })
                && self.particles.missile(root.id, battle.generation)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn hull(&self, battle: &Match, host: usize) -> bool {
        battle
            .simulation
            .entities
            .iter()
            .enumerate()
            .any(|(id, actor)| {
                actor.producer() == Some(host)
                    && actor.physical_present()
                    && battle.simulation.catalog[actor.type_index].name
                        == "ChinaVehicleListeningOutpostDeadHull"
                    && self.body(battle, id, false)
            })
    }

    pub(super) fn debris(&self, battle: &Match, host: usize) -> bool {
        battle
            .simulation
            .entities
            .iter()
            .enumerate()
            .any(|(id, actor)| {
                actor.producer() == Some(host)
                    && actor.physical_present()
                    && actor.physical_model().is_some()
                    && self.body(battle, id, false)
            })
    }
}
