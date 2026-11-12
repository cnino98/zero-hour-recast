//! Ground `OpenContain`/`TransportContain` and `AI_ENTER`/`AI_EXIT` at SH 2bfaea60.
use super::{Entity, Order, Skirmish};

mod audio;
pub mod bonuses;
mod capture;
mod commands;
mod exits;
mod firing;
mod garrison;
mod heal;
mod open;
mod payload;
mod placement;
mod riders;
mod world;
pub use super::tunnel::TunnelRules;
pub use audio::{Sound, SoundHook};
pub use garrison::GarrisonRules;
pub use heal::HealRules;
pub use payload::Payload;
#[cfg(test)]
pub(super) mod tests;

pub struct FirePoint {
    pub position: [f32; 3],
    /// Pristine logical bone basis, in native matrix rows.
    pub basis: [[f32; 3]; 3],
}

pub struct Art {
    pub exits: Vec<([f32; 3], [f32; 3])>,
    pub fire_points: Vec<FirePoint>,
}

pub struct RiderDeath {
    pub damage: f32,
    pub burned: bool,
    pub destroy_if_not_free: bool,
}

pub struct PassengerWeapons {
    pub pass_bonuses: bool,
    pub fire: bool,
    pub upgrade_transport: bool,
}

pub struct Relationships {
    pub allies: bool,
    pub enemies: bool,
    pub neutral: bool,
}

pub struct Rules {
    pub cave_index: Option<usize>,
    pub payload: Option<Payload>,
    pub payload_templates: Vec<String>,
    pub experience_sink_for_rider: bool,
    pub passengers_in_turret: bool,
    pub orient_like_container_on_exit: bool,
    pub assault_heal_ratio: Option<f32>,
    pub tunnel: Option<TunnelRules>,
    pub garrison: Option<GarrisonRules>,
    pub heal: Option<HealRules>,
    pub capacity: u32,
    pub allowed: Vec<String>,
    pub forbidden: Vec<String>,
    pub weapons: PassengerWeapons,
    pub relationships: Relationships,
    /// Bit 0: `GoAggressiveOnExit`; bit 1: `ResetMoodCheckTimeOnExit`.
    pub exit_flags: u8,
    pub exit_frames: u32,
    pub door_frames: u32,
    pub exit_paths: usize,
    pub regen_percent: f32,
    pub death: RiderDeath,
    pub damage_thresholds: [f32; 2],
    pub art: [Art; 3],
}

pub(super) enum GarrisonPlacement {
    None,
    Enclosed,
    Station,
}

pub(super) struct State {
    pub(super) tunnel: Option<super::tunnel::State>,
    pub(super) payload_created: bool,
    pub(super) nested_removed: bool,
    pub(super) cave: Option<super::tunnel::cave::State>,
    pub(super) carrier: Option<usize>,
    pub(super) extra_slots_in_use: i32,
    pub(super) exit_at: Option<usize>,
    pub(super) exit_entry_to_clear: Option<usize>,
    pub(super) passengers: Vec<usize>,
    pub(super) waiting: std::collections::BTreeSet<usize>,
    pub(super) height: f32,
    pub(super) exit_ready: u64,
    pub(super) next_exit: usize,
    pub(super) door_close_countdown: u32,
    pub(super) door_flags: u8,
    pub(super) player_entered_mask: u16,
    pub(super) drawable_conditions: std::collections::BTreeSet<String>,
    garrison: Option<Box<garrison::GarrisonState>>,
    pub(super) entered_at: u64,
    audio: audio::AudioState,
    pub(super) placement: GarrisonPlacement,
    firing: firing::State,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            tunnel: None,
            payload_created: false,
            nested_removed: false,
            cave: None,
            carrier: None,
            extra_slots_in_use: 0,
            exit_at: None,
            exit_entry_to_clear: None,
            passengers: Vec::new(),
            waiting: std::collections::BTreeSet::new(),
            height: 0.0,
            exit_ready: 0,
            next_exit: 0,
            door_close_countdown: 0,
            door_flags: 0,
            player_entered_mask: 0,
            drawable_conditions: std::collections::BTreeSet::new(),
            garrison: None,
            entered_at: 0,
            audio: audio::AudioState {
                enabled: true,
                frames: [0; 2],
            },
            placement: GarrisonPlacement::None,
            firing: firing::State::new(),
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn contained_by(&self) -> Option<usize> {
        self.containment.carrier
    }

