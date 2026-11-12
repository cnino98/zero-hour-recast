//! Paid original Demo Trap controls, sound-only warning and delayed explosion.
use crate::{
    demo_trap_verification::{self, Setup},
    input::Mouse,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::Skirmish;
use std::error::Error;
#[cfg(test)]
mod tests;
mod view;
use view::View;

#[derive(Resource)]
struct Check {
    setup: Setup,
    proximity: bool,
    stage: u8,
    health: f32,
    warning: Option<u64>,
    explosion: Option<(u64, [f32; 3])>,
    audio: [bool; 2],
    visuals: [bool; 2],
}

impl Check {
    fn new(game: &Skirmish, setup: Setup, proximity: bool) -> Self {
        Self {
            health: game.entities[setup.victim].health,
            setup,
            proximity,
            stage: 0,
            warning: None,
            explosion: None,
            audio: [false; 2],
            visuals: [false; 2],
        }
    }

    fn observe(&mut self, game: &Skirmish) -> Result<bool, Box<dyn Error>> {
        let mut pause = false;
        for event in &game.presentation.object_fx_events {
            if event.object == self.setup.trap && event.effect == "FX_GLADemoTrapWarning" {
                require(
                    self.warning.is_none(),
                    "Demo Trap repeated its initial warning",
                )?;
                self.warning = Some(event.frame);
                pause = true;
            }
        }
        for shot in game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == self.setup.trap)
        {
            let unit = &game.catalog[game.entities[shot.source].type_index];
            require(
                unit.weapon_template(shot.weapon_index)
                    .is_some_and(|weapon| weapon.name == "DemoTrapDetonationWeapon")
                    && self.explosion.is_none(),
                "Demo Trap fired a different or repeated final weapon",
            )?;
            let warning = self
                .warning
                .ok_or("Demo Trap fired before its warning callback")?;
            require(
                game.tick - warning == 30 && game.entities[self.setup.victim].health < self.health,
                "Demo Trap final weapon changed its original delay or caused no damage",
            )?;
            self.explosion = Some((game.tick, shot.source_position));
        }
        if self.warning.is_some() && self.explosion.is_none() {
            require(
                game.entities[self.setup.trap].corpse_present()
                    && game.entities[self.setup.victim].health.to_bits() == self.health.to_bits(),
                "Demo Trap warning lost its body or damaged the victim before the final weapon",
            )?;
        }
        // The authored red flare begins black and ramps over five logic ticks.
        // Hold the real two-tick geometry while any new pipeline becomes ready.
        pause |= self
            .explosion
            .is_some_and(|(frame, _)| game.tick >= frame + 2);
        Ok(pause)
    }
}

pub fn prepare(app: &mut App, battle: &mut Match, proximity: bool) -> Result<(), Box<dyn Error>> {
    let setup = demo_trap_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.trap], true);
    app.insert_resource(Check::new(&battle.simulation, setup, proximity));
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let point = Vec2::from_array(game.entities[check.setup.trap].position) * 0.1;
    app.world_mut().resource_mut::<CameraRig>().focus = point;
    app.add_systems(PreUpdate, inspect_audio);
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    for (before, now) in check
        .audio
        .iter_mut()
        .zip(audio.ambient(["GLADemoChargeAlarm", "DemoTrapExplosion"]))
    {
        *before |= now;
    }
}

fn record(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.stage == 255 {
        return;
    }
    match check.observe(&battle.into_inner().simulation) {
        Ok(true) => clock.pause(),
        Ok(false) => {}
        Err(error) => {
            error!("Demo Trap native callback: {error}");
            check.stage = 255;
            exit.write(AppExit::error());
        }
    }
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    clock: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: View, mut control: Control) {
    if check.stage == 255 {
        return;
    }
    if control.real.elapsed_secs() > 90.0 {
        error!(
            "Demo Trap capture timeout: proximity={} stage={} warning={:?} explosion={:?} audio={:?} visuals={:?} frame={} victim_health={} trap={}",
            check.proximity,
            check.stage,
            check.warning,
            check.explosion,
            check.audio,
            check.visuals,
            control.battle.simulation.tick,
            control.battle.simulation.entities[check.setup.victim].health,
            generals_sim::skirmish::diagnostics::movement_details(
                &control.battle.simulation,
                check.setup.trap
            )
        );
        check.stage = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() {
        return;
    }
    if let Err(error) = view.progress(&mut check, &mut control) {
        error!("Demo Trap native stage {}: {error}", check.stage);
        check.stage = 255;
        control.exit.write(AppExit::error());
    }
}
