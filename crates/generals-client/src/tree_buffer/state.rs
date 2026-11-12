//! Native optimized tree lifetime, contact buckets, sway, and terrain draw ordering.
use super::data::Data;
use generals_sim::{
    native_real,
    skirmish::{
        map_scripts::Breeze,
        physical_ocl::transform::{pre_rotate, vector},
    },
};
use num_traits::ToPrimitive;
use std::{f32::consts::PI, sync::Arc};

const LIMIT: f32 = PI / 2.0 - PI / 64.0;

pub(super) enum Topple {
    Upright,
    Falling,
    Fogged,
    Down,
}

pub(super) struct Tree {
    pub placement: usize,
    pub kind: Option<usize>,
    pub position: [f32; 3],
    pub scale: f32,
    pub sine: f32,
    pub cosine: f32,
    pub visible: bool,
    pub center: [f32; 3],
    pub radius: f32,
    pub next: Option<usize>,
    pub sway: usize,
    pub push: f32,
    delta: f32,
    source: Option<usize>,
    last_frame: u32,
    pub push_sine: f32,
    pub push_cosine: f32,
    pub topple: Topple,
    velocity: f32,
    acceleration: f32,
    direction: [f32; 3],
    angle: f32,
    pub basis: [[f32; 3]; 3],
    pub sink: f32,
}

pub struct Effect {
    pub name: String,
    pub position: [f32; 3],
}

pub(super) struct Buffer {
    pub types: Vec<Arc<Data>>,
    pub trees: Vec<Tree>,
    buckets: Vec<Option<usize>>,
    bounds: [f32; 2],
    wave: [[f32; 3]; 100],
    offsets: [f32; 10],
    steps: [f32; 10],
    factors: [f32; 10],
    breeze_version: i16,
    pub effects: Vec<Effect>,
    pub changed: bool,
    pub pushed: bool,
}

impl Buffer {
    pub fn new(bounds: [f32; 2]) -> Self {
        Self {
            types: Vec::new(),
            trees: Vec::new(),
            buckets: vec![None; 10_000],
            bounds,
            wave: [[0.0; 3]; 100],
            offsets: [0.0; 10],
            steps: [0.0; 10],
            factors: [0.0; 10],
            breeze_version: -1,
            effects: Vec::new(),
            changed: true,
            pushed: false,
        }
    }

    fn bucket(&self, point: [f32; 3]) -> usize {
        let index = std::array::from_fn::<_, 2, _>(|axis| {
            native_real::floor(
                (point[axis].clamp(0.0, self.bounds[axis]) / self.bounds[axis]) * 99.9,
            )
        });
        usize::try_from(index[1] * 100 + index[0]).expect("native tree partition bounds")
    }

    pub fn add(
        &mut self,
        placement: usize,
        data: Arc<Data>,
        position: [f32; 3],
        angle: f32,
        scale: f32,
        sway: usize,
    ) {
        if self.trees.len() >= 4000 {
            return;
        }
        let kind = self
            .types
            .iter()
            .position(|old| {
                old.model.eq_ignore_ascii_case(&data.model)
                    && old.texture.eq_ignore_ascii_case(&data.texture)
            })
            .unwrap_or_else(|| {
                assert!(self.types.len() < 64, "native tree type capacity exceeded");
                self.types.push(data);
                self.types.len() - 1
            });
        let data = &self.types[kind];
        let bucket = (data.outward > 2 || data.topple).then(|| self.bucket(position));
        let next = bucket.and_then(|bucket| self.buckets[bucket]);
        if let Some(bucket) = bucket {
            self.buckets[bucket] = Some(self.trees.len());
        }
        let center = std::array::from_fn(|axis| {
            let offset = data.center[axis] * scale;
            offset + position[axis]
        });
        self.trees.push(Tree {
            placement,
            kind: Some(kind),
            position,
            scale,
            sine: angle.sin(),
            cosine: angle.cos(),
            visible: false,
            center,
            radius: data.radius * scale,
            next,
            sway,
            push: 0.0,
            delta: 0.0,
            source: None,
            last_frame: 0,
            push_sine: 1.0,
            push_cosine: 1.0,
            topple: Topple::Upright,
            velocity: 0.0,
            acceleration: 0.0,
            direction: [0.0; 3],
            angle: 0.0,
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            sink: 0.0,
        });
    }