    #[must_use]
    pub const fn loaded(&self) -> bool {
        !self.containment.passengers.is_empty()
    }

    #[must_use]
    pub const fn garrisoned(&self) -> bool {
        !matches!(self.containment.placement, GarrisonPlacement::None)
    }

    #[must_use]
    pub const fn stationed(&self) -> bool {
        matches!(self.containment.placement, GarrisonPlacement::Station)
    }

    #[must_use]
    pub const fn container_door_open(&self, _frame: u64) -> bool {
        self.containment.door_flags & 1 != 0
    }

    pub(in crate::skirmish) fn container_conditions(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.containment.door_flags & 1 != 0 {
            flags.push("DOOR_1_OPENING");
        }
        if self.containment.door_flags & 2 != 0 {
            flags.push("DOOR_1_CLOSING");
        }
        flags
    }
}

impl Skirmish {
    #[must_use]
    pub fn passengers(&self, carrier: usize) -> &[usize] {
        let carrier = self.redirected_container(carrier);
        if self.is_tunnel(carrier) {
            return &self.tunnel_network(carrier).passengers;
        }
        self.entities
            .get(carrier)
            .map_or(&[], |actor| &actor.containment.passengers)
    }

    #[must_use]
    pub fn can_enter(&self, id: usize, carrier: usize) -> bool {
        self.unmanned_admits(id, carrier)
            || self.aircraft_can_enter(id, carrier)
            || (self.heal_command_admits(id, carrier) && self.container_admits(id, carrier, true))
            || self.car_bomb_accepts(id, carrier)
            || self.hijacker_accepts(id, carrier)
            || self.can_sabotage(id, carrier)
            || self.veterancy_crate_enterable(id, carrier)
    }

    pub(in crate::skirmish) fn container_admits(
        &self,
        id: usize,
        carrier: usize,
        capacity: bool,
    ) -> bool {
        let (Some(rider), Some(host)) = (self.entities.get(id), self.entities.get(carrier)) else {
            return false;
        };
        let unit = &self.catalog[rider.type_index];
        let Some(rules) = self.container_rules(carrier) else {
            return false;
        };
        if id == carrier
            || !rider.complete()
            || !host.complete()
            || rider.masked()
            || rider.contained_by().is_some()
            || host.masked()
            || host.subdued()
            || !self.container_relationship(id, carrier, rules)
            || unit.speed <= 0.0
            || unit.kind.is_building()
            || (rules.garrison.is_none()
                && rules.heal.is_none()
                && unit.transport_slots == 0
                && (rules.tunnel.is_none() || capacity))
            || unit.kind_of.iter().any(|kind| {
                matches!(kind.as_str(), "IMMOBILE" | "IGNORED_IN_GUI" | "MOB_NEXUS")
                    || (rules.tunnel.is_none() && rules.forbidden.contains(kind))
            })
            || (rules.tunnel.is_none()
                && !((rules.heal.is_some()
                    || self.catalog[host.type_index].contain_family() == Some("OpenContain"))
                    && rules.allowed.is_empty())
                && !unit.kind_of.iter().any(|kind| rules.allowed.contains(kind)))
        {
            return false;
        }
        if rules.heal.is_some() {
            return true;
        }
        if rules.tunnel.is_some() {
            return !unit.kind_of.iter().any(|kind| kind == "AIRCRAFT")
                && (!capacity || self.passengers(carrier).len() < rules.capacity as usize);
        }
        if rules.garrison.is_some() {
            return !unit.kind_of.iter().any(|kind| kind == "NO_GARRISON")
                && self.garrison_damage_allows_entry(carrier)
                && (!capacity
                    || self.passengers(carrier).len()
                        < usize::try_from(rules.capacity).unwrap_or(usize::MAX));
        }
        self.native_container_admits(id, carrier, capacity)
    }

