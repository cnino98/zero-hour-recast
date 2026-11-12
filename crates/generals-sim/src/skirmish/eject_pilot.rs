//! SH `EjectPilotDie`: ordered `DieMux`, ordinary OCL creation, then eject sounds.
use super::{
    Skirmish, UnitType, death_callbacks::Conditions, object_creation::List, object_sounds::Action,
};

pub struct Rules {
    pub conditions: Conditions,
    pub ground: Option<List>,
    pub airborne: Option<List>,
    pub sounds: [String; 2],
}

pub(in crate::skirmish) fn validate(
    unit: &UnitType,
    catalog: &[UnitType],
) -> Result<(), &'static str> {
    for rules in &unit.behavior.eject_pilots {
        for list in [&rules.ground, &rules.airborne].into_iter().flatten() {
            for nugget in &list.objects {
                for name in nugget
                    .names
                    .iter()
                    .chain(std::iter::once(&nugget.container))
                    .filter(|name| !name.is_empty())
                {
                    if !catalog.iter().any(|target| &target.name == name) {
                        return Err(
                            "EjectPilotDie requires the original admitted OCL object/container",
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

impl UnitType {
    /// `HijackerUpdate` asks whether the actual module exists, even at REGULAR.
    #[must_use]
    pub const fn has_eject_pilot(&self) -> bool {
        !self.behavior.eject_pilots.is_empty()
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn eject_pilot_on_die(
        &mut self,
        id: usize,
        ordinal: usize,
        source: Option<usize>,
    ) {
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .eject_pilots[ordinal];
        if !self.die_conditions_match(id, &rules.conditions) {
            return;
        }
        // Object::isSignificantlyAboveTerrain, not AIRBORNE_TARGET or an AIRCRAFT kind.
        let list = if self.height_above_ground(id) > -9.0 * self.gravity {
            &rules.airborne
        } else {
            &rules.ground
        };
        if let Some(list) = list.clone() {
            let sounds = rules.sounds.clone();
            self.eject_pilot_objects(
                id,
                source.filter(|&source| self.entities[source].exists()),
                &list,
                sounds,
            );
        }
    }

    /// Native static `EjectPilotDie::ejectPilot`, also called by helicopter death.
    pub(in crate::skirmish) fn eject_pilot_objects(
        &mut self,
        id: usize,
        source: Option<usize>,
        list: &List,
        sounds: [String; 2],
    ) {
        self.create_generic_objects(id, source, list);
        let [voice, sound] = sounds;
        self.object_sound(id, voice, Action::PlayAtPosition);
        self.object_sound(id, sound, Action::PlayAtPositionUnowned);
    }
}

#[cfg(test)]
mod tests;
