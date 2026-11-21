use super::{Emitter, Lifecycle, Particles, Pose, Template};

impl Particles {
    pub(super) fn destroy(&mut self, id: u64) {
        // ParticleSystem::destroy marks the whole slave chain immediately;
        // existing particles remain until their normal lifetime ends.
        let mut next = Some(id);
        while let Some(id) = next {
            let Some(emitter) = self.emitters.iter_mut().find(|emitter| emitter.id == id) else {
                break;
            };
            emitter.lifecycle.destroyed = true;
            next = emitter.slave;
        }
    }

    pub(super) fn create(
        &mut self,
        index: usize,
        pose: Pose,
        attachment: Option<u64>,
        random: &mut impl FnMut(f32, f32) -> f32,
    ) -> u64 {
        let rules = &self.templates[index];
        let id = self.next_id;
        self.next_id += 1;
        let delay = rules.initial_delay.sample(random).trunc();
        let start = rules.wind_start.sample(random);
        let end = rules.wind_end.sample(random);
        let angle = random(start, end);
        let mut emitter = Emitter {
            id,
            template: index,
            particles: Vec::new(),
            transform: pose,
            wind: super::wind::State::new(start, end, angle),
            wind_origin: None,
            radius: None,
            previous: None,
            attachment,
            object_attachment: None,
            drawable_attachment: None,
            drawable_offset: None,
            fire_attachment: None,
            control_particle: None,
            slave: None,
            master: false,
            lifecycle: Lifecycle {
                destroyed: false,
                stopped: false,
            },
            shrouded: false,
            delay,
            burst_delay: 0.0,
            remaining: rules.system_lifetime,
            size_bonus: 0.0,
            size_multiplier: 1.0,
            velocity_multiplier: [1.0; 3],
            burst_multiplier: 1.0,
            lifetime: None,
            personality: 0,
        };
        if let Some(slave) = rules.slave {
            // Native constructor recursively adds the slave before the master.
            let slave_id = self.create(slave, Pose::at([0.0; 3]), None, random);
            self.emitters
                .last_mut()
                .expect("created particle slave")
                .master = true;
            emitter.slave = Some(slave_id);
        }
        self.emitters.push(emitter);
        id
    }

    pub(super) fn begin(&mut self, generation: u32, frame: u64) {
        if self.generation != generation {
            self.emitters.clear();
            self.bone_systems.clear();
            self.bodies.clear();
            self.smudges.clear();
            self.object_emitters.clear();
            self.gps_scramblers.clear();
            self.model_states.reset();
            self.fires.reset();
            self.sounds.clear();
            self.scorches.clear();
            self.shake.reset();
            self.pulses.clear();
            self.generation = generation;
            self.frame = None;
            self.processed = None;
            self.advanced = None;
            self.phase = 0.0;
        }
        if self.frame != Some(frame) {
            self.draw(1.0 - self.phase);
            self.phase = 0.0;
            self.frame = Some(frame);
        }
    }

    pub(super) fn logic(
        &mut self,
        frame: u64,
        random: &mut impl FnMut(f32, f32) -> f32,
        ground: &impl Fn([f32; 2]) -> f32,
    ) {
        let mut index = 0;
        while index < self.emitters.len() {
            // The native list iterator advances before updating the current
            // system. Appended children are visited only if that iterator had
            // not already reached the end of the list.
            let had_successor = index + 1 < self.emitters.len();
            self.update_emitter(index, frame, random, ground);
            let emitter = &mut self.emitters[index];
            if emitter.drawable_attachment.is_some() && emitter.previous.is_some() {
                emitter.previous = Some(emitter.transform.position);
            }
            if !had_successor {
                break;
            }
            index += 1;
        }
        self.retire();
        self.smudges.clear();
        // ParticleSystemManager::update recreates smudges after every system update.
        // Their position, size and opacity stay fixed during render interpolation.
        for emitter in &self.emitters {
            if matches!(
                self.templates[emitter.template].drawing.style,
                super::Style::Smudge
            ) {
                for particle in &emitter.particles {
                    self.smudges.push(super::smudge::Smudge {
                        system: emitter.id,
                        position: particle.position,
                        size: particle.size,
                        opacity: particle.alpha,
                        offset: [random(-0.06, 0.06), random(-0.06, 0.06)],
                    });
                }
            }
        }
    }

    pub(super) fn advance_frame(
        &mut self,
        frame: u64,
        random: &mut impl FnMut(f32, f32) -> f32,
        ground: &impl Fn([f32; 2]) -> f32,
    ) {
        if self.advanced != Some(frame) {
            self.advanced = Some(frame);
            self.logic(frame, random, ground);
        }
    }

    pub(super) fn update_emitter(
        &mut self,
        index: usize,
        frame: u64,
        random: &mut impl FnMut(f32, f32) -> f32,
        ground: &impl Fn([f32; 2]) -> f32,
    ) {
        if self.emitters[index].delay > 0.0 {
            self.emitters[index].delay -= 1.0;
            return;
        }
        let emitter = &mut self.emitters[index];
        emitter
            .wind
            .update(&self.templates[emitter.template], random);
        self.update_controlled_position(index);
        let rules = &self.templates[self.emitters[index].template];
        let emitter = &mut self.emitters[index];
        if !emitter.lifecycle.destroyed
            && !emitter.lifecycle.stopped
            && !emitter.shrouded
            && !emitter.master
            && (rules.system_lifetime == 0.0 || emitter.remaining > 0.0)
        {
            if emitter.burst_delay == 0.0 {
                let sampled =
                    (rules.burst_count.sample(random).trunc() * emitter.burst_multiplier).trunc();
                let count = (0..=4096u16)
                    .find(|&count| f32::from(count) >= sampled)
                    .expect("validated particle burst range");
                self.burst(index, count, frame, random, ground);
                let rules = &self.templates[self.emitters[index].template];
                self.emitters[index].burst_delay = rules.burst_delay.sample(random).trunc();
            } else {
                emitter.burst_delay -= 1.0;
            }
        }
        let emitter = &mut self.emitters[index];
        let rules = &self.templates[emitter.template];
        let mut retired_children = Vec::new();
        emitter.particles.retain_mut(|particle| {
            let keep = particle.update(rules, frame);
            if !keep && let Some(child) = particle.controlled {
                retired_children.push(child);
            }
            keep
        });
        emitter.remaining = (emitter.remaining - 1.0).max(0.0);
        if !retired_children.is_empty() {
            for emitter in &mut self.emitters {
                if retired_children.contains(&emitter.id) {
                    emitter.control_particle = None;
                    emitter.lifecycle.destroyed = true;
                }
            }
        }
    }

