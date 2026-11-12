//! Pinned `HordeUpdate` and `AIUpdateInterface::evaluateNationalismBonus*`.
use super::{Entity, Skirmish, TerrainDecal, UnitType};

pub enum Action {
    Classic,
    Fixed,
}

pub struct Rules {
    pub update_frames: u32,
    pub count: i32,
    pub radius: f32,
    pub rub_off_radius: f32,
    pub kind_of: Vec<String>,
    pub allies_only: bool,
    pub exact_match: bool,
    pub allowed_nationalism: bool,
    pub action: Action,
}

pub(super) struct State {
    last_refresh: u64,
    in_horde: bool,
    true_member: bool,
    bonuses: u8,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            last_refresh: 0,
            in_horde: false,
            true_member: false,
            bonuses: 0,
        }
    }
}

impl Entity {
    #[must_use]
    pub const fn in_horde(&self) -> bool {
        self.horde.in_horde
    }

    #[must_use]
    pub const fn true_horde_member(&self) -> bool {
        self.horde.in_horde && self.horde.true_member
    }

    #[must_use]
    pub const fn horde_bonuses(&self) -> u8 {
        self.horde.bonuses
    }
}

pub(super) fn validate(unit: &UnitType) -> Result<(), &'static str> {
    let Some(rules) = &unit.behavior.horde else {
        return Ok(());
    };
    if rules.update_frames == 0
        || !rules.radius.is_finite()
        || rules.radius < 0.0
        || !rules.rub_off_radius.is_finite()
        || rules.rub_off_radius < 0.0
    {
        return Err("Invalid HordeUpdate interval, radius or weapon bonus");
    }
    Ok(())
}

impl Skirmish {
    pub(super) fn construct_horde_update(&mut self, id: usize, registration: usize) {
        let delay = self.catalog[self.entities[id].type_index]
            .behavior
            .horde
            .as_ref()
            .expect("bound HordeUpdate")
            .update_frames;
        self.entities[id].horde.last_refresh = u64::from(self.update_frame());
        let sleep = self.random_update_phase(delay);
        self.wake_update(id, registration, sleep);
    }

    pub(super) fn update_horde_module(&mut self, id: usize) -> i32 {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let rules = unit.behavior.horde.as_ref().expect("bound HordeUpdate");
        let infantry = unit.kind_of.iter().any(|flag| flag == "INFANTRY");
        let interval = rules.update_frames;
        let now = self.update_frame();
        let refreshed = u32::try_from(actor.horde.last_refresh).expect("native frame");
        if infantry || now > refreshed.wrapping_add(interval) {
            self.refresh_horde(id);
        }
        let actor = &mut self.entities[id];
        if actor.drawable.present && !actor.is_effectively_dead() {
            if !self.draw_icon_ui {
                actor.drawable.terrain_decal = TerrainDecal::None;
            } else if actor.in_horde()
                && !self.catalog[actor.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "PORTABLE_STRUCTURE")
            {
                actor.drawable.terrain_decal = if actor.horde.bonuses & 2 != 0 {
                    if actor.horde.bonuses & 4 != 0 {
                        TerrainDecal::HordeFanaticism
                    } else {
                        TerrainDecal::HordeNationalism
                    }
                } else if infantry {
                    TerrainDecal::Horde
                } else {
                    TerrainDecal::HordeVehicle
                };
            }
        }
        if infantry {
            i32::from_ne_bytes(interval.to_ne_bytes())
        } else {
            1
        }
    }

    fn refresh_horde(&mut self, id: usize) {
        let extra = self.passenger_bonus_flags(id);
        let actor = &self.entities[id];
        let rules = self.catalog[actor.type_index]
            .behavior
            .horde
            .as_ref()
            .expect("HordeUpdate");
        let mut count = 0_i64;
        let mut inherited = false;
        let rub_off_squared = rules.rub_off_radius * rules.rub_off_radius;
        for other in self.partition.range_objects(actor.position, rules.radius) {
            if !self.horde_candidate(id, other, rules) {
                continue;
            }
            count += 1;
            let delta = [
                self.entities[other].position[0] - actor.position[0],
                self.entities[other].position[1] - actor.position[1],
            ];
            let x_squared = delta[0] * delta[0];
            let y_squared = delta[1] * delta[1];
            if self.entities[other].true_horde_member() && x_squared + y_squared <= rub_off_squared
            {
                inherited = true;
            }
        }
        let true_member = count >= i64::from(rules.count) - 1;
        let in_horde = true_member || inherited;
        let nationalism =
            rules.allowed_nationalism && self.has_upgrade(actor.owner, "Upgrade_Nationalism");
        let fanaticism =
            rules.allowed_nationalism && self.has_upgrade(actor.owner, "Upgrade_Fanaticism");
        let bonuses = morale(
            rules,
            actor.horde.bonuses,
            in_horde,
            nationalism,
            fanaticism,
        );
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        actor.horde.in_horde = in_horde;
        actor.horde.true_member = true_member;
        actor.horde.last_refresh = u64::from(now);
        // Object::set/clearWeaponBonusCondition invokes every allocated weapon
        // at each actual edge, in the source HORDE/NATIONALISM/FANATICISM order.
        for bit in [1, 2, 4] {
            if actor.horde.bonuses & bit != bonuses & bit {
                actor.horde.bonuses = (actor.horde.bonuses & !bit) | (bonuses & bit);
                actor.weapon_bonus_changed(&self.catalog[actor.type_index], self.tick, extra);
            }
        }
    }

    fn horde_candidate(&self, id: usize, other: usize, rules: &Rules) -> bool {
        let actor = &self.entities[id];
        let candidate = &self.entities[other];
        let unit = &self.catalog[candidate.type_index];
        if id == other
            || !candidate.alive()
            || candidate.contained_by().is_some()
            || unit.behavior.horde.is_none()
            || (rules.exact_match && actor.type_index != candidate.type_index)
            || !rules.kind_of.iter().all(|kind| unit.kind_of.contains(kind))
            || (rules.allies_only && !self.allied(actor.owner, candidate.owner))
            || self.cell_at(actor.position).is_some() != self.cell_at(candidate.position).is_some()
        {
            return false;
        }
        let a = self.object_position(id);
        let b = self.object_position(other);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let dz = (b[2] + unit.geometry_bounds.center_height)
            - (a[2] + self.catalog[actor.type_index].geometry_bounds.center_height);
        let x_squared = dx * dx;
        let y_squared = dy * dy;
        let z_squared = dz * dz;
        let distance = (x_squared + y_squared + z_squared).sqrt();
        let radii = unit.geometry_bounds.sphere_radius
            + self.catalog[actor.type_index].geometry_bounds.sphere_radius;
        let separation = (distance - radii).max(0.0);
        separation * separation < rules.radius * rules.radius
    }
}

fn morale(rules: &Rules, previous: u8, in_horde: bool, nationalism: bool, fanaticism: bool) -> u8 {
    let mut flags = (previous & !1) | u8::from(in_horde);
    match rules.action {
        Action::Classic => {
            if nationalism {
                flags |= 2;
                flags = (flags & !4) | (u8::from(fanaticism) << 2);
            } else {
                // The classic source intentionally leaves Fanaticism untouched here.
                flags &= !2;
            }
        }
        Action::Fixed => {
            if in_horde {
                if nationalism {
                    flags |= 2;
                }
                if fanaticism {
                    flags |= 4;
                }
            } else {
                flags = 0;
            }
        }
    }
    flags
}

#[cfg(test)]
mod tests;
