//! Normal paid production/combat followed by the original end-of-match UI.
mod combat;
pub mod local;

use std::error::Error;

use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    math::Affine2,
    prelude::*,
    text::TextLayoutInfo,
    ui::{CalculatedClip, ComputedStackIndex},
};
use generals_sim::skirmish::Skirmish;

use crate::{
    control_bar::UiData,
    playtest::Match,
    playtest_ui::{Layer, ResultCell, ResultLabel, ResultsScreen},
    playtest_view::Capture,
    skirmish_verification::require,
};

#[derive(Resource)]
pub struct ResultsCapture {
    banner: Banner,
    banner_seen: bool,
    passed: bool,
    expected_rows: usize,
}

enum Banner {
    Victory,
    Defeat,
    Observer,
}

impl Banner {
    const fn image(&self) -> &'static str {
        match self {
            Self::Victory => "Victorious",
            Self::Defeat => "Defeated",
            Self::Observer => "GameOver",
        }
    }

    const fn visible_layer(&self, layer: &Layer) -> bool {
        matches!(
            (self, layer),
            (Self::Victory, Layer::ResultBanner(true))
                | (Self::Defeat, Layer::ResultBanner(false))
                | (Self::Observer, Layer::ObserverQuit)
        )
    }
}

pub fn prepare(game: &mut Skirmish, victorious: bool) -> Result<ResultsCapture, Box<dyn Error>> {
    if victorious {
        crate::opponent_verification::human_victory(game)?;
        println!(
            "Result scenario: human defeated an actively building/fighting original AI at frame {}",
            game.tick
        );
        return Ok(ResultsCapture {
            banner: Banner::Victory,
            banner_seen: false,
            passed: false,
            expected_rows: 2,
        });
    }
    let owner = 1;
    combat::defeat(game)?;
    require(
        game.outcome == Some(owner),
        "produced tank did not win the result scenario",
    )?;
    require(
        game.players[owner].statistics.buildings_destroyed > 0,
        "result scenario did not record a destroyed enemy building",
    )?;
    println!(
        "Result scenario: owner {owner} won at tick {} by normal tank production/combat",
        game.tick
    );
    Ok(ResultsCapture {
        banner: Banner::Defeat,
        banner_seen: false,
        passed: false,
        expected_rows: 2,
    })
}

pub fn prepare_four(game: &mut Skirmish) -> Result<ResultsCapture, Box<dyn Error>> {
    crate::opponent_verification::four_player_result(game)?;
    require(
        game.occupied.iter().filter(|occupied| **occupied).count() == 4,
        "four-player result lost an occupied slot",
    )?;
    require(
        game.players[..4]
            .iter()
            .all(|player| player.statistics.money_earned > 0 && player.statistics.units_built > 1),
        "four-player score scenario did not exercise every army",
    )?;
    Ok(ResultsCapture {
        banner: if game.victorious(0) {
            Banner::Victory
        } else {
            Banner::Defeat
        },
        banner_seen: false,
        passed: false,
        expected_rows: 4,
    })
}

#[derive(SystemParam)]
pub struct ResultsScene<'w, 's> {
    battle: Res<'w, Match>,
    screen: Res<'w, ResultsScreen>,
    data: Res<'w, UiData>,
    pictures: Query<
        'w,
        's,
        (
            &'static Layer,
            &'static ImageNode,
            &'static InheritedVisibility,
        ),
    >,
    cells: Query<
        'w,
        's,
        (
            &'static ResultCell,
            &'static Text,
            &'static InheritedVisibility,
        ),
    >,
    music: Query<'w, 's, &'static AudioSink, With<crate::source_audio::ScoreMusic>>,
    labels: Query<'w, 's, LabelGeometry, With<ResultLabel>>,
}

#[derive(QueryData)]
struct LabelGeometry {
    cell: Option<&'static ResultCell>,
    text: &'static Text,
    node: &'static ComputedNode,
    transform: &'static UiGlobalTransform,
    layout: &'static TextLayoutInfo,
    clip: Option<&'static CalculatedClip>,
    stack: &'static ComputedStackIndex,
    visible: &'static InheritedVisibility,
}

