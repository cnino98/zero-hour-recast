//! Original hacker sound callbacks and `InGameUI` floating income text.
use crate::{
    control_bar::UiData,
    model_animation::Animations,
    playtest::Match,
    playtest_view::{self, WorldCamera},
    source_audio::SourceAudio,
};
use bevy::{prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{income::Effect, visibility::Status};
use std::error::Error;
#[cfg(test)]
mod color_tests;
mod deposits;
pub mod internet;
mod lotus;
pub use lotus::cash_theft;
pub mod map_crates;
mod salvage;
pub mod verification;

#[derive(Resource)]
pub struct FloatingRules {
    timeout: u32,
    rise: f32,
    fade: f32,
    font_size: f32,
}

pub fn load(assets: &VirtualAssets) -> Result<FloatingRules, Box<dyn Error>> {
    use crate::skirmish_data::{definition, duration_frames, field, number};
    let files = [content::scan(assets.read("data/ini/ingameui.ini")?)?];
    let lines = &definition(&files, "InGameUI", "")?.lines;
    let timeout =
        duration_frames(field(lines, 0, "FloatingTextTimeOut").ok_or("missing floating timeout")?)?;
    let language = [content::scan(assets.read("data/english/language.ini")?)?];
    let font = field(
        &definition(&language, "Language", "")?.lines,
        0,
        "DefaultDisplayStringFont",
    );
    Ok(FloatingRules {
        timeout: if timeout == 0 { 10 } else { timeout },
        rise: number(lines, 0, "FloatingTextMoveUpSpeed")? / 30.0,
        fade: number(lines, 0, "FloatingTextVanishRate")? / 30.0,
        font_size: font_pixels(font)?,
    })
}

fn font_pixels(font: Option<&content::ContentLine>) -> Result<f32, Box<dyn Error>> {
    let points = if let Some(font) = font {
        let end = font
            .tokens
            .len()
            .checked_sub(2)
            .ok_or("invalid display font")?;
        if end <= 1
            || font.tokens[1..end].join(" ").trim_matches('"') != "Times New Roman"
            || !font.tokens[end + 1].eq_ignore_ascii_case("no")
        {
            return Err("unrepresented original floating text font".into());
        }
        font.tokens[end].parse::<u16>()?.min(512)
    } else {
        12
    };
    if points == 0 {
        return Err("original display font has zero point size".into());
    }
    // FontCharsClass::Create_GDI_Font always uses MulDiv(points, 96, 72).
    Ok((f32::from(points) * 96.0 / 72.0).round())
}

#[derive(Component)]
pub struct Floating {
    color: [f32; 3],
    generation: u32,
    started: u64,
    last_update: u64,
    frames: u32,
    alpha: u8,
    position: Vec3,
    map_position: [f32; 2],
}

pub fn record(
    mut commands: Commands,
    mut battle: ResMut<Match>,
    mut audio: ResMut<SourceAudio>,
    ui: Res<UiData>,
    mut animations: ResMut<Animations>,
) {
    let ui = ui.into_inner();
    if !battle.started || battle.paused {
        return;
    }
    salvage::record(&mut commands, &mut battle, &mut audio, ui);
    lotus::record(&mut commands, &battle, ui);
    deposits::record(&mut commands, &mut battle, &mut audio, ui, &mut animations);
    for event in std::mem::take(&mut battle.simulation.presentation.hacking_events) {
        let actor = &battle.simulation.entities[event.object];
        let position = playtest_view::unit_position(&battle.simulation, event.object);
        audio.hacker(actor.type_index, event.object, position, &event.effect);
        let Effect::Cash {
            amount,
            position,
            container_radii,
            visible_players,
        } = event.effect
        else {
            continue;
        };
        if visible_players & 1 == 0 {
            continue;
        }
        let (position, map_position) = cash_position(
            position,
            container_radii,
            battle.generation,
            &mut animations,
        );
        let label = ui.label("GUI:AddCash");
        commands.spawn((
            Floating {
                color: [0.0, 1.0, 0.0],
                generation: battle.generation,
                started: event.frame,
                last_update: event.frame.saturating_sub(1),
                frames: 0,
                alpha: 255,
                position,
                map_position,
            },
            Text::new(label.replacen("%d", &amount.to_string(), 1)),
            TextFont {
                font: FontSource::Handle(ui.serif_handle.clone()),
                ..default()
            },
            TextColor(Color::srgb(0.0, 1.0, 0.0)),
            TextShadow {
                offset: Vec2::ONE,
                color: Color::BLACK,
            },
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            FocusPolicy::Pass,
            ZIndex(3),
        ));
    }
}

fn cash_position(
    mut source: [f32; 3],
    container_radii: Option<[f32; 2]>,
    generation: u32,
    animations: &mut Animations,
) -> (Vec3, [f32; 2]) {
    source[2] += 20.0;
    // HackInternetState draws X then Y from the shared client RNG even in fog.
    for (value, radius) in source[..2]
        .iter_mut()
        .zip(container_radii.into_iter().flatten())
    {
        let high = radius * 0.3;
        let low = -high;
        let extent = high - low;
        if low < high {
            let offset = animations.effect_random(generation) * extent;
            *value += offset + low;
        } else {
            *value += high;
        }
    }
    (
        Vec3::new(source[0], source[2], -source[1]) * 0.1,
        [source[0], source[1]],
    )
}

// Both native AutoDepositUpdate and SalvageCrateCollide read the controlling
// Player's packed color. Named map players have no lobby palette slot.
fn owner_color(game: &generals_sim::skirmish::Skirmish, owner: usize) -> [f32; 3] {
    let color = game.players[owner]
        .color
        .expect("floating income owner has native Player color");
    [16, 8, 0].map(|shift| {
        f32::from(u8::try_from((color >> shift) & 255).expect("packed color channel")) / 255.0
    })
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    rules: Res<FloatingRules>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    mut texts: Query<(
        Entity,
        &mut Floating,
        &ComputedNode,
        &mut Node,
        &mut TextFont,
        &mut TextColor,
        &mut TextShadow,
    )>,
) {
    let battle = battle.into_inner();
    let rules = rules.into_inner();
    let window = window.into_inner();
    let camera = camera.into_inner();
    let scale = window.scale_factor();
    let tick = battle.simulation.tick;
    for (entity, mut text, computed, mut node, mut font, mut color, mut shadow) in &mut texts {
        if text.generation != battle.generation || !battle.started {
            commands.entity(entity).despawn();
            continue;
        }
        if text.last_update != tick {
            text.last_update = tick;
            text.frames += 1;
            if tick > text.started + u64::from(rules.timeout) {
                let elapsed = u32::try_from(tick - text.started - u64::from(rules.timeout))
                    .unwrap_or(u32::MAX);
                text.alpha = faded_alpha(text.alpha, elapsed, rules.fade);
            }
        }
        if text.alpha == 0 {
            commands.entity(entity).despawn();
            continue;
        }
        let point = camera.0.world_to_viewport(camera.1, text.position).ok();
        let clear = matches!(
            battle
                .simulation
                .visibility_cell(battle.observer_player.unwrap_or(0), text.map_position),
            Status::Clear
        );
        node.display = if point.is_some() && clear {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(point) = point {
            let point = point * scale;
            node.left = px((point.x.trunc() - (computed.size().x * 0.5).trunc()) / scale);
            let rise = integer_real(text.frames) * rules.rise;
            node.top = px((point.y - rise).trunc() / scale);
        }
        font.font_size = FontSize::Px(rules.font_size / scale);
        let alpha = f32::from(text.alpha) / 255.0;
        color.0 = Color::srgba(text.color[0], text.color[1], text.color[2], alpha);
        shadow.color = Color::srgba(0.0, 0.0, 0.0, alpha);
        shadow.offset = Vec2::splat(1.0 / scale);
    }
}

fn integer_real(value: u32) -> f32 {
    let bytes = value.to_le_bytes();
    let high = f32::from(u16::from_le_bytes([bytes[2], bytes[3]])) * 65_536.0;
    high + f32::from(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn faded_alpha(alpha: u8, elapsed: u32, rate: f32) -> u8 {
    let amount = (integer_real(elapsed) * rate).trunc();
    if amount >= f32::from(alpha) {
        0
    } else {
        alpha
            - amount
                .to_string()
                .parse::<u8>()
                .expect("Bounded alpha decrement")
    }
}

#[cfg(test)]
mod tests {
    use super::{faded_alpha, font_pixels};
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn original_quoted_default_font_uses_fixed_gdi_point_conversion() {
        let file = content::scan(Asset {
            path: "Language.ini".into(),
            origin: "Original default display font row".into(),
            bytes: b"Language\n DefaultDisplayStringFont = \"Times New Roman\" 12 No\nEnd".to_vec(),
        })
        .unwrap();
        assert_eq!(
            font_pixels(Some(&file.declarations[0].lines[0]))
                .unwrap()
                .to_bits(),
            16.0_f32.to_bits()
        );
        assert_eq!(font_pixels(None).unwrap().to_bits(), 16.0_f32.to_bits());
    }

    #[test]
    fn original_fade_subtracts_increasing_amount_each_frame_after_timeout() {
        let mut alpha = 255;
        for frame in 1..10 {
            alpha = faded_alpha(alpha, frame, 0.1);
        }
        assert_eq!(alpha, 255);
        for frame in 10..20 {
            alpha = faded_alpha(alpha, frame, 0.1);
        }
        assert_eq!(alpha, 245);
        assert_eq!(faded_alpha(2, 30, 0.1), 0);
    }
}
