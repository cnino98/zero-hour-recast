//! Ordinary GLA command buttons and their live source drawables.
mod setup;
mod view;
#[cfg(test)]
pub use setup::{paid_radar, sneak_site};

use crate::{
    capture_framing::focus_on,
    input::Mouse,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::Faction;
use std::error::Error;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Ambush,
    Gps,
    Sneak,
    Anthrax,
    Radar,
    Booby,
}

impl Kind {
    pub(crate) const fn power(self) -> &'static str {
        match self {
            Self::Ambush => "SuperweaponRebelAmbush",
            Self::Gps => "SuperweaponGPSScrambler",
            Self::Sneak => "SuperweaponSneakAttack",
            Self::Anthrax => "SuperweaponAnthraxBomb",
            Self::Radar => "SpecialPowerRadarVanScan",
            Self::Booby => "SpecialAbilityBoobyTrap",
        }
    }
}

struct Scene {
    provider: usize,
    target: [f32; 3],
    subject: usize,
    extra: Option<usize>,
}

#[derive(Resource)]
struct Check {
    kind: Kind,
    scene: Scene,
    first: usize,
    phase: Phase,
    began: Option<f64>,
    moved: bool,
}

#[derive(Debug, PartialEq)]
enum Phase {
    Button,
    Target,
    Release,
    Effect,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match, kind: Kind) -> Result<(), Box<dyn Error>> {
    if !matches!(kind, Kind::Radar | Kind::Booby) {
        // Stock multiplayer starting-money option funds the opponent's paid
        // replacement structures through the full rank-five combat exercise.
        battle.simulation.set_match_options(50_000, 1);
    }
    battle.simulation.reset(Faction::Gla, Faction::China)?;
    let scene = setup::prepare(&mut battle.simulation, kind)?;
    battle.select_objects(&[scene.provider], true);
    app.insert_resource(Check {
        kind,
        scene,
        first: battle.simulation.entities.len(),
        phase: Phase::Button,
        began: None,
        moved: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        Update,
        frame
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
    Ok(())
}

fn frame(check: Res<Check>, terrain: Res<Terrain>, mut camera: ResMut<CameraRig>) {
    let check = check.into_inner();
    if matches!(check.kind, Kind::Ambush) {
        camera.distance = camera.settings.min_height;
    }
    focus_on(&mut camera, terrain.into_inner(), check.scene.target);
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    time: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: view::View, mut control: Control) {
    if check.phase == Phase::Done || !control.capture.ready_for_scenario() {
        return;
    }
    let started = *check
        .began
        .get_or_insert_with(|| control.real.elapsed_secs_f64());
    let outcome = if control.real.elapsed_secs_f64() - started < 120.0 {
        view.progress(&mut check, &mut control)
    } else {
        Err(view::timeout_details(&check, &control.battle.simulation).into())
    };
    match outcome {
        Ok(true) => {
            check.phase = Phase::Done;
            control.time.pause();
            control.capture.finish_scenario();
            println!(
                "Native GLA {:?} captured at frame {}",
                check.kind, control.battle.simulation.tick
            );
        }
        Ok(false) => {}
        Err(error) => {
            error!("Native GLA {:?} {:?}: {error}", check.kind, check.phase);
            check.phase = Phase::Done;
            control.exit.write(AppExit::error());
        }
    }
}
