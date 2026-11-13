//! World cursor names from `InGameUI::createMouseoverHint` and `Mouse::drawTooltip`.
use crate::{
    control_bar::UiData,
    input::Mouse,
    playtest::Match,
    skirmish_data::{definition, field, text},
};
use bevy::{prelude::*, text::TextLayoutInfo, ui::FocusPolicy, window::PrimaryWindow};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Skirmish, stealth::Appearance, visibility::Status};
use std::{error::Error, time::Duration};

#[derive(Resource)]
pub struct Style {
    font: Option<Font>,
    points: f32,
    font_adjustment: f32,
    delay: Duration,
    fill: Duration,
    width_fraction: f32,
    animate_background: bool,
    text: [u8; 4],
    highlight: Color,
    shadow: Color,
    background: [u8; 4],
    border: Color,
    player_colors: [bool; 2],
    adjust_color: bool,
}

pub fn load(assets: &VirtualAssets) -> Result<Style, Box<dyn Error>> {
    let mouse = [content::scan(assets.read("data/ini/mouse.ini")?)?];
    let mouse = &definition(&mouse, "Mouse", "")?.lines;
    let languages = [content::scan(assets.read("data/english/language.ini")?)?];
    let language = &definition(&languages, "Language", "")?.lines;
    let font = field(language, 0, "TooltipFontName").ok_or("missing tooltip font")?;
    let [_, family, size, bold] = font.tokens.as_slice() else {
        return Err("invalid localized tooltip font".into());
    };
    let value = |key| text(mouse, 0, key);
    let yes = |key| value(key).map(|value| value.eq_ignore_ascii_case("yes"));
    let color = |key| -> Result<[u8; 4], Box<dyn Error>> {
        let line = field(mouse, 0, key).ok_or_else(|| format!("Mouse.ini is missing {key}"))?;
        let mut rgba = [0; 4];
        for (component, prefix) in rgba.iter_mut().zip(["R:", "G:", "B:", "A:"]) {
            *component = line
                .tokens
                .iter()
                .find_map(|token| token.strip_prefix(prefix))
                .ok_or_else(|| format!("Mouse.ini {key} is missing {prefix}"))?
                .parse()?;
        }
        Ok(rgba)
    };
    Ok(Style {
        font: Some(crate::fonts::load(
            family,
            bold.eq_ignore_ascii_case("yes"),
        )?),
        points: size.parse()?,
        font_adjustment: text(language, 0, "ResolutionFontAdjustment")?.parse()?,
        delay: Duration::from_millis(value("TooltipDelayTime")?.parse()?),
        fill: Duration::from_millis(value("TooltipFillTime")?.parse()?),
        width_fraction: value("TooltipWidth")?.parse::<f32>()? / 100.0,
        animate_background: yes("TooltipAnimateBackground")?,
        text: color("TooltipTextColor")?,
        highlight: rgba(color("TooltipHighlightColor")?),
        shadow: rgba(color("TooltipShadowColor")?),
        background: color("TooltipBackgroundColor")?,
        border: rgba(color("TooltipBorderColor")?),
        player_colors: [
            yes("UseTooltipAltTextColor")?,
            yes("UseTooltipAltBackColor")?,
        ],
        adjust_color: yes("AdjustTooltipAltColor")?,
    })
}

const fn rgba([red, green, blue, alpha]: [u8; 4]) -> Color {
    Color::srgba_u8(red, green, blue, alpha)
}

impl Style {
    fn font_pixels(&self, width: f32) -> f32 {
        // GlobalLanguage::adjustFontSize, then FontCharsClass's MulDiv(points, 96, 72).
        let increase = (width / 800.0 - 1.0) * self.font_adjustment;
        let scale = (1.0 + increase).max(1.0);
        ((self.points * scale).floor() * 96.0 / 72.0).round()
    }

    fn text_color(&self, owner: Option<u32>) -> Color {
        self.player_color(owner.filter(|_| self.player_colors[0]), self.text, 255)
    }

    fn background_color(&self, owner: Option<u32>) -> Color {
        self.player_color(owner.filter(|_| self.player_colors[1]), self.background, 0)
    }

    fn player_color(&self, owner: Option<u32>, default: [u8; 4], adjustment: u16) -> Color {
        let Some(owner) = owner else {
            return rgba(default);
        };
        let [red, green, blue] = [16, 8, 0].map(|shift| {
            let value = u16::try_from((owner >> shift) & 255).expect("packed color channel");
            u8::try_from(if self.adjust_color {
                u16::midpoint(value, adjustment)
            } else {
                value
            })
            .expect("tooltip color channel")
        });
        rgba([red, green, blue, default[3]])
    }
}

#[derive(Component)]
pub struct Panel(AssetId<Font>);

#[derive(Component)]
pub enum Clip {
    Background,
    Text { highlight: bool },
}

#[derive(Component)]
pub struct Label {
    highlight: bool,
}

