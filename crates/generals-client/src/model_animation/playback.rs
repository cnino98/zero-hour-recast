//! EA `adjustAnimation`, `setModelState`, `Compute_Current_Frame`, client RNG.
use super::{Player, State, clock::FrameTime, data::Draw};
use generals_assets::w3d::animation::Clip;
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

pub(super) struct Random([u32; 6]);
impl Random {
    pub fn new(seed: u32) -> Self {
        Self(
            [
                0xf22d_0e56_u32,
                0x8831_26e9,
                0xc624_dd2f,
                0x0702_c49c,
                0x9e35_3f7d,
                0x6fdf_3b64,
            ]
            .map(|word| word.wrapping_add(seed)),
        )
    }
    fn value(&mut self) -> u32 {
        let seed = &mut self.0;
        let mut value = seed[5].wrapping_add(seed[4]);
        let mut carry = u32::from(value < seed[5] || value < seed[4]);
        seed[4] = value;
        for index in (0..4).rev() {
            value = value.wrapping_add(seed[index]).wrapping_add(carry);
            carry = u32::from(value < seed[index]);
            seed[index] = value;
        }
        for index in (0..6).rev() {
            seed[index] = seed[index].wrapping_add(1);
            if seed[index] != 0 {
                break;
            }
            if index == 0 {
                value = value.wrapping_add(1);
            }
        }
        value
    }
    pub(super) fn index(&mut self, length: usize) -> usize {
        if length <= 1 {
            return 0;
        }
        usize::try_from(self.value() % u32::try_from(length).expect("bounded animation choices"))
            .expect("u32 index")
    }
    pub(super) fn real(&mut self, low: f32, high: f32) -> f32 {
        if low >= high {
            return high;
        }
        let bytes = self.value().to_le_bytes();
        let upper = f32::from(u16::from_le_bytes([bytes[2], bytes[3]])) * 65_536.0;
        let value = upper + f32::from(u16::from_le_bytes([bytes[0], bytes[1]]));
        let scaled = value * (1.0 / 4_294_967_296.0) * (high - low);
        scaled + low
    }
}

pub(super) fn manual_frame(
    player: &mut Player,
    draw: &Draw,
    clips: &BTreeMap<String, Clip>,
    command: Option<(u64, i32)>,
) {
    if command == player.manual_frame {
        return;
    }
    player.manual_frame = command;
    let Some((_, frame)) = command else {
        return;
    };
    let Some(state) = draw.states.get(player.state) else {
        return;
    };
    let Some(motion) = state.motions.get(player.choice) else {
        return;
    };
    // Unlike adjustAnimation, the explicit setter passes the null handle to
    // Set_Animation, which releases any retained clip and restores the base pose.
    player.retained_animation = None;
    if clips.contains_key(&motion.name) {
        player.frame = frame
            .to_f32()
            .expect("native animation Int converts to Real");
        player.manual_animation = true;
    } else {
        player.frame = 0.0;
        player.manual_animation = false;
        player.restart_clock = false;
    }
}

pub(super) fn select(
    player: &mut Player,
    draw: &Draw,
    clips: &BTreeMap<String, Clip>,
    desired: usize,
    random: &mut Random,
) -> bool {
    let completed = player.state != usize::MAX && complete(player, draw, clips);
    if completed
        && player.pending == Some(desired)
        && let Some(next) = player.pending.take()
    {
        let duration = player.pending_duration.take();
        switch(player, draw, clips, next, random);
        if let Some(frames) = duration {
            apply_duration(player, draw, clips, frames);
        }
        return true;
    }
    if (player.state == desired && player.pending.is_none()) || player.pending == Some(desired) {
        if completed && draw.states[player.state].model.is_some() {
            let state = &draw.states[player.state];
            if state
                .motions
                .get(player.choice)
                .is_some_and(|motion| motion.idle)
                || state.flags.contains("RESTART_ANIM_WHEN_COMPLETE")
            {
                let animation = player
                    .retained_animation
                    .unwrap_or((player.state, player.choice));
                let bound = player
                    .animation(draw)
                    .is_some_and(|(_, motion)| clips.contains_key(&motion.name));
                if choose(player, state, clips, true, random).is_none() && bound {
                    player.retained_animation = Some(animation);
                }
            }
        }
        return false;
    }
    if let Some(previous) = draw.states.get(player.state) {
        let next = &draw.states[desired];
        if !next.finish_key.is_empty() && next.finish_key == previous.key && !completed {
            player.pending = Some(desired);
            return false;
        }
        if !previous.key.is_empty()
            && !next.key.is_empty()
            && let Some(transition) = draw.states.iter().position(|state| {
                state
                    .transition
                    .as_ref()
                    .is_some_and(|(from, to)| *from == previous.key && *to == next.key)
            })
        {
            switch(player, draw, clips, transition, random);
            player.pending = Some(desired);
            return true;
        }
    }
    player.pending = None;
    switch(player, draw, clips, desired, random);
    true
}

