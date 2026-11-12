//! Installed placement through window coordinates and ordinary pointer commands.

use crate::{
    control_bar::CommandAction,
    input::{Mouse, picking::View},
    particles::orientation::{Indicator, Orientation},
    playtest::Match,
    playtest_ui,
    playtest_view::{Capture, Placement},
    skirmish_verification::{construct, friendly_entity},
    source_material::SourceMaterial,
};
use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

#[derive(Resource)]
pub struct PlacementCapture {
    pub focus: Vec2,
    builder: usize,
    factory: usize,
    site: Vec2,
    screen: Option<[Vec2; 2]>,
    direction: Vec2,
    expected_angle: Option<f32>,
    angle: Option<f32>,
    cash: u32,
    phase: Phase,
}

enum Phase {
    EscapePress,
    EscapeCancel,
    RightPress,
    RightCancel,
    Anchor,
    Threshold,
    ThresholdCheck,
    Drag,
    Preview,
    Release,
    Done,
}

pub fn prepare(battle: &mut Match) -> Result<PlacementCapture, Box<dyn Error>> {
    battle.start(Faction::Gla);
    let game = &mut battle.simulation;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    construct(game, worker, Kind::Barracks, 0)?;
    let factory = game.type_for(0, Kind::Factory).ok_or("no GLA factory")?;
    let home = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    let site =
        rotated_site(game, worker, factory, home).ok_or("no rotation-dependent factory site")?;
    battle.select_objects(&[worker], true);
    Ok(PlacementCapture {
        focus: site * 0.1,
        builder: worker,
        factory,
        site,
        screen: None,
        direction: Vec2::ZERO,
        expected_angle: None,
        angle: None,
        cash: 0,
        phase: Phase::EscapePress,
    })
}

fn rotated_site(game: &Skirmish, builder: usize, factory: usize, home: [f32; 2]) -> Option<Vec2> {
    for radius in 4_i16..60 {
        for edge in -radius..=radius {
            for offset in [
                [radius, edge],
                [-radius, edge],
                [edge, radius],
                [edge, -radius],
            ] {
                let point = Vec2::from_array(home)
                    + Vec2::new(f32::from(offset[0]), f32::from(offset[1])) * 10.0;
                if rotation_changes_legality(game, builder, factory, point) {
                    return Some(point);
                }
            }
        }
    }
    None
}

fn rotation_changes_legality(game: &Skirmish, builder: usize, factory: usize, point: Vec2) -> bool {
    // The margin protects this diagnostic from pixel quantization, without
    // changing gameplay's footprint or mouse projection.
    [
        Vec2::ZERO,
        Vec2::X * 3.0,
        Vec2::NEG_X * 3.0,
        Vec2::Y * 3.0,
        Vec2::NEG_Y * 3.0,
    ]
    .into_iter()
    .all(|offset| {
        let site = (point + offset).to_array();
        game.can_place_facing(factory, site, 0.0)
            && !game.can_place(factory, site)
            && game.build_site_reachable(builder, site)
    })
}

pub fn register(app: &mut App, scenario: Option<PlacementCapture>) {
    let Some(scenario) = scenario else {
        return;
    };
    app.insert_resource(scenario)
        .add_systems(Update, (plan, drive).chain().before(playtest_ui::buttons))
        .add_systems(
            PostUpdate,
            inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        );
}

fn plan(view: View, battle: Res<Match>, scenario: ResMut<PlacementCapture>, capture: Res<Capture>) {
    if capture.into_inner().ready_for_scenario() {
        view.plan_placement(battle.into_inner(), scenario.into_inner());
    }
}

impl View<'_, '_> {
    fn plan_placement(self, battle: &Match, scenario: &mut PlacementCapture) {
        if scenario.screen.is_some() || !matches!(scenario.phase, Phase::EscapePress) {
            return;
        }
        let project = |point: Vec2| {
            let rendered = point * 0.1;
            self.camera
                .0
                .world_to_viewport(
                    self.camera.1,
                    Vec3::new(rendered.x, self.terrain.sample(rendered), -rendered.y),
                )
                .ok()
        };
        let (Some(start), Some(end)) = (
            project(scenario.site),
            project(scenario.site + Vec2::X * 80.0),
        ) else {
            return;
        };
        if [start, end].into_iter().any(|point| {
            playtest_ui::captures_pointer(battle, &self.window, point, &self.ui, &self.promotions)
        }) {
            return;
        }
        let quantize = |point: Vec2| {
            (point * self.window.scale_factor()).as_ivec2().as_vec2() / self.window.scale_factor()
        };
        let (Some(world_start), Some(world_end)) = (
            self.terrain_point(quantize(start)),
            self.terrain_point(quantize(end)),
        ) else {
            return;
        };
        if !rotation_changes_legality(
            &battle.simulation,
            scenario.builder,
            scenario.factory,
            world_start,
        ) {
            return;
        }
        scenario.site = world_start;
        // Standalone Zero Hour Coord2D::toAngle reference from the actual
        // quantized projected endpoints; independent of pending placement state.
        let delta = world_end - world_start;
        scenario.direction = delta;
        let squared = [delta.x * delta.x, delta.y * delta.y];
        let length = (squared[0] + squared[1]).sqrt();
        let bearing = if length == 0.0 {
            0.0
        } else {
            (delta.x / length).clamp(-1.0, 1.0).acos()
        };
        scenario.expected_angle = Some(if delta.y < 0.0 { -bearing } else { bearing });
        scenario.screen = Some([start, end]);
    }
}