pub fn update(
    mut scenario: ResMut<ResultsCapture>,
    scene: ResultsScene,
    mut capture: ResMut<Capture>,
    time: Res<Time<Real>>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let time = time.into_inner();
    if scenario.passed {
        return;
    }
    let result = if time.elapsed_secs() > 120.0 {
        Err(scene
            .timeout_context(&scenario, time.elapsed_secs(), &clock)
            .into())
    } else {
        scene.check(&mut scenario)
    };
    match result {
        Ok(true) => {
            eprintln!(
                "Result render readiness: {:.3} real seconds, {:.3} virtual seconds",
                time.elapsed_secs(),
                clock.elapsed_secs()
            );
            scenario.passed = true;
            clock.pause();
            capture.finish_scenario();
        }
        Ok(false) => {}
        Err(error) => {
            error!("Result verification failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl ResultsScene<'_, '_> {
    fn timeout_context(
        &self,
        scenario: &ResultsCapture,
        real_seconds: f32,
        clock: &Time<Virtual>,
    ) -> String {
        let game = &self.battle.simulation;
        let banners: Vec<_> = self
            .pictures
            .iter()
            .filter_map(|(layer, _, visibility)| match layer {
                Layer::ResultBanner(victory) => Some((*victory, visibility.get())),
                _ => None,
            })
            .collect();
        let score_background = self
            .pictures
            .iter()
            .any(|(layer, _, visibility)| matches!(layer, Layer::Results) && visibility.get());
        let cells = self
            .cells
            .iter()
            .filter(|(_, _, visible)| visible.get())
            .count();
        let labels = self
            .labels
            .iter()
            .filter(|label| {
                label.visible.get() && !label.node.is_empty() && !label.layout.glyphs.is_empty()
            })
            .count();
        let music: Vec<_> = self
            .music
            .iter()
            .map(|sink| {
                (
                    sink.empty(),
                    sink.is_paused(),
                    sink.position().as_secs_f64(),
                )
            })
            .collect();
        format!(
            "Result UI or original score music did not become ready within 120 seconds: real={real_seconds:.3}s virtual={:.3}s delta={:.6}s virtual_paused={} started={} paused={} tick={} ended={:?} since_end={:?} result_elapsed={:?} rows={:?} banner_seen={} banner_visibility={banners:?} score_background={score_background} visible_cells={cells} laid_out_visible_labels={labels} music(empty,paused,seconds)={music:?}",
            clock.elapsed_secs(),
            clock.delta_secs(),
            clock.is_paused(),
            self.battle.started,
            self.battle.paused,
            game.tick,
            game.ended_at,
            game.ended_at.map(|ended| game.tick.saturating_sub(ended)),
            self.screen.elapsed,
            self.screen.rows,
            scenario.banner_seen,
        )
    }

    fn check(self, scenario: &mut ResultsCapture) -> Result<bool, Box<dyn Error>> {
        let Some(elapsed) = self.screen.elapsed else {
            return Ok(false);
        };
        let game = &self.battle.simulation;
        require(
            self.screen.rows.len() == scenario.expected_rows,
            "score screen lost an occupied player row",
        )?;
        let deadline = game.end_game_at.ok_or("result lost its script end timer")?;
        require(
            u64::from(elapsed) <= game.tick.saturating_sub(deadline - 120).min(120)
                && game.tick.saturating_sub(deadline - 120) - u64::from(elapsed) <= 1,
            "result screen diverged from the source 120 logic-frame timer",
        )?;
        if !self.screen.scores() {
            let expected = self.data.image(scenario.banner.image());
            scenario.banner_seen |= self.pictures.iter().any(|(layer, image, visibility)| {
                scenario.banner.visible_layer(layer)
                    && visibility.get()
                    && image.image == expected.image
                    && image.rect == expected.rect
            });
            return Ok(false);
        }
        require(
            scenario.banner_seen,
            "source victory/defeat image was never visible",
        )?;
        let expected = self.data.image("MutiPlayer_ScoreScreen");
        if !self.pictures.iter().any(|(layer, image, visible)| {
            matches!(layer, Layer::Results)
                && visible.get()
                && image.image == expected.image
                && image.rect == expected.rect
        }) {
            return Ok(false);
        }
        let mut seen = 0;
        for (cell, text, visible) in &self.cells {
            let Some(&owner) = self.screen.rows.get(cell.row) else {
                require(text.0.is_empty(), "unoccupied result row contains text")?;
                continue;
            };
            if !visible.get() {
                return Ok(false);
            }
            let stats = &game.players[owner].statistics;
            let values = [
                stats.units_built,
                stats.units_lost,
                stats.units_destroyed,
                stats.buildings_built,
                stats.buildings_lost,
                stats.buildings_destroyed,
                stats.money_earned,
            ];
            let expected = cell.column.map_or_else(
                || {
                    crate::playtest_ui::result_player_name(&self.battle, &self.data, owner)
                        .to_owned()
                },
                |column| values[column].to_string(),
            );
            require(
                text.0 == expected,
                "visible result cell disagrees with source ScoreKeeper",
            )?;
            seen += 1;
        }
        require(
            seen == self.screen.rows.len() * 8,
            "score screen does not have every occupied seven-stat row and player name",
        )?;
        check_labels(&self.labels, self.screen.rows.len())?;
        if !self
            .music
            .iter()
            .any(|sink| !sink.empty() && sink.position().as_secs_f64() > 0.025)
        {
            return Ok(false);
        }
        println!(
            "Result UI passed: original {} banner, 120-frame transition, {seen} score/name cells, original background and playing score music",
            scenario.banner.image()
        );
        Ok(true)
    }
}

fn check_labels(
    labels: &Query<LabelGeometry, With<ResultLabel>>,
    rows: usize,
) -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for label in labels {
        if label.cell.is_some_and(|cell| cell.row >= rows) {
            require(
                label.text.0.is_empty(),
                "unoccupied result label contains text",
            )?;
            continue;
        }
        // Match extract_text_sections: glyph quads are relative to the content box.
        let transform = Affine2::from(*label.transform)
            * Affine2::from_translation(label.node.content_box().min);
        let painted = label.visible.get()
            && !label.node.is_empty()
            && label.layout.glyphs.iter().any(|glyph| {
                let bounds = Rect::from_center_size(
                    transform.transform_point2(glyph.position),
                    glyph.atlas_info.rect.size(),
                );
                label
                    .clip
                    .map_or(bounds, |clip| bounds.intersect(clip.clip))
                    .area()
                    > 0.0
            });
        if !painted {
            return Err(format!(
                "result label {:?} has no painted glyph area: node={:?}, position={:?}, glyphs={}, layout={:?}, clip={:?}, stack={}",
                label.text.0,
                label.node.size(),
                label.transform.translation,
                label.layout.glyphs.len(),
                label.layout.size,
                label.clip.map(|clip| clip.clip),
                label.stack.0,
            ).into());
        }
        count += 1;
    }
    require(
        count == 8 + rows * 8,
        "result screen must paint eight headings and every occupied score/name label",
    )
}
