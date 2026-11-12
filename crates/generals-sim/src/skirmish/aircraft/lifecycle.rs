//! `JetAIUpdate` constructor / onObjectCreated / onDelete, SH `JetAIUpdate`.cpp1880–1953.
use crate::skirmish::Skirmish;
impl Skirmish {
    pub(in crate::skirmish) fn construct_jet_module(&mut self, id: usize, registration: usize) {
        self.construct_ai_module(id, registration);
        self.entities[id].aircraft = Some(super::State::new());
    }

    pub(in crate::skirmish) fn create_jet_module(&mut self, id: usize) {
        self.initialize_primary_ai(id);
        self.choose_aircraft_locomotor(id, false);
    }

    pub(in crate::skirmish) fn delete_jet_module(&mut self, id: usize) {
        if let Some(producer) = self.aircraft_producer(id) {
            self.release_parking_space(producer, id);
        }
    }

    pub(in crate::skirmish) fn destroy_jet_module(&mut self, id: usize) {
        self.entities[id]
            .aircraft
            .as_mut()
            .expect("Jet constructor")
            .lockon_drawable = None;
        self.exit_aircraft_phase(id);
    }
}
