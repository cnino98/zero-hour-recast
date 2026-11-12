//! SH `BridgeBehavior` for the original no-tower Generic and Monument train bridges.
use super::{BodyKind, Skirmish, UnitType};

pub struct Rules {
    /// Native INI velocity: world units per logic frame, including unused scaffold data.
    pub lateral_scaffold_speed: f32,
    pub vertical_scaffold_speed: f32,
}

pub(super) struct State {
    death_frame: u32,
    fx_resolved: bool,
}

pub(super) fn validate(unit: &UnitType) -> Result<(), String> {
    if unit.behavior.bridges.is_empty() {
        return Ok(());
    }
    let supported = match unit.name.as_str() {
        "GenericBridge" => {
            matches!(unit.body_kind, BodyKind::Active)
                && unit.max_health.to_bits() == 1.0_f32.to_bits()
                && unit.initial_health.to_bits() == 1.0_f32.to_bits()
                && unit.minimum_health.to_bits() == 1.0_f32.to_bits()
        }
        "MonumentTrainBridge" => matches!(unit.body_kind, BodyKind::Active),
        _ => false,
    };
    if !supported || !unit.kind_of.iter().any(|kind| kind == "BRIDGE") {
        return Err(format!(
            "{}: BridgeBehavior requires the admitted original bridge/body contract",
            unit.name
        ));
    }
    Ok(())
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_bridge_behavior(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.bridges.len(), ordinal);
        // TerrainLogic.cpp263 excludes all four towers. No scaffold can be
        // requested without those towers; the destructor/onDelete lists are empty.
        self.entities[id].behaviors.bridges.push(State {
            death_frame: 0,
            fx_resolved: false,
        });
    }

    pub(in crate::skirmish) fn bridge_body_state_changed(
        &mut self,
        id: usize,
        ordinal: usize,
        old_state: usize,
        new_state: usize,
    ) {
        if new_state != 3 {
            self.entities[id].behaviors.bridges[ordinal].death_frame = 0;
        }
        assert_eq!(
            self.catalog[self.entities[id].type_index].name, "MonumentTrainBridge",
            "mutable regular bridge terrain-buffer transitions are not admitted"
        );
        if !self.entities[id].behaviors.bridges[ordinal].fx_resolved {
            let Some((_, terrain_bridge)) = self.bridge_at_position(self.entities[id].position)
            else {
                return;
            };
            assert_eq!(
                self.catalog[self.entities[terrain_bridge].type_index].name, "MonumentTrainBridge",
                "overlapping regular bridge transition effects are not admitted"
            );
            // The parsed MonumentTrainBridge Roads entry has no transition FX,
            // OCL or sound. Empty lists perform no random draws or emission.
            self.entities[id].behaviors.bridges[ordinal].fx_resolved = true;
        }
        // ActiveBody runs this callback before Object::onDie. In particular,
        // TerrainLogic may kill occupants before BridgeBehavior's later DIE loop.
        self.update_bridge_states();
        if old_state == 3 || new_state == 3 {
            self.radar_terrain_refresh_frame = self.update_frame();
        }
    }

    pub(in crate::skirmish) fn bridge_behavior_on_die(&mut self, id: usize, ordinal: usize) {
        self.bridge_on_die(id);
        self.entities[id].behaviors.bridges[ordinal].death_frame = self.update_frame();
    }
}

#[cfg(test)]
mod tests;
