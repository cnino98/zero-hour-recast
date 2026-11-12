//! Miles completion callbacks: attack, reselected body/delay, requested-stop decay.
use super::random::{Random, choice};
use rodio::{
    ChannelCount, SampleRate, Source, buffer::SamplesBuffer, source::UniformSourceIterator,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(super) const FRAME_MS: f32 = 1000.0 / 30.0;

// The callback publishes a selected body to the game thread before retiring its
// sink. The event keeps its RNG, stop handle and sampled play information.
#[derive(Clone)]
pub(super) struct Template {
    pub attack: Option<SamplesBuffer>,
    pub bodies: Vec<Option<SamplesBuffer>>,
    pub decay: Option<SamplesBuffer>,
    pub previous: usize,
    pub delay: [f32; 2],
    pub pending: Arc<Mutex<Option<(usize, f32)>>>,
    pub random: Arc<Mutex<Random>>,
    pub stop: Arc<AtomicBool>,
}

pub(super) enum Decoder {
    Once(rodio::source::FromIter<std::vec::IntoIter<SamplesBuffer>>),
    Loop(Box<Loop>),
}

pub(super) struct Loop {
    source: Template,
    current: Option<UniformSourceIterator<SamplesBuffer>>,
    decaying: bool,
}

impl Template {
    pub(super) fn take_continuation(&mut self) -> Option<f32> {
        let (index, delay) = self
            .pending
            .lock()
            .expect("audio continuation lock")
            .take()?;
        self.previous = index;
        self.attack = None;
        Some(delay)
    }

    pub(super) fn can_start(&self) -> bool {
        self.attack.is_some() || self.bodies[self.previous].is_some()
    }

    pub(super) fn decoder(&self) -> Decoder {
        Decoder::Loop(Box::new(Loop {
            source: self.clone(),
            current: self.portion(
                self.attack
                    .clone()
                    .or_else(|| self.bodies[self.previous].clone()),
            ),
            decaying: false,
        }))
    }

    fn portion(
        &self,
        sample: Option<SamplesBuffer>,
    ) -> Option<UniformSourceIterator<SamplesBuffer>> {
        let format = self.format_sample();
        // Miles opens each recording with its own format. Rodio's continuous
        // loop source needs a stable output format; convert each portion lazily
        // without changing the cached original samples or completion RNG.
        sample.map(|sample| {
            UniformSourceIterator::new(sample, format.channels(), format.sample_rate())
        })
    }

    fn format_sample(&self) -> &SamplesBuffer {
        self.attack
            .as_ref()
            .or_else(|| self.bodies.iter().flatten().next())
            .expect("queued loop starts with a present recording")
    }
}

impl Iterator for Decoder {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Once(source) => source.next(),
            Self::Loop(source) => {
                if let Some(sample) = source.current.as_mut().and_then(Iterator::next) {
                    return Some(sample);
                }
                if source.decaying {
                    return None;
                }
                if source.source.stop.load(Ordering::Acquire) {
                    source.decaying = true;
                    source.current = source.source.portion(source.source.decay.clone());
                } else {
                    // notifyOfAudioCompletion changes Attack to Sound before
                    // startNextLoop. Even the first body is selected again.
                    let mut random = source.source.random.lock().expect("audio RNG lock");
                    let index = choice(
                        &mut random,
                        source.source.bodies.len(),
                        Some(source.source.previous),
                        true,
                    );
                    let delay = random.real(source.source.delay[0], source.source.delay[1]);
                    drop(random);
                    source.source.previous = index;
                    if delay > FRAME_MS {
                        *source
                            .source
                            .pending
                            .lock()
                            .expect("audio continuation lock") = Some((index, delay));
                        // A stopped source must stay exhausted while its already
                        // selected continuation waits for the game-thread update.
                        source.decaying = true;
                        source.current = None;
                        return None;
                    }
                    source.current = source.source.portion(source.source.bodies[index].clone());
                    if source.current.is_none() {
                        // A failed startNextLoop advances Sound to Decay in
                        // notifyOfAudioCompletion; it never picks a replacement.
                        source.decaying = true;
                        source.current = source.source.portion(source.source.decay.clone());
                    }
                }
                source.current.as_mut().and_then(Iterator::next)
            }
        }
    }
}

impl Source for Decoder {
    fn current_span_len(&self) -> Option<usize> {
        match self {
            Self::Once(source) => source.current_span_len(),
            Self::Loop(source) => source.current.as_ref().and_then(Source::current_span_len),
        }
    }

    fn channels(&self) -> ChannelCount {
        match self {
            Self::Once(source) => source.channels(),
            Self::Loop(source) => source.source.format_sample().channels(),
        }
    }

