use super::{Check, Control, View, emitter_visible, require};
use crate::{particles::SmudgeFan, playtest::CameraRig};
use bevy::prelude::*;
use std::error::Error;

pub(super) struct Evidence {
    focus: Vec2,
    yaw: f32,
    distance: f32,
    transform: [u32; 16],
    frame: u64,
    fans: Vec<SmudgeFan>,
}

impl Evidence {
    pub(super) const fn restore_camera(&self, camera: &mut CameraRig) {
        camera.focus = self.focus;
        camera.settings.yaw = self.yaw;
        camera.distance = self.distance;
    }
}

// Queue once, then let the normal FixedUpdate simulation/presentation pipeline
// publish production, creation and exit events before starting the emitter phase.
pub(super) fn begin(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    use crate::microwave_verification;
    if check.phase == 4 {
        check.production = Some(microwave_verification::queue_emitter(
            &mut control.battle.simulation,
            &check.setup,
        )?);
        check.phase = 8;
        control.time.unpause();
        return Ok(());
    }
    let production = check
        .production
        .as_ref()
        .ok_or("missing paid emitter production")?;
    let Some(victim) =
        microwave_verification::emitter_ready(&control.battle.simulation, production)
    else {
        return Ok(());
    };
    check.emission = Some(microwave_verification::prepare_emitter(
        &mut control.battle.simulation,
        &check.setup,
        victim,
        check.last_actual,
    )?);
    check.phase = 5;
    Ok(())
}

pub(super) fn inspect(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    if !emitter_visible(
        check,
        &view.battle,
        &view.framing,
        &view.particles,
        *view.camera,
    )? {
        return Ok(());
    }
    let game = &view.battle.simulation;
    let fans =
        view.particles
            .smudge_fans("MicrowaveEmitter", view.battle.generation, *view.camera)?;
    if !fans
        .iter()
        .any(|fan| fan.alpha > 0.0 && fan.displacement.iter().any(|value| *value != 0.0))
    {
        return Ok(());
    }
    let transform = view.camera.1.to_matrix().to_cols_array().map(f32::to_bits);
    if let Some(first) = &check.evidence {
        require(
            transform == first.transform,
            "emitter comparison camera moved",
        )?;
        if game.tick < first.frame + 6 || !changed(&first.fans, &fans) {
            return Ok(());
        }
        report(game.tick, &fans, view.camera.0);
        view.time.pause();
        view.capture.finish_scenario();
        check.phase = 255;
        println!(
            "Original Microwave native state gates passed: paid tank/targets, unchanged building health, garrison clearance, armor-correct emitter damage, and two fixed-camera source SMUDGE states; paired pixels require independent review"
        );
    } else {
        report(game.tick, &fans, view.camera.0);
        check.evidence = Some(Evidence {
            focus: view.rig.focus,
            yaw: view.rig.settings.yaw,
            distance: view.rig.distance,
            transform,
            frame: game.tick,
            fans,
        });
        view.time.pause();
        view.capture.intermediate("microwave-emitter-first.png");
        check.phase = 6;
    }
    Ok(())
}

fn changed(first: &[SmudgeFan], next: &[SmudgeFan]) -> bool {
    first.len() != next.len()
        || first.iter().zip(next).any(|(left, right)| {
            left.displacement.map(f32::to_bits) != right.displacement.map(f32::to_bits)
                || left.alpha.to_bits() != right.alpha.to_bits()
        })
}

fn report(frame: u64, fans: &[SmudgeFan], camera: &Camera) {
    let regions: Vec<_> = fans
        .iter()
        .map(|fan| (fan.bounds, fan.displacement, fan.alpha))
        .collect();
    println!(
        "Microwave SMUDGE pixel review: frame={frame} logical_viewport={:?} physical_viewport={:?} fans(logical_ROI, sampled_UV_displacement, alpha)={regions:?}",
        camera.logical_viewport_size(),
        camera.physical_viewport_size()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitter_pair_ignores_geometry_motion_and_requires_changed_sampling_or_alpha() {
        let first = [SmudgeFan {
            bounds: Rect::new(10.0, 20.0, 90.0, 100.0),
            displacement: [0.02, -0.01],
            alpha: 0.5,
        }];
        let mut next = [SmudgeFan {
            bounds: Rect::new(20.0, 30.0, 100.0, 110.0),
            displacement: [0.02, -0.01],
            alpha: 0.5,
        }];
        assert!(!changed(&first, &next));
        next[0].displacement[0] = 0.03;
        assert!(changed(&first, &next));
    }
}
