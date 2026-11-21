//! Native `VictoryConditions` messages and `InGameUI` six-entry scrolling text.
use crate::{
    control_bar::UiData, playtest::Match, playtest_ui::ResultsScreen, source_audio::SourceAudio,
};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::error::Error;

struct Message {
    text: String,
    color: [f32; 4],
    timestamp: u64,
}

#[derive(Resource)]
pub struct Notifications {
    position: Vec2,
    colors: [[f32; 4]; 2],
    point_size: f32,
    font_scale: f32,
    delay: u64,
    font: Option<Font>,
    generation: Option<u32>,
    defeated: Vec<bool>,
    messages: Vec<Message>,
}

#[derive(Component)]
pub struct Row(usize);

pub fn load(assets: &VirtualAssets) -> Result<Notifications, Box<dyn Error>> {
    use crate::skirmish_data::{definition, field, text};
    let source = content::scan(assets.read("data/ini/ingameui.ini")?)?;
    let block = definition(std::slice::from_ref(&source), "InGameUI", "")?;
    let number = |key, prefix| -> Result<u16, Box<dyn Error>> {
        field(&block.lines, 0, key)
            .and_then(|line| {
                line.tokens
                    .iter()
                    .find_map(|token| token.strip_prefix(prefix))
            })
            .ok_or("missing original message component")?
            .parse()
            .map_err(Into::into)
    };
    let mut colors = [[0.0; 4]; 2];
    for (color, key) in colors.iter_mut().zip(["MessageColor1", "MessageColor2"]) {
        for (channel, prefix) in color[..3].iter_mut().zip(["R:", "G:", "B:"]) {
            *channel = f32::from(u8::try_from(number(key, prefix)?)?);
        }
        color[3] = 255.0;
    }
    let delay: i32 = text(&block.lines, 0, "MessageDelayMS")?.parse()?;
    // InGameUI::init corrects the stock 75000 typo before converting to frames.
    let delay = u32::try_from(if delay == 75000 { 7500 } else { delay.max(0) })?;
    let language = [content::scan(assets.read("data/english/language.ini")?)?];
    let language = &definition(&language, "Language", "")?.lines;
    let font = field(language, 0, "MessageFont").ok_or("missing localized message font")?;
    if font.tokens.len() != 4 || font.tokens[1] != "Arial" {
        return Err("unrepresented original message font".into());
    }
    Ok(Notifications {
        position: Vec2::new(
            f32::from(number("MessagePosition", "X:")?),
            f32::from(number("MessagePosition", "Y:")?),
        ),
        colors,
        point_size: f32::from(font.tokens[2].parse::<u16>()?),
        font_scale: crate::fonts::resolution_adjustment(language)?,
        delay: (u64::from(delay) * 30).div_ceil(1000),
        font: Some(crate::fonts::load(
            "Arial",
            font.tokens[3].eq_ignore_ascii_case("yes"),
        )?),
        generation: None,
        defeated: Vec::new(),
        messages: Vec::new(),
    })
}

impl Notifications {
    fn font_pixels(&self, width: f32) -> f32 {
        crate::fonts::display_pixels(self.point_size, width, self.font_scale)
    }

    pub fn add(&mut self, text: String, timestamp: u64) {
        let color = if self.messages.first().is_none_or(|message| {
            message.color.map(f32::to_bits) == self.colors[1].map(f32::to_bits)
        }) {
            self.colors[0]
        } else {
            self.colors[1]
        };
        self.messages.insert(
            0,
            Message {
                text,
                color,
                timestamp,
            },
        );
        self.messages.truncate(6);
    }

    fn fade(&mut self, tick: u64) {
        for message in &mut self.messages {
            let elapsed = tick.saturating_sub(message.timestamp);
            if elapsed > self.delay {
                // Alpha is an exact integer in f32. Beyond 25600 frames the
                // native integer amount already exceeds every possible alpha.
                let age = u16::try_from(elapsed.min(25_600)).expect("bounded message age");
                let amount = (f32::from(age) * 0.01).trunc();
                message.color[3] = (message.color[3] - amount).max(0.0);
            }
        }
        self.messages.retain(|message| message.color[3] > 0.0);
    }
}

