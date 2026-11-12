//! Source `ParkingPlaceBehavior` space, runway and healing ownership.
//! SH 2bfaea60, CRC=0: upper-row takeoff reservations are postponed once.
pub(super) mod doors;
mod geometry;
mod production;
mod runtime;
pub use doors::Rules as DoorRules;
#[cfg(test)]
mod runtime_tests;
mod runway;
#[cfg(test)]
pub(super) mod tests;

pub struct Pose {
    pub position: [f32; 3],
    pub direction: [f32; 3],
}

impl Pose {
    pub(super) fn orientation(&self) -> f32 {
        self.direction[1].atan2(self.direction[0])
    }
}

struct SpaceRules {
    pub hangar: Pose,
    pub parking: Pose,
    pub prep: [f32; 3],
    pub runway: usize,
}

struct RunwayRules {
    pub start: [f32; 3],
    pub end: [f32; 3],
}

pub struct Rules {
    pub rows: u16,
    pub columns: u16,
    pub has_runways: bool,
    pub approach_height: f32,
    pub deck_height: f32,
    pub park_in_hangars: bool,
    pub heal_per_second: f32,
}

pub struct Info {
    pub parking: Pose,
    pub prep: [f32; 3],
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub approach: [f32; 3],
    pub hangar: Pose,
    pub takeoff_distance: f32,
}

struct Space {
    geometry: SpaceRules,
    object: Option<usize>,
    reserved: bool,
    postponed: bool,
}

struct Runway {
    geometry: RunwayRules,
    object: Option<usize>,
    next: Option<usize>,
    was_in_line: bool,
}

pub(super) struct State {
    spaces: Vec<Space>,
    runways: Vec<Runway>,
    healing: Vec<usize>,
    next_heal: u32,
    got_info: bool,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            spaces: Vec::new(),
            runways: Vec::new(),
            healing: Vec::new(),
            next_heal: 0x3fff_ffff,
            got_info: false,
        }
    }

    pub(in crate::skirmish) const fn runway_count(&self) -> usize {
        self.runways.len()
    }

    pub(super) fn space_index(&self, id: usize) -> Option<usize> {
        self.spaces
            .iter()
            .position(|space| space.object == Some(id))
    }

    /// The client query must not purge records or change runway ordering.
    pub(super) fn available(&self, helicopter: bool, alive: impl Fn(usize) -> bool) -> bool {
        self.got_info
            && (helicopter
                || self
                    .spaces
                    .iter()
                    .any(|space| space.object.is_none_or(|id| !alive(id)) && !space.reserved))
    }

    pub(super) fn purge(
        &mut self,
        now: u64,
        mut doors: Option<&mut doors::State>,
        alive: impl Fn(usize) -> bool,
    ) {
        for (index, space) in self.spaces.iter_mut().enumerate() {
            if space.object.is_some_and(|id| !alive(id)) {
                space.object = None;
                space.reserved = false;
                space.postponed = false;
                if let Some(doors) = doors.as_deref_mut() {
                    doors.set_hold(index, false, now);
                }
            }
        }
        for runway in &mut self.runways {
            if runway.object.is_some_and(|id| !alive(id)) {
                runway.object = None;
                runway.was_in_line = false;
            }
            if runway.next.is_some_and(|id| !alive(id)) {
                runway.next = None;
            }
        }
        let count = self.healing.len();
        self.healing.retain(|&id| alive(id));
        if self.healing.len() != count {
            self.reset_heal(now);
        }
    }

    pub(super) fn reserve_door(&mut self, helicopter: bool) -> Result<Option<usize>, &'static str> {
        if helicopter {
            return Ok(None);
        }
        let door = self
            .spaces
            .iter()
            .position(|space| space.object.is_none() && !space.reserved)
            .ok_or("Airfield has no available parking space")?;
        self.spaces[door].reserved = true;
        Ok(Some(door))
    }

    pub(super) fn unreserve_door(&mut self, door: Option<usize>) {
        if let Some(door) = door {
            let space = &mut self.spaces[door];
            space.object = None;
            space.reserved = false;
        }
    }

    pub(super) fn exit(&mut self, door: Option<usize>, id: usize) -> Result<(), &'static str> {
        if let Some(door) = door {
            let space = self
                .spaces
                .get_mut(door)
                .filter(|space| space.object.is_none() && space.reserved)
                .ok_or("Produced aircraft lost its reserved exit")?;
            space.object = Some(id);
            space.reserved = false;
        }
        Ok(())
    }

    pub(super) fn reserve_space(
        &mut self,
        id: usize,
        now: u64,
        doors: Option<&mut doors::State>,
    ) -> Option<usize> {
        let index = self.space_index(id).or_else(|| {
            self.spaces
                .iter()
                .position(|space| space.object.is_none() && !space.reserved)
        })?;
        let space = &mut self.spaces[index];
        space.object = Some(id);
        space.reserved = false;
        if let Some(doors) = doors {
            doors.set_hold(index, true, now);
        }
        Some(index)
    }

    pub(super) fn release_space(&mut self, id: usize, doors: Option<&mut doors::State>) {
        if let Some(index) = self.space_index(id) {
            let space = &mut self.spaces[index];
            space.object = None;
            space.reserved = false;
            space.postponed = false;
            if let Some(doors) = doors {
                doors.set_hold(index, false, 0);
            }
        }
    }

    pub(super) fn heal(&mut self, id: usize, add: bool, now: u64) {
        if add {
            if self.healing.contains(&id) {
                return;
            }
            self.healing.push(id);
        } else if let Some(index) = self.healing.iter().position(|&target| target == id) {
            self.healing.remove(index);
        } else {
            return;
        }
        self.reset_heal(now);
    }

    fn reset_heal(&mut self, now: u64) {
        self.next_heal = if self.healing.is_empty() {
            0x3fff_ffff
        } else {
            native_frame(now).wrapping_add(6)
        };
    }

    pub(super) fn healing(&mut self, now: u64) -> &[usize] {
        let now = native_frame(now);
        if now >= self.next_heal {
            self.next_heal = now.wrapping_add(6);
            &self.healing
        } else {
            &[]
        }
    }
}

fn native_frame(frame: u64) -> u32 {
    u32::try_from(frame & u64::from(u32::MAX)).expect("masked native frame")
}
