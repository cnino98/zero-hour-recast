//! `ActiveBody.cpp:573–603`: retained damage prefers real combat sources during
//! the current/preceding frame; clearing its attacker never erases its source.
use crate::skirmish::{DamageType, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn apply_healing(
        &mut self,
        target: usize,
        source: Option<usize>,
        raw_amount: f32,
    ) {
        let actor = &self.entities[target];
        let unit = &self.catalog[actor.type_index];
        let bridge = unit
            .kind_of
            .iter()
            .any(|kind| matches!(kind.as_str(), "BRIDGE" | "BRIDGE_TOWER"));
        if matches!(unit.body_kind, crate::skirmish::BodyKind::Inactive)
            || actor.is_effectively_dead() && !bridge
        {
            return;
        }
        let fx_set = self.body_damage_fx_set(target);
        // attemptHealing uses ArmorTemplate alone, without the damage scalar.
        let amount = actor.armor_damage(unit, DamageType::Healing, raw_amount);
        if amount.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            self.do_body_damage_fx(target, fx_set, 0.0, DamageType::Healing, source);
            return;
        }
        let old_state = self.body_damage_state(target);
        let old_health = actor.health;
        let increased = old_health + amount;
        let capped = if increased > actor.max_health {
            actor.max_health
        } else {
            increased
        };
        self.entities[target].damage_state.previous_health = old_health;
        self.entities[target].health = if capped < 0.0 { 0.0 } else { capped };
        if self.catalog[self.entities[target].type_index]
            .body_kind
            .is_active()
        {
            self.entities[target].refresh_body_health();
        }
        self.body_particle_state_changed(target, old_state);
        self.record_body_healing(target, source, DamageType::Healing);
        // onHealing callbacks run only if the clamped health actually rose.
        if self.entities[target].health > old_health {
            self.notify_body_healing(target, source);
        }
        self.notify_containment_body_state(target, old_state);
        self.structure_damage_transition(target, old_health);
        self.do_body_damage_fx(target, fx_set, amount, DamageType::Healing, source);
    }

    pub(in crate::skirmish) fn record_body_healing(
        &mut self,
        target: usize,
        source: Option<usize>,
        damage_type: DamageType,
    ) {
        // ActiveBody.cpp:844; CRC=0 and PRESERVE_STRUCTURE_STEALTH_DURING_REPAIR=0.
        // Healing replaces the record without changing its damage timestamp.
        let damage = &mut self.entities[target].damage_state;
        damage.current_source = source;
        damage.retained_source = source;
        damage.last_attacker = source;
        damage.damage_type = damage_type;
    }

    pub(in crate::skirmish) fn record_body_damage(
        &mut self,
        target: usize,
        source: Option<usize>,
        damage_type: DamageType,
    ) {
        // The native logic clock and invalid timestamp are both unsigned 32-bit.
        let frame = u32::try_from(self.tick & u64::from(u32::MAX)).expect("native frame");
        let damage = &self.entities[target].damage_state;
        let recent =
            damage.retained_frame == frame || damage.retained_frame == frame.wrapping_sub(1);
        let object = |id: Option<usize>| {
            id.and_then(|id| self.entities.get(id))
                .filter(|actor| actor.exists())
        };
        let replace = !recent
            || object(source).is_some_and(|new| {
                object(damage.retained_source).is_none()
                    || self.catalog[new.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| preferred_kind(kind))
            });
        let damage = &mut self.entities[target].damage_state;
        // onDie receives this hit, independently of the body's retained record.
        damage.current_source = source;
        if replace {
            damage.retained_source = source;
            damage.last_attacker = source;
            damage.retained_frame = frame;
            damage.damage_type = damage_type;
        }
    }
}

fn preferred_kind(kind: &str) -> bool {
    // Object::isFactionStructure is KINDOFMASK_FS, not STRUCTURE or a faction.
    // GeneralsMD Source/Common/System/KindOf.cpp:171 enumerates these 14 bits.
    matches!(
        kind,
        "VEHICLE"
            | "INFANTRY"
            | "FS_FACTORY"
            | "FS_BASE_DEFENSE"
            | "FS_TECHNOLOGY"
            | "FS_SUPPLY_DROPZONE"
            | "FS_SUPERWEAPON"
            | "FS_BLACK_MARKET"
            | "FS_SUPPLY_CENTER"
            | "FS_STRATEGY_CENTER"
            | "FS_FAKE"
            | "FS_INTERNET_CENTER"
            | "FS_ADVANCED_TECH"
            | "FS_BARRACKS"
            | "FS_WARFACTORY"
            | "FS_AIRFIELD"
    )
}

#[cfg(test)]
mod tests;
