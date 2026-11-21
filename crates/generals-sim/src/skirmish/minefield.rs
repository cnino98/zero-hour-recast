//! SH `GenerateMinefieldBehavior` and `MinefieldBehavior`, authored callback instances.
use super::{Skirmish, WeaponRules, module_updates::FOREVER};
use crate::native_real::{ceil as native_ceil, floor as native_floor};
use generation::clip_to_footprint;
use num_traits::ToPrimitive;

mod generation;
mod land_mine;
#[cfg(test)]
mod tests;

pub enum Placement {
    Smart { skip_interior: bool },
    Border,
    Footprint,
}

pub struct GeneratorRules {
    pub mine_name: String,
    pub upgraded_name: String,
    pub emp_upgrade: Option<String>,
    pub generation_fx: Option<String>,
    pub distance: f32,
    pub density: f32,
    pub placement: Placement,
    pub on_death: bool,
    pub circular: bool,
    pub upgradable: bool,
    pub jitter: f32,
    pub skip_under_structure: f32,
}

pub struct MineRules {
    pub weapon: Option<WeaponRules>,
    pub detonated_by: u8,
    pub stop_regeneration: bool,
    pub regenerates: bool,
    pub workers_detonate: bool,
    pub creator_check_frames: u32,
    pub scoot_frames: u32,
    pub virtual_mines: u32,
    pub repeat_move_threshold: f32,
    pub drain_percent: f32,
    pub creation: Option<super::object_creation::List>,
}

pub(super) struct State {
    generators: Vec<Generator>,
    mines: Vec<LandMine>,
}

struct Generator {
    target: Option<[f32; 3]>,
    generated: bool,
    upgraded: bool,
    mines: Vec<usize>,
}

struct LandMine {
    next_death_check: u32,
    scoot_frames: u32,
    velocity: [f32; 3],
    acceleration: [f32; 3],
    remaining: u32,
    immunes: [(Option<usize>, u32); 3],
    detonators: Vec<(usize, [f32; 3])>,
    ignore_damage: bool,
    regenerates: bool,
    draining: bool,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            generators: Vec::new(),
            mines: Vec::new(),
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn set_first_minefield_target(
        &mut self,
        id: usize,
        position: [f32; 3],
    ) {
        if let Some(generator) = self.entities[id].mines.generators.first_mut() {
            generator.target = Some(position);
        }
    }

    pub(in crate::skirmish) fn has_land_mine_interface(&self, id: usize) -> bool {
        !self.entities[id].mines.mines.is_empty()
    }

    pub(in crate::skirmish) fn construct_mine_generator(&mut self, id: usize, ordinal: usize) {
        let states = &mut self.entities[id].mines.generators;
        assert_eq!(states.len(), ordinal);
        states.push(Generator {
            target: None,
            generated: false,
            upgraded: false,
            mines: Vec::new(),
        });
    }

    pub(in crate::skirmish) fn update_mine_generator(&mut self, id: usize, ordinal: usize) -> i32 {
        let state = &self.entities[id].mines.generators[ordinal];
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .mine_generators[ordinal];
        if !rules.upgradable || state.upgraded {
            return i32::from_ne_bytes(FOREVER.to_ne_bytes());
        }
        // Source ignores the parsed UpgradedTriggeredBy string and checks only
        // this object's completed mask, not the player's completed upgrades.
        if state.generated
            && rules
                .emp_upgrade
                .as_ref()
                .is_some_and(|upgrade| self.object_has_upgrade(id, upgrade))
        {
            let state = &mut self.entities[id].mines.generators[ordinal];
            state.upgraded = true;
            let mines = std::mem::take(&mut state.mines);
            for mine in mines {
                if self.entities[mine].exists() {
                    self.destroy_maintenance_object(mine);
                }
            }
            self.entities[id].mines.generators[ordinal].generated = false;
            self.generate_minefield(id, ordinal);
        }
        1
    }

    pub(in crate::skirmish) fn mine_generator_on_die(&mut self, id: usize, ordinal: usize) {
        if self.catalog[self.entities[id].type_index]
            .behavior
            .mine_generators[ordinal]
            .on_death
        {
            self.generate_minefield(id, ordinal);
        }
    }

    pub(in crate::skirmish) fn construct_land_mine(&mut self, id: usize, ordinal: usize) {
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .minefields[ordinal];
        let state = &mut self.entities[id].mines.mines;
        assert_eq!(state.len(), ordinal);
        state.push(LandMine {
            next_death_check: 0,
            scoot_frames: 0,
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            remaining: rules.virtual_mines,
            immunes: [(None, 0); 3],
            detonators: Vec::new(),
            ignore_damage: false,
            regenerates: rules.regenerates,
            draining: false,
        });
        self.entities[id].object_status |= 1 << 26;
    }

    fn mine_sleep(&self, id: usize, ordinal: usize) -> u32 {
        let state = &self.entities[id].mines.mines[ordinal];
        if state.draining
            || state.scoot_frames > 0
            || state.immunes.iter().any(|(id, _)| id.is_some())
        {
            return 1;
        }
        let rules = &self.catalog[self.entities[id].type_index]
            .behavior
            .minefields[ordinal];
        let sleep = if state.regenerates && rules.stop_regeneration {
            FOREVER.min(state.next_death_check.wrapping_sub(self.update_frame()))
        } else {
            FOREVER
        };
        if sleep == 0 { 1 } else { sleep }
    }

    fn refresh_mine_mask(&mut self, id: usize, ordinal: usize) {
        let empty = self.entities[id].mines.mines[ordinal].remaining == 0;
        self.entities[id].maintenance.masked = empty;
        if self.entities[id].has_drawable() {
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(1 << 5, empty);
        }
    }
}

fn integer_real(value: i32) -> f32 {
    value.to_f32().expect("native Int to Real")
}

fn unsigned_real(value: u32) -> f32 {
    value.to_f32().expect("native UnsignedInt to Real")
}

// Reject domains where the source generator cannot advance its geometric loop.
pub(super) fn validate(
    unit: &super::UnitType,
    catalog: &[super::UnitType],
) -> Result<(), &'static str> {
    for rules in &unit.behavior.mine_generators {
        if [
            rules.distance,
            rules.density,
            rules.jitter,
            rules.skip_under_structure,
        ]
        .iter()
        .any(|value| !value.is_finite())
        {
            return Err("Non-finite minefield generation value");
        }
        for name in [&rules.mine_name, &rules.upgraded_name] {
            let Some(mine) = catalog.iter().find(|unit| &unit.name == name) else {
                continue;
            };
            if generation::circle_radius(&mine.geometry) <= 0.0 {
                return Err("Minefield generation requires a positive mine radius");
            }
        }
        if matches!(unit.geometry.shape, crate::collision_geometry::Shape::Box)
            && !matches!(rules.placement, Placement::Smart { .. })
            && (unit.geometry.major + rules.distance <= 0.0
                || unit.geometry.minor + rules.distance <= 0.0)
        {
            return Err("Minefield expanded rectangle must have positive edge lengths");
        }
    }
    for rules in &unit.behavior.minefields {
        if !rules.repeat_move_threshold.is_finite() || !rules.drain_percent.is_finite() {
            return Err("Non-finite land mine callback value");
        }
    }
    Ok(())
}
