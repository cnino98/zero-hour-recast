//! Actual `Object::enterGroup`/`leaveGroup` identity retained for native `getCount`.
//! No member-list duplication: the current consumer only needs identity/count.
use super::{Skirmish, UnitAiUpdate};

pub struct CommandGroup {
    pub members: Vec<usize>,
    group: u32,
}

impl Skirmish {
    /// `logicMessageDispatcher` materializes the authoritative Squad before every command.
    pub fn begin_command_group(&mut self, owner: usize) -> CommandGroup {
        let selected = self.players[owner].selected_objects.clone();
        let members = self.materialize_selection_group(owner, &selected);
        CommandGroup {
            members,
            group: self.next_ai_group_id,
        }
    }

    /// AIGROUP=0 `AIGroup::removeAll` preserves transfers performed by callbacks.
    pub fn finish_command_group(&mut self, command: CommandGroup) {
        self.remove_group(command.group, command.members);
    }

    fn remove_group(&mut self, group: u32, members: Vec<usize>) {
        for id in members {
            if self.entities[id].ai.command.ai_group == Some(group) {
                self.entities[id].ai.command.ai_group = None;
            }
        }
    }

    /// Skirmish startup calls `selectObject` directly for the first command center.
    pub fn initialize_command_center_selection(&mut self, owner: usize) -> Option<usize> {
        let object = self
            .team_membership
            .player_objects(owner)
            .into_iter()
            .find(|&id| {
                let actor = &self.entities[id];
                actor.exists()
                    && self.catalog[actor.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "COMMANDCENTER")
            })?;
        let current = self.players[owner].selected_objects.clone();
        drop(self.select_object(owner, &current, object, true));
        Some(object)
    }

