//! Native `Player::init` identity and `GameLogic` skirmish color override.
use crate::skirmish::{ProgressionRules, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn validate_player_identities(
        &self,
        rules: &ProgressionRules,
    ) -> Result<(), &'static str> {
        for (owner, player) in self.players.iter().enumerate() {
            if owner == self.neutral_owner() {
                continue;
            }
            let active = self.occupied.get(owner).copied().unwrap_or(false);
            let named = owner > self.neutral_owner();
            if !active && !named {
                continue;
            }
            if !rules
                .factions
                .iter()
                .any(|template| template.faction == player.faction)
            {
                return Err("Configured player has no authored PlayerTemplate identity");
            }
            if active
                && self
                    .player_colors
                    .get(owner)
                    .copied()
                    .flatten()
                    .is_none_or(|index| index >= rules.multiplayer_colors.len())
            {
                return Err("Configured player has no authored multiplayer color");
            }
        }
        Ok(())
    }

    pub(in crate::skirmish) fn reset_player_identities(&mut self) -> Result<(), &'static str> {
        if let Some(rules) = &self.progression {
            self.validate_player_identities(rules)?;
        }
        let neutral = self.neutral_owner();
        for (owner, player) in self.players.iter_mut().enumerate() {
            player.side = None;
            player.playable_side = false;
            player.color = if owner == neutral {
                Some(0xffff_ffff)
            } else {
                None
            };
            let Some(rules) = &self.progression else {
                continue;
            };
            let active = self.occupied.get(owner).copied().unwrap_or(false);
            if owner == neutral || (!active && owner < neutral) {
                continue;
            }
            let template = rules
                .factions
                .iter()
                .find(|template| template.faction == player.faction)
                .expect("validated PlayerTemplate identity");
            player.side = Some(template.side.clone());
            player.playable_side = template.playable_side;
            let color = if active {
                rules.multiplayer_colors[self.player_colors[owner].expect("validated slot color")]
            } else {
                template.preferred_color
            };
            player.color = Some(color | 0xff00_0000);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