pub fn spawn(
    mut commands: Commands,
    style: Option<ResMut<Style>>,
    mut fonts: ResMut<Assets<Font>>,
    old: Query<(Entity, &Panel)>,
) {
    let Some(style) = style else {
        return;
    };
    let style = style.into_inner();
    let Some(font) = style.font.take() else {
        return;
    };
    for (entity, panel) in &old {
        commands.entity(entity).despawn();
        fonts.remove(panel.0);
    }
    let font = fonts.add(font);
    commands
        .spawn((
            Panel(font.id()),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(i32::MAX - 1),
            FocusPolicy::Pass,
        ))
        .with_children(|parent| {
            parent.spawn((
                Clip::Background,
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                BackgroundColor::DEFAULT,
                BorderColor::all(style.border),
                FocusPolicy::Pass,
            ));
            for highlight in [false, true] {
                parent
                    .spawn((
                        Clip::Text { highlight },
                        Node {
                            position_type: PositionType::Absolute,
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        FocusPolicy::Pass,
                    ))
                    .with_child((
                        Label { highlight },
                        Text::default(),
                        TextFont::default().with_font(font.clone()),
                        TextColor::default(),
                        TextShadow {
                            offset: Vec2::ONE,
                            color: style.shadow,
                        },
                        Node {
                            position_type: PositionType::Absolute,
                            ..default()
                        },
                        FocusPolicy::Pass,
                    ));
            }
        });
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    scale: Res<'w, UiScale>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    panel: Single<'w, 's, (&'static mut Node, &'static mut Visibility), With<Panel>>,
    clips: Query<'w, 's, ClipNodes, (Without<Panel>, Without<Label>)>,
    labels: Query<'w, 's, LabelNodes, (Without<Panel>, Without<Clip>)>,
}

type ClipNodes = (
    &'static Clip,
    &'static mut Node,
    Option<&'static mut BackgroundColor>,
);

type LabelNodes = (
    &'static Label,
    &'static mut Node,
    &'static mut Text,
    &'static mut TextFont,
    &'static mut TextColor,
    &'static mut TextShadow,
    &'static TextLayoutInfo,
);

pub fn update(
    battle: Res<Match>,
    mouse: Res<Mouse>,
    data: Res<UiData>,
    style: Res<Style>,
    mut hover: Local<Hover>,
    view: View,
) {
    let battle = battle.into_inner();
    let mouse = mouse.into_inner();
    let data = data.into_inner();
    let style = style.into_inner();
    let cursor = view
        .window
        .cursor_position()
        .filter(|_| view.window.focused);
    let target = mouse.hovered.filter(|_| {
        battle.started
            && !battle.paused
            && !mouse.dragging
            && cursor.is_some()
            && battle.simulation.end_game_at.is_none()
    });
    let elapsed = hover.elapsed(
        view.clock.elapsed(),
        target.map(|id| (battle.generation, id)),
        cursor,
        style.delay,
    );
    let tooltip = target
        .filter(|_| battle.simulation.map_script_fade().is_none())
        .and_then(|id| hint(battle, data, id));
    view.draw(style, cursor, tooltip, elapsed, &mut hover);
}

impl View<'_, '_> {
    fn draw(
        mut self,
        style: &Style,
        cursor: Option<Vec2>,
        tooltip: Option<(String, Option<u32>)>,
        elapsed: Option<Duration>,
        hover: &mut Hover,
    ) {
        let (mut panel, mut visibility) = self.panel.into_inner();
        let (Some(cursor), Some((name, owner))) = (cursor, tooltip) else {
            visibility.set_if_neq(Visibility::Hidden);
            return;
        };
        let window_scale = self.window.scale_factor();
        let ui_scale = self.scale.into_inner().0;
        let scale = window_scale * ui_scale;
        let screen = self.window.resolution.physical_size().as_vec2();
        let cursor = cursor * window_scale;
        let width = (screen.x * style.width_fraction).floor();
        let wrap = if width < 10.0 {
            120.0
        } else {
            width.min(screen.x)
        };
        let font_size = FontSize::Px(style.font_pixels(screen.x) / scale);
        let mut size = Vec2::ZERO;
        let mut matched = true;
        for (label, mut node, mut text, mut font, mut color, mut shadow, layout) in &mut self.labels
        {
            matched &=
                text.0 == name && font.font_size == font_size && node.width == px(wrap / scale);
            text.reborrow()
                .map_unchanged(|text| &mut text.0)
                .set_if_neq(name.clone());
            font.reborrow()
                .map_unchanged(|font| &mut font.font_size)
                .set_if_neq(font_size);
            color.set_if_neq(TextColor(if label.highlight {
                style.highlight
            } else {
                style.text_color(owner)
            }));
            shadow
                .reborrow()
                .map_unchanged(|shadow| &mut shadow.offset)
                .set_if_neq(Vec2::splat(scale.recip()));
            node.reborrow()
                .map_unchanged(|node| &mut node.width)
                .set_if_neq(px(wrap / scale));
            // TextLayoutInfo uses logical UI pixels. Mouse::drawTooltip measures
            // and places the border and highlight in physical display pixels.
            size = size.max(layout.size * layout.scale_factor);
        }
        let Some(elapsed) = elapsed.filter(|_| matched && size.cmpgt(Vec2::ZERO).all()) else {
            visibility.set_if_neq(Visibility::Hidden);
            return;
        };
        let size = size.ceil();
        let origin = tooltip_origin(cursor, size, screen);
        panel.left = px(origin.x / scale);
        panel.top = px(origin.y / scale);
        let revealed = hover.reveal(style, size.x, elapsed);
        let highlight_start = (revealed - 15.0).max(0.0);
        for (clip, mut node, background) in &mut self.clips {
            clip.layout(&mut node, style, size, scale, revealed, highlight_start);
            if let Some(mut background) = background {
                background.set_if_neq(BackgroundColor(style.background_color(owner)));
            }
        }
        for (label, mut node, ..) in &mut self.labels {
            if label.highlight {
                node.left = px(-highlight_start / scale);
            }
        }
        visibility.set_if_neq(Visibility::Inherited);
    }
}

impl Style {
    fn revealed_width(&self, width: f32, elapsed: Duration) -> f32 {
        if self.fill.is_zero() {
            return width + 15.0;
        }
        (width * elapsed.as_secs_f32() / self.fill.as_secs_f32())
            .floor()
            .min(width + 15.0)
    }
}

impl Clip {
    fn layout(
        &self,
        node: &mut Node,
        style: &Style,
        size: Vec2,
        scale: f32,
        revealed: f32,
        highlight_start: f32,
    ) {
        match *self {
            Self::Background => {
                let width = if style.animate_background {
                    revealed.min(size.x)
                } else {
                    size.x
                };
                node.width = px((width + 2.0) / scale);
                node.height = px((size.y + 2.0) / scale);
                node.border = UiRect::all(px(scale.recip()));
            }
            Self::Text { highlight } => {
                let from = if highlight { highlight_start } else { 0.0 };
                node.left = px((2.0 + from) / scale);
                node.top = px(scale.recip());
                node.width = px((revealed - from) / scale);
                node.height = px(size.y / scale);
            }
        }
    }
}

fn tooltip_origin(cursor: Vec2, size: Vec2, screen: Vec2) -> Vec2 {
    let mut origin = cursor + Vec2::new(20.0, 0.0);
    if origin.x + size.x + 4.0 > screen.x {
        origin.x -= 20.0 + size.x;
    }
    if origin.y + size.y + 4.0 > screen.y {
        origin.y -= size.y;
    }
    origin
}

fn identity(game: &Skirmish, target: usize, local: usize) -> Appearance {
    let actor = &game.entities[target];
    if !game.allied(actor.owner, local)
        && game.player_active(local)
        && game.catalog[actor.type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "DISGUISER")
        && let Some(appearance) = actor.disguise_hover_appearance()
    {
        return appearance;
    }
    Appearance {
        type_index: actor.type_index,
        owner: actor.owner,
    }
}

fn hint(battle: &Match, data: &UiData, target: usize) -> Option<(String, Option<u32>)> {
    let game = &battle.simulation;
    let actor = game.entities.get(target)?;
    let observer = battle.resource_owner()?;
    if !actor.exists()
        || !matches!(
            game.visibility_cell(observer, actor.position),
            Status::Clear
        )
    {
        return None;
    }
    let appearance = identity(game, target, 0);
    let display_name = &data.units.get(appearance.type_index)?.name;
    if display_name == data.label("OBJECT:Prop") {
        return None;
    }
    let name = if display_name.is_empty() {
        data.label(&format!(
            "ThingTemplate:{}",
            game.catalog[actor.type_index].name
        ))
        .to_owned()
    } else {
        display_name.clone()
    };
    Some((name, game.players.get(appearance.owner)?.color))
}

#[derive(Default)]
pub struct Hover {
    target: Option<(u32, usize)>,
    pointer: Option<Vec2>,
    still_since: Duration,
    fill_since: Option<Duration>,
    revealed: f32,
}

impl Hover {
    fn elapsed(
        &mut self,
        now: Duration,
        target: Option<(u32, usize)>,
        pointer: Option<Vec2>,
        delay: Duration,
    ) -> Option<Duration> {
        if target != self.target || pointer != self.pointer {
            self.target = target;
            self.pointer = pointer;
            self.still_since = now;
            self.fill_since = None;
            self.revealed = 0.0;
        }
        target?;
        if now.saturating_sub(self.still_since) < delay {
            return None;
        }
        Some(now.saturating_sub(*self.fill_since.get_or_insert(now)))
    }

    fn reveal(&mut self, style: &Style, width: f32, elapsed: Duration) -> f32 {
        // Mouse::drawTooltip draws m_highlightPos before calculating the
        // position for the following rendered frame.
        std::mem::replace(&mut self.revealed, style.revealed_width(width, elapsed))
            .min(width + 15.0)
    }
}

#[cfg(test)]
mod tests;