fn drive(
    mut scenario: ResMut<PlacementCapture>,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    input: (
        ResMut<ButtonInput<MouseButton>>,
        ResMut<ButtonInput<KeyCode>>,
    ),
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut clock: ResMut<Time<Virtual>>,
) {
    let Some([start, end]) = scenario.screen else {
        return;
    };
    if matches!(scenario.phase, Phase::Done) {
        return;
    }
    clock.pause();
    let (mut buttons, mut keys) = input;
    let point = match scenario.phase {
        Phase::Threshold | Phase::ThresholdCheck => {
            ((start * window.scale_factor()).trunc() + Vec2::X * 4.0) / window.scale_factor()
        }
        Phase::Drag | Phase::Preview => end,
        Phase::Release => Vec2::new(3.0, window.height() - 3.0),
        _ => start,
    };
    window.set_cursor_position(Some(point));
    match scenario.phase {
        Phase::EscapePress | Phase::RightPress | Phase::Anchor => {
            keys.release(KeyCode::Escape);
            playtest_ui::dispatch_command(
                &mut battle,
                &mut mouse,
                CommandAction::Produce(scenario.factory),
            );
            if matches!(scenario.phase, Phase::RightPress) {
                buttons.release(MouseButton::Left);
            } else {
                buttons.press(MouseButton::Left);
            }
        }
        Phase::EscapeCancel => keys.press(KeyCode::Escape),
        Phase::RightCancel => {
            // Both events belong to one ordinary pointer update. Separating
            // them by a GPU frame can exceed the native short-click timeout
            // while the first placement material pipeline is compiling.
            buttons.press(MouseButton::Right);
            buttons.release(MouseButton::Right);
        }
        Phase::Release => {
            scenario.cash = battle.simulation.players[0].cash;
            buttons.release(MouseButton::Left);
        }
        Phase::Threshold | Phase::ThresholdCheck | Phase::Drag | Phase::Preview | Phase::Done => {}
    }
}