pub fn setup(
    mut commands: Commands,
    mut messages: ResMut<Notifications>,
    mut fonts: ResMut<Assets<Font>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let window = window.into_inner();
    let font = fonts.add(messages.font.take().expect("original message font"));
    let parent = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(messages.position.x),
                top: px(messages.position.y),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ZIndex(70),
        ))
        .id();
    for index in 0..6 {
        let row = commands
            .spawn((
                Row(index),
                Text::new(""),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(messages.font_pixels(window.width())),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextShadow {
                    offset: Vec2::ONE,
                    color: Color::BLACK,
                },
                TextLayout::justify(Justify::Left).with_no_wrap(),
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();
        commands.entity(parent).add_child(row);
    }
}

pub fn record(
    mut messages: ResMut<Notifications>,
    mut battle: ResMut<Match>,
    data: Res<UiData>,
    mut audio: ResMut<SourceAudio>,
    mut radar: ResMut<crate::radar::Radar>,
) {
    let battle = battle.as_mut();
    let data = data.into_inner();
    if messages.generation != Some(battle.generation) {
        messages.generation = Some(battle.generation);
        messages.messages.clear();
        messages.defeated = vec![false; battle.simulation.slots.len()];
    }
    if !battle.started {
        return;
    }
    record_dozer(&mut messages, battle, data, &mut audio, &mut radar);
    for event in std::mem::take(&mut battle.simulation.presentation.overcharge_events) {
        if event.owner == battle.observer_player.unwrap_or(0) {
            messages.add(
                data.label("GUI:OverchargeExhausted").to_owned(),
                event.frame,
            );
            audio.ui("GUIMessageReceived");
            radar.information(event.position, event.frame, battle.generation);
        }
    }
    for owner in 0..messages.defeated.len() {
        if !battle.simulation.defeated[owner] || messages.defeated[owner] {
            continue;
        }
        messages.defeated[owner] = true;
        if battle.simulation.tick <= 1 {
            continue;
        }
        let name = crate::playtest_ui::result_player_name(battle, data, owner);
        let text = format_name(data.label("GUI:PlayerHasBeenDefeated"), name);
        messages.add(text, battle.simulation.tick);
        audio.ui("GUIMessageReceived");
    }
}

fn record_dozer(
    messages: &mut Notifications,
    battle: &mut Match,
    data: &UiData,
    audio: &mut SourceAudio,
    radar: &mut crate::radar::Radar,
) {
    use generals_sim::skirmish::DozerFeedback;
    let viewed = battle.observer_player.unwrap_or(0);
    for event in std::mem::take(&mut battle.simulation.presentation.dozer_feedback) {
        match event {
            DozerFeedback::Construction {
                frame,
                owner,
                builder,
                builder_type,
                object_type,
                position,
            } if owner == viewed => {
                let display_name = &data.units[object_type].name;
                let name = if display_name.is_empty() {
                    format_name(
                        data.label("INI:MissingDisplayName"),
                        &battle.simulation.catalog[object_type].name,
                    )
                } else {
                    display_name.clone()
                };
                let text = format_name(data.label("DOZER:ConstructionComplete"), &name);
                messages.add(text, frame);
                audio.task_complete(battle, builder, builder_type);
                radar.construction(position, frame, battle.generation);
            }
            DozerFeedback::Repair { frame } => {
                messages.add(data.label("DOZER:RepairComplete").to_owned(), frame);
            }
            DozerFeedback::Construction { .. } => {}
        }
    }
}

fn format_name(template: &str, name: &str) -> String {
    let placeholder = if template.contains("%ls") {
        "%ls"
    } else if template.contains("%hs") {
        "%hs"
    } else {
        "%s"
    };
    template.replacen(placeholder, name, 1)
}

pub fn update(
    mut messages: ResMut<Notifications>,
    battle: Res<Match>,
    placement: (Res<UiData>, ResMut<crate::input::Mouse>),
    screen: Res<ResultsScreen>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mut rows: Query<(
        &Row,
        &mut Text,
        &mut TextColor,
        &mut TextShadow,
        &mut TextFont,
        &mut Node,
    )>,
) {
    let battle = battle.into_inner();
    let screen = screen.into_inner();
    let window = window.into_inner();
    let (data, mut mouse) = placement;
    let data = data.into_inner();
    for (frame, failure) in mouse.sneak_placement_rejections.drain(..) {
        use generals_sim::skirmish::PlacementFailure;
        let label = match failure {
            PlacementFailure::Generic => "GUI:CantBuildThere",
            PlacementFailure::RestrictedTerrain => "GUI:CantBuildRestrictedTerrain",
            PlacementFailure::NotFlatEnough => "GUI:CantBuildNotFlatEnough",
            PlacementFailure::ObjectsInTheWay => "GUI:CantBuildObjectsInTheWay",
            PlacementFailure::TooCloseToSupplies => "GUI:CantBuildTooCloseToSupplies",
            PlacementFailure::NoClearPath => "GUI:CantBuildNoClearPath",
            PlacementFailure::Shroud => "GUI:CantBuildShroud",
        };
        messages.add(data.label(label).to_owned(), frame);
    }
    if battle.started {
        messages.fade(battle.simulation.tick);
    }
    for (row, mut text, mut color, mut shadow, mut font, mut node) in &mut rows {
        let message = messages
            .messages
            .iter()
            .rev()
            .nth(row.0)
            .filter(|_| battle.started && !screen.scores());
        node.reborrow()
            .map_unchanged(|node| &mut node.display)
            .set_if_neq(if message.is_some() {
                Display::Flex
            } else {
                Display::None
            });
        if let Some(message) = message {
            font.reborrow()
                .map_unchanged(|font| &mut font.font_size)
                .set_if_neq(FontSize::Px(messages.font_pixels(window.width())));
            text.reborrow()
                .map_unchanged(|text| &mut text.0)
                .set_if_neq(message.text.clone());
            let [red, green, blue, alpha] = message.color.map(|channel| channel / 255.0);
            color.set_if_neq(TextColor(Color::srgba(red, green, blue, alpha)));
            shadow.set_if_neq(TextShadow {
                offset: Vec2::ONE,
                color: Color::srgba(0.0, 0.0, 0.0, alpha),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn player_names_are_arguments_not_recursive_format_strings() {
        assert_eq!(
            format_name("%ls has been defeated", "Me%s"),
            "Me%s has been defeated"
        );
        assert_eq!(format_name("Player %s lost", "Me%ls"), "Player Me%ls lost");
    }
    #[test]
    fn six_native_messages_alternate_and_fade_after_strict_timeout() {
        let mut queue = Notifications {
            position: Vec2::ZERO,
            colors: [[255.0; 4], [180.0, 180.0, 255.0, 255.0]],
            point_size: 10.0,
            font_scale: 0.7,
            delay: 225,
            font: None,
            generation: None,
            defeated: vec![],
            messages: vec![],
        };
        assert_eq!(queue.font_pixels(800.0).to_bits(), 13_f32.to_bits());
        assert_eq!(queue.font_pixels(1600.0).to_bits(), 23_f32.to_bits());
        assert_eq!(queue.font_pixels(80_000.0).to_bits(), 683_f32.to_bits());
        for index in 0..7 {
            queue.add(index.to_string(), 100);
        }
        assert_eq!(queue.messages.len(), 6);
        assert_eq!(queue.messages.last().unwrap().text, "1");
        assert_eq!(
            queue.messages[0].color.map(f32::to_bits),
            queue.colors[0].map(f32::to_bits)
        );
        queue.fade(325);
        assert_eq!(queue.messages[0].color[3].to_bits(), 255_f32.to_bits());
        queue.fade(326);
        assert_eq!(queue.messages[0].color[3].to_bits(), 253_f32.to_bits());
        queue.fade(326);
        assert_eq!(queue.messages[0].color[3].to_bits(), 251_f32.to_bits());
        queue.fade(30_000);
        assert!(queue.messages.is_empty());
    }
}
