//! Object team relations take precedence over player relations.
use super::Skirmish;

impl Skirmish {
    /// Native `Relationship`: enemies 0, neutral 1, allies 2.
    pub(in crate::skirmish) fn object_relationship(&self, actor: usize, target: usize) -> u8 {
        self.map_team_relationship(actor, target)
            .unwrap_or_else(|| {
                let first = self.entities[actor].owner;
                let second = self.entities[target].owner;
                if self.allied(first, second) {
                    2
                } else {
                    u8::from(!self.enemies(first, second))
                }
            })
    }
}
