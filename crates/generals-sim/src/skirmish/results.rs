//! `SuperHackers` 2bfaea60 `ScoreKeeper`, `VictoryConditions` and `Player::killPlayer`.

use super::Skirmish;

/// Native player-owned advice counters already produced by gameplay.
pub(super) struct AcademyStats {
    pub special_powers_used: u32,
    pub units_entered_tunnel: u32,
    pub mines_cleared: u32,
    pub researched_radar: bool,
    pub buildings_captured: u32,
}

pub struct ScoreKeeper {
    pub units_built: u32,
    pub units_lost: u32,
    pub units_destroyed: u32,
    pub buildings_built: u32,
    pub buildings_lost: u32,
    pub buildings_destroyed: u32,
    pub money_earned: u32,
    pub money_spent: u32,
}

impl ScoreKeeper {
    pub(super) const fn new() -> Self {
        Self {
            units_built: 0,
            units_lost: 0,
            units_destroyed: 0,
            buildings_built: 0,
            buildings_lost: 0,
            buildings_destroyed: 0,
            money_earned: 0,
            money_spent: 0,
        }
    }

    /// Native score used to order the skirmish result rows.
    #[must_use]
    pub const fn calculate_score(&self) -> u32 {
        // Object::scoreTheKill credits destroyed objects only to an enemy;
        // destroyed objects do not include allied or neutral casualties.
        (self.units_built + self.buildings_built + self.units_destroyed + self.buildings_destroyed)
            * 100
            + self.money_earned
    }
}

impl Skirmish {
    /// `ScriptActions::doVictory`/`doDefeat` disable input and start 120 logic frames.
    pub const fn start_end_game_timer(&mut self) {
        self.end_game_at = Some(self.tick + 120);
    }

    pub(super) fn record_built(&mut self, id: usize) {
        let entity = &self.entities[id];
        let unit = &self.catalog[entity.type_index];
        if !unit.scoring.create {
            return;
        }
        let statistics = &mut self.players[entity.owner].statistics;
        if unit.kind.is_building() {
            statistics.buildings_built += 1;
        } else {
            statistics.units_built += 1;
        }
    }

    pub(super) fn record_kill(&mut self, killer: usize, victim: usize) {
        let Some(killer) = self.entities.get(killer) else {
            return;
        };
        self.record_player_kill(killer.owner, victim);
    }

    pub(super) fn record_player_kill(&mut self, owner: usize, victim: usize) {
        let victim = &self.entities[victim];
        let unit = &self.catalog[victim.type_index];
        if victim.under_construction() || unit.progression.ignored_in_gui || !unit.scoring.destroy {
            return;
        }
        let statistics = &mut self.players[victim.owner].statistics;
        if unit.kind.is_building() {
            statistics.buildings_lost += 1;
        } else {
            statistics.units_lost += 1;
        }
        if self.enemies(owner, victim.owner) {
            let statistics = &mut self.players[owner].statistics;
            if unit.kind.is_building() {
                statistics.buildings_destroyed += 1;
            } else {
                statistics.units_destroyed += 1;
            }
        }
    }

    pub(super) fn update_victory(&mut self) {
        // GameLogic inserts MultiplayerScripts only when more than one team is
        // occupied. An Open/Closed opponent slot is a sandbox, without end scripts.
        let first = self.slots.iter().position(|slot| slot.occupied);
        if first.is_none_or(|first| !(0..self.slots.len()).any(|other| self.enemies(first, other)))
        {
            return;
        }
        let defeated: Vec<_> = (0..self.slots.len())
            .map(|owner| {
                self.occupied[owner]
                    && (self.defeated[owner]
                        || !self.entities.iter().any(|entity| {
                            entity.owner == owner
                                && entity.alive()
                                && self.catalog[entity.type_index].scoring.victory
                        }))
            })
            .collect();
        // Latch the surviving alliance BEFORE killing eliminated players' units.
        // SuperHackers' fix also awards victory to already-defeated allies.
        let survivor =
            (0..self.slots.len()).find(|&owner| self.occupied[owner] && !defeated[owner]);
        if self.ended_at.is_none()
            && survivor.is_none_or(|winner| {
                !(0..self.slots.len()).any(|owner| {
                    self.occupied[owner] && !defeated[owner] && !self.allied(owner, winner)
                })
            })
        {
            self.outcome = survivor;
            self.ended_at = Some(self.tick);
        }
        for (owner, defeated) in defeated.into_iter().enumerate() {
            if !defeated || self.defeated[owner] {
                continue;
            }
            self.defeated[owner] = true;
            self.reveal_defeated_player(owner);
            for id in 0..self.entities.len() {
                if self.entities[id].owner == owner && self.entities[id].alive() {
                    if self.catalog[self.entities[id].type_index]
                        .maintenance
                        .has_kind(super::MaintenanceRules::TECH_BUILDING)
                    {
                        self.transfer_maintenance_owner(id, self.neutral_owner());
                        continue;
                    }
                    self.entities[id].health = 0.0;
                    if self.catalog[self.entities[id].type_index]
                        .body_kind
                        .is_active()
                    {
                        self.entities[id].refresh_body_health();
                    }
                    // Player::killPlayer has INVALID_ID: no attacker/loss credit.
                    self.finish_death(id, None);
                }
            }
            self.players[owner].cash = 0;
        }
        self.update_visibility();
        self.projectiles
            .retain(|projectile| !self.defeated[projectile.owner]);
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn record_special_power_used(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        if self.catalog[actor.type_index].special_powers[ordinal].academy_classification
            == "ACT_SUPERPOWER"
        {
            let used = &mut self.players[actor.owner].academy.special_powers_used;
            *used = used.wrapping_add(1);
        }
    }
}