    pub fn moved(
        &mut self,
        frame: u64,
        object: usize,
        position: [f32; 3],
        direction: [f32; 3],
        geometry: &generals_sim::collision_geometry::Geometry,
        crusher: u8,
    ) {
        let mut radius = geometry.major;
        if matches!(geometry.shape, generals_sim::collision_geometry::Shape::Box) {
            radius = radius.min(geometry.minor);
        }
        radius += 7.0;
        let lower = std::array::from_fn::<_, 2, _>(|axis| {
            native_real::floor(
                ((position[axis] - radius).clamp(0.0, self.bounds[axis]) / self.bounds[axis])
                    * 99.9,
            )
        });
        let upper = std::array::from_fn::<_, 2, _>(|axis| {
            native_real::ceil(
                ((position[axis] + radius).clamp(0.0, self.bounds[axis]) / self.bounds[axis])
                    * 99.9,
            )
        });
        for x in lower[0]..upper[0] {
            for y in lower[1]..upper[1] {
                let mut current =
                    self.buckets[usize::try_from(x + y * 100).expect("native tree partition")];
                while let Some(index) = current {
                    let tree = &self.trees[index];
                    current = tree.next;
                    let Some(kind) = tree.kind else {
                        continue;
                    };
                    let delta =
                        std::array::from_fn::<_, 3, _>(|axis| tree.position[axis] - position[axis]);
                    let x_squared = delta[0] * delta[0];
                    let y_squared = delta[1] * delta[1];
                    let z_squared = delta[2] * delta[2];
                    let distance = (x_squared + y_squared) + z_squared;
                    if radius * radius > distance {
                        if crusher > 1 && self.types[kind].topple {
                            self.topple(index, [delta[0], delta[1], 0.0]);
                        } else if self.types[kind].outward > 1 {
                            self.push(index, frame, object, position, direction);
                        }
                    }
                }
            }
        }
    }

    fn push(
        &mut self,
        index: usize,
        frame: u64,
        object: usize,
        position: [f32; 3],
        direction: [f32; 3],
    ) {
        let frame =
            u32::try_from(frame & u64::from(u32::MAX)).expect("native frame wraps at 32 bits");
        let tree = &mut self.trees[index];
        let previous = tree.last_frame;
        tree.last_frame = frame;
        if tree.source == Some(object) && frame.wrapping_sub(previous) < 3 {
            return;
        }
        if tree.push != 0.0 {
            return;
        }
        tree.source = Some(object);
        let dx = tree.position[0] - position[0];
        let dy = tree.position[1] - position[1];
        let left = direction[0] * dy;
        let right = direction[1] * dx;
        if left - right > 0.0 {
            tree.push_cosine = -direction[1];
            tree.push_sine = direction[0];
        } else {
            tree.push_cosine = direction[1];
            tree.push_sine = -direction[0];
        }
        tree.delta = 1.0
            / self.types[tree.kind.expect("live tree")]
                .outward
                .to_f32()
                .expect("native duration");
        self.pushed = true;
    }

    fn topple(&mut self, index: usize, mut direction: [f32; 3]) {
        let tree = &mut self.trees[index];
        if !matches!(tree.topple, Topple::Upright) {
            return;
        }
        let data = &self.types[tree.kind.expect("live tree")];
        let x_squared = direction[0] * direction[0];
        let y_squared = direction[1] * direction[1];
        let z_squared = direction[2] * direction[2];
        let length = ((x_squared + y_squared) + z_squared).sqrt();
        if length != 0.0 {
            for value in &mut direction {
                *value /= length;
            }
        }
        tree.direction = direction;
        tree.angle = 0.0;
        tree.velocity = data.minimum_speed * data.velocity;
        tree.acceleration = data.minimum_speed * data.acceleration;
        tree.topple = Topple::Falling;
        self.pushed = true;
        if !data.topple_fx.is_empty() && !data.topple_fx.eq_ignore_ascii_case("NONE") {
            self.effects.push(Effect {
                name: data.topple_fx.clone(),
                position: tree.position,
            });
        }
    }