    fn update_controlled_position(&mut self, index: usize) {
        let Some((system, personality)) = self.emitters[index].control_particle else {
            return;
        };
        let position = self
            .emitters
            .iter()
            .find(|emitter| emitter.id == system)
            .and_then(|emitter| {
                emitter
                    .particles
                    .iter()
                    .find(|particle| particle.personality == personality)
            })
            .map(|particle| particle.position);
        if let Some(position) = position {
            let emitter = &mut self.emitters[index];
            if emitter.previous.is_some() {
                emitter.previous = Some(emitter.transform.position);
            }
            emitter.transform.position = position;
        }
    }

    fn burst(
        &mut self,
        index: usize,
        count: u16,
        frame: u64,
        random: &mut impl FnMut(f32, f32) -> f32,
        ground: &impl Fn([f32; 2]) -> f32,
    ) {
        for number in 0..count {
            let rules = &self.templates[self.emitters[index].template];
            let emitter = &mut self.emitters[index];
            let particle = emitter.generate(rules, self.scale, number, count, frame, random);
            // Generate all source attributes before the terrain gate; rejected
            // attempts retain their RNG/size effects but create no personality,
            // attached system or slave particle.
            if rules.emit_above_ground_only
                && particle.position[2] < ground([particle.position[0], particle.position[1]])
            {
                continue;
            }
            let personality = particle.personality;
            emitter.personality = emitter.personality.wrapping_add(1);
            emitter.particles.push(particle);
            let parent = emitter.id;
            let slave_id = emitter.slave;
            if let Some(template) = rules.attached {
                let child = self.create(template, Pose::at([0.0; 3]), None, random);
                let emitter = self
                    .emitters
                    .last_mut()
                    .expect("created attached particle system");
                emitter.control_particle = Some((parent, personality));
                // Control particles replace the world transform, not the local
                // translation returned by getPosition to native wind motion.
                emitter.wind_origin = Some([0.0; 3]);
                self.emitters[index]
                    .particles
                    .last_mut()
                    .expect("created controlling particle")
                    .controlled = Some(child);
            }
            if let Some(slave_id) = slave_id {
                let rules = &self.templates[self.emitters[index].template];
                let mut info =
                    self.emitters[index].generate(rules, self.scale, 1, 1, frame, random);
                let slave = self
                    .emitters
                    .iter_mut()
                    .find(|other| other.id == slave_id)
                    .expect("live particle slave");
                let slave_rules = &self.templates[slave.template];
                let slave_info = slave.generate(slave_rules, self.scale, 1, 1, frame, random);
                info.merge(&slave_info, slave_rules.slave_offset);
                info.personality = slave.personality;
                slave.personality = slave.personality.wrapping_add(1);
                slave.particles.push(info);
            }
        }
    }

    pub(super) fn retire(&mut self) {
        // Destroying the parent marks its forever slave destroyed; finite slaves
        // retain the source's fixed lifetime relationship until they finish.
        let retired: Vec<u64> = self
            .emitters
            .iter()
            .filter(|emitter| {
                let rules = &self.templates[emitter.template];
                emitter.particles.is_empty()
                    && (emitter.lifecycle.destroyed
                        || (rules.system_lifetime != 0.0 && emitter.remaining == 0.0))
                    && emitter.slave.is_none_or(|slave| {
                        self.emitters.iter().all(|other| {
                            other.id != slave
                                || self.templates[other.template].system_lifetime == 0.0
                        })
                    })
            })
            .map(|emitter| emitter.id)
            .collect();
        if retired.is_empty() {
            return;
        }
        let slaves: Vec<u64> = self
            .emitters
            .iter()
            .filter(|e| retired.contains(&e.id))
            .filter_map(|e| e.slave)
            .collect();
        for emitter in &mut self.emitters {
            for particle in &mut emitter.particles {
                if particle.controlled.is_some_and(|id| retired.contains(&id)) {
                    particle.controlled = None;
                }
            }
            if slaves.contains(&emitter.id) {
                emitter.lifecycle.destroyed = true;
                emitter.master = false;
            }
            if emitter.slave.is_some_and(|id| retired.contains(&id)) {
                emitter.slave = None;
            }
        }
        self.emitters.retain(|e| !retired.contains(&e.id));
    }

    pub(super) fn draw(&mut self, step: f32) {
        if step <= 0.0 {
            return;
        }
        for emitter in &mut self.emitters {
            if emitter.delay != 0.0 {
                continue;
            }
            let rules: &Template = &self.templates[emitter.template];
            let wind = (!matches!(rules.wind_motion, super::wind::Motion::Unused)).then_some((
                emitter.wind.angle,
                emitter.wind_origin.unwrap_or(emitter.transform.position),
            ));
            for particle in &mut emitter.particles {
                particle.draw(rules, step, wind);
            }
        }
    }
}
