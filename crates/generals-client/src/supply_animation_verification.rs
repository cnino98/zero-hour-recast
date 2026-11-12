//! Normal depot command and live mesh deformation, without forced model states.
use crate::{
    input::{self, Mouse, picking::View},
    model_animation::{AnimatedPart, Animations, DrawInspection, Players},
    playtest::{CameraRig, Match},
    playtest_view::{Capture, UnitVisual},
    supply_verification::SupplyCapture,
};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

#[derive(Resource)]
pub struct SupplyAnimation {
    collector: usize,
    source: usize,
    issued: bool,
    passed: bool,
    deployment: Option<NetPose>,
    loaded_deployment: Option<NetPose>,
    recovery: Option<NetPose>,
    deployment_complete: bool,
}

struct NetPose {
    clip: String,
    frame: f32,
    bounds: [[f32; 3]; 2],
    cargo: u32,
}

impl SupplyAnimation {
    pub const fn new(scenario: &SupplyCapture) -> Self {
        Self {
            collector: scenario.collector,
            source: scenario.source,
            issued: false,
            passed: false,
            deployment: None,
            loaded_deployment: None,
            recovery: None,
            deployment_complete: false,
        }
    }
}

#[derive(SystemParam)]
pub struct AnimatedScene<'w, 's> {
    animation: Res<'w, Animations>,
    units: Query<'w, 's, (Entity, &'static UnitVisual, &'static Players)>,
    parts: Query<
        'w,
        's,
        (
            &'static AnimatedPart,
            &'static Mesh3d,
            &'static ChildOf,
            &'static Visibility,
        ),
    >,
    meshes: Res<'w, Assets<Mesh>>,
}

pub fn update(
    mut scenario: ResMut<SupplyAnimation>,
    view: View,
    scene: AnimatedScene,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    clock: (Res<Time<Real>>, ResMut<Time<Virtual>>, ResMut<CameraRig>),
    output: (ResMut<Capture>, MessageWriter<AppExit>),
) {
    let (time, mut clock, mut camera) = clock;
    let (mut capture, mut exit) = output;
    if scenario.passed {
        return;
    }
    let time = time.into_inner();
    let result = if time.elapsed_secs() > 120.0 {
        Err("Live supply net did not deploy, finish loading and recover within 120 seconds".into())
    } else if time.elapsed_secs() < 2.0 {
        return;
    } else {
        view.issue_lift(&mut scenario, &mut battle, &mut mouse)
            .and_then(|()| scene.check(&mut scenario, &battle))
    };
    if scenario.issued {
        camera.focus =
            Vec2::from_array(battle.simulation.entities[scenario.collector].position) * 0.1;
    }
    match result {
        Ok(true) => {
            scenario.passed = true;
            clock.pause();
            capture.finish_scenario();
        }
        Ok(false) => {}
        Err(error) => {
            error!("Supply animation verification failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl View<'_, '_> {
    fn issue_lift(
        self,
        scenario: &mut SupplyAnimation,
        battle: &mut Match,
        mouse: &mut Mouse,
    ) -> Result<(), Box<dyn Error>> {
        if !scenario.issued {
            let Some(point) = self.supply_screen_point(battle, scenario.source) else {
                return Ok(());
            };
            battle.select_objects(&[scenario.collector], true);
            if !input::context_order(
                battle,
                self.terrain_point(point),
                self.pick(battle, point),
                mouse,
            ) {
                return Err("Visible source did not accept the collector order".into());
            }
            scenario.issued = true;
        }
        Ok(())
    }
}

impl AnimatedScene<'_, '_> {
    fn check(self, scenario: &mut SupplyAnimation, battle: &Match) -> Result<bool, Box<dyn Error>> {
        let collector = scenario.collector;
        let Some((entity, _, players)) = self
            .units
            .iter()
            .find(|(_, unit, _)| unit.generation == battle.generation && unit.index == collector)
        else {
            return Ok(false);
        };
        let unit = &battle.simulation.entities[scenario.collector];
        let Some(net) = self
            .animation
            .inspect(players, unit.type_index)?
            .into_iter()
            .find(|module| module.module == 1 && module.bounds.is_some())
        else {
            return Ok(false);
        };
        let Some(bounds) = self.bounds(entity, net.model) else {
            return Ok(false);
        };
        Ok(scenario.observe(&net, bounds, unit.cargo))
    }
}

impl SupplyAnimation {
    fn observe(&mut self, net: &DrawInspection, bounds: [[f32; 3]; 2], cargo: u32) -> bool {
        let pose = || NetPose {
            clip: net.clip.clone(),
            frame: net.frame,
            bounds,
            cargo,
        };
        if net.mode == "ONCE_BACKWARDS" {
            let deployment = self.deployment.get_or_insert_with(pose);
            if net.clip != deployment.clip {
                return false;
            }
            if cargo > 0 && net.frame > 5.0 && net.frame < deployment.frame - 5.0 {
                self.loaded_deployment.get_or_insert_with(pose);
            }
            if net.frame <= 0.0
                && !self.deployment_complete
                && let Some(loaded) = &self.loaded_deployment
                && loaded.bounds[0][1] > bounds[0][1] + 0.1
            {
                eprintln!(
                    "Live source supply deployment: {} frame {} -> loaded {} at {} -> 0; net bottom {} -> {}",
                    net.clip,
                    deployment.frame,
                    loaded.cargo,
                    loaded.frame,
                    deployment.bounds[0][1],
                    bounds[0][1]
                );
                self.deployment_complete = true;
            }
            return false;
        }
        if !self.deployment_complete
            || net.mode != "ONCE"
            || self
                .deployment
                .as_ref()
                .is_none_or(|pose| pose.clip != net.clip)
        {
            return false;
        }
        if self.recovery.is_none() && cargo > 0 {
            self.recovery = Some(pose());
        }
        let Some(first) = &self.recovery else {
            return false;
        };
        if net.frame < first.frame + 30.0 || bounds[0][1] <= first.bounds[0][1] + 0.1 {
            return false;
        }
        eprintln!(
            "Live source supply recovery: {} ONCE frame {} -> {}, cargo {} -> {cargo}, rendered net bottom {} -> {}",
            net.clip, first.frame, net.frame, first.cargo, first.bounds[0][1], bounds[0][1]
        );
        true
    }
}

impl AnimatedScene<'_, '_> {
    fn bounds(&self, entity: Entity, model: usize) -> Option<[[f32; 3]; 2]> {
        let mut bounds: Option<[[f32; 3]; 2]> = None;
        for (part, handle, parent, visibility) in &self.parts {
            if parent.parent() != entity
                || part.model != model
                || matches!(visibility, Visibility::Hidden)
            {
                continue;
            }
            let mesh = self.meshes.get(&handle.0)?;
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            for point in points {
                let bounds = bounds.get_or_insert([*point; 2]);
                for axis in 0..3 {
                    bounds[0][axis] = bounds[0][axis].min(point[axis]);
                    bounds[1][axis] = bounds[1][axis].max(point[axis]);
                }
            }
        }
        bounds
    }
}
