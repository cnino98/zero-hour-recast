//! SH AutoFindHealingUpdate.cpp53–179; AIUpdate.cpp3744–3753.
//! Counter is internal state: this UPDATE always returns native NONE (1).
use super::{Skirmish, UnitAiUpdate};

pub struct Rules {
    pub scan_frames: u32,
    pub scan_range: f32,
    pub never_heal: f32,
    pub always_heal: f32,
}

pub(super) struct State {
    next_scan: i32,
}

impl Skirmish {
    pub(super) fn construct_auto_find_healing(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].behaviors.auto_find_healing.len(), ordinal);
        self.entities[id]
            .behaviors
            .auto_find_healing
            .push(State { next_scan: 0 });
    }

    pub(super) fn update_auto_find_healing(&mut self, id: usize, ordinal: usize) -> i32 {
        // computer_player retains exact HUMAN/COMPUTER source metadata,
        // including neutral/civilian sides; this is not an opponent-controller test.
        if !self.computer_player(self.entities[id].owner) {
            return 1;
        }
        let type_index = self.entities[id].type_index;
        let state = &mut self.entities[id].behaviors.auto_find_healing[ordinal];
        if state.next_scan > 0 {
            state.next_scan -= 1;
            return 1;
        }
        let rules = &self.catalog[type_index].behavior.auto_find_healing[ordinal];
        state.next_scan = i32::from_ne_bytes(rules.scan_frames.to_ne_bytes());
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if matches!(unit.ai.update, UnitAiUpdate::Absent)
            || !(unit.body_kind.is_active()
                || matches!(unit.body_kind, crate::skirmish::BodyKind::Inactive))
            || actor.health > actor.max_health * rules.never_heal
            || !actor.ai_idle()
        {
            return 1;
        }
        // The source returns before its AlwaysHeal branch when non-idle.
        // Do not resurrect that unreachable behavior based on the parsed value.
        if let Some(target) = self.closest_heal_pad(id, rules.scan_range) {
            self.ai_get_healed(id, target);
        }
        1
    }

    fn ai_get_healed(&mut self, id: usize, target: usize) {
        let (Some(actor), Some(pad)) = (self.entities.get(id), self.entities.get(target)) else {
            return;
        };
        let unit = &self.catalog[actor.type_index];
        let structure = &self.catalog[pad.type_index];
        // ActionManager::canGetHealedAt; CMD_FROM_AI does not impose human fog.
        if self.object_relationship(id, target) != 2
            || !pad.alive()
            || actor.under_construction()
            || pad.under_construction()
            || pad.selling()
            || !unit.kind_of.iter().any(|kind| kind == "INFANTRY")
            || !structure.kind_of.iter().any(|kind| kind == "HEAL_PAD")
            || (unit.body_kind.is_active()
                || matches!(unit.body_kind, crate::skirmish::BodyKind::Inactive))
                && actor.health.partial_cmp(&actor.max_health) == Some(std::cmp::Ordering::Equal)
        {
            return;
        }
        // aiEnter performs its own original container admission. An invalid
        // nearest HEAL_PAD does not cause scanning for a farther alternative.
        if !self.heal_command_admits(id, target) || !self.container_admits(id, target, false) {
            return;
        }
        self.begin_container_enter(id, target);
        self.entities[id].ai.command.from_ai = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(game: &mut Skirmish, frames: u32) -> usize {
        game.slots[1].difficulty = Some(crate::skirmish::OpponentDifficulty::Hard);
        let type_index = 5;
        game.catalog[type_index].behavior.auto_find_healing = vec![Rules {
            scan_frames: frames,
            scan_range: 300.0,
            never_heal: 0.95,
            always_heal: 0.25,
        }];
        crate::skirmish::tests::register_update(
            &mut game.catalog[type_index],
            "AutoFindHealingUpdate",
        );
        game.spawn(type_index, 1, [100.0, 100.0], true)
    }

    #[test]
    fn owner_change_to_human_preserves_in_progress_native_counter() {
        let mut game = crate::skirmish::tests::game();
        let id = install(&mut game, 2);
        assert!(game.computer_player(1));
        assert!(!game.computer_player(0));
        assert_eq!(
            game.entities[id].behaviors.auto_find_healing[0].next_scan,
            0
        );
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[id].behaviors.auto_find_healing[0].next_scan,
            2
        );
        game.entities[id].owner = 0;
        game.tick += 1;
        game.run_object_updates();
        game.tick += 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[id].behaviors.auto_find_healing[0].next_scan,
            2
        );
        game.entities[id].owner = 1;
        game.tick += 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[id].behaviors.auto_find_healing[0].next_scan,
            1
        );
    }

    #[test]
    fn unsigned_scan_duration_preserves_native_signed_counter_assignment() {
        let mut game = crate::skirmish::tests::game();
        let id = install(&mut game, u32::MAX);
        assert_eq!(
            game.entities[id].behaviors.auto_find_healing[0].next_scan,
            0
        );
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[id].behaviors.auto_find_healing[0].next_scan,
            -1
        );
        game.tick += 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[id].behaviors.auto_find_healing[0].next_scan,
            -1
        );
    }
}
