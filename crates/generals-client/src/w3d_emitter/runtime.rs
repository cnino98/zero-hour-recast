//! Scene update-list ordering and native emitter/buffer lifetime.
use super::{Binding, buffer::Buffer, pose::Pose, random::Random};
use num_traits::ToPrimitive;
use std::{collections::BTreeSet, sync::Arc};

pub(super) struct Emitter {
    pub id: u64,
    pub object: usize,
    pub template: usize,
    pub buffer: Buffer,
    previous: Pose,
    remain: u32,
    active: bool,
    installed: bool,
    rate: u32,
    left: u32,
    complete: bool,
}

enum Callback {
    Emitter(u64),
    Buffer(u64),
}

pub(super) struct Scene {
    pub emitters: Vec<Emitter>,
    callbacks: Vec<Callback>,
    pub random: Random,
    validated: BTreeSet<String>,
    next: u64,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            emitters: Vec::new(),
            callbacks: Vec::new(),
            random: Random::new(),
            validated: BTreeSet::new(),
            next: 0,
        }
    }

    pub fn reset(&mut self) {
        // Map purge retires scene objects, not process-static random streams or
        // the pristine validation flags in surviving original template data.
        self.emitters.clear();
        self.callbacks.clear();
    }

    pub fn create(
        &mut self,
        object: usize,
        template: usize,
        binding: &Binding,
        pose: Pose,
        now: u32,
        validate_logic: bool,
    ) {
        let definition = Arc::clone(&binding.definition);
        if validate_logic && self.validated.insert(binding.name.clone()) {
            // validateCachedBones creates/releases a real temporary render object.
            let _validation = Buffer::new(Arc::clone(&definition), &mut self.random, now);
        }
        let rate = if definition.emission.rate > 0.0 {
            (1000.0 / definition.emission.rate)
                .to_u32()
                .expect("validated emission rate")
        } else {
            1000
        };
        let left = definition
            .emission
            .maximum
            .to_u32()
            .expect("validated maximum emissions");
        let buffer = Buffer::new(definition, &mut self.random, now);
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        self.emitters.push(Emitter {
            id,
            object,
            template,
            buffer,
            previous: pose,
            remain: 0,
            active: false,
            installed: false,
            rate,
            left,
            complete: false,
        });
        self.callbacks.insert(0, Callback::Emitter(id));
    }

    pub fn destroy(&mut self, object: usize) {
        for emitter in &mut self.emitters {
            if emitter.object == object {
                emitter.active = false;
                emitter.buffer.dead = true;
            }
        }
        self.callbacks.retain(|callback| !matches!(callback,
            Callback::Emitter(id) if self.emitters.iter().any(|emitter| emitter.id == *id && emitter.object == object)));
    }

    pub fn update(
        &mut self,
        now: u32,
        frame: u32,
        mut pose: impl FnMut(usize) -> Option<(Pose, bool)>,
    ) {
        self.visibility_pass(now, &mut pose);
        self.frame_callbacks(now, frame, &mut pose);
        for emitter in &mut self.emitters {
            emitter.buffer.update(now);
        }
        self.release_dead_buffers();
    }

    fn visibility_pass(&mut self, now: u32, pose: &mut impl FnMut(usize) -> Option<(Pose, bool)>) {
        // Visibility_Check asks live buffer bounds before UpdateList emission.
        // Its lazy kinematic update advances LastUpdateTime before this pass's
        // new births; the same-time render update cannot consume those births.
        for emitter in &mut self.emitters {
            if emitter.installed {
                emitter.buffer.update(now);
            }
        }
        // Visibility_Check precedes UpdateList. Frustum Set_Visible(false) still
        // starts an unhidden emitter; only actual hidden flags stop it.
        for emitter in &mut self.emitters {
            if emitter.buffer.dead {
                continue;
            }
            if let Some((current, hidden)) = pose(emitter.object) {
                if !hidden && !emitter.active {
                    emitter.previous = current;
                    emitter.active = true;
                    if emitter.complete {
                        emitter.complete = false;
                        emitter.left = emitter
                            .buffer
                            .definition
                            .emission
                            .maximum
                            .to_u32()
                            .expect("validated cap");
                    }
                } else if hidden {
                    emitter.active = false;
                }
            }
        }
    }

    fn frame_callbacks(
        &mut self,
        now: u32,
        frame: u32,
        pose: &mut impl FnMut(usize) -> Option<(Pose, bool)>,
    ) {
        // Capture the list before callbacks. Head-inserted buffers cannot receive
        // On_Frame_Update during the pass that installs them.
        let mut install = Vec::new();
        for callback in &self.callbacks {
            let (Callback::Emitter(id) | Callback::Buffer(id)) = callback;
            let Some(emitter) = self.emitters.iter_mut().find(|emitter| emitter.id == *id) else {
                continue;
            };
            match callback {
                Callback::Emitter(_)
                    if !emitter.installed && emitter.active && !emitter.complete =>
                {
                    emitter.installed = true;
                    if let Some((current, _)) = pose(emitter.object) {
                        emitter.previous = current;
                    }
                    install.push(*id);
                }
                Callback::Buffer(_) if !emitter.buffer.dead => {
                    if let Some((current, _)) = pose(emitter.object) {
                        emitter.emit(now, frame, &current, &mut self.random);
                        emitter.previous = current;
                    }
                }
                _ => {}
            }
        }
        for id in install {
            self.callbacks.insert(0, Callback::Buffer(id));
        }
    }

    fn release_dead_buffers(&mut self) {
        self.emitters.retain(|emitter| {
            !(emitter.buffer.dead
                && emitter.buffer.particles.is_empty()
                && emitter.buffer.pending.is_empty())
        });
        self.callbacks.retain(|callback| {
            let (Callback::Emitter(id) | Callback::Buffer(id)) = callback;
            self.emitters.iter().any(|emitter| emitter.id == *id)
        });
    }
}

