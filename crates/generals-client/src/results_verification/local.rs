//! Ordinary defeat, continued viewing and the original replacement end window.
mod controls;
use bevy::{ecs::system::SystemParam, prelude::*, window::CursorOptions};
use generals_sim::skirmish::Faction;
use std::error::Error;

use crate::{
    control_bar::UiData,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, Field, Layer, Picture, PromotionTransition, ResultsScreen},
    playtest_view::Capture,
    skirmish_verification::require,
};

#[derive(Resource)]
pub struct Flow {
    target: Target,
    scores: Option<super::ResultsCapture>,
    controls_done: bool,
    notice_at: Option<u64>,
    viewed_at: Option<u64>,
    camera_before: Option<Vec2>,
    done: bool,
}

pub enum Target {
    Defeat,
    PlayerInfo,
    Scores,
}

pub fn prepare(app: &mut App, battle: &mut Match, target: Target) -> Result<(), Box<dyn Error>> {
    battle.start(Faction::Usa);
    crate::opponent_verification::local_defeat_result(&mut battle.simulation)?;
    app.insert_resource(Flow {
        target,
        scores: None,
        controls_done: false,
        notice_at: None,
        viewed_at: None,
        camera_before: None,
        done: false,
    })
    .add_systems(First, controls::update)
    .add_systems(
        PostUpdate,
        update.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    Ok(())
}

#[derive(SystemParam)]
struct Scene<'w, 's> {
    results: super::ResultsScene<'w, 's>,
    battle: Res<'w, Match>,
    screen: Res<'w, ResultsScreen>,
    data: Res<'w, UiData>,
    camera: Res<'w, CameraRig>,
    transition: Res<'w, PromotionTransition>,
    window: Single<'w, 's, &'static Window>,
    cursor: Single<'w, 's, &'static CursorOptions>,
    actions: Query<'w, 's, (&'static Action, &'static Node, Option<&'static ImageNode>)>,
    fields: Query<'w, 's, (&'static Field, &'static Node)>,
    controls: Query<'w, 's, (&'static Picture, &'static Node)>,
    pictures: Query<
        'w,
        's,
        (
            &'static Layer,
            &'static ImageNode,
            &'static InheritedVisibility,
        ),
    >,
}

fn update(
    mut flow: ResMut<Flow>,
    scene: Scene,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut capture: ResMut<Capture>,
    time: Res<Time<Real>>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let time = time.into_inner();
    if flow.done {
        return;
    }
    let result = if time.elapsed_secs() > 600.0 {
        Err(format!(
            "personal defeat rendering timed out at frame {}; notice={:?}, viewed={:?}, outcome={:?}, end timer={:?}",
            scene.battle.simulation.tick, flow.notice_at, flow.viewed_at,
            scene.battle.simulation.ended_at, scene.battle.simulation.end_game_at,
        ).into())
    } else {
        scene.check(&mut flow, &mut keys)
    };
    match result {
        Ok(true) => {
            flow.done = true;
            clock.pause();
            capture.finish_scenario();
        }
        Ok(false) => {}
        Err(error) => {
            error!("Original personal defeat flow failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl Scene<'_, '_> {
    fn check(
        self,
        flow: &mut Flow,
        keys: &mut ButtonInput<KeyCode>,
    ) -> Result<bool, Box<dyn Error>> {
        if let Some(scores) = &mut flow.scores {
            return self.results.check(scores);
        }
        if matches!(flow.target, Target::PlayerInfo) && flow.controls_done {
            return Ok(true);
        }
        let game = &self.battle.simulation;
        if self.screen.local_defeat() {
            require(
                game.end_game_at.is_none() && !self.battle.paused,
                "personal defeat stopped the game",
            )?;
            if !self.image_visible(false) {
                return Ok(false);
            }
            self.observer_controls()?;
            if flow.notice_at.is_none() {
                flow.notice_at = Some(game.tick);
                println!(
                    "LocalDefeat: original Defeated art visible at frame {}; independent simulation and input remain active",
                    game.tick
                );
            }
            return Ok(matches!(flow.target, Target::Defeat));
        }
        let Some(notice_at) = flow.notice_at else {
            return Ok(false);
        };
        if flow.viewed_at.is_none() && game.end_game_at.is_none() {
            require(
                game.tick > notice_at,
                "personal defeat did not allow continued viewing",
            )?;
            require(
                !self.battle.paused && self.cursor.visible,
                "continued viewing disabled normal input",
            )?;
            if let Some(before) = flow.camera_before {
                if self.camera.focus != before {
                    keys.release(KeyCode::ArrowRight);
                    flow.viewed_at = Some(game.tick);
                    println!(
                        "Continued viewing: original notice closed and keyboard camera moved at frame {}",
                        game.tick
                    );
                }
            } else {
                flow.camera_before = Some(self.camera.focus);
                keys.press(KeyCode::ArrowRight);
            }
        }
        if !self.screen.observer_quit() || !self.image_visible(true) {
            return Ok(false);
        }
        require(
            flow.viewed_at.is_some() && flow.controls_done,
            "final result arrived before continued camera viewing was verified",
        )?;
        require(
            game.ended_at.is_some() && game.end_game_at.is_some(),
            "ObserverQuit has no native end timer",
        )?;
        require(
            !self.cursor.visible && self.battle.selected.is_empty(),
            "final script did not disable cursor and selection",
        )?;
        println!(
            "ObserverQuit: original GameOver art visible at frame {} after genuine personal defeat and continued AI combat",
            game.tick
        );
        let mut scores = super::ResultsCapture {
            banner: super::Banner::Observer,
            banner_seen: false,
            passed: false,
            expected_rows: game.occupied.iter().filter(|occupied| **occupied).count(),
        };
        self.results.check(&mut scores)?;
        flow.scores = Some(scores);
        Ok(false)
    }

    fn observer_controls(&self) -> Result<(), Box<dyn Error>> {
        require(
            self.fields.iter().all(|(field, node)| {
                !matches!(field, Field::Money) || node.display == Display::None
            }) && self.controls.iter().all(|(picture, node)| {
                !matches!(picture, Picture::Power(_) | Picture::PowerNeedle)
                    || node.display == Display::None
            }),
            "observer with no viewed player retained money or power",
        )?;
        let scheme = &self.data.schemes[3];
        for (source_y, expected) in [(scheme.area.min.y - 0.5, false), (scheme.area.min.y, true)] {
            let point = Vec2::new(0.0, self.window.height() * source_y / 600.0);
            require(
                crate::playtest_ui::captures_pointer(
                    &self.battle,
                    &self.window,
                    point,
                    &self.data,
                    &self.transition,
                ) == expected,
                "observer input boundary does not match its original HUD rectangle",
            )?;
        }
        let background = self.data.image(&scheme.background);
        require(
            self.pictures.iter().any(|(layer, image, visible)| {
                matches!(layer, Layer::Game)
                    && visible.get()
                    && image.image == background.image
                    && image.rect == background.rect
            }),
            "defeated player retained its active faction background",
        )?;
        let disabled = self.data.image(&scheme.general[3]);
        let mut general = false;
        for (action, node, image) in &self.actions {
            match action {
                Action::Worker
                | Action::Slot(_)
                | Action::CancelConstruction
                | Action::CancelProduction(_)
                | Action::CancelUpgrade(_) => {
                    require(
                        node.display == Display::None,
                        "observer retained an ordinary player command or queue",
                    )?;
                }
                Action::Promotions => {
                    general = image.is_some_and(|image| {
                        image.image == disabled.image && image.rect == disabled.rect
                    });
                }
                _ => {}
            }
        }
        require(
            general,
            "observer General button did not use original disabled art",
        )?;
        Ok(())
    }

    fn image_visible(&self, observer: bool) -> bool {
        let expected = self
            .data
            .image(if observer { "GameOver" } else { "Defeated" });
        self.pictures.iter().any(|(layer, image, visible)| {
            (if observer {
                matches!(layer, Layer::ObserverQuit)
            } else {
                matches!(layer, Layer::LocalDefeat)
            }) && visible.get()
                && image.image == expected.image
                && image.rect == expected.rect
        })
    }
}
