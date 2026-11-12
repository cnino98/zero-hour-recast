//! `PowerPlantUpdate`: authored rod state and its exact scheduler wake.
use crate::skirmish::{Entity, Skirmish, module_updates::FOREVER};

pub(super) struct State {
    extended: bool,
}

impl Entity {
    pub(in crate::skirmish) fn power_plant_conditions(&self) -> impl Iterator<Item = &'static str> {
        let flags = self.maintenance.power_conditions;
        [
            (flags & 1 != 0).then_some("POWER_PLANT_UPGRADING"),
            (flags & 2 != 0).then_some("POWER_PLANT_UPGRADED"),
        ]
        .into_iter()
        .flatten()
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_power_plant(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].maintenance.power_plants.len(), ordinal);
        self.entities[id]
            .maintenance
            .power_plants
            .push(State { extended: false });
    }

    pub(in crate::skirmish) fn extend_power_rods(
        &mut self,
        id: usize,
        ordinal: usize,
        extend: bool,
    ) {
        let actor = &mut self.entities[id];
        let state = &mut actor.maintenance.power_plants[ordinal];
        if extend {
            if state.extended {
                return;
            }
            state.extended = true;
            if actor.has_drawable() {
                actor.maintenance.power_conditions |= 1;
            }
            let delay = self.catalog[actor.type_index].maintenance.power_plants[ordinal];
            self.wake_family_update(id, "PowerPlantUpdate", ordinal, delay);
        } else {
            state.extended = false;
            if actor.has_drawable() {
                actor.maintenance.power_conditions = 0;
            }
            self.wake_family_update(id, "PowerPlantUpdate", ordinal, FOREVER);
        }
    }

    pub(in crate::skirmish) fn extend_all_power_rods(&mut self, id: usize, extend: bool) {
        for ordinal in 0..self.entities[id].maintenance.power_plants.len() {
            self.extend_power_rods(id, ordinal, extend);
        }
    }

    pub(in crate::skirmish) fn update_power_plant(&mut self, id: usize, ordinal: usize) -> i32 {
        let actor = &mut self.entities[id];
        actor.maintenance.power_plants[ordinal].extended = true;
        if actor.has_drawable() {
            actor.maintenance.power_conditions = (actor.maintenance.power_conditions & !1) | 2;
        }
        i32::from_ne_bytes(FOREVER.to_ne_bytes())
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Faction, Kind, tests};

    #[test]
    fn authored_rod_callback_sleeps_until_extension_and_repeat_does_not_restart() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::Usa, Kind::Defense);
        unit.maintenance.power_plants = vec![3];
        tests::register_update(&mut unit, "PowerPlantUpdate");
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [300.0, 200.0], true);
        game.tick = 10;
        game.run_object_updates();
        assert!(game.entities[id].power_plant_conditions().next().is_none());
        game.extend_power_rods(id, 0, true);
        assert_eq!(
            game.entities[id]
                .power_plant_conditions()
                .collect::<Vec<_>>(),
            ["POWER_PLANT_UPGRADING"]
        );
        game.tick = 12;
        game.extend_power_rods(id, 0, true);
        game.run_object_updates();
        assert_eq!(
            game.entities[id]
                .power_plant_conditions()
                .collect::<Vec<_>>(),
            ["POWER_PLANT_UPGRADING"]
        );
        game.tick = 13;
        game.run_object_updates();
        assert_eq!(
            game.entities[id]
                .power_plant_conditions()
                .collect::<Vec<_>>(),
            ["POWER_PLANT_UPGRADED"]
        );
        game.extend_power_rods(id, 0, false);
        assert!(game.entities[id].power_plant_conditions().next().is_none());
        game.tick = 100;
        game.run_object_updates();
        assert!(game.entities[id].power_plant_conditions().next().is_none());
    }
}
