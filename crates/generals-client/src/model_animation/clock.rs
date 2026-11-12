//! `FramePacer` and WW3D's render-relative, integer-millisecond animation time.
use crate::playtest::Match;
use bevy::prelude::*;
use num_traits::ToPrimitive;

#[derive(Resource)]
pub struct Clock {
    sync_ms: u32,
    fractional_ms: f32,
    previous_sync_ms: u32,
    tick: u64,
    generation: Option<u32>,
}

impl Clock {
    pub const fn new() -> Self {
        Self {
            sync_ms: 0,
            fractional_ms: 0.0,
            previous_sync_ms: 0,
            tick: 0,
            generation: None,
        }
    }

    fn advance(&mut self, tick: u64, generation: u32, paused: bool, ratio: f32) {
        // WW3D's static clock survives map loads; only the logic tick identity changes.
        let updated = self.generation != Some(generation) || self.tick != tick;
        self.generation = Some(generation);
        self.previous_sync_ms = self.sync_ms;
        // FramePacer::getLogicTimeStepMilliseconds caps each rendered engine
        // iteration at one logic step, including stalls and uncapped game speed.
        if !paused {
            let step_ms = (1000.0 / 30.0) * ratio.min(1.0);
            self.fractional_ms += step_ms;
        }
        // WW3D::Sync retains the sub-millisecond remainder when logic steps.
        if updated {
            let integral = self.fractional_ms.to_u32().expect("bounded logic time");
            self.fractional_ms -= integral.to_f32().expect("u32 logic time");
            self.sync_ms = self.sync_ms.wrapping_add(integral);
        }
        self.tick = tick;
    }

    #[cfg(test)]
    pub(super) const fn add_ms_for_test(&mut self, milliseconds: u32) {
        self.sync_ms = self.sync_ms.wrapping_add(milliseconds);
    }

    #[cfg(test)]
    pub(super) fn sync_ms_for_test(&mut self, tick: u64) {
        self.sync_ms = FrameTime::logic_tick(tick).milliseconds;
        self.fractional_ms = 0.0;
    }

    pub const fn committed_milliseconds(&self) -> u32 {
        self.sync_ms
    }

    pub const fn frame_milliseconds(&self) -> u32 {
        self.sync_ms.wrapping_sub(self.previous_sync_ms)
    }

    pub fn milliseconds(&self) -> u32 {
        self.sync_ms.wrapping_add(
            self.fractional_ms
                .to_u32()
                .expect("bounded fractional logic time"),
        )
    }

    pub(super) fn time(&self, tick: u64) -> FrameTime {
        FrameTime {
            tick,
            milliseconds: self.milliseconds(),
        }
    }
}

pub(super) struct FrameTime {
    pub tick: u64,
    pub milliseconds: u32,
}

impl FrameTime {
    // Fixed-cadence installed verification and CPU benchmark inputs. Runtime
    // animation uses Clock, never derives render time from an integer tick.
    pub fn logic_tick(tick: u64) -> Self {
        Self {
            tick,
            milliseconds: u32::try_from((tick.wrapping_mul(1000) / 30) & u64::from(u32::MAX))
                .expect("masked native time"),
        }
    }
}

pub fn update_clock(
    battle: Res<Match>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Virtual>>,
    mut clock: ResMut<Clock>,
) {
    let battle = battle.into_inner();
    let fixed = fixed.into_inner();
    let time = time.into_inner();
    if battle.started {
        clock.advance(
            battle.simulation.tick,
            battle.generation,
            battle.paused || time.is_paused(),
            time.delta_secs() / fixed.timestep().as_secs_f32(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Clock;

    #[test]
    fn render_steps_accumulate_truncated_milliseconds_between_logic_updates() {
        let mut clock = Clock::new();
        let mut times = Vec::new();
        for (tick, ratio) in [(1, 0.25), (1, 0.25), (1, 0.25), (2, 0.25), (2, 4.0)] {
            clock.advance(tick, 0, false, ratio);
            times.push(clock.time(tick).milliseconds);
        }
        assert_eq!(times, [8, 16, 25, 33, 66]);
    }

    #[test]
    fn speed_pause_and_match_restart_preserve_the_source_clock_boundaries() {
        let mut clock = Clock::new();
        for fps in [15.0, 30.0, 60.0, 1000.0] {
            clock.advance(1, 0, false, (1.0 / 120.0) * fps);
        }
        assert_eq!(clock.time(1).milliseconds, 62);
        clock.advance(1, 0, true, 1000.0);
        assert_eq!(clock.time(1).milliseconds, 62);
        clock.advance(2, 0, false, 0.5);
        assert_eq!(clock.time(2).milliseconds, 79);
        clock.advance(0, 1, true, 1.0);
        assert_eq!(clock.time(0).milliseconds, 79);
        assert_eq!(clock.frame_milliseconds(), 0);
        clock.advance(1, 1, false, 0.5);
        assert_eq!(clock.committed_milliseconds(), 95);
        clock.advance(1, 1, false, 0.5);
        assert_eq!(clock.frame_milliseconds(), 0);
        assert_eq!(clock.committed_milliseconds(), 95);
        assert_eq!(clock.milliseconds(), 112);
    }
}
