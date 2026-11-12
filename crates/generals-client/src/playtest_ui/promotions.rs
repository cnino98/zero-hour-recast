use super::{Action, Layer, PromotionIcon, UiData, at, faction_index, text};
use crate::playtest::Match;
use bevy::{
    prelude::*, ui::FocusPolicy, ui_render::ui_material::MaterialNode, window::PrimaryWindow,
};

#[derive(Component)]
pub struct SerifFont;

#[derive(Component)]
pub struct ClosePanel;

#[derive(Component)]
pub struct CloseText;

#[derive(Component)]
pub enum PanelPart {
    Background,
    Progress,
}

#[derive(Component)]
pub enum PanelText {
    Rank,
    Points,
}

#[derive(Component)]
pub struct ScienceOverlay {
    faction: usize,
    index: usize,
    button: Entity,
}

pub(super) fn activate(
    action: &Action,
    data: &UiData,
    battle: &mut Match,
    transition: &mut PromotionTransition,
) -> bool {
    match *action {
        Action::Promotions => {
            if battle.started
                && !battle.paused
                && battle.simulation.player_active(0)
                && battle.simulation.end_game_at.is_none()
            {
                transition.toggle(battle);
                battle.placing = None;
            }
        }
        Action::ClosePromotions => battle.promotions_open = false,
        Action::Science { faction, index } => {
            let science = &data.promotions.buttons[faction][index].science;
            if battle.started
                && !battle.paused
                && battle.promotions_open
                && faction == faction_index(battle.simulation.players[0].faction)
                && battle.simulation.can_purchase_science(0, science)
                && let Err(error) = battle.simulation.purchase_science(0, science)
            {
                eprintln!("Science purchase rejected: {error}");
            }
        }
        _ => return false,
    }
    true
}

pub(super) fn setup(commands: &mut Commands, data: &UiData, icons: &mut Assets<PromotionIcon>) {
    commands.init_resource::<PromotionTransition>();
    let scheme = &data.schemes[0];
    commands.spawn((
        Button,
        Action::Promotions,
        super::ReleaseButton { armed: false },
        Layer::Game,
        data.image(&scheme.general[0]),
        at(scheme.general_area),
        ZIndex(5),
    ));
    setup_panel(commands, data);
    for (faction, buttons) in data.promotions.buttons.iter().enumerate() {
        for (index, button) in buttons.iter().enumerate() {
            let area = data.windows[&button.window];
            let entity = commands
                .spawn((
                    Button,
                    Action::Science { faction, index },
                    super::ReleaseButton { armed: false },
                    Layer::Promotions,
                    MaterialNode(icons.add(data.icon(&button.image))),
                    at(area),
                    ZIndex(13),
                ))
                .id();
            commands.spawn((
                ScienceOverlay {
                    faction,
                    index,
                    button: entity,
                },
                Layer::Promotions,
                data.image("Cameo_hilited"),
                at(area),
                FocusPolicy::Pass,
                ZIndex(14),
            ));
        }
    }
}

fn setup_panel(commands: &mut Commands, data: &UiData) {
    let scheme = &data.schemes[0];
    commands.spawn((
        PanelPart::Background,
        Layer::Promotions,
        data.image(&scheme.science_background),
        at(panel_area(data, 0)),
        ZIndex(12),
    ));
    commands.spawn((
        Layer::Promotions,
        data.image("ExperienceBar"),
        at(data.promotions.experience_frame),
        ZIndex(13),
    ));
    commands.spawn((
        PanelPart::Progress,
        Layer::Promotions,
        at(data.windows["GeneralsExpPoints.wnd:ProgressBarExperience"]),
        BackgroundColor(Color::srgb_u8(47, 55, 168)),
        ZIndex(14),
    ));
    commands.spawn((
        PanelText::Rank,
        Layer::Promotions,
        text("", 14.0),
        at(data.windows["GeneralsExpPoints.wnd:StaticTextTitle"]),
        ZIndex(14),
    ));
    commands.spawn((
        PanelText::Points,
        SerifFont,
        Layer::Promotions,
        text("", 14.0),
        at(data.windows["GeneralsExpPoints.wnd:StaticTextRankPointsAvailable"]),
        ZIndex(14),
    ));
    for label in &data.promotions.labels {
        commands.spawn((
            Layer::Promotions,
            text(&label.label, label.size),
            at(label.area),
            ZIndex(14),
        ));
    }
    // WND STATUS=IMAGE with NoImage slots selects ImageDrawOne: text only.
    commands
        .spawn((
            Button,
            ClosePanel,
            Action::ClosePromotions,
            Layer::Promotions,
            super::ReleaseButton { armed: false },
            Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..at(data.windows["GeneralsExpPoints.wnd:ButtonExit"])
            },
            ZIndex(14),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    CloseText,
                    text(&data.promotions.close_label, 12.0),
                    TextShadow {
                        offset: Vec2::ONE,
                        color: Color::BLACK,
                    },
                    FocusPolicy::Pass,
                ))
                .insert(TextColor(Color::WHITE));
        });
}

