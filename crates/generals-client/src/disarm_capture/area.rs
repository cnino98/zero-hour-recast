use super::{Check, Control, Error, Stage, View, require};
use crate::{input::CommandMode, playtest_ui};
use bevy::{
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
};
use generals_sim::skirmish::{Order, Skirmish};

pub(super) struct Progress {
    start: Option<u64>,
    spent: Option<u64>,
    hazards: Option<usize>,
    pub finished: bool,
}

impl Progress {
    pub const fn new() -> Self {
        Self {
            start: None,
            spent: None,
            hazards: None,
            finished: false,
        }
    }

    pub fn record(&mut self, game: &mut Skirmish, id: usize) -> Result<(), Box<dyn Error>> {
        let actor = &game.entities[id];
        require(
            game.presentation.shots.iter().all(|shot| shot.source != id)
                && game
                    .projectiles
                    .iter()
                    .all(|projectile| projectile.source != id),
            "empty-ground DISARM emitted a weapon FX event or projectile",
        )?;
        let count = *self.hazards.get_or_insert(game.hazards.len());
        require(
            game.hazards.len() == count,
            "empty-ground DISARM created a poison field",
        )?;
        if let Some((start, frames, _)) = actor.animation_duration() {
            require(frames == 36, "area DISARM lost original digging duration")?;
            self.start.get_or_insert(start);
        }
        if self.spent.is_none() && actor.weapon_rounds_remaining(0) == 0 {
            require(
                self.start.is_some_and(|start| game.tick == start + 36),
                "area DISARM did not spend its one round after digging",
            )?;
            require(
                actor.weapon_condition() == Some("RELOADING_A"),
                "DISARM incorrectly updated ordinary weapon last-fire time",
            )?;
            self.spent = Some(game.tick);
        }
        if let Some(frame) = self.spent {
            require(
                actor.weapon_rounds_remaining(0) == u32::from(game.tick >= frame + 120),
                "area DISARM changed original reload timing",
            )?;
            require(
                game.mine_clearing_ready(id, 0) == (game.tick >= frame + 122),
                "area button lost its original ready-frame flicker guard",
            )?;
            self.finished = game.tick >= frame + 122;
        }
        Ok(())
    }
}

pub(super) fn progress(
    check: &mut Check,
    view: &View,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    match check.stage {
        Stage::AreaMode => {
            // Wait for actual sink retirement, including an empty sink awaiting
            // normal audio cleanup. The new click must prove its own voice.
            if !check.prior_voice_retired {
                return Ok(());
            }
            let action = view.mine_button(&control.battle)?;
            let count = control.mouse.voices.len();
            playtest_ui::dispatch_command(&mut control.battle, &mut control.mouse, action);
            require(
                matches!(control.mouse.pending_command, CommandMode::ClearMines(0))
                    && control.mouse.voices.len() == count,
                "area mode/empty UnitSpecificSound changed",
            )?;
            let point = control.battle.simulation.entities[check.setup.builder].position;
            check.pointer = view.ground_pointer([point[0] + 25.0, point[1]]);
            require(
                check.pointer.is_some(),
                "area ground click outside viewport",
            )?;
            check.audio[0] = false;
            check.stage = Stage::AreaPress;
        }
        Stage::AreaPress => {
            let game = &control.battle.simulation;
            let actor = &game.entities[check.setup.builder];
            if !view.mine_radius(
                actor
                    .active_weapon(&game.catalog[actor.type_index])
                    .continue_attack_range,
            ) {
                return Ok(());
            }
            control.events.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                window: *control.window,
            });
            check.stage = Stage::AreaRelease;
        }
        Stage::AreaRelease => {
            control.events.write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Released,
                window: *control.window,
            });
            check.stage = Stage::AreaOrder;
        }
        Stage::AreaOrder => {
            require(
                matches!(control.mouse.pending_command, CommandMode::Normal)
                    && matches!(
                        control.battle.simulation.entities[check.setup.builder]
                            .ai
                            .order,
                        Order::FireAt { .. }
                    ),
                "real ground click did not commit the original area command",
            )?;
            // Voice requests are consumed by the normal audio system; the
            // queued request and native playback are checked in separate stages.
            check.stage = Stage::AreaRun;
            control.time.unpause();
        }
        Stage::AreaRun if check.area.finished && check.audio[0] => {
            require(
                view.body(&control.battle, check.setup.builder),
                "area command lost original Dozer body",
            )?;
            control.capture.finish_scenario();
            check.stage = Stage::Done;
            println!(
                "Original Dozer DISARM native passed: paid Scout/context, advancing dig and original clearing FX/scorch/audio, custom removal/reload, slot14 icon/CLEARMINES radius, actual empty-ground click/VoiceDisarm and one harmless dig/reload without projectile or poison FX"
            );
        }
        _ => {}
    }
    Ok(())
}
