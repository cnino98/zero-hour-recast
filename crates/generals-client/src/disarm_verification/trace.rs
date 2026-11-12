use super::{Error, Setup, Skirmish, require, verify_removed};

pub struct Shot {
    pub frame: u64,
    pub weapon: usize,
    pub veterancy: u8,
}

pub struct Trace {
    pub preattack: Option<u64>,
    pub shot: Option<Shot>,
    pub reloaded: bool,
    frame: Option<u64>,
}

impl Trace {
    pub const fn new() -> Self {
        Self {
            preattack: None,
            shot: None,
            reloaded: false,
            frame: None,
        }
    }

    pub fn record(&mut self, game: &Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
        if self.frame == Some(game.tick) {
            return Ok(());
        }
        self.frame = Some(game.tick);
        let actor = &game.entities[setup.builder];
        if self.shot.is_none()
            && let Some((start, duration, _)) = actor.animation_duration()
        {
            require(duration == 36, "Dozer dig lost original 1200ms preattack")?;
            self.preattack = Some(start);
        }
        for shot in game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == setup.builder)
        {
            let unit = &game.catalog[actor.type_index];
            require(
                self.shot.is_none(),
                "Dozer fired more than one clearing shot",
            )?;
            require(
                unit.weapon_template(shot.weapon_index)
                    .is_some_and(|weapon| {
                        weapon.name == "DozerMineDisarmingWeapon"
                            && weapon.reload_seconds.to_bits() == 4.0_f32.to_bits()
                    })
                    && shot.target == Some(setup.trap)
                    && shot.source_position.map(f32::to_bits) == setup.position.map(f32::to_bits)
                    && self.preattack.is_some_and(|start| game.tick == start + 36),
                "Dozer did not fire its exact conditional disarm weapon at the original target pose/time",
            )?;
            self.shot = Some(Shot {
                frame: game.tick,
                weapon: shot.weapon_index,
                veterancy: shot.veterancy,
            });
        }
        require(
            !game
                .presentation
                .object_fx_events
                .iter()
                .any(|event| event.object == setup.trap),
            "disarmed trap ran an OnDie effect",
        )?;
        if let Some(shot) = &self.shot {
            verify_removed(game, setup)?;
            let ready = game.tick >= shot.frame + 120;
            require(
                actor.weapon_rounds_remaining(0) == u32::from(ready),
                "Dozer clip did not follow its original 120-frame reload",
            )?;
            self.reloaded |= ready;
        }
        Ok(())
    }
}