fn inspect(
    mut scenario: ResMut<PlacementCapture>,
    battle: Res<Match>,
    previews: Query<(&Placement, &Transform, &InheritedVisibility)>,
    orientation: IndicatorInspection,
    mut capture: ResMut<Capture>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let time = time.into_inner();
    if matches!(scenario.phase, Phase::Done) {
        return;
    }
    if time.elapsed_secs() > 60.0 {
        error!("Original placement gesture diagnostic timed out");
        exit.write(AppExit::error());
        return;
    }
    if scenario.screen.is_none() {
        return;
    }
    let battle = battle.into_inner();
    let checked = if matches!(
        scenario.phase,
        Phase::Anchor | Phase::ThresholdCheck | Phase::Preview
    ) {
        orientation.check(battle, matches!(scenario.phase, Phase::Preview))
    } else {
        Ok(())
    };
    match checked.and_then(|()| scenario.check(battle, &previews)) {
        Ok(true) => capture.finish_scenario(),
        Ok(false) => {}
        Err(error) => {
            error!("Original placement gesture diagnostic failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl PlacementCapture {
    fn check(
        &mut self,
        battle: &Match,
        previews: &Query<(&Placement, &Transform, &InheritedVisibility)>,
    ) -> Result<bool, Box<dyn Error>> {
        match self.phase {
            Phase::EscapePress | Phase::RightPress | Phase::Anchor => {
                if battle.placing.is_none() {
                    return Err("ordinary build command did not begin placement".into());
                }
                self.phase = match self.phase {
                    Phase::EscapePress => Phase::EscapeCancel,
                    Phase::RightPress => Phase::RightCancel,
                    _ => Phase::Threshold,
                };
            }
            Phase::EscapeCancel | Phase::RightCancel => {
                if battle.placing.is_some() || battle.paused {
                    let input = if matches!(self.phase, Phase::EscapeCancel) {
                        "Escape"
                    } else {
                        "right button"
                    };
                    return Err(format!(
                        "{input} cancellation: placing={}, paused={}",
                        battle.placing.is_some(),
                        battle.paused
                    )
                    .into());
                }
                self.phase = if matches!(self.phase, Phase::EscapeCancel) {
                    Phase::RightPress
                } else {
                    Phase::Anchor
                };
            }
            Phase::Threshold => self.phase = Phase::ThresholdCheck,
            Phase::ThresholdCheck => self.phase = Phase::Drag,
            Phase::Drag => self.phase = Phase::Preview,
            Phase::Preview => {
                self.check_preview(battle, previews)?;
                self.phase = Phase::Release;
            }
            Phase::Release => {
                self.check_building(battle)?;
                self.phase = Phase::Done;
                return Ok(true);
            }
            Phase::Done => {}
        }
        Ok(false)
    }

    fn check_preview(
        &mut self,
        battle: &Match,
        previews: &Query<(&Placement, &Transform, &InheritedVisibility)>,
    ) -> Result<(), Box<dyn Error>> {
        let pending = battle.placing.as_ref().ok_or("drag lost placement")?;
        let (position, angle, legal) = pending.pose().ok_or("drag has no projected preview")?;
        if position.distance(self.site) > 0.001 {
            return Err("drag moved the original anchor".into());
        }
        if !legal
            || !battle
                .simulation
                .can_place_facing(self.factory, position.to_array(), angle)
            || battle
                .simulation
                .can_place(self.factory, position.to_array())
        {
            return Err(
                "drag did not rotate an obstructed default footprint into a legal site".into(),
            );
        }
        let expected_angle = self
            .expected_angle
            .ok_or("missing projected-input bearing")?;
        if angle.to_bits() != expected_angle.to_bits() {
            return Err(format!(
                "placement bearing differs from source input: screen={:?} projected_delta={:?} expected={expected_angle:?} actual={angle:?}",
                self.screen, self.direction
            ).into());
        }
        let expected = Vec3::new(expected_angle.cos(), 0.0, -expected_angle.sin());
        let (_, transform, _) = previews
            .iter()
            .find(|(preview, _, visible)| preview.model == self.factory && visible.get())
            .ok_or("rotated original preview has no visible entity")?;
        let forward = transform.rotation * Vec3::X;
        if forward.distance(expected) > 0.000_01 {
            return Err(format!(
                "rendered preview differs from native input bearing: screen={:?} projected_delta={:?} expected_angle={expected_angle:?} actual_angle={angle:?} expected_forward={expected:?} rendered_forward={forward:?}",
                self.screen, self.direction
            ).into());
        }
        self.angle = Some(angle);
        Ok(())
    }

    fn check_building(&self, battle: &Match) -> Result<(), Box<dyn Error>> {
        if battle.placing.is_some() {
            return Err("GUI release failed to commit the world anchor".into());
        }
        let game = &battle.simulation;
        let building = game
            .entities
            .iter()
            .find(|unit| unit.owner == 0 && unit.type_index == self.factory)
            .ok_or("release created no factory")?;
        if building.facing.to_bits() != self.angle.ok_or("no checked preview angle")?.to_bits()
            || Vec2::from_array(building.position).distance(self.site) > 0.001
        {
            return Err("committed factory differs from displayed position/facing".into());
        }
        if game.players[0].cash != self.cash - game.catalog[self.factory].cost {
            return Err("placement did not charge exactly the source building price".into());
        }
        println!(
            "Original placement: Escape/right-click cancel; Locater01 draws at anchor and below five physical pixels, Locater02 draws after drag; actual preview and arrow match source XY direction; GUI release commits angle {} and charges {}",
            building.facing, game.catalog[self.factory].cost
        );
        Ok(())
    }
}

type IndicatorRoot = (
    &'static Indicator,
    &'static Transform,
    &'static InheritedVisibility,
    &'static Children,
);
type IndicatorPart = (
    &'static Mesh3d,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static InheritedVisibility,
    &'static ViewVisibility,
);

#[derive(SystemParam)]
struct IndicatorInspection<'w, 's> {
    data: Res<'w, Orientation>,
    roots: Query<'w, 's, IndicatorRoot>,
    parts: Query<'w, 's, IndicatorPart>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
}

impl IndicatorInspection<'_, '_> {
    fn check(self, battle: &Match, arrow: bool) -> Result<(), Box<dyn Error>> {
        let data = self.data.into_inner();
        let meshes = self.meshes.into_inner();
        let materials = self.materials.into_inner();
        let pending = battle
            .placing
            .as_ref()
            .ok_or("orientation lost placement")?;
        let (point, angle, actual_arrow) = pending
            .orientation_indicator()
            .ok_or("orientation lost anchor")?;
        if actual_arrow != arrow {
            return Err("physical-pixel threshold chose wrong native orientation model".into());
        }
        let (indicator, transform, inherited, children) = self.roots.single()?;
        if !data.matches(indicator, arrow) || !inherited.get() {
            return Err("original orientation model is absent or hidden".into());
        }
        let expected = Vec3::new(angle.cos(), 0.0, -angle.sin());
        if (transform.rotation * Vec3::X).distance(expected) > 0.000_01
            || Vec2::new(transform.translation.x, -transform.translation.z).distance(point * 0.1)
                > 0.000_01
        {
            return Err("native orientation model differs from placement pose".into());
        }
        if !children.iter().any(|child| {
            self.parts
                .get(child)
                .is_ok_and(|(mesh, material, inherited, visible)| {
                    inherited.get()
                        && visible.get()
                        && meshes
                            .get(&mesh.0)
                            .is_some_and(|mesh| mesh.count_vertices() > 0)
                        && materials.get(&material.0).is_some()
                })
        }) {
            return Err(
                "original orientation has no visible source mesh/material in the camera".into(),
            );
        }
        Ok(())
    }
}