fn switch(
    player: &mut Player,
    draw: &Draw,
    clips: &BTreeMap<String, Clip>,
    next: usize,
    random: &mut Random,
) {
    let previous = draw.states.get(player.state);
    let animation = player
        .retained_animation
        .unwrap_or((player.state, player.choice));
    let fraction = player
        .animation(draw)
        .and_then(|(_, motion)| clips.get(&motion.name))
        .map(|clip| player.frame / f32::from(clip.frame_count - 1));
    let state = &draw.states[next];
    if previous.is_none_or(|previous| previous.render_model != state.render_model) {
        // A newly created RenderObj is initially in the base pose.
        player.retained_animation = None;
        player.frame = 0.0;
        player.direction = 1.0;
        player.speed = 1.0;
        player.manual_animation = false;
        player.restart_clock = false;
    }
    let clip = choose(player, state, clips, false, random);
    if let Some(clip) = clip {
        if !state.flags.contains("RANDOMSTART")
            && !state.flags.contains("START_FRAME_FIRST")
            && !state.flags.contains("START_FRAME_LAST")
            && previous.is_some_and(|previous| {
                previous.hlod
                    && previous
                        .flags
                        .intersection(&state.flags)
                        .any(|flag| flag.starts_with("MAINTAIN_FRAME_ACROSS_STATES"))
            })
            && let Some(fraction) = fraction
        {
            let scaled = fraction * f32::from(clip.frame_count);
            player.frame = (scaled - 1.0).trunc().max(0.0);
        }
    } else if state.render_model.is_some()
        && previous.is_some_and(|previous| previous.render_model == state.render_model)
        && fraction.is_some()
    {
        // A null HAnim or a state without animations never calls Set_Animation.
        player.retained_animation = Some(animation);
    }
    player.state = next;
    player.pending_duration = None;
}

// W3DModelDraw::setAnimationCompletionTime splits transition and destination
// durations; setAnimationLoopDuration rounds logic-frame milliseconds upward.
pub(super) fn set_duration(
    player: &mut Player,
    draw: &Draw,
    clips: &BTreeMap<String, Clip>,
    frames: u32,
    completion: bool,
) {
    let bytes = frames.to_le_bytes();
    let high = f32::from(u16::from_le_bytes([bytes[2], bytes[3]])) * 65_536.0;
    let frames = high + f32::from(u16::from_le_bytes([bytes[0], bytes[1]]));
    let state = &draw.states[player.state];
    player.pending_duration = None;
    if completion
        && state.transition.is_some()
        && let Some(next) = player.pending.and_then(|index| draw.states.get(index))
        && next.transition.is_none()
        && let (Some(first), Some(second)) = (
            natural_duration(state, clips),
            natural_duration(next, clips),
        )
    {
        let transition = (frames * first / (first + second)).floor();
        apply_duration(player, draw, clips, transition);
        player.pending_duration = Some(frames - transition);
        return;
    }
    apply_duration(player, draw, clips, frames);
}

fn natural_duration(state: &State, clips: &BTreeMap<String, Clip>) -> Option<f32> {
    let clip = clips.get(&state.motions.first()?.name)?;
    Some(f32::from(clip.frame_count) * 1000.0 / f32::from(clip.frame_rate))
}

