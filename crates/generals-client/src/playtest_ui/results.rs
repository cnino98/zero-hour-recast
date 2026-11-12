//! `ScriptActions` result banner followed by the original skirmish `ScoreScreen`.
#[cfg(test)]
mod tests;
use super::{Action, FontSizeAt600, Layer, Menu, UiData, at, faction_index, menu};
use crate::{
    control_bar::results::{COLUMNS, Window},
    playtest::Match,
};
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub enum ResultAction {
    Victory,
    Defeat,
    LocalDefeat,
}

#[derive(Resource)]
pub struct ResultsScreen {
    generation: u32,
    pub elapsed: Option<u32>,
    message: Option<ResultAction>,
    local_defeat_shown: bool,
    close_at: Option<u64>,
    pub rows: Vec<usize>,
}

impl ResultsScreen {
    pub const fn local_defeat(&self) -> bool {
        matches!(self.message, Some(ResultAction::LocalDefeat)) && !self.scores()
    }

    pub const fn observer_quit(&self) -> bool {
        self.local_defeat_shown
            && matches!(
                self.message,
                Some(ResultAction::Victory | ResultAction::Defeat)
            )
            && matches!(self.elapsed, Some(0..120))
    }

    pub fn apply(&mut self, action: ResultAction, battle: &mut Match, menu: &mut Menu) {
        self.show(action, battle.simulation.tick);
        menu.clear_confirmation();
        battle.paused = false;
        battle.promotions_open = false;
        match action {
            ResultAction::LocalDefeat => {}
            ResultAction::Victory | ResultAction::Defeat => {
                battle.simulation.start_end_game_timer();
                battle.clear_drawable_selection();
                self.enter(battle, 0);
            }
        }
    }

    const fn show(&mut self, action: ResultAction, frame: u64) {
        self.message = Some(action);
        if matches!(action, ResultAction::LocalDefeat) {
            self.local_defeat_shown = true;
            self.close_at = Some(frame + 120);
        }
    }

    fn close_expired(&mut self, frame: u64) {
        if self.close_at.is_some_and(|deadline| frame >= deadline) {
            self.close_at = None;
            self.message = None;
        }
    }

    pub const fn scores(&self) -> bool {
        matches!(self.elapsed, Some(120..))
    }

    pub const fn banner(&self, victorious: bool) -> bool {
        !self.local_defeat_shown
            && matches!(self.elapsed, Some(0..120))
            && matches!(
                (self.message, victorious),
                (Some(ResultAction::Victory), true) | (Some(ResultAction::Defeat), false)
            )
    }

    pub fn enter(&mut self, battle: &mut Match, elapsed: u32) {
        self.generation = battle.generation;
        self.elapsed = Some(elapsed);
        // ScoreScreen inserts into a score-key map; collisions increment the key.
        let mut scores = std::collections::BTreeMap::new();
        for owner in 0..battle.simulation.slots.len() {
            if !battle.simulation.occupied[owner] {
                continue;
            }
            let mut score = battle.simulation.players[owner]
                .statistics
                .calculate_score();
            while scores.contains_key(&score) {
                score += 1;
            }
            scores.insert(score, owner);
        }
        self.rows = scores.into_values().rev().collect();
        battle.paused = elapsed == 120;
        battle.promotions_open = false;
        battle.placing = None;
    }
}

#[derive(Component)]
pub struct Cell {
    pub row: usize,
    pub column: Option<usize>,
}

#[derive(Component)]
pub struct Label;

#[derive(Component)]
pub struct SideIcon(usize);

pub fn tick(mut screen: ResMut<ResultsScreen>, mut battle: ResMut<Match>) {
    if screen.generation != battle.generation || !battle.started {
        screen.generation = battle.generation;
        screen.elapsed = None;
        screen.message = None;
        screen.local_defeat_shown = false;
        screen.close_at = None;
    }
    if !battle.started || battle.paused {
        return;
    }
    let frame = battle.simulation.tick;
    screen.close_expired(frame);
    let Some(deadline) = battle.simulation.end_game_at else {
        return;
    };
    let elapsed = u32::try_from(frame.saturating_sub(deadline - 120))
        .unwrap_or(120)
        .min(120);
    screen.enter(&mut battle, elapsed);
}

pub(super) fn setup(commands: &mut Commands, data: &UiData) {
    commands.insert_resource(ResultsScreen {
        generation: u32::MAX,
        elapsed: None,
        message: None,
        local_defeat_shown: false,
        close_at: None,
        rows: vec![0, 1],
    });
    for (name, victorious) in [
        ("Victorious.wnd:", true),
        ("Victorious.wnd:Victorious", true),
        ("Defeat.wnd:Defeat", false),
        ("Defeat.wnd:DefeatImage", false),
    ] {
        window(commands, data, name, Layer::ResultBanner(victorious), 60);
    }
    for name in ["LocalDefeat.wnd:", "LocalDefeat.wnd:Defeat"] {
        window(commands, data, name, Layer::LocalDefeat, 60);
    }
    for name in ["ObserverQuit.wnd:Defeat", "ObserverQuit.wnd:DefeatImage"] {
        window(commands, data, name, Layer::ObserverQuit, 60);
    }
    for name in ["ParentScoreScreen", "MainBackdrop", "GadgetParent"] {
        window(
            commands,
            data,
            &format!("ScoreScreen.wnd:{name}"),
            Layer::Results,
            60,
        );
    }
    label(commands, data, "StaticTextPlayerName", None);
    for (index, (header, column, icon)) in COLUMNS.iter().enumerate() {
        label(commands, data, &format!("StaticText{header}"), None);
        window(
            commands,
            data,
            &format!("ScoreScreen.wnd:Cameo{icon}"),
            Layer::Results,
            62,
        );
        for row in 0..8 {
            label(
                commands,
                data,
                &format!("StaticText{column}{row}"),
                Some(Cell {
                    row,
                    column: Some(index),
                }),
            );
        }
    }
    for row in 0..8 {
        label(
            commands,
            data,
            &format!("StaticTextPlayer{row}"),
            Some(Cell { row, column: None }),
        );
        let area = data.results.windows[&format!("ScoreScreen.wnd:GameWindowWinner{row}")].area;
        commands.spawn((
            Layer::Results,
            SideIcon(row),
            data.image(&data.results.side_icons[0]),
            at(area),
            ZIndex(62),
        ));
    }
    let button = &data.results.windows["ScoreScreen.wnd:ButtonOk"];
    menu::button(
        commands,
        data,
        Action::ConfirmMenu,
        Layer::Results,
        button.area,
        &button.text,
    );
}