    /// Orders the rider to approach a source-compatible ground transport.
    /// # Errors
    /// Rejects dead, held, incompatible, foreign or full containers and riders.
    pub fn enter(&mut self, id: usize, carrier: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        // Player::applyCommandToList checks the last entrant before dispatching AI_ENTER.
        if let Some(host) = self.entities.get(carrier) {
            let player = 1_u16 << self.entities[id].owner;
            if host.containment.player_entered_mask != 0
                && host.containment.player_entered_mask != player
            {
                return Err("Another player entered this container during the current update");
            }
        }
        if self.entities[id].aircraft.is_some()
            && self
                .entities
                .get(carrier)
                .is_some_and(|actor| self.catalog[actor.type_index].parking.is_some())
        {
            return self.land_aircraft(id, carrier);
        }
        if !self.can_enter(id, carrier) {
            return Err("This unit cannot enter that transport");
        }
        self.dozer_player_command(id);
        self.set_last_command_source(id, super::ai_goals::COMMAND_FROM_PLAYER);
        self.assault_command(id, Order::Enter(carrier));
        self.begin_container_enter(id, carrier);
        self.finish_dozer_player_command(id);
        Ok(())
    }

    pub(super) fn begin_container_enter(&mut self, id: usize, carrier: usize) {
        let source = self.entities[id].ai.command.last_command_source;
        self.native_enter_from(id, carrier, source);
    }

    pub(in crate::skirmish) fn vertical_overlap(&self, left: usize, right: usize) -> bool {
        let low = self.object_position(left)[2];
        let other = self.object_position(right)[2];
        low + self.catalog[self.entities[left].type_index]
            .geometry_bounds
            .max_height
            >= other
            && low
                <= other
                    + self.catalog[self.entities[right].type_index]
                        .geometry_bounds
                        .max_height
    }

    pub(super) fn container_contact(&mut self, id: usize, carrier: usize) -> bool {
        if self.container_rules(carrier).is_none()
            || self.entities[id].ai.registration.is_none()
            || !matches!(self.entities[id].ai.order, Order::Enter(target) if target == carrier)
        {
            return false;
        }
        let owner = self.entities[id].owner;
        for passenger in self.passengers(carrier).to_vec() {
            if self.entities[passenger].owner != owner {
                self.set_order(passenger, Order::ExitContainer);
            }
        }
        if !self.container_admits(id, carrier, true) {
            return false;
        }
        self.board(id, carrier);
        true
    }

    pub(super) fn board(&mut self, id: usize, carrier: usize) {
        // Recheck after movement/contact: source fixes reject destroyed riders
        // and containers, and never attach a rider already held elsewhere.
        if !self.container_admits(id, carrier, true) {
            return;
        }
        self.add_to_contain(id, carrier);
    }

    /// Native `Contain::addToContain`; callers decide whether to test admission.
    pub(in crate::skirmish) fn add_to_contain(&mut self, id: usize, carrier: usize) {
        let unit = &self.catalog[self.entities[carrier].type_index];
        if unit.is_helix_container()
            && self.entities[carrier].attachment_child().is_none()
            && self.catalog[self.entities[id].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "PORTABLE_STRUCTURE")
        {
            self.link_helix_portable(carrier, id);
            return;
        }
        if !self.entities[carrier].exists()
            || !self.entities[id].exists()
            || self.update_objects[self.entities[carrier].update_owner].destroy_requested
            || self.update_objects[self.entities[id].update_owner].destroy_requested
            || self.entities[id].contained_by().is_some()
        {
            return;
        }
        self.board_admitted(id, carrier);
    }