fn apply_duration(player: &mut Player, draw: &Draw, clips: &BTreeMap<String, Clip>, frames: f32) {
    let Some(clip) = player
        .animation(draw)
        .filter(|(state, _)| state.hlod)
        .and_then(|(_, motion)| clips.get(&motion.name))
    else {
        return;
    };
    let desired = (frames * (1000.0 / 30.0)).ceil();
    let natural = f32::from(clip.frame_count) * 1000.0 / f32::from(clip.frame_rate);
    if desired > 0.0 && natural > 0.0 {
        player.speed = natural / desired;
    }
}

fn choose<'a>(
    player: &mut Player,
    state: &State,
    clips: &'a BTreeMap<String, Clip>,
    avoid: bool,
    random: &mut Random,
) -> Option<&'a Clip> {
    let old = player.choice;
    player.choice = random.index(state.motions.len());
    while avoid && state.motions.len() > 1 && player.choice == old {
        player.choice = random.index(state.motions.len());
    }
    // adjustAnimation chooses first, but a null render object/animation handle
    // does not call Set_Animation or alter its clock, mode, frame or multiplier.
    let clip = state
        .model
        .and_then(|_| state.motions.get(player.choice))
        .and_then(|motion| clips.get(&motion.name))?;
    player.retained_animation = None;
    player.restart_clock = true;
    player.manual_animation = false;
    player.frame = 0.0;
    player.direction = if state.mode.ends_with("BACKWARDS") {
        -1.0
    } else {
        1.0
    };
    let last = f32::from(clip.frame_count - 1);
    if player.direction < 0.0 {
        player.frame = last;
    }
    if state.flags.contains("RANDOMSTART") {
        player.frame = f32::from(
            u16::try_from(random.index(usize::from(clip.frame_count))).expect("bounded clip frame"),
        );
    } else if state.flags.contains("START_FRAME_FIRST") {
        player.frame = 0.0;
    } else if state.flags.contains("START_FRAME_LAST") {
        player.frame = last;
    }
    player.speed = if state.hlod {
        random.real(state.speed[0], state.speed[1])
    } else {
        1.0
    };
    Some(clip)
}

pub(super) fn complete(player: &Player, draw: &Draw, clips: &BTreeMap<String, Clip>) -> bool {
    // An absent/non-HLOD render object is complete; an HLOD in base pose isn't.
    if !draw.states[player.state].hlod {
        return true;
    }
    let Some((state, clip)) = player
        .animation(draw)
        .and_then(|(state, motion)| clips.get(&motion.name).map(|clip| (state, clip)))
    else {
        return false;
    };
    match (state.mode.as_str(), player.manual_animation) {
        ("ONCE", false) => player.frame >= f32::from(clip.frame_count - 1),
        ("ONCE_BACKWARDS", false) => player.frame <= 0.0,
        _ => false,
    }
}

pub(super) fn update(
    player: &mut Player,
    draw: &Draw,
    clips: &BTreeMap<String, Clip>,
    position: [f32; 2],
    time: &FrameTime,
) {
    if std::mem::take(&mut player.restart_clock) {
        // Set_Animation starts at the current WW3D time, including its fraction.
        player.last_sync_ms = time.milliseconds;
        player.tick = time.tick;
        player.position = position;
    }
    let elapsed_ticks = time.tick.saturating_sub(player.tick);
    if elapsed_ticks != 0 {
        adjust_movement_speed(player, draw, clips, position, elapsed_ticks);
        player.tick = time.tick;
        player.position = position;
    }
    let milliseconds = time.milliseconds.wrapping_sub(player.last_sync_ms);
    if milliseconds != 0 {
        if let Some((state, clip)) = player
            .animation(draw)
            .filter(|(state, _)| state.mode != "MANUAL")
            .and_then(|(state, motion)| clips.get(&motion.name).map(|clip| (state, clip)))
        {
            let elapsed = milliseconds.to_f32().expect("native u32 elapsed time");
            let animation_ms =
                f32::from(clip.frame_rate) * player.speed * player.direction * elapsed;
            let delta = animation_ms * 0.001;
            player.frame += delta;
            wrap(player, &state.mode, f32::from(clip.frame_count - 1));
        }
        player.last_sync_ms = time.milliseconds;
    }
}

