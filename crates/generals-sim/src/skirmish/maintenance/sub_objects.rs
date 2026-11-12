//! Native `SubObjectsUpgrade` and persistent W3D drawable overrides.
use crate::skirmish::{Entity, Skirmish};

pub struct SubObjectUpgrade {
    pub show: Vec<String>,
    pub hide: Vec<String>,
}

/// Native case-insensitive, insertion-ordered `W3DModelDraw` override list.
#[derive(Clone, PartialEq, Eq)]
pub struct SubObjectOverride {
    pub name: String,
    pub visible: bool,
}

impl Entity {
    #[must_use]
    pub fn sub_object_overrides(&self) -> &[SubObjectOverride] {
        &self.drawable.sub_objects
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn apply_sub_object_upgrade(
        &mut self,
        id: usize,
        index: usize,
        ordinal: usize,
    ) {
        let unit = &self.catalog[self.entities[id].type_index];
        // Implementation rechecks the live masks after common upgrade removal.
        if unit.behavior_modules[index]
            .upgrade
            .conflicts
            .iter()
            .any(|name| {
                self.object_has_upgrade(id, name) || self.has_upgrade(self.entities[id].owner, name)
            })
            || !self.entities[id].has_drawable()
        {
            return;
        }
        let rules = &unit.behavior.sub_object_upgrades[ordinal];
        let overrides = &mut self.entities[id].drawable.sub_objects;
        for (names, visible) in [(&rules.show, true), (&rules.hide, false)] {
            for name in names {
                if name.is_empty() {
                    continue;
                }
                if let Some(setting) = overrides
                    .iter_mut()
                    .find(|setting| setting.name.eq_ignore_ascii_case(name))
                {
                    setting.visible = visible;
                } else {
                    overrides.push(SubObjectOverride {
                        name: name.clone(),
                        visible,
                    });
                }
            }
        }
    }
}
