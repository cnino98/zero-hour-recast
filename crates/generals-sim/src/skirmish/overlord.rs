//! `ObjectCreationUpgrade` and non-enclosing `OverlordContain`, SH 2bfaea60.
use super::{Entity, Skirmish};

mod deletion;
mod propaganda;
pub use propaganda::{Propaganda, Pulse};
#[cfg(test)]
mod tests;

pub struct Upgrade {
    pub trigger: String,
    pub conflicts: Vec<String>,
    pub object: String,
    pub command_set: String,
}

pub(super) fn validate(
    unit: &super::UnitType,
    catalog: &[super::UnitType],
) -> Result<(), &'static str> {
    if unit.attachment_point.iter().any(|value| !value.is_finite())
        || unit
            .attachment_thresholds
            .iter()
            .any(|value| !(0.0..=1.0).contains(value))
        || unit.attachment_thresholds[0] < unit.attachment_thresholds[1]
    {
        return Err("Invalid attachment coordinates or damage thresholds");
    }
    for upgrade in &unit.behavior.attachments {
        let child = catalog
            .iter()
            .find(|candidate| candidate.name == upgrade.object)
            .ok_or("Missing attachment template")?;
        if child.attachment_bone.as_ref().is_none_or(String::is_empty)
            || !child
                .kind_of
                .iter()
                .any(|kind| kind == "PORTABLE_STRUCTURE")
            || !child.behavior.attachments.is_empty()
            || upgrade.command_set.is_empty()
            || !unit
                .maintenance
                .research
                .iter()
                .any(|research| research.name == upgrade.trigger && !research.player)
        {
            return Err("Invalid attachment object or research binding");
        }
    }
    if let Some(rules) = &unit.behavior.propaganda
        && (!rules.radius.is_finite()
            || rules.radius <= 0.0
            || rules.delay == 0
            || rules
                .heal
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            || rules.upgrade.is_empty()
            || rules.pulse.iter().any(String::is_empty))
    {
        return Err("Invalid propaganda source rules");
    }
    Ok(())
}

pub(super) struct State {
    pub(super) parent: Option<usize>,
    child: Option<usize>,
    redirection_active: bool,
    damage_category: u8,
    pub(super) influence: Vec<usize>,
    pub(super) scanned: u64,
    pub(super) bonuses: u8,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            parent: None,
            child: None,
            redirection_active: false,
            damage_category: 0,
            influence: Vec::new(),
            scanned: 0,
            bonuses: 0,
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn attachment_parent(&self) -> Option<usize> {
        self.attachment.parent
    }

