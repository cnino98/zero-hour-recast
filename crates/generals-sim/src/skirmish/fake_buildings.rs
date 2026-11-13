//! SH `ReplaceObjectUpgrade` and fake-building command weapons.
use super::{
    Skirmish, UnitType, ai_goals::COMMAND_FROM_PLAYER, combat::attack::Goal, weapons::Lock,
};

pub struct Replacement {
    pub upgrade: String,
    pub object: String,
}

pub struct CommandSet {
    pub upgrade: String,
    pub alternate: Option<(String, String)>,
    pub command_set: String,
    pub removes: Vec<String>,
}

pub struct ReplacementEvent {
    pub frame: u64,
    pub previous: usize,
    pub next: usize,
}

impl UnitType {
    #[must_use]
    pub fn fake_building(&self) -> bool {
        self.kind_of.iter().any(|kind| kind == "FS_FAKE")
    }
}

impl Skirmish {
    #[must_use]
    pub fn can_detonate_fake(&self, owner: usize, id: usize) -> bool {
        self.end_game_at.is_none()
            && self.entities.get(id).is_some_and(|actor| {
                actor.owner == owner
                    && actor.complete()
                    && !actor.masked()
                    && !actor.hacked()
                    && !actor.paralyzed()
                    && self.catalog[actor.type_index].fake_building()
                    && self.allocated_weapon(id, 0).is_some()
            })
    }

    /// Fires the authored PRIMARY `SuicideWeapon` at the source position.
    /// # Errors
    /// Rejects unavailable fake structures and unfinished foundations.
    pub fn detonate_fake(&mut self, owner: usize, id: usize) -> Result<(), &'static str> {
        if !self.can_detonate_fake(owner, id) {
            return Err("Fake building cannot detonate");
        }
        // MSG_DO_WEAPON locks the allocated weapon before groupAttackPosition
        // supplies each member's own position to its ordinary AI attack state.
        let weapons = &mut self.entities[id].weapon_set;
        if !matches!(weapons.lock, Lock::Permanent) {
            weapons.current = 0;
            weapons.lock = Lock::Temporary;
        }
        self.native_attack_goal_from(
            id,
            Goal::Position(self.object_position(id)),
            i32::MAX.unsigned_abs(),
            COMMAND_FROM_PLAYER,
            false,
        );
        Ok(())
    }

    pub(in crate::skirmish) fn apply_replacement_upgrade(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let name = &self.catalog[actor.type_index].behavior.replacement_names[ordinal];
        let Some(replacement) = self.catalog.iter().position(|unit| &unit.name == name) else {
            return; // Native missing replacement template returns; UpgradeMux still executes.
        };
        let owner = actor.owner;
        let team = self.team_membership.team_of(id);
        let (position, basis) = self.object_transform(id);
        self.unregister_object_obstacle(id);
        self.destroy_maintenance_object(id);
        let next = self.new_object_on_team(replacement, owner, team);
        self.set_object_transform(next, position, basis);
        self.rebuild_navigation();
        self.native_build_complete(next);
        self.record_built(next);
        self.players[owner].statistics.money_spent += self.build_cost(owner, replacement);
        self.update_power();
        self.opponent_structure_produced(next);
        self.map_structure_produced(next);
        self.presentation.replacement_events.push(ReplacementEvent {
            frame: self.tick,
            previous: id,
            next,
        });
    }
}

#[cfg(test)]
mod tests;