    pub(super) fn board_parachute(&mut self, id: usize, carrier: usize) {
        self.add_to_contain(id, carrier);
    }

    fn board_admitted(&mut self, id: usize, carrier: usize) {
        let carrier = self.redirected_container(carrier);
        if self.is_tunnel(carrier) {
            self.tunnel_network_mut(carrier).passengers.push(id);
        } else {
            // OpenContain::addToContainList appends. The reverse deployment
            // loop's older comment about prepending does not match the code.
            self.entities[carrier].containment.passengers.push(id);
        }
        self.entities[carrier].containment.player_entered_mask = 1_u16 << self.entities[id].owner;
        if self.container_encloses(carrier, id) {
            self.containment_world_membership(id, false);
        }
        self.container_enter_callback(carrier);
        if !matches!(
            self.catalog[self.entities[carrier].type_index].contain_family(),
            Some("OpenContain" | "HealContain")
        ) {
            self.set_disabled_held(id);
        }
        self.capture_cave_network(carrier, id);
        if matches!(
            self.catalog[self.entities[carrier].type_index].contain_family(),
            Some("TransportContain" | "InternetHackContain" | "HelixContain" | "OverlordContain")
        ) {
            let slots = i32::from_ne_bytes(
                self.catalog[self.entities[id].type_index]
                    .transport_slots
                    .to_ne_bytes(),
            );
            self.entities[carrier].containment.extra_slots_in_use = self.entities[carrier]
                .containment
                .extra_slots_in_use
                .wrapping_add(slots.wrapping_sub(1));
        }
        if self.entities[carrier].containment.tunnel.is_some() {
            let owner = self.entities[id].owner;
            self.players[owner].academy.units_entered_tunnel = self.players[owner]
                .academy
                .units_entered_tunnel
                .wrapping_add(1);
            self.observe_ground_partition(id);
        }
        self.garrison_board(id, carrier);
        self.entities[id].helix.passenger_bonus =
            self.catalog[self.entities[carrier].type_index].is_helix_container();
        self.update_armed_rider_weapons(carrier);
        self.overlord_on_containing(carrier, id);
        if self.catalog[self.entities[carrier].type_index].contain_family()
            == Some("InternetHackContain")
        {
            self.start_contained_hacking(id);
        }
        if self.entities[carrier].parachute.is_some() {
            self.parachute_on_containing(carrier, id);
        }
        self.deploy_passengers(carrier);
        // Object::onContainedBy stores these status bits independently of the
        // pointer and of later onRemoving changes to rider/container state.
        self.entities[id].maintenance.instance_flags |= super::maintenance::INSTANCE_UNSELECTABLE;
        self.entities[id].maintenance.masked = self.container_encloses(carrier, id);
        // onContainedBy preserves the already redeployed XYZ across the Z-cache switch.
        let height = self.object_position(id)[2];
        self.entities[id].containment.height = height;
        self.entities[id].containment.carrier = Some(carrier);
        self.entities[id].containment.entered_at = self.tick;
        self.update_position_claim(id);
        self.observe_ground_partition(id);
        self.queue_container_sound(carrier, true);
    }

    #[must_use]
    pub fn passenger_can_fire(&self, id: usize) -> bool {
        self.entities
            .get(id)
            .and_then(Entity::contained_by)
            .is_some_and(|carrier| self.container_fire_allowed(carrier, Some(id)))
    }

    pub(super) fn update_contained(&mut self, id: usize) {
        if let Order::Enter(host) = self.entities[id].ai.order {
            // AI_ENTER still updates after a collision boards the passenger.
            self.update_enter(id, host);
        } else if matches!(self.entities[id].ai.order, Order::ExitContainer) {
            self.update_container_exit(id);
        } else if self.passenger_can_fire(id) {
            match self.entities[id].ai.order {
                Order::Idle => {
                    self.acquire_target(id);
                }
                Order::Attack(target) => {
                    if !self.update_attack(id, target) {
                        self.set_order(id, Order::Idle);
                    }
                }
                Order::FireAt { .. } => self.update_fire_position(id),
                _ => self.set_order(id, Order::Idle),
            }
        }
    }

