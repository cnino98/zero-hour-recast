//! Independent native `DISABLED_UNMANNED` and `OBJECT_STATUS_HIJACKED` states.
use super::Entity;

mod names;
mod pilot;

#[cfg(test)]
mod tests;

/// Original `MiscAudio` pilot-splatter callback, emitted at the victim before defection.
pub struct PilotEvent {
    pub frame: u64,
    pub object: usize,
    pub position: [f32; 3],
}

pub(super) struct State {
    pub(super) unmanned: bool,
    pub(super) hijacked: bool,
    pub(super) hijackers: Vec<super::hijacker::State>,
    pub(super) sight: Option<[f32; 2]>,
    pub(super) parachuting: bool,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            unmanned: false,
            hijacked: false,
            hijackers: Vec::new(),
            sight: None,
            parachuting: false,
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn unmanned(&self) -> bool {
        self.crew.unmanned
    }

    #[must_use]
    pub const fn hijacked(&self) -> bool {
        self.crew.hijacked
    }
}
