//! SH GameLOD.cpp712–747 and W3DDisplay.cpp1018–1048/1857: displayed FPS selects slow-death scale.
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{error::Error, path::Path, time::Instant};

struct Level {
    minimum_fps: i32,
    slow_death_scale: f32,
}

#[derive(Resource)]
pub struct DynamicLod {
    levels: [Level; 4],
    enabled: bool,
    history: [f32; 30],
    offset: usize,
    last_frame: Option<Instant>,
    occluded: bool,
}

pub fn load(assets: &VirtualAssets, root: &Path) -> Result<DynamicLod, Box<dyn Error>> {
    let source = content::scan(assets.read("data/ini/gamelod.ini")?)?;
    let mut levels = std::array::from_fn(|_| Level {
        minimum_fps: 0,
        slow_death_scale: 1.0,
    });
    for (index, name) in ["Low", "Medium", "High", "VeryHigh"]
        .into_iter()
        .enumerate()
    {
        for definition in source
            .declarations
            .iter()
            .filter(|item| item.kind == "DynamicGameLOD" && item.name.eq_ignore_ascii_case(name))
        {
            for line in definition.lines.iter().filter(|line| line.depth == Some(0)) {
                match line.tokens[0].as_str() {
                    "MinimumFPS" => {
                        levels[index].minimum_fps = crate::skirmish_data::scalar(line)?.parse()?;
                    }
                    "SlowDeathScale" => {
                        levels[index].slow_death_scale = crate::skirmish_data::finite_number(
                            crate::skirmish_data::scalar(line)?,
                        )?;
                    }
                    _ => {}
                }
            }
        }
    }
    let options = match std::fs::read_to_string(root.join("UserData/Options.ini")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let option = |name| {
        options
            .lines()
            .filter_map(|line| line.split_once('='))
            .filter(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim())
            .next_back()
    };
    // Static presets retain GameLODInfo's true default; custom reads OptionPreferences.
    let enabled = option("StaticGameLOD").is_none_or(|name| !name.eq_ignore_ascii_case("Custom"))
        || option("DynamicLOD").is_none_or(|value| value.eq_ignore_ascii_case("Yes"));
    Ok(DynamicLod {
        levels,
        enabled,
        history: [0.0; 30],
        offset: 0,
        last_frame: None,
        occluded: false,
    })
}

impl DynamicLod {
    fn scale(&self, fps: f32) -> f32 {
        let integer_fps = fps.trunc();
        let index = if self.enabled {
            (0..self.levels.len())
                .rev()
                .find(|&index| f64::from(self.levels[index].minimum_fps) < f64::from(integer_fps))
                .unwrap_or(0)
        } else {
            3
        };
        self.levels[index].slow_death_scale
    }
}

pub fn update(
    mut data: ResMut<DynamicLod>,
    mut battle: ResMut<crate::playtest::Match>,
    mut occlusion: MessageReader<bevy::window::WindowOccluded>,
) {
    for event in occlusion.read() {
        data.occluded = event.occluded;
    }
    if data.occluded {
        return;
    }
    let now = Instant::now();
    let fps = data
        .last_frame
        .map_or(0.0, |last| 1.0 / now.duration_since(last).as_secs_f32());
    data.last_frame = Some(now);
    let offset = data.offset;
    data.history[offset] = fps;
    data.offset = (offset + 1) % 30;
    let sum = data.history.iter().fold(0.0, |sum, &sample| sum + sample);
    battle.simulation.slow_death_scale = data.scale(sum / 30.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_uses_strict_integer_thresholds_and_disabled_forces_very_high() {
        let mut lod = DynamicLod {
            levels: [
                Level {
                    minimum_fps: 0,
                    slow_death_scale: 0.0,
                },
                Level {
                    minimum_fps: 10,
                    slow_death_scale: 0.3,
                },
                Level {
                    minimum_fps: 20,
                    slow_death_scale: 0.5,
                },
                Level {
                    minimum_fps: 25,
                    slow_death_scale: 1.0,
                },
            ],
            enabled: true,
            history: [0.0; 30],
            offset: 0,
            last_frame: None,
            occluded: false,
        };
        assert_eq!(lod.scale(25.99).to_bits(), 0.5_f32.to_bits());
        assert_eq!(lod.scale(26.0).to_bits(), 1.0_f32.to_bits());
        assert_eq!(lod.scale(10.99).to_bits(), 0.0_f32.to_bits());
        lod.enabled = false;
        assert_eq!(lod.scale(0.0).to_bits(), 1.0_f32.to_bits());
    }
}
