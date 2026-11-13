//! `doTeamUseCommandButtonOnNearestObjectType`, including the CRC-off empty result guard.
use super::{Command, Skirmish};
use crate::skirmish::{ai_goals::COMMAND_FROM_SCRIPT, movement::distance_squared};

const REALLY_FAR: f32 = 1_000_000.0;

impl Skirmish {
    pub(in crate::skirmish) fn team_power_on_nearest_type(
        &mut self,
        members: &[usize],
        button: &str,
        types: &[String],
    ) -> Option<()> {
        let members = self.materialize_native_group(members);
        let Some(definition) = self
            .command_buttons
            .buttons
            .iter()
            .find(|item| item.name == button)
        else {
            return Some(());
        };
        if !matches!(definition.command, Command::SpecialPower) {
            return None;
        }
        let Some(power) = &definition.power else {
            return Some(());
        };
        let name = power.name.clone();
        let kind = power.kind;
        let Some(&source) = members
            .iter()
            .find(|&&id| self.special_power_ordinal(id, &name).is_some())
        else {
            return Some(());
        };
        let Some(center) = self.native_group_center(&members) else {
            return Some(());
        };
        let point = [center[0], center[1]];
        let candidates = self.partition.outward_range_rings(point, REALLY_FAR);
        let owner = self.entities[source].owner;
        let mut best = None;
        for template in types {
            let Some(template) = self.catalog.iter().position(|unit| unit.name == *template) else {
                continue;
            };
            // Source searches each list entry separately and preserves the first
            // entry on equal distances; the partition supplies per-template ties.
            let mut nearest = None;
            let mut distance_squared_limit = REALLY_FAR * REALLY_FAR;
            let mut last_ring = f64::INFINITY;
            for &(ring, target) in &candidates {
                if ring > last_ring {
                    break;
                }
                let actor = &self.entities[target];
                let distance = distance_squared(point, actor.position);
                if distance >= distance_squared_limit {
                    continue;
                }
                let relation = self
                    .map_side_relationship(owner, actor.owner)
                    .unwrap_or_else(|| {
                        if self.allied(owner, actor.owner) {
                            2
                        } else {
                            u8::from(!self.enemies(owner, actor.owner))
                        }
                    });
                if !self.catalog[template].equivalent_to(&self.catalog[actor.type_index])
                    || actor.is_off_map() != self.entities[source].is_off_map()
                    || (actor.owner != owner && relation == 2)
                {
                    continue;
                }
                if self.special_power_object_admission(
                    source,
                    &name,
                    kind,
                    target,
                    COMMAND_FROM_SCRIPT,
                    false,
                ) {
                    distance_squared_limit = distance;
                    nearest = Some(target);
                    last_ring = ring;
                }
            }
            if let Some(target) = nearest {
                // getClosestObject returns a Real distance, after sqrtf.
                let distance = distance_squared_limit.sqrt();
                if best.is_none_or(|(_, previous)| distance < previous) {
                    best = Some((target, distance));
                }
            }
        }
        if let Some((target, _)) = best {
            for id in members {
                self.native_special_power(id, &name, Some(target), None, true);
            }
        }
        Some(())
    }
}
