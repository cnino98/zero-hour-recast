//! `ThingFactory::newObject` and `ThingTemplate::isEquivalentTo`.
use super::{Skirmish, UnitType, startup};

impl UnitType {
    #[must_use]
    pub fn equivalent_to(&self, other: &Self) -> bool {
        self.name == other.name
            || self.reskin.as_ref().is_some_and(|name| name == &other.name)
            || other.reskin.as_ref().is_some_and(|name| name == &self.name)
            || (self.reskin.is_some() && self.reskin == other.reskin)
            || self
                .variations
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&other.name))
            || other
                .variations
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&self.name))
    }
}

impl Skirmish {
    pub(super) fn resolve_variation(&mut self, index: usize) -> usize {
        let names = &self.catalog[index].variations;
        if names.is_empty() {
            return index;
        }
        let count = u32::try_from(names.len()).expect("validated variation count");
        let selected = usize::try_from(if count == 1 {
            0
        } else {
            startup::random_value(&mut self.random) % count
        })
        .expect("variation index fits usize");
        self.catalog
            .iter()
            .position(|unit| unit.name == names[selected])
            .expect("validated build variation template")
    }
}