    pub(super) fn container_notifications(&mut self, id: usize, order: Order) {
        if matches!(self.entities[id].ai.order, Order::Enter(_)) {
            self.entities[id]
                .ai
                .motion
                .set_allow_invalid_position(false);
        }
        let carrier = match self.entities[id].ai.order {
            Order::Enter(carrier) => Some(carrier),
            // AIExitState::onExit uses its saved entry, not the replacement goal.
            Order::ExitContainer => self.entities[id].containment.exit_entry_to_clear.take(),
            _ => None,
        };
        if let Some(carrier) = carrier {
            self.entities[carrier].containment.waiting.remove(&id);
        }
        if !matches!(order, Order::ExitContainer) {
            self.entities[id].containment.exit_at = None;
        }
        let carrier = match order {
            Order::Enter(carrier) if self.can_enter(id, carrier) => Some(carrier),
            Order::ExitContainer => self.container_exit_target(id),
            _ => None,
        };
        if let Some(carrier) = carrier {
            self.entities[carrier].containment.waiting.insert(id);
            if matches!(order, Order::ExitContainer) {
                self.entities[id].containment.exit_entry_to_clear = Some(carrier);
            }
        }
    }

    fn art(&self, carrier: usize) -> Option<&Art> {
        let actor = &self.entities[carrier];
        let rules = self.catalog[actor.type_index].contain.as_ref()?;
        let ratio = actor.health / actor.max_health;
        let index = if ratio <= rules.damage_thresholds[1] {
            2
        } else {
            usize::from(ratio <= rules.damage_thresholds[0])
        };
        Some(&rules.art[index])
    }

    fn deploy_passengers(&mut self, carrier: usize) {
        if self.entities[carrier].parachute.is_some() {
            for rider in self.passengers(carrier).to_vec() {
                self.position_parachute_rider(rider, carrier);
            }
            return;
        }
        if self.deploy_helix_passengers(carrier) {
            return;
        }
        if self.station_garrison(carrier) {
            if let Some(state) = &mut self.entities[carrier].containment.garrison {
                state.stations.clear();
                state.points.fill(None);
            }
            self.position_station_passengers(carrier);
            return;
        }
        if self.is_garrison(carrier) || self.is_tunnel(carrier) {
            return;
        }
        self.deploy_firepoint_passengers(carrier);
    }

    fn container_point(&self, carrier: usize, local: [f32; 3]) -> [f32; 3] {
        let (position, basis) = self.object_transform(carrier);
        std::array::from_fn(|axis| {
            let x = basis[axis][0] * local[0];
            let y = basis[axis][1] * local[1];
            let z = basis[axis][2] * local[2];
            ((x + y) + z) + position[axis]
        })
    }
}

pub(super) fn validate(catalog: &[super::UnitType]) -> Result<(), &'static str> {
    for unit in catalog {
        let Some(rules) = &unit.contain else { continue };
        if rules.passengers_in_turret
            && rules.art.iter().any(|art| !art.fire_points.is_empty())
            && (unit.turret.is_none() || matches!(unit.ai.update, super::UnitAiUpdate::Absent))
        {
            return Err(
                "Turret firepoints require actual AI and MAIN turret; native query otherwise has undefined output",
            );
        }
        if rules
            .payload
            .as_ref()
            .is_some_and(|payload| !catalog.iter().any(|unit| unit.name == payload.template))
        {
            return Err("Missing transport initial payload template");
        }
        if rules
            .assault_heal_ratio
            .is_some_and(|ratio| !ratio.is_finite())
        {
            return Err("Invalid assault transport healing threshold");
        }
    }
    Ok(())
}