fn window(commands: &mut Commands, data: &UiData, name: &str, layer: Layer, order: i32) {
    let window = &data.results.windows[name];
    let mut entity = commands.spawn((layer, at(window.area), ZIndex(order)));
    if let Some(image) = &window.image {
        entity.insert(data.image(image));
    } else {
        entity.insert((
            Node {
                border: UiRect::all(px(1)),
                ..at(window.area)
            },
            BackgroundColor(window.background),
            BorderColor::all(window.border),
        ));
    }
}

fn label(commands: &mut Commands, data: &UiData, name: &str, cell: Option<Cell>) {
    let source: &Window = &data.results.windows[&format!("ScoreScreen.wnd:{name}")];
    // GameWindowManagerScript initializes centeredVertically=true and leftMargin=7.
    // W3DStaticText clips to its window and wraps within width-10.
    let parent = commands
        .spawn((
            Layer::Results,
            Node {
                align_items: AlignItems::Center,
                justify_content: if source.centered {
                    JustifyContent::Center
                } else {
                    JustifyContent::FlexStart
                },
                padding: UiRect {
                    left: px(if source.centered { 5.0 } else { 7.0 }),
                    right: px(if source.centered { 5.0 } else { 3.0 }),
                    ..default()
                },
                overflow: Overflow::clip(),
                ..at(source.area)
            },
            ZIndex(62),
        ))
        .id();
    let mut entity = commands.spawn((
        Label,
        Text::new(&source.text),
        TextFont {
            font_size: FontSize::Px(source.font_size),
            ..default()
        },
        FontSizeAt600(source.font_size),
        TextColor(source.text_color),
        TextShadow {
            offset: Vec2::ONE,
            color: source.text_border,
        },
        TextLayout::justify(if source.wrap_centered {
            Justify::Center
        } else {
            Justify::Left
        }),
        Node {
            max_width: percent(100),
            ..default()
        },
    ));
    if let Some(cell) = cell {
        entity.insert(cell);
    }
    let child = entity.id();
    commands.entity(parent).add_child(child);
}

pub fn update(
    screen: Res<ResultsScreen>,
    battle: Res<Match>,
    data: Res<UiData>,
    mut fields: Query<(&Cell, &mut Text, &mut TextColor)>,
    mut icons: Query<(&SideIcon, &mut ImageNode, &mut Node)>,
) {
    let screen = screen.into_inner();
    let battle = battle.into_inner();
    let data = data.into_inner();
    if !battle.started || !screen.scores() {
        return;
    }
    for (cell, mut text, mut color) in &mut fields {
        let Some(&owner) = screen.rows.get(cell.row) else {
            text.0.clear();
            continue;
        };
        let stats = &battle.simulation.players[owner].statistics;
        let values = [
            stats.units_built,
            stats.units_lost,
            stats.units_destroyed,
            stats.buildings_built,
            stats.buildings_lost,
            stats.buildings_destroyed,
            stats.money_earned,
        ];
        text.0 = cell.column.map_or_else(
            || player_name(battle, data, owner).to_owned(),
            |column| values[column].to_string(),
        );
        let [red, green, blue] = data.results.colors
            [battle.simulation.player_colors[owner].expect("occupied score row has a color")];
        color.0 = Color::srgb(red, green, blue);
    }
    for (row, mut image, mut node) in &mut icons {
        let Some(&owner) = screen.rows.get(row.0) else {
            node.display = Display::None;
            continue;
        };
        node.display = Display::Flex;
        let faction = faction_index(battle.simulation.players[owner].faction);
        *image = data.image(&data.results.side_icons[faction]);
    }
}

pub fn player_name<'a>(battle: &Match, data: &'a UiData, owner: usize) -> &'a str {
    use generals_sim::skirmish::OpponentDifficulty;
    if owner == 0 {
        return &data.preferences.name;
    }
    data.label(match battle.simulation.slots[owner].difficulty {
        Some(OpponentDifficulty::Easy) => "GUI:EasyAI",
        Some(OpponentDifficulty::Normal) => "GUI:MediumAI",
        Some(OpponentDifficulty::Hard) => "GUI:HardAI",
        None => "GUI:Closed",
    })
}

pub fn keys(
    screen: Res<ResultsScreen>,
    mut battle: ResMut<Match>,
    mut menu: ResMut<Menu>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    let screen = screen.into_inner();
    if !battle.started || !screen.scores() {
        return;
    }
    keys.clear_just_pressed(KeyCode::Escape);
    if keys.just_released(KeyCode::Escape) {
        menu::activate(&Action::ConfirmMenu, &mut menu, &mut battle);
        keys.clear_just_released(KeyCode::Escape);
    }
}
