use super::{Check, Control, Error, Match, SystemParam, require};
use crate::{
    control_bar::{CommandAction, UiData},
    model_animation::AnimatedPart,
    particles::ParticleInspection,
    playtest_ui::{Action, PromotionIcon, dispatch_command, selected_command},
    playtest_view::UnitVisual,
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
    roots: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    parts: Query<'w, 's, (&'static AnimatedPart, &'static ViewVisibility)>,
    particles: ParticleInspection<'w, 's>,
    scorch: crate::scorch::Inspection<'w, 's>,
}

impl View<'_, '_> {
    pub(super) fn progress(
        self,
        check: &mut Check,
        control: &mut Control,
    ) -> Result<(), Box<dyn Error>> {
        match check.stage {
            0 if self.visible(&control.battle, check.setup.trap) => {
                let game = &control.battle.simulation;
                require(
                    game.entities[check.setup.trap].stealthed()
                        && !game.logically_visible(1, check.setup.trap),
                    "Demo Trap's native concealment is absent",
                )?;
                self.controls(&control.battle)?;
                dispatch_command(
                    &mut control.battle,
                    &mut control.mouse,
                    CommandAction::SwitchWeapon(2),
                );
                check.submitted_frame = Some(control.battle.simulation.tick);
                check.stage = 5;
                control.clock.unpause();
            }
            5 if control.clock.is_paused()
                && check
                    .submitted_frame
                    .is_some_and(|frame| control.battle.simulation.tick > frame) =>
            {
                require(
                    control.battle.simulation.entities[check.setup.trap].weapon_slot() == 2,
                    "original manual button did not select the tertiary slot",
                )?;
                check.submitted_frame = None;
                control.capture.intermediate(if check.proximity {
                    "demo-trap-proximity-controls.png"
                } else {
                    "demo-trap-controls.png"
                });
                check.stage = 1;
            }
            1 => {
                let button = if check.proximity { 1 } else { 5 };
                let command = selected_command(&control.battle, &self.data, button)
                    .ok_or("original Demo Trap activation button missing")?;
                dispatch_command(&mut control.battle, &mut control.mouse, command.action);
                control.clock.unpause();
                check.stage = 2;
            }
            2 if check.warning.is_some()
                && check.audio[0]
                && self.visible(&control.battle, check.setup.trap) =>
            {
                control.capture.intermediate(if check.proximity {
                    "demo-trap-proximity-warning.png"
                } else {
                    "demo-trap-warning.png"
                });
                check.stage = 3;
            }
            3 => {
                control.clock.unpause();
                check.stage = 4;
            }
            4 => self.explosion(check, control)?,
            _ => {}
        }
        Ok(())
    }

    fn explosion(&self, check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
        let Some((frame, point)) = check.explosion else {
            return Ok(());
        };
        if control.battle.simulation.tick < frame + 2 {
            return Ok(());
        }
        let generation = control.battle.generation;
        let mut particles = true;
        for name in [
            "BombTruckDefaultExplosionArms",
            "DemoTrapLenzFlare",
            "BombTruckHEShockwave",
        ] {
            particles &= self.particles.incendiary(name, generation)?;
        }
        check.visuals = [particles, self.scorch.explosion(point)?];
        if check.visuals.iter().all(|visible| *visible) && check.audio.iter().all(|heard| *heard) {
            require(
                !control.battle.simulation.entities[check.setup.trap].corpse_present(),
                "Demo Trap retained its body after the original final weapon",
            )?;
            control.clock.pause();
            control.capture.finish_scenario();
            check.stage = 255;
            println!(
                "Original Demo Trap native: proximity={}, paid construction/body/concealment, all three source buttons, manual hold, sound-only warning, exact thirty-frame delayed damage, three original explosion systems/scorch and both advancing audio events passed",
                check.proximity
            );
        }
        Ok(())
    }

    fn visible(&self, battle: &Match, actor: usize) -> bool {
        self.roots
            .iter()
            .find_map(|(visual, children)| {
                (visual.index == actor && visual.generation == battle.generation)
                    .then_some(children)
            })
            .is_some_and(|children| {
                children.iter().any(|child| {
                    self.parts
                        .get(child)
                        .is_ok_and(|(_, visible)| visible.get())
                })
            })
    }

    fn controls(&self, battle: &Match) -> Result<(), Box<dyn Error>> {
        for (button, slot) in [(1, 1), (2, 2), (5, 0)] {
            let command = selected_command(battle, &self.data, button)
                .ok_or("Demo Trap original command missing")?;
            require(
                matches!(command.action, CommandAction::SwitchWeapon(index) if index == slot),
                "Demo Trap button did not retain its original weapon slot",
            )?;
            let expected = self.data.icon(&command.image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(index) if *index == button)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "Demo Trap source button is not painted",
            )?;
        }
        Ok(())
    }
}