fn adjust_movement_speed(
    player: &mut Player,
    draw: &Draw,
    clips: &BTreeMap<String, Clip>,
    position: [f32; 2],
    elapsed_ticks: u64,
) {
    let Some(motion) = draw.states[player.state].motions.get(player.choice) else {
        return;
    };
    let Some(clip) = player
        .animation(draw)
        .and_then(|(_, active)| clips.get(&active.name))
    else {
        return;
    };
    let [dx, dy] = [
        position[0] - player.position[0],
        position[1] - player.position[1],
    ];
    let squares = [dx * dx, dy * dy];
    // W3DModelDraw reads physics velocity in distance per logic frame. The
    // represented displacement estimate must stay independent of render cadence.
    let speed = (squares[0] + squares[1]).sqrt()
        / elapsed_ticks
            .to_f32()
            .expect("represented logic tick interval");
    if motion.distance > 0.0 && speed > 0.0 {
        let natural_ms = f32::from(clip.frame_count) * 1000.0 / f32::from(clip.frame_rate);
        let desired_ms = motion.distance / speed * (1000.0 / 30.0);
        player.speed = natural_ms / desired_ms;
    }
}

fn wrap(player: &mut Player, mode: &str, last: f32) {
    match mode {
        "ONCE" => player.frame = player.frame.min(last),
        "ONCE_BACKWARDS" => player.frame = player.frame.max(0.0),
        "LOOP" if player.frame >= last => {
            player.frame -= last;
            if player.frame >= last {
                player.frame = 0.0;
            }
        }
        "LOOP_BACKWARDS" if player.frame < 0.0 => {
            player.frame += last;
            if player.frame < 0.0 {
                player.frame = last;
            }
        }
        "LOOP_PINGPONG" if player.direction >= 1.0 && player.frame >= last => {
            let twice_last = last * 2.0;
            player.frame = twice_last - player.frame;
            if player.frame >= last - 1.0 {
                player.frame = last;
            }
            player.direction = -player.direction;
        }
        "LOOP_PINGPONG" if player.direction < 1.0 && player.frame < 0.0 => {
            player.frame = -player.frame;
            if player.frame >= last {
                player.frame = 0.0;
            }
            player.direction = -player.direction;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{Player, wrap};
    use generals_assets::w3d::animation::Clip;

    #[test]
    fn client_integer_choice_uses_native_modulo_between_real_draws() {
        for seed in [0, 1, 41, u32::MAX] {
            let mut expected = super::Random::new(seed);
            let first = expected.real(0.0, 1.0);
            let choice = expected.value() % 4;
            let last = expected.real(0.0, 1.0);
            let mut actual = super::Random::new(seed);
            assert_eq!(actual.real(0.0, 1.0).to_bits(), first.to_bits());
            assert_eq!(actual.index(4), usize::try_from(choice).unwrap());
            assert_eq!(actual.real(0.0, 1.0).to_bits(), last.to_bits());
            assert_eq!(actual.value(), expected.value());
        }
    }

    pub(super) fn test_clip() -> Clip {
        let mut header = [0; 44];
        header[..4].copy_from_slice(&0x0004_0001_u32.to_le_bytes());
        header[4..8].copy_from_slice(b"IDLE");
        header[20..23].copy_from_slice(b"RIG");
        header[36..40].copy_from_slice(&31_u32.to_le_bytes());
        header[40..].copy_from_slice(&30_u32.to_le_bytes());
        let mut bytes = 0x200_u32.to_le_bytes().to_vec();
        bytes.extend(0x8000_0034_u32.to_le_bytes());
        bytes.extend(0x201_u32.to_le_bytes());
        bytes.extend(44_u32.to_le_bytes());
        bytes.extend(header);
        Clip::parse(&bytes).unwrap()
    }

    pub(super) fn one_draw(state: super::State) -> super::Draw {
        super::Draw {
            states: vec![state],
            ignored: std::collections::BTreeSet::new(),
            recoil: crate::model_animation::muzzle::Rules::new(),
            receives_dynamic_lights: true,
            requires_power: false,
            attach_bone: None,
            attach_offset: [0.0; 3],
            upgrade_show: Vec::new(),
            supply: std::collections::BTreeMap::new(),
            treads: None,
            truck_wheels: None,
            track: None,
        }
    }

    #[test]
    fn looping_uses_last_frame_period_and_native_large_step_reset() {
        let mut player = Player::new([0.0; 2], 0);
        player.frame = 29.5;
        wrap(&mut player, "LOOP", 29.0);
        assert!((player.frame - 0.5).abs() < 0.000_01);
        player.frame = 60.0;
        wrap(&mut player, "LOOP", 29.0);
        assert!(player.frame.abs() < 0.000_01);
        player.frame = -0.5;
        wrap(&mut player, "LOOP_BACKWARDS", 29.0);
        assert!((player.frame - 28.5).abs() < 0.000_01);
    }

    #[test]
    fn manual_frame_targets_current_clip_and_releases_when_model_state_switches() {
        use super::{Random, State, manual_frame, select};
        use crate::model_animation::data::{Draw, Motion};
        use std::collections::{BTreeMap, BTreeSet};
        let clip = test_clip();
        let clips = BTreeMap::from([(clip.name.clone(), clip)]);
        let mut state = State::empty();
        state.model = Some(0);
        state.hlod = true;
        state.motions.push(Motion {
            name: "RIG.IDLE".into(),
            distance: 0.0,
            idle: false,
        });
        let draw = Draw {
            truck_wheels: None,
            treads: None,
            track: None,
            supply: BTreeMap::new(),
            upgrade_show: Vec::new(),
            attach_bone: None,
            attach_offset: [0.0; 3],
            receives_dynamic_lights: true,
            requires_power: false,
            states: vec![state.clone(), state],
            ignored: BTreeSet::new(),
            recoil: crate::model_animation::muzzle::Rules::new(),
        };
        let mut player = Player::new([0.0; 2], 0);
        let mut random = Random::new(0);
        select(&mut player, &draw, &clips, 0, &mut random);
        manual_frame(&mut player, &draw, &clips, Some((8, 7)));
        assert!(player.manual_animation);
        assert_eq!(player.frame.to_bits(), 7.0_f32.to_bits());
        select(&mut player, &draw, &clips, 1, &mut random);
        assert!(!player.manual_animation);
        assert_eq!(player.frame.to_bits(), 0.0_f32.to_bits());
        manual_frame(&mut player, &draw, &clips, Some((8, 7)));
        assert!(
            !player.manual_animation,
            "an already delivered setter is not replayed onto a new clip"
        );
        manual_frame(&mut player, &draw, &clips, Some((9, 8)));
        assert!(player.manual_animation);
        assert_eq!(player.frame.to_bits(), 8.0_f32.to_bits());
    }

    #[test]
    fn state_switch_starts_at_current_time_without_consuming_previous_tick() {
        use super::{Random, State, select, update};
        use crate::model_animation::data::{Draw, Motion};
        use std::collections::{BTreeMap, BTreeSet};
        let clip = test_clip();
        let clips = BTreeMap::from([(clip.name.clone(), clip)]);
        let mut state = State::empty();
        state.model = Some(0);
        state.hlod = true;
        state.motions.push(Motion {
            name: "RIG.IDLE".into(),
            distance: 0.0,
            idle: false,
        });
        let draw = Draw {
            truck_wheels: None,
            treads: None,
            track: None,
            supply: std::collections::BTreeMap::new(),
            upgrade_show: Vec::new(),
            attach_bone: None,
            attach_offset: [0.0; 3],
            receives_dynamic_lights: true,
            requires_power: true,
            states: vec![state.clone(), state],
            ignored: BTreeSet::new(),
            recoil: crate::model_animation::muzzle::Rules::new(),
        };
        let mut player = Player::new([0.0; 2], 0);
        let mut random = Random::new(0);
        select(&mut player, &draw, &clips, 0, &mut random);
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(10),
        );
        assert!(player.frame.abs() < 0.000_01);
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(11),
        );
        assert!((player.frame - 0.99).abs() < 0.000_01);
        select(&mut player, &draw, &clips, 1, &mut random);
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(12),
        );
        assert!(player.frame.abs() < 0.000_01);
    }

    #[test]
    fn acquiring_cargo_waits_for_the_backward_pickup_animation_to_finish() {
        use super::{Random, State, select, update};
        use crate::model_animation::data::{Draw, Motion};
        use std::collections::{BTreeMap, BTreeSet};
        let clip = test_clip();
        let clips = BTreeMap::from([(clip.name.clone(), clip)]);
        let mut pickup = State::empty();
        pickup.model = Some(0);
        pickup.hlod = true;
        pickup.motions.push(Motion {
            name: "RIG.IDLE".into(),
            distance: 0.0,
            idle: false,
        });
        pickup.mode = "ONCE_BACKWARDS".into();
        pickup.flags.insert("START_FRAME_LAST".into());
        pickup.speed = [0.75; 2];
        pickup.key = "trans_pickingup".into();
        let mut carrying = pickup.clone();
        carrying.mode = "LOOP".into();
        carrying.flags.clear();
        carrying.key = "trans_full".into();
        carrying.finish_key = "trans_pickingup".into();
        let draw = Draw {
            truck_wheels: None,
            treads: None,
            track: None,
            supply: std::collections::BTreeMap::new(),
            upgrade_show: Vec::new(),
            attach_bone: None,
            attach_offset: [0.0; 3],
            receives_dynamic_lights: true,
            requires_power: true,
            states: vec![pickup, carrying],
            ignored: BTreeSet::new(),
            recoil: crate::model_animation::muzzle::Rules::new(),
        };
        let mut player = Player::new([0.0; 2], 0);
        let mut random = Random::new(0);
        assert!(select(&mut player, &draw, &clips, 0, &mut random));
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(0),
        );
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(15),
        );
        assert!(!select(&mut player, &draw, &clips, 1, &mut random));
        assert_eq!(player.state, 0);
        assert_eq!(player.pending, Some(1));
        assert!((player.frame - 18.75).abs() < 0.000_01);
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(40),
        );
        // 1333 integer milliseconds are just short of the 0.75-speed clip.
        assert!(!select(&mut player, &draw, &clips, 1, &mut random));
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &super::FrameTime::logic_tick(41),
        );
        assert!(select(&mut player, &draw, &clips, 1, &mut random));
        assert_eq!(player.state, 1);
        assert!(player.pending.is_none());
    }
}

