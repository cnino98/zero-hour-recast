//! `AIPlayer` priority default-team dozer `WorkOrder` production callbacks.
use super::State;
use crate::skirmish::Skirmish;

pub(super) struct WorkOrder {
    pub(super) type_index: usize,
    pub(super) producer: Option<usize>,
    pub(super) complete: bool,
    pub(super) resource_gatherer: bool,
}

pub(super) fn queue_dozer(game: &mut Skirmish, state: &mut State) {
    let allowed = game.map_build_allowed(state.owner, true);
    game.set_map_build_allowed(state.owner, true, true);
    queue_dozer_enabled(game, state);
    game.set_map_build_allowed(state.owner, true, allowed);
}

fn queue_dozer_enabled(game: &mut Skirmish, state: &mut State) {
    if state.work.iter().any(|order| {
        game.layout
            .map_template_order
            .iter()
            .any(|template| template.dozer && template.name == game.catalog[order.type_index].name)
    }) {
        return;
    }
    for template in &game.layout.map_template_order {
        let Some(type_index) = game
            .catalog
            .iter()
            .position(|unit| unit.name == template.name)
        else {
            continue;
        };
        if template.dozer && factory(game, state, type_index, true).is_some() {
            state.work.insert(
                0,
                WorkOrder {
                    type_index,
                    producer: None,
                    complete: false,
                    resource_gatherer: false,
                },
            );
            state.team_delay = 0;
            start_training(game, state, 0);
            return;
        }
    }
}

pub(super) fn update(game: &mut Skirmish, state: &mut State) {
    // Default TeamPrototype has unlimited build time; completed one-unit
    // requests leave the build queue before queueUnits validates factories.
    state.work.retain(|order| !order.complete);
    if !game.map_build_allowed(state.owner, true) {
        return;
    }
    state.team_delay -= 1;
    if state.team_delay >= 1 {
        return;
    }
    super::collectors::queue(game, state);
    for index in 0..state.work.len() {
        if state.work[index].producer.is_none() {
            start_training(game, state, index);
        } else if state.work[index]
            .producer
            .is_some_and(|id| !game.entities[id].exists() || game.entities[id].owner != state.owner)
        {
            state.work[index].producer = None;
        }
    }
    state.team_delay = 150;
}

pub(super) fn start_training(game: &mut Skirmish, state: &mut State, index: usize) {
    let type_index = state.work[index].type_index;
    if let Some(producer) = factory(game, state, type_index, true)
        && game.queue_unit(producer, type_index).is_ok()
    {
        state.work[index].producer = Some(producer);
    }
}

pub(super) fn factory(
    game: &Skirmish,
    state: &mut State,
    type_index: usize,
    busy_ok: bool,
) -> Option<usize> {
    let mut busy = None;
    for record in &mut state.records {
        let Some(id) = record.object else {
            continue;
        };
        let actor = &game.entities[id];
        if actor.exists() && actor.owner != state.owner {
            record.object = None;
            continue;
        }
        let producer = &game.catalog[actor.type_index];
        if !actor.exists()
            || actor.owner != state.owner
            || actor.under_construction()
            || actor.selling()
            || producer.production.is_none()
            || !game.production_possible(id, type_index)
        {
            continue;
        }
        if actor.queue.is_empty() {
            return Some(id);
        }
        if busy_ok {
            busy = Some(id);
        }
    }
    busy
}

impl Skirmish {
    pub(in crate::skirmish) fn map_npc_unit_produced(&mut self, factory: usize, unit: usize) {
        let owner = self.entities[unit].owner;
        let type_index = self.entities[unit].type_index;
        let mut states = std::mem::take(&mut self.map_scripts.npc_builds);
        let mut resource = false;
        for state in &mut states {
            if state.owner != owner {
                continue;
            }
            state.team_delay = 0;
            if let Some(order) = state.work.iter_mut().find(|order| {
                !order.complete
                    && order.producer == Some(factory)
                    && self.catalog[type_index].equivalent_to(&self.catalog[order.type_index])
            }) {
                order.complete = true;
                order.producer = None;
                resource = order.resource_gatherer;
                if self.catalog[type_index].supply.capacity > 0 {
                    self.entities[unit].ai.supply.force_wanting = resource;
                    if resource {
                        super::collectors::assign(self, state, unit);
                    }
                }
                break;
            }
        }
        self.map_scripts.npc_builds = states;
        if !resource
            && self.catalog[type_index]
                .kind_of
                .iter()
                .any(|flag| flag == "DOZER")
        {
            for state in &mut self.map_scripts.npc_builds {
                if state.owner == owner && !super::repair::produced(state, unit) {
                    state.build_delay = 0;
                    state.structure_timer = 1.0;
                    state.team_delay = 0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_priority_dozer_request_retries_and_matches_the_producing_factory() {
        let mut game = super::super::tests::game(Vec::new());
        let owner = game.map_side_state.owners["NPC"];
        game.catalog[0].production = Some(Vec::new());
        game.catalog[0].buildable_objects = vec![game.catalog[1].name.clone()];
        let factory = game.spawn(0, owner, [650.0, 300.0], true);
        game.layout.map_build_lists[0].entries.clear();
        game.map_scripts.npc_builds.clear();
        game.initialize_map_build_lists().unwrap();
        let mut states = std::mem::take(&mut game.map_scripts.npc_builds);
        let state = &mut states[0];
        game.players[owner].cash = 0;
        queue_dozer(&mut game, state);
        assert_eq!(state.work.len(), 1);
        assert!(state.work[0].producer.is_none());
        assert!(game.entities[factory].queue.is_empty());
        game.players[owner].cash = 1000;
        update(&mut game, state);
        assert!(state.work[0].producer.is_none());
        game.set_map_build_allowed(owner, true, true);
        update(&mut game, state);
        assert_eq!(state.work[0].producer, Some(factory));
        assert_eq!(game.entities[factory].queue.len(), 1);
        assert_eq!(game.players[owner].cash, 800);
        game.map_scripts.npc_builds = states;
        let worker = game.spawn(1, owner, [650.0, 325.0], true);
        game.map_unit_produced(Some(factory), worker);
        let state = &game.map_scripts.npc_builds[0];
        assert!(state.work[0].complete);
        assert!(state.work[0].producer.is_none());
        assert_eq!(state.structure_timer.to_bits(), 1.0_f32.to_bits());
        assert_eq!(
            game.team_membership.team_name(
                game.team_membership
                    .team_of(worker)
                    .expect("object native team")
            ),
            "teamNPC"
        );
    }
}