    #[must_use]
    pub const fn attachment_height(&self) -> Option<f32> {
        if self.attachment.parent.is_some() {
            Some(self.containment.height)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn attachment_child(&self) -> Option<usize> {
        self.attachment.child
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn notify_containment_body_state(
        &mut self,
        id: usize,
        previous: usize,
    ) {
        if self.body_damage_state(id) == previous {
            return;
        }
        let kind = self.entities[id].type_index;
        for index in 0..self.catalog[kind].behavior_modules.len() {
            let module = &self.catalog[kind].behavior_modules[index];
            if module.interfaces & 4 == 0 {
                continue;
            }
            let state = self.body_damage_state(id);
            let ordinal = self.catalog[kind].behavior_modules[..index]
                .iter()
                .filter(|prior| prior.family == module.family)
                .count();
            match module.family.as_str() {
                "BridgeBehavior" => self.bridge_body_state_changed(id, ordinal, previous, state),
                "BoneFXDamage" => self.bone_body_changed(id, state),
                "GarrisonContain" => self.garrison_body_state_changed(id, state),
                "SupplyWarehouseCripplingBehavior" => {
                    self.supply_crippling_body_state(id, previous, state);
                }
                "HelixContain" if state != 3 => {
                    if let Some(child) = self.entities[id]
                        .attachment_child()
                        .filter(|&child| self.entities[child].exists())
                    {
                        self.set_body_damage_state(child, state);
                    }
                }
                "OverlordContain"
                    if state != 3 && self.entities[id].containment.passengers.len() == 1 =>
                {
                    let child = self.entities[id].containment.passengers[0];
                    self.set_body_damage_state(child, state);
                }
                _ => {}
            }
        }
    }

    pub(in crate::skirmish) fn attachment_research_available(&self, id: usize, name: &str) -> bool {
        let actor = &self.entities[id];
        self.catalog[actor.type_index]
            .behavior
            .attachments
            .iter()
            .find(|upgrade| upgrade.trigger == name)
            .is_none_or(|upgrade| {
                upgrade
                    .conflicts
                    .iter()
                    .all(|name| !self.object_has_upgrade(id, name))
            })
    }

    pub(in crate::skirmish) fn link_helix_portable(&mut self, id: usize, child: usize) {
        // HelixContain::addToContain's first portable bypasses OpenContain's
        // list, onContaining, redeployment and onContainedBy callbacks.
        self.entities[id].attachment.child = Some(child);
        self.entities[child].attachment.parent = Some(id);
        self.entities[child].containment.carrier = Some(id);
        self.entities[child].containment.entered_at = self.tick;
    }

    pub(in crate::skirmish) fn update_attachments(&mut self) {
        for parent in 0..self.entities.len() {
            if let Some(child) = self.entities[parent].attachment.child
                && self.entities[parent].alive()
                && self.entities[child].alive()
            {
                self.update_attachment(parent, child);
            }
        }
    }

    fn update_attachment(&mut self, parent: usize, child: usize) {
        let host = &self.entities[parent];
        let unit = &self.catalog[host.type_index];
        if unit.is_helix_container() {
            // HelixContain::update uses the host's logical transform. The
            // animated CHASSISBONE is a separate drawable dependency.
            let position = self.object_position(parent);
            let facing = host.facing;
            let owner = host.owner;
            let rider = &mut self.entities[child];
            rider.position = [position[0], position[1]];
            rider.containment.height = position[2];
            self.set_object_orientation(child, facing);
            self.entities[child].owner = owner;
            health_changed(&mut self.entities, &self.catalog, parent);
            return;
        }
        health_changed(&mut self.entities, &self.catalog, parent);
    }

    #[must_use]
    pub fn redirected_container(&self, id: usize) -> usize {
        let Some(actor) = self.entities.get(id) else {
            return id;
        };
        if self.catalog[actor.type_index].is_helix_container() {
            return id;
        }
        if self.catalog[actor.type_index].contain_family() == Some("OverlordContain") {
            return actor
                .containment
                .passengers
                .first()
                .copied()
                .filter(|&rider| {
                    actor.attachment.redirection_active
                        && self.catalog[self.entities[rider].type_index]
                            .contain
                            .is_some()
                })
                .unwrap_or(id);
        }
        actor
            .attachment
            .child
            .filter(|&rider| {
                self.catalog[self.entities[rider].type_index]
                    .contain
                    .is_some()
            })
            .unwrap_or(id)
    }

    pub(in crate::skirmish) fn overlord_on_containing(&mut self, carrier: usize, rider: usize) {
        if self.catalog[self.entities[carrier].type_index].contain_family()
            != Some("OverlordContain")
        {
            return;
        }
        if self.entities[carrier].containment.passengers.first() == Some(&rider) {
            self.entities[carrier].attachment.child = Some(rider);
            self.entities[rider].attachment.parent = Some(carrier);
        }
        if self.catalog[self.entities[rider].type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "PORTABLE_STRUCTURE")
        {
            self.entities[carrier].attachment.redirection_active = true;
            if self.entities[carrier].stealthed() {
                self.grant_stealth(rider);
            }
            if self.catalog[self.entities[carrier].type_index]
                .contain
                .as_ref()
                .expect("Overlord data")
                .experience_sink_for_rider
            {
                self.entities[rider].set_experience_sink(Some(carrier));
            }
        }
    }

    #[must_use]
    pub fn container_rules(&self, id: usize) -> Option<&super::containment::Rules> {
        let id = self.redirected_container(id);
        self.catalog[self.entities.get(id)?.type_index]
            .contain
            .as_ref()
    }

    pub(in crate::skirmish) fn attachment_death(&mut self, parent: usize) {
        if let Some(child) = self.entities[parent].attachment.child.take()
            && self.entities[child].alive()
        {
            // OverlordContain disables redirection, kills its portable object,
            // then runs its own containment teardown.
            self.entities[child].containment.carrier = None;
            self.entities[child].health = 0.0;
            if self.catalog[self.entities[child].type_index]
                .body_kind
                .is_active()
            {
                self.entities[child].refresh_body_health();
            }
            self.finish_death(child, None);
        }
        self.remove_propaganda(parent);
    }
}

/// Native body category callback, invoked before later health-dependent work.
pub(in crate::skirmish) fn health_changed(
    entities: &mut [Entity],
    catalog: &[super::UnitType],
    parent: usize,
) {
    let host = &entities[parent];
    // Source-bound containers receive their authored Body edge callback instead.
    if matches!(
        catalog[host.type_index].contain_family(),
        Some("HelixContain" | "OverlordContain")
    ) {
        return;
    }
    let Some(child) = host.attachment.child else {
        return;
    };
    if !host.alive() {
        return; // OverlordContain delegates RUBBLE to its separate death callback.
    }
    let thresholds = catalog[host.type_index].attachment_thresholds;
    let category = damage_category(host, thresholds);
    if host.attachment.damage_category == category {
        return;
    }
    entities[parent].attachment.damage_category = category;
    let child = &mut entities[child];
    let ratio = [1.0, thresholds[0], thresholds[1]][usize::from(category)];
    let desired = child.max_health * ratio;
    child.health = (desired - 1.0).max(0.0);
    if catalog[child.type_index].body_kind.is_active() {
        child.refresh_body_health();
    }
}

fn damage_category(actor: &Entity, thresholds: [f32; 2]) -> u8 {
    let ratio = actor.health / actor.max_health;
    if ratio < thresholds[1] {
        2
    } else {
        u8::from(ratio < thresholds[0])
    }
}
