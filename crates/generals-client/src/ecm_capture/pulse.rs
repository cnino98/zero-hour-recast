use super::{Check, View, require};
use crate::{particles::AdditiveQuad, playtest::CameraRig};
use bevy::prelude::*;
use std::error::Error;

pub(super) struct Evidence {
    focus: Vec2,
    yaw: f32,
    distance: f32,
    transform: [u32; 16],
    frame: u64,
    quad: AdditiveQuad,
}

impl Evidence {
    pub(super) const fn restore_camera(&self, camera: &mut CameraRig) {
        camera.focus = self.focus;
        camera.settings.yaw = self.yaw;
        camera.distance = self.distance;
    }
}

pub(super) fn inspect(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    let game = &view.battle.simulation;
    require(
        game.entities[check.setup.tank].alive(),
        "ECM died before pulse comparison",
    )?;
    let Some(viewport) = view.framing.uncovered_viewport() else {
        return Ok(());
    };
    let quads =
        view.particles
            .additive_quads("SonicRange1", view.battle.generation, *view.camera)?;
    let transform = view.camera.1.to_matrix().to_cols_array().map(f32::to_bits);
    if let Some(first) = &check.evidence {
        require(
            transform == first.transform,
            "ECM pulse comparison camera moved",
        )?;
        if game.tick < first.frame + 8 {
            return Ok(());
        }
        let Some(quad) = quads.iter().find(|quad| {
            quad.system == first.quad.system
                && quad.born == first.quad.born
                && quad.ordinal == first.quad.ordinal
                && changed(&first.quad, quad)
                && inside(viewport, quad.bounds)
        }) else {
            return Ok(());
        };
        report(game.tick, quad, view.camera.0);
        view.time.pause();
        view.capture.finish_scenario();
        check.phase = 255;
        println!(
            "Original ECM native state gates passed with paired fixed-camera SonicRange1 additive quads; independent red-ring pixels still require review"
        );
    } else {
        // Authored size growth clears the tank before the source red fades.
        // Alpha is zero in SonicRange1; original ONE+ONE blending uses RGB.
        let Some(quad) = quads.into_iter().find(|quad| {
            quad.size >= 60.0 && quad.color[0] >= 0.08 && inside(viewport, quad.bounds)
        }) else {
            return Ok(());
        };
        report(game.tick, &quad, view.camera.0);
        check.evidence = Some(Evidence {
            focus: view.rig.focus,
            yaw: view.rig.settings.yaw,
            distance: view.rig.distance,
            transform,
            frame: game.tick,
            quad,
        });
        view.time.pause();
        view.capture.intermediate("ecm-pulse-first.png");
        check.phase = 6;
    }
    Ok(())
}

fn inside(viewport: Rect, bounds: Rect) -> bool {
    bounds.min.cmpge(viewport.min + Vec2::splat(12.0)).all()
        && bounds.max.cmple(viewport.max - Vec2::splat(12.0)).all()
}

fn changed(first: &AdditiveQuad, next: &AdditiveQuad) -> bool {
    first.size.to_bits() != next.size.to_bits()
        && first.color.map(f32::to_bits) != next.color.map(f32::to_bits)
}

fn report(frame: u64, quad: &AdditiveQuad, camera: &Camera) {
    println!(
        "ECM SonicRange1 pixel review: frame={frame} system={} born={} ordinal={} logical_ROI={:?} source_size={} rendered_RGBA={:?} logical_viewport={:?} physical_viewport={:?}; original EXSonic2.tga additive ONE+ONE, compare ring RGB/radius with stationary outside controls",
        quad.system,
        quad.born,
        quad.ordinal,
        quad.bounds,
        quad.size,
        quad.color,
        camera.logical_viewport_size(),
        camera.physical_viewport_size()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn additive_zero_alpha_ring_requires_source_growth_and_fade_not_camera_motion() {
        let first = AdditiveQuad {
            system: 1,
            born: 20,
            ordinal: 0,
            size: 60.0,
            bounds: Rect::new(10.0, 10.0, 100.0, 100.0),
            color: [0.12, 0.0, 0.0, 0.0],
        };
        let mut next = AdditiveQuad {
            system: 1,
            born: 20,
            ordinal: 0,
            size: 60.0,
            bounds: Rect::new(20.0, 20.0, 110.0, 110.0),
            color: first.color,
        };
        assert!(!changed(&first, &next));
        next.size = 70.0;
        assert!(!changed(&first, &next));
        next.color[0] = 0.1;
        assert!(changed(&first, &next));
    }
}
