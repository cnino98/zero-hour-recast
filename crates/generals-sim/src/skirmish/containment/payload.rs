//! `TransportContain::createPayload`, including the fixed destroyed-host guard.
use super::Skirmish;

pub struct Payload {
    pub template: String,
    pub count: i32,
}

impl Skirmish {
    pub(in crate::skirmish) fn create_transport_payload(&mut self, carrier: usize) {
        if self.entities[carrier].containment.payload_created
            || self.update_objects[self.entities[carrier].update_owner].destroy_requested
        {
            return;
        }
        let actor = &self.entities[carrier];
        let Some(payload) = self.catalog[actor.type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.payload.as_ref())
        else {
            self.entities[carrier].containment.payload_created = true;
            return;
        };
        let count = payload.count;
        let type_index = self
            .catalog
            .iter()
            .position(|unit| unit.name == payload.template)
            .expect("validated transport payload template");
        let owner = actor.owner;
        self.entities[carrier].containment.audio.enabled = false;
        for _ in 0..count {
            // ThingFactory::newObject runs create callbacks; it does not call
            // ProductionUpdate's unit-built scoring callback.
            let id = self.new_object(type_index, owner, true);
            if self.native_container_admits(id, carrier, true) {
                self.add_to_contain(id, carrier);
            }
        }
        self.entities[carrier].containment.audio.enabled = true;
        self.entities[carrier].containment.payload_created = true;
    }
}