#[cfg(test)]
mod null_animation_tests {
    use super::*;
    use crate::model_animation::data::Motion;

    #[test]
    fn absent_handle_keeps_playback_and_consumes_only_native_choice_rng() {
        let mut state = State::empty();
        state.model = Some(0);
        state.hlod = true;
        state.speed = [0.5, 2.0];
        state.flags.insert("RANDOMSTART".into());
        state.mode = "ONCE_BACKWARDS".into();
        state.motions = ["STATIC.MISSING_A", "STATIC.MISSING_B"]
            .map(|name| Motion {
                name: name.into(),
                distance: 0.0,
                idle: true,
            })
            .into();
        let clips = BTreeMap::new();
        let mut player = Player::new([0.0; 2], 0);
        player.frame = 12.0;
        player.speed = 1.25;
        player.direction = 1.0;
        player.manual_animation = true;
        player.restart_clock = false;
        player.last_sync_ms = 77;
        let mut random = Random::new(19);
        let mut expected = Random::new(19);
        let choice = expected.index(2);
        assert!(choose(&mut player, &state, &clips, false, &mut random).is_none());
        assert_eq!(player.choice, choice);
        assert_eq!(random.0, expected.0);
        assert_eq!(player.frame.to_bits(), 12.0_f32.to_bits());
        assert_eq!(player.speed.to_bits(), 1.25_f32.to_bits());
        assert_eq!(player.direction.to_bits(), 1.0_f32.to_bits());
        assert!(player.manual_animation);
        assert!(!player.restart_clock);
        assert_eq!(player.last_sync_ms, 77);
        player.state = 0;
        let mut draw = super::tests::one_draw(state);
        assert!(!complete(&player, &draw, &clips));
        draw.states[0].hlod = false;
        assert!(complete(&player, &draw, &clips));
        draw.states[0].model = None;
        assert!(complete(&player, &draw, &clips));
    }