    /// The specific-source special-power path uses its own temporary `AIGroup`.
    /// # Errors
    /// Rejects a provider without the source group interface or an unavailable power.
    pub fn cast_power_from_provider(
        &mut self,
        caster: usize,
        name: &str,
        target: super::PowerTarget,
    ) -> Result<(), &'static str> {
        if self.entities.get(caster).is_none() {
            return Err("Power provider does not exist");
        }
        let members = self.materialize_native_group(&[caster]);
        let group = self.next_ai_group_id;
        let result = if members.is_empty() {
            Err("Power provider has no command-group interface")
        } else {
            self.cast_power(caster, name, target)
        };
        self.remove_group(group, members);
        result
    }

    const fn next_group(&mut self) -> u32 {
        self.next_ai_group_id = self.next_ai_group_id.wrapping_add(1);
        self.next_ai_group_id
    }

    fn selectable_group_member(&self, id: usize) -> bool {
        self.entities
            .get(id)
            .is_some_and(|actor| actor.selectable(&self.catalog[actor.type_index]))
    }

    fn can_enter_group(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            let unit = &self.catalog[actor.type_index];
            actor.exists()
                && (!matches!(unit.ai.update, UnitAiUpdate::Absent)
                    || unit
                        .kind_of
                        .iter()
                        .any(|kind| matches!(kind.as_str(), "STRUCTURE" | "ALWAYS_SELECTABLE")))
        })
    }

    fn enter_group(&mut self, id: usize, group: u32) {
        if self.can_enter_group(id) {
            // Overwrite implements leaveGroup followed by entry. Other former
            // members keep their handle, so their getCount shrinks immediately.
            self.entities[id].ai.command.ai_group = Some(group);
        }
    }

    pub(super) fn single_ai_group(&self, id: usize) -> bool {
        self.entities[id].ai.command.ai_group.is_some_and(|group| {
            self.entities
                .iter()
                .filter(|actor| actor.exists() && actor.ai.command.ai_group == Some(group))
                .count()
                == 1
        })
    }

    /// `Team::getTeamAsAIGroup` adds every native member, including unselectable
    /// actors. `AIGroup::add` itself applies the actual AI/structure gate.
    pub(in crate::skirmish) fn materialize_native_group(
        &mut self,
        objects: &[usize],
    ) -> Vec<usize> {
        let group = self.next_group();
        let mut members = Vec::new();
        for &id in objects {
            self.enter_group(id, group);
            if self.entities[id].ai.command.ai_group == Some(group) {
                members.push(id);
            }
        }
        members
    }

    /// `GameLogic` dispatcher materializes a group from the selected Squad before
    /// every admitted logic message, including selection messages themselves.
    pub fn materialize_selection_group(&mut self, owner: usize, selected: &[usize]) -> Vec<usize> {
        let group = self.next_group();
        let mut members = Vec::new();
        for &id in selected {
            if self.selectable_group_member(id) {
                self.enter_group(id, group);
                if self.entities[id].ai.command.ai_group == Some(group) {
                    members.retain(|&member| member != id);
                    members.push(id);
                }
            }
        }
        // Native adds first, then removes foreign objects: their old membership
        // is lost even though the message cannot command them.
        members.retain(|&id| {
            let actor = &mut self.entities[id];
            if actor.owner == owner {
                true
            } else {
                actor.ai.command.ai_group = None;
                false
            }
        });
        members
    }

    /// Client-side result of a pending selection message, without entering any AI group.
    #[must_use]
    pub fn preview_selection_create(
        &self,
        current: &[usize],
        added: &[usize],
        replace: bool,
    ) -> Vec<usize> {
        let mut selection = current.to_vec();
        let mut first = true;
        for &id in added {
            if !self.entities.get(id).is_some_and(super::Entity::exists) {
                continue;
            }
            let create_new = replace && first;
            first = false;
            let mass_selectable = self.selectable_group_member(id)
                && !self.catalog[self.entities[id].type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "STRUCTURE");
            if !create_new && !mass_selectable {
                continue;
            }
            if create_new {
                selection.clear();
            }
            if self.can_enter_group(id) {
                selection.push(id);
            }
        }
        selection
    }

    /// Client-side Squad removal, retaining the source's last-occurrence ordering.
    #[must_use]
    pub fn preview_selection_remove(&self, current: &[usize], removed: &[usize]) -> Vec<usize> {
        let mut selection = current.to_vec();
        for &id in removed {
            if !self.entities.get(id).is_some_and(super::Entity::exists) {
                continue;
            }
            let mut members = Vec::new();
            for member in selection {
                if self.selectable_group_member(member) && self.can_enter_group(member) {
                    members.retain(|&prior| prior != member);
                    members.push(member);
                }
            }
            members.retain(|&member| member != id);
            selection = members;
        }
        selection
    }

    /// Command translation may inspect pending Squad members without materializing a group.
    #[must_use]
    pub fn preview_selection_group(&self, owner: usize, selected: &[usize]) -> Vec<usize> {
        let mut members = Vec::new();
        for &id in selected {
            if self.selectable_group_member(id)
                && self.can_enter_group(id)
                && self.entities[id].owner == owner
            {
                members.retain(|&member| member != id);
                members.push(id);
            }
        }
        members
    }

    /// `MSG_CREATE_SELECTED_GROUP`: caller supplies the message's ordered objects,
    /// not merely the final selection diff. Re-selecting a member is observable.
    pub fn create_selection(
        &mut self,
        owner: usize,
        current: &[usize],
        added: &[usize],
        replace: bool,
    ) -> Vec<usize> {
        let members = self.materialize_selection_group(owner, current);
        let group = self.next_ai_group_id;
        let selection = self.apply_selection_create(owner, current, added, replace);
        self.remove_group(group, members);
        selection
    }

    /// Selection body after the source dispatcher has materialized its command group.
    pub fn apply_selection_create(
        &mut self,
        owner: usize,
        current: &[usize],
        added: &[usize],
        replace: bool,
    ) -> Vec<usize> {
        let mut selection = current.to_vec();
        let mut first = true;
        for &id in added {
            if !self.entities.get(id).is_some_and(super::Entity::exists) {
                continue;
            }
            let create_new = replace && first;
            first = false;
            selection = self.select_object(owner, &selection, id, create_new);
        }
        self.players[owner].selected_objects.clone_from(&selection);
        selection
    }

    /// `GameLogic::selectObject` inside an existing message does not materialize
    /// the prior selected group again (SpectreGunshipDeploymentUpdate.cpp).
    pub(in crate::skirmish) fn select_object(
        &mut self,
        owner: usize,
        current: &[usize],
        id: usize,
        create_new: bool,
    ) -> Vec<usize> {
        let mut selection = current.to_vec();
        let Some(actor) = self.entities.get(id).filter(|actor| actor.exists()) else {
            return selection;
        };
        let mass_selectable = self.selectable_group_member(id)
            && !self.catalog[actor.type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "STRUCTURE");
        if !create_new && !mass_selectable {
            return selection;
        }
        let group = self.next_group();
        self.enter_group(id, group);
        if create_new {
            selection.clear();
        }
        if self.entities[id].ai.command.ai_group == Some(group) {
            selection.push(id);
            self.entities[id].ai.command.ai_group = None;
        }
        self.players[owner].selected_objects.clone_from(&selection);
        selection
    }

    /// `MSG_REMOVE_FROM_SELECTED_GROUP`, including each deselectObject's separate
    /// current-selection group and final removeAll, in message argument order.
    pub fn remove_selection(
        &mut self,
        owner: usize,
        current: &[usize],
        removed: &[usize],
    ) -> Vec<usize> {
        let members = self.materialize_selection_group(owner, current);
        let group = self.next_ai_group_id;
        let selection = self.apply_selection_remove(owner, current, removed);
        self.remove_group(group, members);
        selection
    }

    /// Deselect body after source command-group materialization.
    pub fn apply_selection_remove(
        &mut self,
        owner: usize,
        current: &[usize],
        removed: &[usize],
    ) -> Vec<usize> {
        self.players[owner].selected_objects = current.to_vec();
        for &id in removed {
            self.deselect_object_for_player(owner, id);
        }
        self.players[owner].selected_objects.clone()
    }

    fn deselect_object_for_player(&mut self, owner: usize, id: usize) {
        if !self.entities.get(id).is_some_and(super::Entity::exists) {
            return;
        }
        let group = self.next_group();
        let current = std::mem::take(&mut self.players[owner].selected_objects);
        let mut members = Vec::new();
        for member in current {
            if self.selectable_group_member(member) {
                self.enter_group(member, group);
                if self.entities[member].ai.command.ai_group == Some(group) {
                    members.retain(|&previous| previous != member);
                    members.push(member);
                }
            }
        }
        members.retain(|&member| member != id);
        self.players[owner].selected_objects = members;
        for actor in &mut self.entities {
            if actor.ai.command.ai_group == Some(group) {
                actor.ai.command.ai_group = None;
            }
        }
    }

    pub(in crate::skirmish) fn mask_object(&mut self, id: usize, masked: bool) {
        self.entities[id].maintenance.masked = masked;
        if masked {
            // Object::maskObject passes the complement of its controlling player's mask.
            for owner in 0..self.players.len() {
                if owner != self.entities[id].owner {
                    self.deselect_object_for_player(owner, id);
                }
            }
            if self.entities[id].has_drawable() {
                self.deselected_objects.push(id);
            }
        }
    }

    // GameLogic::deselectObject(PLAYERMASK_ALL, TRUE) changes logic groups immediately.
    pub(in crate::skirmish) fn deselect_dead_object(&mut self, id: usize) {
        for owner in 0..self.players.len() {
            self.deselect_object_for_player(owner, id);
        }
        if self.entities[id].has_drawable() {
            self.deselected_objects.push(id);
        }
    }

    /// Object destruction leaves its retained group and disregards its `Drawable`.
    /// `Squad::getLiveObjects` also drops the now-invalid `ObjectID`, without
    /// rematerializing the surviving selection or changing their group identity.
    pub(in crate::skirmish) fn discard_destroyed_selection(&mut self, id: usize) {
        let mut was_selected = false;
        for player in &mut self.players {
            was_selected |= player.selected_objects.contains(&id);
            player.selected_objects.retain(|&member| member != id);
        }
        self.entities[id].ai.command.ai_group = None;
        if was_selected && self.entities[id].has_drawable() {
            self.deselected_objects.push(id);
        }
    }

    #[must_use]
    pub fn selected_objects(&self, owner: usize) -> &[usize] {
        &self.players[owner].selected_objects
    }

    /// `MSG_DESTROY_SELECTED_GROUP` clears the Squad, then the dispatcher removes its group.
    pub fn destroy_selection(&mut self, owner: usize, current: &[usize]) {
        let members = self.materialize_selection_group(owner, current);
        let group = self.next_ai_group_id;
        self.apply_selection_destroy(owner);
        self.remove_group(group, members);
    }

    /// `Player::processSelectTeamGameMessage` / `processAddTeamGameMessage` copy a Squad.
    pub fn apply_selection_recall(&mut self, owner: usize, members: &[usize], replace: bool) {
        if replace {
            self.players[owner].selected_objects.clear();
        }
        self.players[owner].selected_objects.extend(
            members
                .iter()
                .copied()
                .filter(|&id| self.entities.get(id).is_some_and(super::Entity::exists)),
        );
    }

    /// Destroy-selection body within an already materialized command.
    pub fn apply_selection_destroy(&mut self, owner: usize) {
        self.players[owner].selected_objects.clear();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod preview_tests {
    use crate::skirmish::{Kind, tests::game};

    #[test]
    fn queued_selection_preview_retains_duplicates_without_transferring_live_groups() {
        let mut game = game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        let a = game.spawn(tank, 0, [305.0, 405.0], true);
        let b = game.spawn(tank, 0, [355.0, 405.0], true);
        let enemy = game.spawn(tank, 1, [405.0, 405.0], true);
        game.materialize_selection_group(0, &[a, b]);
        game.materialize_selection_group(1, &[enemy]);
        let groups = [a, b, enemy].map(|id| game.entities[id].ai.command.ai_group);
        let pending = game.preview_selection_create(&[], &[a, b, a, enemy], true);
        assert_eq!(pending, [a, b, a, enemy]);
        assert_eq!(game.preview_selection_group(0, &pending), [b, a]);
        assert_eq!(game.preview_selection_remove(&pending, &[a]), [b, enemy]);
        assert_eq!(
            [a, b, enemy].map(|id| game.entities[id].ai.command.ai_group),
            groups
        );
        let selected = game.create_selection(0, &[], &[a, b, a, enemy], true);
        assert_eq!(selected, pending);
        assert_eq!(game.materialize_selection_group(0, &selected), [b, a]);
        assert!(game.entities[enemy].ai.command.ai_group.is_none());
        assert_eq!(game.remove_selection(0, &selected, &[a]), [b, enemy]);
    }
}
