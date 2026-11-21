//! `InGameUI::draw` public countdown rows and per-provider ready edges.
use super::{Layer, Match, UiData};
use bevy::{ecs::system::SystemParam, prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use generals_sim::skirmish::Skirmish;
use std::collections::BTreeSet;

#[derive(Component)]
pub(super) struct Timers;

#[derive(Default)]
struct State {
    generation: u32,
    ready: BTreeSet<(usize, usize, usize)>,
    flash: u64,
    use_flash: bool,
    rows: Vec<Row>,
}

#[derive(PartialEq)]
struct Row {
    label: String,
    time: String,
    color: Color,
    size: f32,
    ready: bool,
}

#[derive(SystemParam)]
pub struct View<'w, 's> {
    commands: Commands<'w, 's>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    timers: Single<'w, 's, Entity, With<Timers>>,
    state: Local<'s, State>,
    audio: ResMut<'w, crate::source_audio::SourceAudio>,
}

pub(super) fn setup(commands: &mut Commands, data: &UiData) {
    let Some(data) = &data.power_shortcuts else {
        return;
    };
    commands.spawn((
        Timers,
        Layer::Game,
        Node {
            position_type: PositionType::Absolute,
            left: percent(data.countdown.position.x * 100.0),
            top: percent(data.countdown.position.y * 100.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        FocusPolicy::Pass,
        ZIndex(9),
    ));
}

impl View<'_, '_> {
    pub(super) fn update(mut self, battle: &mut Match, data: &UiData) {
        let Some(config) = data.power_shortcuts.as_ref().map(|data| &data.countdown) else {
            return;
        };
        if self.state.generation != battle.generation {
            self.state.generation = battle.generation;
            self.state.ready.clear();
            self.state.flash = 0;
            self.state.use_flash = false;
        }
        let parent = *self.timers;
        if !battle.started || battle.simulation.tick == 0 {
            if !self.state.rows.is_empty() {
                self.commands.entity(parent).despawn_children();
                self.state.rows.clear();
            }
            return;
        }
        let viewer = battle.resource_owner().unwrap_or(0);
        let simulation = &mut battle.simulation;
        let rows = public_providers(simulation);
        let now = simulation.tick;
        let mut statuses = BTreeSet::new();
        let mut display = Vec::new();
        let mut truncated = false;
        let sizes = config.point_sizes.map(|points| {
            crate::fonts::display_pixels(points, self.window.width(), config.resolution_adjustment)
        });
        let mut y = config.position.y * self.window.height();
        for (owner, kind, ordinal, id) in rows {
            let name = simulation.catalog[kind].special_powers[ordinal]
                .name
                .clone();
            let ready = simulation.special_power_ready(id, &name);
            let seconds = simulation.power_cooldown_remaining(id, &name) / 30;
            if ready {
                statuses.insert((owner, id, ordinal));
                if !self.state.ready.contains(&(owner, id, ordinal)) {
                    let relation = if owner == viewer {
                        0
                    } else if simulation.enemies(viewer, owner) {
                        2
                    } else {
                        1
                    };
                    let power_type = &simulation.catalog[kind].special_powers[ordinal].power_type;
                    self.audio.ready_superweapon(relation, power_type);
                }
                if config.flash_frames > 0
                    && now >= self.state.flash + u64::from(config.flash_frames)
                {
                    self.state.flash = now;
                    self.state.use_flash = !self.state.use_flash;
                }
            }
            if y >= self.window.height() * 0.82 {
                if !truncated {
                    display.push(Row {
                        label: String::new(),
                        time: "...".into(),
                        color: config.flash_color,
                        size: sizes[0],
                        ready: false,
                    });
                    truncated = true;
                }
                continue;
            }
            let color = if ready && config.flash_frames > 0 && self.state.use_flash {
                config.flash_color
            } else {
                let packed = simulation.players[owner].color.unwrap_or_default();
                Color::srgb_u8(channel(packed, 16), channel(packed, 8), channel(packed, 0))
            };
            let size = sizes[usize::from(ready)];
            let key = format!("GUI:{name}");
            let label = format!("{}: ", data.label(&key));
            let time = format!("{}:{:02}", seconds / 60, seconds % 60);
            display.push(Row {
                label,
                time,
                color,
                size,
                ready,
            });
            y += size;
        }
        self.state.ready = statuses;
        self.display(parent, display, config, &data.font_handle);
    }

    fn display(
        &mut self,
        parent: Entity,
        display: Vec<Row>,
        config: &crate::control_bar::power_shortcuts::Countdown,
        normal: &Handle<Font>,
    ) {
        if display != self.state.rows {
            self.commands.entity(parent).despawn_children();
            for row in &display {
                let font = if row.ready {
                    config.ready_handle.clone()
                } else {
                    normal.clone()
                };
                spawn_row(&mut self.commands, parent, row, font);
            }
            self.state.rows = display;
        }
    }
}

fn spawn_row(commands: &mut Commands, parent: Entity, row: &Row, font: Handle<Font>) {
    let Row {
        label,
        time,
        color,
        size,
        ..
    } = row;
    let style = TextFont {
        font: FontSource::Handle(font),
        font_size: FontSize::Px(*size),
        ..default()
    };
    let shadow = TextShadow {
        offset: Vec2::ONE,
        color: Color::BLACK,
    };
    commands.entity(parent).with_children(|children| {
        children
            .spawn((
                Node {
                    height: px(*size),
                    ..default()
                },
                FocusPolicy::Pass,
            ))
            .with_children(|row| {
                row.spawn((
                    Text::new(label.clone()),
                    TextLayout::no_wrap(),
                    style.clone(),
                    TextColor(*color),
                    shadow,
                    Node {
                        position_type: PositionType::Absolute,
                        right: percent(100.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                ));
                row.spawn((
                    Text::new(time.clone()),
                    TextLayout::no_wrap(),
                    style,
                    TextColor(*color),
                    shadow,
                    FocusPolicy::Pass,
                ));
            });
    });
}

fn public_providers(simulation: &Skirmish) -> Vec<(usize, usize, usize, usize)> {
    let mut rows = Vec::new();
    let mut shared = BTreeSet::new();
    for id in 0..simulation.entities.len() {
        let actor = &simulation.entities[id];
        if !actor.complete() || actor.masked() {
            continue;
        }
        let owner = actor.owner;
        let kind = actor.type_index;
        for (ordinal, power) in simulation.catalog[kind].special_powers.iter().enumerate() {
            if power.timer.public
                && !power.script_only
                && (power.required_science.is_empty()
                    || power.required_science.eq_ignore_ascii_case("NONE")
                    || simulation.players[owner].has_science(&power.required_science))
                && (!power.timer.shared || shared.insert((owner, power.name.as_str())))
            {
                rows.push((owner, kind, ordinal, id));
            }
        }
    }
    rows.sort_by_key(|&(owner, kind, ordinal, _)| {
        (
            owner,
            simulation.catalog[kind].special_powers[ordinal]
                .name
                .as_str(),
        )
    });
    rows
}

fn channel(color: u32, shift: u32) -> u8 {
    u8::try_from((color >> shift) & 255).expect("native packed player color")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{ecs::world::CommandQueue, text::LineBreak};

    #[test]
    fn countdown_label_and_time_remain_single_lines_on_either_side_of_anchor() {
        let mut world = World::new();
        let parent = world.spawn(Node::default()).id();
        let mut queue = CommandQueue::default();
        spawn_row(
            &mut Commands::new(&mut queue, &world),
            parent,
            &Row {
                label: "Particle Uplink Cannon: ".into(),
                time: "0:00".into(),
                color: Color::WHITE,
                size: 19.0,
                ready: true,
            },
            Handle::default(),
        );
        queue.apply(&mut world);
        let row = world.get::<Children>(parent).unwrap()[0];
        let children = world.get::<Children>(row).unwrap();
        let [label, time] = [children[0], children[1]];
        for child in [label, time] {
            assert_eq!(
                world.get::<TextLayout>(child).unwrap().linebreak,
                LineBreak::NoWrap
            );
        }
        let label_node = world.get::<Node>(label).unwrap();
        assert_eq!(label_node.position_type, PositionType::Absolute);
        assert_eq!(label_node.right, percent(100.0));
        assert_eq!(
            world.get::<Node>(time).unwrap().position_type,
            PositionType::Relative
        );
    }
}