    #[test]
    fn missing_clip_retains_previous_playback_then_recreated_model_restores_base_pose() {
        let clip = super::tests::test_clip();
        let mut active = State::empty();
        active.model = Some(0);
        active.render_model = Some(0);
        active.hlod = true;
        active.mode = "LOOP_BACKWARDS".into();
        active.speed = [1.25; 2];
        active.motions.push(Motion {
            name: clip.name.clone(),
            distance: 0.0,
            idle: false,
        });
        let clips = BTreeMap::from([(clip.name.clone(), clip)]);
        let mut draw = super::tests::one_draw(active.clone());
        let mut missing = active;
        missing.mode = "ONCE".into();
        missing.speed = [2.0, 3.0];
        missing.flags.insert("RANDOMSTART".into());
        missing.motions[0].name = "RIG.NOT_EXPORTED".into();
        draw.states.push(missing.clone());
        missing.model = Some(1);
        missing.render_model = Some(1);
        draw.states.push(missing);
        let mut player = Player::new([0.0; 2], 0);
        let mut random = Random::new(0);
        assert!(select(&mut player, &draw, &clips, 0, &mut random));
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime::logic_tick(3),
        );
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime::logic_tick(6),
        );
        let before = player.frame;
        let random_before = random.0;

        assert!(select(&mut player, &draw, &clips, 1, &mut random));
        assert_eq!(random.0, random_before);
        let (mode, animation) = player.animation(&draw).unwrap();
        assert_eq!(mode.mode, "LOOP_BACKWARDS");
        assert_eq!(animation.name, "RIG.IDLE");
        assert_eq!(player.frame.to_bits(), before.to_bits());
        assert_eq!(player.speed.to_bits(), 1.25_f32.to_bits());
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime::logic_tick(9),
        );
        assert!((player.frame - before + 3.75).abs() < 0.000_01);
        set_duration(&mut player, &draw, &clips, 45, false);
        assert!((player.speed - (31.0 / 45.0)).abs() < 0.000_01);
        assert!(!complete(&player, &draw, &clips));

        assert!(select(&mut player, &draw, &clips, 2, &mut random));
        assert!(!clips.contains_key(&player.animation(&draw).unwrap().1.name));
        assert_eq!(player.frame.to_bits(), 0.0_f32.to_bits());
        assert_eq!(player.speed.to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn completed_missing_render_object_does_not_reselect_idle_each_draw() {
        use generals_assets::{content, virtual_assets::Asset};
        let file = content::scan(Asset {
            path: "null.ini".into(), origin: "native doDrawModule render-object guard".into(),
            bytes: b"Object Missing\nDraw=W3DModelDraw Draw\nDefaultConditionState\nModel=NONE\nIdleAnimation=STATIC.A\nIdleAnimation=STATIC.B\nEnd\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let draw = crate::model_animation::data::parse(&file.declarations[0].lines).unwrap();
        let mut player = Player::new([0.0; 2], 0);
        let mut random = Random::new(11);
        let mut expected = Random::new(11);
        expected.index(2);
        assert!(select(&mut player, &draw, &BTreeMap::new(), 0, &mut random));
        assert!(!select(
            &mut player,
            &draw,
            &BTreeMap::new(),
            0,
            &mut random
        ));
        assert_eq!(random.0, expected.0);
    }

    #[test]
    fn actual_handle_speed_rng_requires_hlod_but_start_frame_requires_only_render_object() {
        let clip = super::tests::test_clip();
        let mut state = State::empty();
        state.model = Some(0);
        state.motions.push(Motion {
            name: clip.name.clone(),
            distance: 0.0,
            idle: false,
        });
        state.speed = [0.5, 2.0];
        state.flags.insert("RANDOMSTART".into());
        let count = usize::from(clip.frame_count);
        let clips = BTreeMap::from([(clip.name.clone(), clip)]);
        for (present, hlod) in [(true, true), (true, false), (false, false)] {
            state.model = present.then_some(0);
            state.hlod = hlod;
            let mut player = Player::new([0.0; 2], 0);
            let mut random = Random::new(23);
            let mut expected = Random::new(23);
            let frame = present.then(|| expected.index(count));
            let speed = hlod.then(|| expected.real(0.5, 2.0));
            assert_eq!(
                choose(&mut player, &state, &clips, false, &mut random).is_some(),
                present
            );
            assert_eq!(random.0, expected.0, "present={present} hlod={hlod}");
            if let Some(frame) = frame {
                assert_eq!(player.frame.to_bits(), frame.to_f32().unwrap().to_bits());
            }
            if let Some(speed) = speed {
                assert_eq!(player.speed.to_bits(), speed.to_bits());
            }
        }
    }
}

#[cfg(test)]
mod fractional_tests {
    use super::*;
    use crate::model_animation::data::Motion;

    fn fixture(mode: &str) -> (Draw, BTreeMap<String, Clip>, Player) {
        let clip = super::tests::test_clip();
        let mut state = State::empty();
        state.model = Some(0);
        state.hlod = true;
        state.mode = mode.into();
        state.motions.push(Motion {
            name: clip.name.clone(),
            distance: 0.0,
            idle: false,
        });
        let clips = BTreeMap::from([(clip.name.clone(), clip)]);
        let mut player = Player::new([0.0; 2], 10);
        let draw = super::tests::one_draw(state);
        select(&mut player, &draw, &clips, 0, &mut Random::new(0));
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime {
                tick: 10,
                milliseconds: 700,
            },
        );
        (draw, clips, player)
    }

    #[test]
    fn fractional_milliseconds_advance_without_a_logic_tick_and_restart_at_current_time() {
        let (draw, clips, mut player) = fixture("ONCE");
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime {
                tick: 10,
                milliseconds: 708,
            },
        );
        assert!((player.frame - 0.24).abs() < 0.000_01);
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime {
                tick: 10,
                milliseconds: 716,
            },
        );
        assert!((player.frame - 0.48).abs() < 0.000_01);
        let frame = player.frame;
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime {
                tick: 10,
                milliseconds: 716,
            },
        );
        assert_eq!(player.frame.to_bits(), frame.to_bits());
        assert!(
            choose(
                &mut player,
                &draw.states[0],
                &clips,
                false,
                &mut Random::new(0)
            )
            .is_some()
        );
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime {
                tick: 11,
                milliseconds: 733,
            },
        );
        assert_eq!(player.frame.to_bits(), 0.0_f32.to_bits());
        update(
            &mut player,
            &draw,
            &clips,
            [0.0; 2],
            &FrameTime {
                tick: 11,
                milliseconds: 741,
            },
        );
        assert!((player.frame - 0.24).abs() < 0.000_01);
    }

    #[test]
    fn fractional_loop_once_backwards_pingpong_and_manual_keep_native_endpoints() {
        for (mode, expected, direction) in [
            ("ONCE", 30.0, 1.0_f32),
            ("LOOP", 0.24, 1.0),
            ("ONCE_BACKWARDS", 0.0, -1.0),
            ("LOOP_BACKWARDS", 29.76, -1.0),
            ("LOOP_PINGPONG", 30.0, -1.0),
            ("MANUAL", 0.0, 1.0),
        ] {
            let (draw, clips, mut player) = fixture(mode);
            update(
                &mut player,
                &draw,
                &clips,
                [0.0; 2],
                &FrameTime {
                    tick: 40,
                    milliseconds: 1708,
                },
            );
            assert!(
                (player.frame - expected).abs() < 0.000_01,
                "{mode}: {}",
                player.frame
            );
            assert_eq!(player.direction.to_bits(), direction.to_bits());
        }
    }

    #[test]
    fn moving_animation_speed_uses_logic_displacement_between_fractional_renders() {
        let (mut draw, clips, mut player) = fixture("LOOP");
        draw.states[0].motions[0].distance = 31.0;
        update(
            &mut player,
            &draw,
            &clips,
            [1.0, 0.0],
            &FrameTime {
                tick: 11,
                milliseconds: 733,
            },
        );
        assert!((player.speed - 1.0).abs() < 0.000_01);
        let frame = player.frame;
        update(
            &mut player,
            &draw,
            &clips,
            [1.0, 0.0],
            &FrameTime {
                tick: 11,
                milliseconds: 741,
            },
        );
        assert!((player.speed - 1.0).abs() < 0.000_01);
        assert!((player.frame - frame - 0.24).abs() < 0.000_01);
    }
}