    fn sample_rate(&self) -> SampleRate {
        match self {
            Self::Once(source) => source.sample_rate(),
            Self::Loop(source) => source.source.format_sample().sample_rate(),
        }
    }

    fn total_duration(&self) -> Option<Duration> {
        match self {
            Self::Once(source) => source.total_duration(),
            Self::Loop(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Template {
        let sample = |value| {
            SamplesBuffer::new(
                ChannelCount::new(1).unwrap(),
                SampleRate::new(8000).unwrap(),
                vec![value; 3],
            )
        };
        Template {
            attack: Some(sample(0.1)),
            bodies: vec![Some(sample(0.2)), Some(sample(0.3)), Some(sample(0.4))],
            decay: Some(sample(0.5)),
            previous: 0,
            delay: [0.0; 2],
            pending: Arc::new(Mutex::new(None)),
            random: Arc::new(Mutex::new(Random::new(5))),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn delayed_completion_keeps_selected_body_and_stop_plays_it_before_decay() {
        let mut template = fixture();
        template.delay = [60_000.0, 100_000.0];
        let mut expected = Random::new(5);
        let selected = choice(&mut expected, 3, Some(0), true);
        let delay = expected.real(60_000.0, 100_000.0);
        let mut decoder = template.decoder();
        assert_eq!(decoder.by_ref().collect::<Vec<_>>(), [0.1; 3]);
        assert!(decoder.next().is_none());
        assert_eq!(
            template.take_continuation().unwrap().to_bits(),
            delay.to_bits()
        );
        assert_eq!(template.previous, selected);
        assert!(template.take_continuation().is_none());
        template.stop.store(true, Ordering::Release);
        let tail = template.decoder().collect::<Vec<_>>();
        assert_eq!(&tail[..3], &[[0.2; 3], [0.3; 3], [0.4; 3]][selected]);
        assert_eq!(&tail[3..], &[0.5; 3]);
        assert!(template.pending.lock().unwrap().is_none());
        assert_eq!(
            template.random.lock().unwrap().index(10_000),
            expected.index(10_000)
        );
    }

    #[test]
    fn only_delay_strictly_above_one_native_frame_requeues_the_callback() {
        for delay in [0.0, FRAME_MS, f32::from_bits(FRAME_MS.to_bits() + 1)] {
            let mut template = fixture();
            template.delay = [delay; 2];
            let mut decoder = template.decoder();
            assert_eq!(decoder.by_ref().take(3).collect::<Vec<_>>(), [0.1; 3]);
            if delay > FRAME_MS {
                assert!(decoder.next().is_none());
                assert_eq!(
                    template.take_continuation().unwrap().to_bits(),
                    delay.to_bits()
                );
            } else {
                assert!(decoder.next().is_some());
                assert!(template.take_continuation().is_none());
            }
        }
    }

    #[test]
    fn missing_delayed_body_retains_its_slot_until_the_requeued_open_fails() {
        let mut template = fixture();
        template.delay = [60_000.0, 100_000.0];
        let mut expected = Random::new(5);
        let missing = choice(&mut expected, 3, Some(0), true);
        let delay = expected.real(60_000.0, 100_000.0);
        template.bodies[missing] = None;
        assert_eq!(template.decoder().collect::<Vec<_>>(), [0.1; 3]);
        assert_eq!(
            template.take_continuation().unwrap().to_bits(),
            delay.to_bits()
        );
        assert_eq!(template.previous, missing);
        assert!(!template.can_start());
        assert_eq!(
            template.random.lock().unwrap().index(10_000),
            expected.index(10_000)
        );
    }

    #[test]
    fn mixed_recording_formats_preserve_portion_duration_order_and_stop() {
        let sample = |channels, rate, frames, value| {
            SamplesBuffer::new(
                ChannelCount::new(channels).unwrap(),
                SampleRate::new(rate).unwrap(),
                vec![value; frames * usize::from(channels)],
            )
        };
        let mut template = fixture();
        template.attack = Some(sample(1, 8000, 800, 0.125));
        template.bodies = vec![Some(sample(1, 16000, 1600, 0.25))];
        template.decay = Some(sample(2, 4000, 400, 0.5));
        template.previous = 0;
        let mut decoder = template.decoder();
        let mut counts = [0_u32; 3];
        let mut phase = 0;
        while let Some(value) = decoder.next() {
            assert_eq!(decoder.channels().get(), 1);
            assert_eq!(decoder.sample_rate().get(), 8000);
            let current = [0.125_f32, 0.25, 0.5]
                .iter()
                .position(|expected| value.to_bits() == expected.to_bits())
                .unwrap();
            assert!(current >= phase, "portions must not reorder");
            phase = current;
            counts[current] += 1;
            if current == 1 {
                template.stop.store(true, Ordering::Release);
            }
        }
        assert_eq!(counts[0], 800);
        // Rodio drains the last input frame instead of interpolating beyond it;
        // these 100ms portions remain within one output frame of duration.
        assert!(counts[1].abs_diff(800) <= 1);
        assert!(counts[2].abs_diff(800) <= 1);
        assert_eq!(
            template.random.lock().unwrap().index(10000),
            Random::new(5).index(10000)
        );
        assert_eq!(
            template.bodies[0].as_ref().unwrap().sample_rate().get(),
            16000
        );
        assert_eq!(template.decay.as_ref().unwrap().channels().get(), 2);
    }

    #[test]
    fn mixed_channel_portion_uses_pinned_rodio_mapping_without_interleaving_right_samples() {
        let mut template = fixture();
        template.attack = Some(SamplesBuffer::new(
            ChannelCount::new(1).unwrap(),
            SampleRate::new(8000).unwrap(),
            vec![0.125; 2],
        ));
        template.bodies = vec![Some(SamplesBuffer::new(
            ChannelCount::new(2).unwrap(),
            SampleRate::new(8000).unwrap(),
            vec![0.25, 0.75, 0.5, -0.5],
        ))];
        template.previous = 0;
        template.decay = None;
        let mut decoder = template.decoder();
        assert_eq!(decoder.by_ref().take(2).collect::<Vec<_>>(), [0.125; 2]);
        assert_eq!(decoder.next().unwrap().to_bits(), 0.25_f32.to_bits());
        template.stop.store(true, Ordering::Release);
        assert_eq!(decoder.collect::<Vec<_>>(), [0.5]);
        // This locks the existing Rodio stereo-to-mono channel-drop boundary;
        // it is not a claim of Miles stereo spatialization or custom downmixing.
        assert_eq!(template.bodies[0].as_ref().unwrap().channels().get(), 2);
    }

    #[test]
    fn attack_completion_reselects_body_without_replaying_attack_or_drawing_pitch() {
        let template = fixture();
        let mut decoder = template.decoder();
        let mut expected = Random::new(5);
        let mut previous = 0;
        assert_eq!(decoder.by_ref().take(3).collect::<Vec<_>>(), [0.1; 3]);
        for _ in 0..20 {
            let selected = choice(&mut expected, 3, Some(previous), true);
            assert_ne!(selected, previous);
            previous = selected;
            assert_eq!(
                decoder.by_ref().take(3).collect::<Vec<_>>(),
                [[0.2; 3], [0.3; 3], [0.4; 3]][selected]
            );
        }
        assert_eq!(
            template.random.lock().unwrap().index(10000),
            expected.index(10000)
        );
    }

    #[test]
    fn missing_decay_finishes_current_portion_without_a_silent_tail() {
        let mut template = fixture();
        template.decay = None;
        let mut decoder = template.decoder();
        assert_eq!(decoder.next(), Some(0.1));
        template.stop.store(true, Ordering::Release);
        assert_eq!(decoder.collect::<Vec<_>>(), [0.1; 2]);
    }

    #[test]
    fn missing_random_body_keeps_its_slot_and_finishes_attack_then_decay() {
        let mut template = fixture();
        let mut expected = Random::new(5);
        let missing = choice(&mut expected, 3, Some(template.previous), true);
        template.bodies[missing] = None;
        // The original attack exists even when a future selected body does not.
        let samples = template.decoder().collect::<Vec<_>>();
        assert_eq!(samples, [0.1, 0.1, 0.1, 0.5, 0.5, 0.5]);
        assert_eq!(
            template.random.lock().unwrap().index(10000),
            expected.index(10000)
        );
    }

    #[test]
    fn missing_attack_starts_the_already_selected_body_without_extra_random_draws() {
        let mut template = fixture();
        template.attack = None;
        template.decay = None;
        let mut decoder = template.decoder();
        template.stop.store(true, Ordering::Release);
        assert_eq!(decoder.by_ref().collect::<Vec<_>>(), [0.2; 3]);
        assert_eq!(
            template.random.lock().unwrap().index(10000),
            Random::new(5).index(10000)
        );
    }

    #[test]
    fn requested_stop_finishes_current_portion_then_decay_without_an_extra_body() {
        for after in [0, 1, 3, 4, 5, 6, 8] {
            let template = fixture();
            let mut decoder = template.decoder();
            for _ in 0..after {
                decoder.next().unwrap();
            }
            template.stop.store(true, Ordering::Release);
            let tail = decoder.collect::<Vec<_>>();
            let portion_left = if after == 0 { 3 } else { (3 - after % 3) % 3 };
            assert_eq!(tail.len(), portion_left + 3);
            assert_eq!(&tail[portion_left..], &[0.5; 3]);
        }
    }
}
