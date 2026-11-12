//! Installed static-defense draw selection and original barrel motion.
use super::{Animations, Player, clock::FrameTime, playback};
use generals_assets::w3d::Model;
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn verify_defense_power_playback(
        &mut self,
        game: &generals_sim::skirmish::Skirmish,
    ) -> Result<(), Box<dyn Error>> {
        let unit = game
            .entities
            .iter()
            .find(|unit| {
                unit.alive() && game.catalog[unit.type_index].name == "ChinaGattlingCannon"
            })
            .ok_or("installed Gattling playback actor missing")?;
        let mut player = Player::new(unit.position, game.tick);
        let mut flags = BTreeSet::from(["ATTACKING".to_owned(), "CONTINUOUS_FIRE_FAST".to_owned()]);
        self.advance(
            &mut player,
            unit,
            0,
            &flags,
            &FrameTime::logic_tick(game.tick),
            false,
        );
        self.advance(
            &mut player,
            unit,
            0,
            &flags,
            &FrameTime::logic_tick(game.tick + 1),
            false,
        );
        let frame = player.frame;
        if frame == 0.0 {
            return Err("powered Gattling animation did not advance".into());
        }
        flags.insert("DISABLED".into());
        self.advance(
            &mut player,
            unit,
            0,
            &flags,
            &FrameTime::logic_tick(game.tick + 15),
            false,
        );
        if player.frame.to_bits() != frame.to_bits() {
            return Err("underpowered Gattling animation continued spinning".into());
        }
        flags.remove("DISABLED");
        self.advance(
            &mut player,
            unit,
            0,
            &flags,
            &FrameTime::logic_tick(game.tick + 16),
            false,
        );
        let elapsed = FrameTime::logic_tick(game.tick + 16).milliseconds
            - FrameTime::logic_tick(game.tick + 15).milliseconds;
        let state = &self.draws[unit.type_index][0].states[player.state];
        let rate = self.clips[&state.motions[player.choice].name].frame_rate;
        let elapsed_frame =
            f32::from(rate) * player.speed * f32::from(u16::try_from(elapsed)?) * 0.001;
        let resumed = frame + elapsed_frame;
        if player.frame.to_bits() != resumed.to_bits() {
            return Err("restored Gattling animation did not resume at its one-frame rate".into());
        }
        println!(
            "Installed Gattling animation pauses at brownout and resumes without elapsed disabled time"
        );
        Ok(())
    }

    pub fn verify_defense_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let cannon = types
            .iter()
            .position(|unit| unit.name == "ChinaGattlingCannon")
            .ok_or("Gattling Cannon missing")?;
        let draw = &self.draws[cannon][0];
        if !draw.requires_power {
            return Err("Gattling Cannon lost source animation power requirement".into());
        }
        let mut checked = 0;
        for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
            for (condition, speed) in [
                (None, 0.5_f32),
                (Some("CONTINUOUS_FIRE_SLOW"), 0.8),
                (Some("CONTINUOUS_FIRE_MEAN"), 1.5),
                (Some("CONTINUOUS_FIRE_FAST"), 3.0),
            ] {
                let mut flags = BTreeSet::from(["ATTACKING".to_owned()]);
                flags.extend(damage.into_iter().chain(condition).map(str::to_owned));
                let state_index = draw.select(&flags);
                let state = &draw.states[state_index];
                if state.mode != "LOOP" || state.speed.map(f32::to_bits) != [speed.to_bits(); 2] {
                    return Err(
                        "Gattling Cannon selected incorrect source barrel spin state".into(),
                    );
                }
                let clip = state
                    .motions
                    .first()
                    .and_then(|motion| self.clips.get(&motion.name))
                    .ok_or("Gattling barrel clip missing")?;
                let model = state
                    .model
                    .and_then(|index| models.get(index))
                    .ok_or("Gattling barrel model missing")?;
                let mut player = Player::new([0.0; 2], 0);
                let mut random = playback::Random::new(1);
                playback::select(&mut player, draw, &self.clips, state_index, &mut random);
                playback::update(
                    &mut player,
                    state,
                    &self.clips,
                    [0.0; 2],
                    &FrameTime::logic_tick(0),
                );
                playback::update(
                    &mut player,
                    state,
                    &self.clips,
                    [0.0; 2],
                    &FrameTime::logic_tick(5),
                );
                let rest = model.pose(clip, 0.0)?;
                let moved = model.pose(clip, player.frame)?;
                if !rest
                    .iter()
                    .zip(moved)
                    .flat_map(|(a, b)| a.positions.iter().zip(b.positions))
                    .any(|(a, b)| a.iter().zip(b).any(|(a, b)| (a - b).abs() > 0.001))
                {
                    return Err(
                        "Gattling barrel animation left original vertices stationary".into(),
                    );
                }
                checked += 1;
            }
        }
        println!(
            "Installed Gattling Cannon: {checked} original damaged/spin-rate clip poses change barrel vertices"
        );
        Ok(())
    }
}
