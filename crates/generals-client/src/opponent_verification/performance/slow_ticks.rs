//! Bounded headless benchmark context, collected after the measured logic call.
use generals_sim::skirmish::diagnostics::NavigationTickContext;
use std::time::Duration;

pub(super) struct Sample {
    pub tick: u64,
    pub iteration: usize,
    pub elapsed: Duration,
    pub records: usize,
    pub live: usize,
    pub moving: usize,
    pub structure_events: usize,
    pub before: NavigationTickContext,
    pub after: NavigationTickContext,
}

pub(super) struct Samples {
    thresholds: [usize; 3],
    slowest: Vec<Sample>,
}

impl Samples {
    pub(super) fn new() -> Self {
        Self {
            thresholds: [0; 3],
            slowest: Vec::with_capacity(16),
        }
    }

    pub(super) fn record(&mut self, sample: Sample) {
        for (count, micros) in self.thresholds.iter_mut().zip([33_333, 50_000, 100_000]) {
            *count += usize::from(sample.elapsed >= Duration::from_micros(micros));
        }
        if sample.elapsed < Duration::from_micros(33_333) {
            return;
        }
        if self.slowest.len() == 16 {
            let last = self.slowest.last().expect("full slow sample list");
            if sample.elapsed < last.elapsed
                || (sample.elapsed == last.elapsed && sample.tick >= last.tick)
            {
                return;
            }
            self.slowest.pop();
        }
        self.slowest.push(sample);
        self.slowest
            .sort_by(|a, b| b.elapsed.cmp(&a.elapsed).then(a.tick.cmp(&b.tick)));
    }

    pub(super) fn report(&self) {
        println!(
            "SKIRMISH_SLOW_COUNTS scope=step_wall ge_33333us={} ge_50000us={} ge_100000us={} retained={} limit=16 context=outside_timer",
            self.thresholds[0],
            self.thresholds[1],
            self.thresholds[2],
            self.slowest.len()
        );
        for sample in &self.slowest {
            // Source processPathfindQueue returns before resetting cells when
            // full zones are due. A deadline snapshot is context, not a phase timer.
            let due_before = sample
                .before
                .zone_recalculate_at
                .is_some_and(|at| at <= sample.tick);
            println!(
                "SKIRMISH_SLOW_TICK frame={} iteration={} wall_us={} records={} mobile={} moving={} structure_events={} zone_deadline_before={:?} zone_deadline_after={:?} zone_due_before={} retained_cells_before={} retained_cells_after={} attribution=context_only",
                sample.tick,
                sample.iteration,
                sample.elapsed.as_micros(),
                sample.records,
                sample.live,
                sample.moving,
                sample.structure_events,
                sample.before.zone_recalculate_at,
                sample.after.zone_recalculate_at,
                due_before,
                sample.before.cumulative_cells,
                sample.after.cumulative_cells
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(tick: u64, micros: u64) -> Sample {
        Sample {
            tick,
            iteration: usize::try_from(tick).unwrap(),
            elapsed: Duration::from_micros(micros),
            records: 5000,
            live: 200,
            moving: 70,
            structure_events: 1,
            before: NavigationTickContext {
                zone_recalculate_at: Some(tick),
                cumulative_cells: 5001,
            },
            after: NavigationTickContext {
                zone_recalculate_at: None,
                cumulative_cells: 5001,
            },
        }
    }

    #[test]
    fn slow_samples_preserve_tick_context_and_threshold_boundaries() {
        let mut samples = Samples::new();
        for (tick, micros) in [33_332, 33_333, 49_999, 50_000, 99_999, 100_000]
            .into_iter()
            .enumerate()
        {
            samples.record(sample(u64::try_from(tick).unwrap(), micros));
        }
        assert_eq!(samples.thresholds, [5, 3, 1]);
        let longest = &samples.slowest[0];
        assert_eq!(longest.tick, 5);
        assert_eq!(longest.elapsed, Duration::from_millis(100));
        assert_eq!(longest.before.zone_recalculate_at, Some(5));
        assert_eq!(longest.after.zone_recalculate_at, None);
        // Full rebuild can retain the previous counter; it is not a search delta.
        assert_eq!(
            longest.before.cumulative_cells,
            longest.after.cumulative_cells
        );
    }

    #[test]
    fn top_sixteen_stay_bounded_with_stable_tick_ties() {
        let mut samples = Samples::new();
        for tick in (0..40).rev() {
            samples.record(sample(tick, 100_000));
            assert!(samples.slowest.len() <= 16);
        }
        assert_eq!(samples.thresholds, [40; 3]);
        assert_eq!(
            samples
                .slowest
                .iter()
                .map(|entry| entry.tick)
                .collect::<Vec<_>>(),
            (0..16).collect::<Vec<_>>()
        );
        samples.record(sample(99, 240_397));
        samples.record(sample(100, 50_000));
        assert_eq!(samples.slowest[0].tick, 99);
        assert_eq!(samples.slowest.last().unwrap().tick, 14);
        assert_eq!(samples.slowest.len(), 16);
        assert_eq!(samples.thresholds, [42, 42, 41]);
    }
}