    pub fn draw(
        &mut self,
        breeze: &Breeze,
        time: f32,
        mut random: impl FnMut(f32, f32) -> f32,
        mut fogged: impl FnMut([f32; 3]) -> bool,
    ) -> [[f32; 3]; 11] {
        if breeze.version != self.breeze_version {
            for (index, wave) in self.wave.iter_mut().enumerate() {
                let factor = (index.to_f32().expect("100 samples") * 2.0 * PI / 101.0).cos();
                let lean = breeze.intensity * factor;
                let angle = breeze.lean + lean;
                let sine = angle.sin();
                *wave = [
                    breeze.direction[0] * sine,
                    breeze.direction[1] * sine,
                    angle.cos() - 1.0,
                ];
            }
            let delta = breeze.randomness * 0.5;
            for index in 0..10 {
                self.steps[index] = 100.0 / f32::from(breeze.period);
                self.steps[index] *= random(1.0 - delta, 1.0 + delta);
                self.steps[index] = self.steps[index].max(0.0);
                self.offsets[index] = 0.0;
                self.factors[index] = random(1.0 - delta, 1.0 + delta);
            }
            self.breeze_version = breeze.version;
        }
        let mut sway = [[0.0; 3]; 11];
        for index in 0..10 {
            let step = self.steps[index] * time;
            self.offsets[index] += step;
            if self.offsets[index] > 99.0 {
                self.offsets[index] -= 99.0;
            }
            let low = native_real::floor(self.offsets[index]);
            if (0..99).contains(&low) {
                let low = usize::try_from(low).expect("native wave index");
                let second = self.offsets[index] - low.to_f32().expect("100 samples");
                let first = 1.0 - second;
                sway[index + 1] = std::array::from_fn(|axis| {
                    let first_sample = first * self.wave[low][axis];
                    let second_sample = second * self.wave[low + 1][axis];
                    (first_sample + second_sample) * self.factors[index]
                });
            }
        }
        for tree in &mut self.trees {
            let Some(kind) = tree.kind else {
                continue;
            };
            let data = &self.types[kind];
            match tree.topple {
                Topple::Falling | Topple::Fogged => {
                    update_topple(tree, data, time, fogged(tree.position), &mut self.effects);
                }
                Topple::Down => {
                    if data.kill {
                        if tree.sink <= 0.0 {
                            tree.kind = None;
                            self.changed = true;
                        }
                        tree.sink -= time;
                        let sink = (data.sink_distance / data.sink_frames) * time;
                        tree.position[2] -= sink;
                    }
                }
                Topple::Upright => {
                    if tree.delta != 0.0 {
                        tree.push += tree.delta;
                        if tree.push >= 1.0 {
                            tree.delta = -1.0 / data.inward.to_f32().expect("native duration");
                        } else if tree.push <= 0.0 {
                            tree.delta = 0.0;
                            tree.push = 0.0;
                        }
                    }
                }
            }
        }
        sway
    }

    pub fn needs_vertex_update(&self) -> bool {
        self.changed
            || self.pushed
            || self.trees.iter().any(|tree| {
                tree.kind.is_some()
                    && tree.visible
                    && (tree.delta != 0.0 || !matches!(tree.topple, Topple::Upright))
            })
    }
}

fn update_topple(tree: &mut Tree, data: &Data, time: f32, fogged: bool, effects: &mut Vec<Effect>) {
    if fogged {
        tree.topple = Topple::Fogged;
        return;
    }
    if matches!(tree.topple, Topple::Fogged) {
        tree.velocity = 0.0;
        tree.topple = Topple::Down;
        pre_rotate(&mut tree.basis, 0, -LIMIT * tree.direction[1]);
        pre_rotate(&mut tree.basis, 1, LIMIT * tree.direction[0]);
        if data.kill {
            tree.sink = 0.0;
        }
        return;
    }
    let mut velocity = tree.velocity * time;
    if tree.angle + velocity > LIMIT {
        velocity = LIMIT - tree.angle;
    }
    pre_rotate(&mut tree.basis, 0, -velocity * tree.direction[1]);
    pre_rotate(&mut tree.basis, 1, velocity * tree.direction[0]);
    tree.angle += velocity;
    if tree.angle >= LIMIT && tree.velocity > 0.0 {
        tree.velocity *= -data.bounce;
        if tree.velocity.abs() < 0.01 {
            tree.velocity = 0.0;
            tree.topple = Topple::Down;
            if data.kill {
                tree.sink = data.sink_frames;
            }
        } else if tree.velocity.abs() >= 0.03
            && !data.bounce_fx.is_empty()
            && !data.bounce_fx.eq_ignore_ascii_case("NONE")
        {
            let offset = vector(tree.basis, [0.0, 0.0, 21.0]);
            effects.push(Effect {
                name: data.bounce_fx.clone(),
                position: std::array::from_fn(|axis| offset[axis] + tree.position[axis]),
            });
        }
    } else {
        let acceleration = tree.acceleration * time;
        tree.velocity += acceleration;
    }
}