impl Emitter {
    fn emit(&mut self, now: u32, mut frame: u32, current: &Pose, random: &mut Random) {
        if !self.active || self.complete {
            return;
        }
        if frame > 100 * self.rate {
            let capacity = u32::try_from(self.buffer.capacity).expect("validated emitter capacity");
            let burst = self.buffer.definition.emission.burst.max(1);
            let cycle = self.rate * (capacity / gcd(capacity, burst));
            frame = if cycle > 1 { frame % cycle } else { 1 };
        }
        self.remain = self.remain.wrapping_add(frame);
        let interval = frame.to_f32().expect("native emitter frame");
        let mut alpha = 1.0 - self.remain.to_f32().expect("native emission remainder") / interval;
        let delta = self.rate.to_f32().expect("native emission rate") / interval;
        let inherit = self.buffer.definition.motion.inherit;
        let inherited = std::array::from_fn(|axis| {
            if inherit == 0.0 {
                0.0
            } else {
                (current.position[axis] - self.previous.position[axis]) * (inherit / interval)
            }
        });
        while self.remain > self.rate {
            self.remain -= self.rate;
            alpha += delta;
            let pose = self.previous.interpolate(current, alpha);
            let mut burst = self.buffer.definition.emission.burst.max(1);
            if self.left > 0 {
                burst = burst.min(self.left);
                self.left -= burst;
                self.complete = self.left == 0;
            }
            for _ in 0..burst {
                self.create_particle(&pose, inherited, now.wrapping_sub(self.remain), random);
            }
            if self.complete {
                break;
            }
        }
    }

    fn create_particle(
        &mut self,
        pose: &Pose,
        inherited: [f32; 3],
        born: u32,
        random: &mut Random,
    ) {
        let motion = &self.buffer.definition.motion;
        let local = random.box_vector(motion.position_box);
        let rotated = pose.rotate(local);
        let position = std::array::from_fn(|axis| rotated[axis] + pose.position[axis]);
        let mut velocity = random.box_vector(motion.velocity_box.map(|value| value * 0.001));
        let xx = local[0] * local[0];
        let yy = local[1] * local[1];
        let zz = local[2] * local[2];
        let length_squared = xx + yy + zz;
        let outward = motion.outward * 0.001;
        if outward != 0.0 {
            let outwards = if length_squared == 0.0 {
                [outward, 0.0, 0.0]
            } else {
                let scale = outward * generals_sim::native_real::inverse_sqrt(length_squared);
                local.map(|value| value * scale)
            };
            for axis in 0..3 {
                velocity[axis] += outwards[axis];
            }
        }
        for (axis, component) in velocity.iter_mut().enumerate() {
            let base_velocity = motion.velocity[axis] * 0.001;
            *component += base_velocity;
        }
        let rotated = pose.rotate(velocity);
        let velocity = std::array::from_fn(|axis| inherited[axis] + rotated[axis]);
        self.buffer.enqueue(position, velocity, born);
    }
}

const fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}