pub(super) fn panel_area(data: &UiData, faction: usize) -> Rect {
    // ControlBarScheme overrides the parent size with its faction artwork size.
    let origin = data.windows["GeneralsExpPoints.wnd:GenExpParent"].min;
    Rect::from_corners(
        origin,
        origin + data.image_size(&data.schemes[faction].science_background),
    )
}

pub fn update(
    battle: Res<Match>,
    data: Res<UiData>,
    mut icons: ResMut<Assets<PromotionIcon>>,
    mut buttons: Query<(
        &Action,
        &Interaction,
        &MaterialNode<PromotionIcon>,
        &mut Node,
    )>,
    interactions: Query<&Interaction, With<Action>>,
    mut overlays: Query<(&ScienceOverlay, &mut ImageNode, &mut Node), Without<Action>>,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let faction = faction_index(battle.simulation.players[0].faction);
    for (action, _, icon, mut node) in &mut buttons {
        let Action::Science {
            faction: button_faction,
            index,
        } = *action
        else {
            continue;
        };
        let science = &data.promotions.buttons[button_faction][index].science;
        let visible = button_faction == faction;
        node.reborrow()
            .map_unchanged(|node| &mut node.display)
            .set_if_neq(if visible {
                Display::Flex
            } else {
                Display::None
            });
        if let Some(current) = icons.get(&icon.0) {
            let mut next = current.clone();
            let owned = battle.simulation.players[0].has_science(science);
            let available = battle.simulation.can_purchase_science(0, science);
            // W3DPushButton: purchased uses ALWAYS_COLOR * 0xff909090;
            // unavailable prerequisites/points use Render2D's opaque DOT3 gray.
            next.state.x = if !owned && !available { 1.0 } else { 0.0 };
            next.state.y = if owned { 144.0 / 255.0 } else { 1.0 };
            super::promotion_icon::update(&mut icons, &icon.0, next);
        }
    }
    for (overlay, mut image, mut node) in &mut overlays {
        let science = &data.promotions.buttons[overlay.faction][overlay.index].science;
        let interaction = interactions
            .get(overlay.button)
            .copied()
            .unwrap_or(Interaction::None);
        let visible = overlay.faction == faction
            && battle.simulation.can_purchase_science(0, science)
            && interaction != Interaction::None;
        node.reborrow()
            .map_unchanged(|node| &mut node.display)
            .set_if_neq(if visible {
                Display::Flex
            } else {
                Display::None
            });
        super::set_image(
            &mut image,
            data.image(if interaction == Interaction::Pressed {
                "Cameo_push"
            } else {
                "Cameo_hilited"
            }),
        );
    }
}

pub fn panel(
    battle: Res<Match>,
    data: Res<UiData>,
    mut parts: Query<(&PanelPart, &mut Node, Option<&mut ImageNode>)>,
    mut fields: Query<(&PanelText, &mut Text)>,
    close: Query<&Interaction, With<ClosePanel>>,
    mut close_text: Query<(&ChildOf, &mut TextColor, &mut TextShadow), With<CloseText>>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let faction = faction_index(battle.simulation.players[0].faction);
    for (part, mut node, image) in &mut parts {
        match part {
            PanelPart::Background => {
                node.set_if_neq(at(panel_area(data, faction)));
                if let Some(mut image) = image {
                    super::set_image(
                        &mut image,
                        data.image(&data.schemes[faction].science_background),
                    );
                }
            }
            PanelPart::Progress => {
                let mut area = data.windows["GeneralsExpPoints.wnd:ProgressBarExperience"];
                area.max.x = area.min.x
                    + area.width() * (battle.simulation.rank_progress(0) * 100.0).trunc() / 100.0;
                node.set_if_neq(at(area));
            }
        }
    }
    for (field, mut text) in &mut fields {
        let value = match field {
            PanelText::Rank => data
                .label(&format!(
                    "SCIENCE:Rank{}",
                    battle.simulation.players[0].rank
                ))
                .into(),
            PanelText::Points => battle.simulation.players[0].science_points.to_string(),
        };
        text.reborrow()
            .map_unchanged(|text| &mut text.0)
            .set_if_neq(value);
    }
    let window = window.into_inner();
    for (parent, mut color, mut shadow) in &mut close_text {
        let highlighted = close
            .get(parent.parent())
            .is_ok_and(|state| *state != Interaction::None);
        color.set_if_neq(TextColor(if highlighted {
            Color::srgb_u8(186, 255, 12)
        } else {
            Color::WHITE
        }));
        let mut next = *shadow;
        next.color = if highlighted {
            Color::srgb_u8(0, 2, 0)
        } else {
            Color::BLACK
        };
        // W3DDisplayString::draw defaults to a one-pixel drop shadow.
        next.offset = Vec2::splat(1.0 / window.scale_factor());
        shadow.set_if_neq(next);
    }
}

// FadeTransition hides every child until frame10 and draws only the enabled
// parent image at alpha25..225; ControlBar::hidePurchaseScience closes immediately.
#[derive(Resource, Default)]
pub struct PromotionTransition {
    open: bool,
    frame: f32,
}

impl PromotionTransition {
    pub const fn visible(&self) -> bool {
        self.open && self.frame >= 10.0
    }

    pub(super) const fn toggle(&mut self, battle: &mut Match) {
        // ControlBar::togglePurchaseScience tests winIsHidden. During WINFADE
        // the hidden parent is shown again, restarting the same transition group.
        battle.promotions_open = !(battle.promotions_open && self.visible());
        if battle.promotions_open {
            self.open = false;
        }
    }
}

pub fn transition(
    time: Res<Time<Real>>,
    battle: Res<Match>,
    data: Res<UiData>,
    mut state: ResMut<PromotionTransition>,
    mut nodes: Query<(
        &Layer,
        Option<&PanelPart>,
        &mut Visibility,
        Option<&mut ImageNode>,
    )>,
) {
    let time = time.into_inner();
    let battle = battle.into_inner();
    let data = data.into_inner();
    let open = battle.started
        && !battle.paused
        && battle.promotions_open
        && battle.simulation.end_game_at.is_none();
    if !open {
        state.open = false;
        return;
    }
    if state.open {
        // TransitionGroup::update and FramePacer's 5fps floor, independent of logic time.
        let step = 30.0 * time.delta_secs().min(0.2) * data.promotions.transition_speed;
        state.frame = (state.frame + step).min(10.0);
    } else {
        state.open = true;
        state.frame = 0.0;
    }
    let frame = state.frame.trunc();
    for (layer, part, mut visibility, image) in &mut nodes {
        if !matches!(layer, Layer::Promotions) {
            continue;
        }
        let background = matches!(part, Some(PanelPart::Background));
        visibility.set_if_neq(if frame >= 10.0 || background && frame >= 1.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if background && let Some(mut image) = image {
            image
                .reborrow()
                .map_unchanged(|image| &mut image.color)
                .set_if_neq(Color::srgba(
                    1.0,
                    1.0,
                    1.0,
                    if frame < 10.0 {
                        frame * 25.0 / 255.0
                    } else {
                        1.0
                    },
                ));
        }
    }
}
