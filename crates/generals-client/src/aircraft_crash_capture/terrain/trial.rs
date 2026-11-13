use crate::{
    aircraft_verification::{self, crash},
    skirmish_verification::{advance_until, produce_type, require},
    strategy_verification::template,
};
use bevy::math::Vec2;
use generals_sim::skirmish::{DeathType, Order, Skirmish};
use std::error::Error;

// Killing Fields' authored plane is at 15 across the entire playable map.
const WATER: f32 = 15.0;
pub(super) const WATER_DROP_FRAMES: u64 = 15;

#[derive(Clone, Copy, Debug)]
pub(super) enum Site {
    Bridge,
    Water,
}

impl Site {
    const fn position(self) -> [f32; 2] {
        match self {
            Self::Bridge => [208.0, 2145.0], // First Tampico marker pair over the river.
            Self::Water => [860.0, 1600.0],  // Deep section of the river away from bridges.
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Fall {
    pub canopy: usize,
    pub pilot: usize,
}

#[derive(Debug)]
enum Phase {
    Priming,
    StartPass,
    Crossing,
    Lethal,
}

pub(super) struct Trial {
    pub aircraft: usize,
    pub fall: Option<Fall>,
    attacker: usize,
    site: Site,
    layer: Option<u8>,
    phase: Phase,
    shot_damage: f32,
    shot_count: u16,
    last_shot: Option<(u64, [f32; 2], f32)>,
    release_position: Option<[f32; 3]>,
}

impl Trial {
    pub fn prepare(game: &mut Skirmish, site: Site) -> Result<Self, Box<dyn Error>> {
        if matches!(site, Site::Water) {
            return Self::prepare_water(game);
        }
        let (aircraft, factory) = crash::prepare_veteran_raptor(game)?;
        let kind = template(game, "ChinaTankGattling")?;
        let attacker = produce_type(game, factory, kind, 1)?;
        let point = site.position();
        let layer = game.radar_bridge_at(point).map(|(layer, _, _)| layer);
        require(
            game.ground_height(point) < WATER
                && match site {
                    Site::Bridge => layer.is_some_and(|layer| game.radar_bridge_working(layer)),
                    Site::Water => layer.is_none(),
                },
            "the original map has no required bridge-over-water or open-water site",
        )?;
        let weapon = game.catalog[game.entities[attacker].type_index]
            .secondary_weapon
            .as_ref()
            .ok_or("the original Gattling lacks its anti-air weapon")?;
        let armor = game.catalog[game.entities[aircraft.raptor].type_index].armor
            [weapon.damage_type as usize];
        let trial = Self {
            aircraft: aircraft.raptor,
            attacker,
            fall: None,
            site,
            layer,
            phase: Phase::Priming,
            shot_damage: weapon.damage * armor,
            shot_count: 0,
            last_shot: None,
            release_position: None,
        };
        trial.stage_defender(game)?;
        Ok(trial)
    }

    fn prepare_water(game: &mut Skirmish) -> Result<Self, Box<dyn Error>> {
        let (aircraft, factory) = crash::prepare_veteran_comanche(game)?;
        let kind = template(game, "ChinaTankGattling")?;
        let attacker = produce_type(game, factory, kind, 1)?;
        let site = Site::Water;
        let point = site.position();
        require(
            game.ground_height(point) < WATER && game.radar_bridge_at(point).is_none(),
            "original water fixture requires an off-bridge river site",
        )?;
        let trial = Self {
            aircraft,
            fall: None,
            attacker,
            site,
            layer: None,
            phase: Phase::Lethal,
            shot_damage: 0.0,
            shot_count: 0,
            last_shot: None,
            release_position: None,
        };
        trial.stage_defender(game)?;
        game.materialize_selection_group(0, &[aircraft]);
        game.move_to(aircraft, point)?;
        advance_until(
            game,
            3600,
            "veteran Comanche settles in original deep river reach",
            |game| {
                let actor = &game.entities[aircraft];
                actor.alive()
                    && game.height_above_ground(aircraft) > 80.0
                    && !actor.moving()
                    && distance(actor.position, point) < 15.0
            },
        )
        .map_err(|error| format!("{error}; {}", trial.details(game)))?;
        Ok(trial)
    }

    fn stage_defender(&self, game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
        let position = self.defender_goal(game)?;
        game.move_to(self.attacker, position)?;
        advance_until(
            game,
            7200,
            "paid anti-air unit reaches actual riverbank",
            |game| {
                let actor = &game.entities[self.attacker];
                matches!(actor.ai.order, Order::Idle)
                    && Vec2::from_array(actor.position).distance(Vec2::from_array(position)) < 25.0
            },
        )
    }

    fn defender_goal(&self, game: &Skirmish) -> Result<[f32; 2], &'static str> {
        let center = Vec2::from_array(self.site.position());
        let home = Vec2::from_array(game.entities[self.attacker].position);
        let bank = (home - center).normalize_or_zero();
        [290.0, 310.0, 330.0]
            .into_iter()
            .find_map(|distance| {
                [
                    0.0, -0.25, 0.25, -0.5, 0.5, -0.75, 0.75, -1.0, 1.0, -1.25, 1.25, -1.5, 1.5,
                ]
                .into_iter()
                .map(|angle| (center + Vec2::from_angle(angle).rotate(bank) * distance).to_array())
                .find(|&position| {
                    game.ground_height(position) > WATER
                        && game.build_site_reachable(self.attacker, position)
                })
            })
            .ok_or("no paid anti-air route to dry shoreline within original weapon range")
    }

    pub fn begin(&self, game: &mut Skirmish) -> Result<(), &'static str> {
        if matches!(self.site, Site::Water) {
            return game.attack(self.attacker, self.aircraft);
        }
        let [x, y] = self.site.position();
        game.group_guard_world(&[self.aircraft], [x, y, game.ground_height([x, y])], true)
    }

    fn fly_to_start(&mut self, game: &mut Skirmish) -> Result<(), &'static str> {
        let [x, y] = self.site.position();
        // The original Gattling's automatic acquisition is limited by
        // its 150-unit vision, while a legal ordered shot reaches 350.
        // Stop the volley at one shot's remaining health, then approach
        // from the side opposite the bank where the same Gattling is staged.
        game.stop(self.attacker)?;
        game.move_to(self.aircraft, [x, y + 480.0])?;
        self.phase = Phase::StartPass;
        Ok(())
    }

    fn observe_flight(&mut self, game: &mut Skirmish) -> Result<(), &'static str> {
        let jet = &game.entities[self.aircraft];
        let center = self.site.position();
        let attack = match self.phase {
            Phase::Priming => self.near_site(jet.position),
            Phase::Crossing => self.crossing_window(jet.position, jet.facing),
            _ => false,
        };
        match self.phase {
            Phase::Priming => {
                if jet.health <= self.shot_damage {
                    return self.fly_to_start(game);
                }
            }
            Phase::StartPass => {
                if distance(jet.position, [center[0], center[1] + 480.0]) < 80.0 {
                    game.move_to(self.aircraft, [center[0], center[1] - 420.0])?;
                    self.phase = Phase::Crossing;
                }
            }
            Phase::Crossing => {
                if attack && game.logically_visible(1, self.aircraft) {
                    self.phase = Phase::Lethal;
                }
            }
            Phase::Lethal => {}
        }
        if attack
            && game.logically_visible(1, self.aircraft)
            && !matches!(game.entities[self.attacker].ai.order, Order::Attack(id) if id == self.aircraft)
        {
            game.attack(self.attacker, self.aircraft)?;
        }
        Ok(())
    }

    pub fn observe(&mut self, game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
        if self.fall.is_some() {
            return Ok(());
        }
        let jet = &game.entities[self.aircraft];
        if game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == self.attacker && shot.weapon_slot == 1)
        {
            self.shot_count += 1;
            self.last_shot = Some((game.tick, jet.position, jet.health));
        }
        if jet.alive() {
            if matches!(self.site, Site::Bridge) {
                self.observe_flight(game)?;
            }
            return Ok(());
        }
        require(
            jet.last_attacker() == Some(self.attacker),
            &self.details(game),
        )?;
        let rider = game.entities.iter().position(|actor| {
            actor.producer() == Some(self.aircraft)
                && game.catalog[actor.type_index].name == "AmericaInfantryPilot"
                && actor.veterancy == jet.veterancy
        });
        if let Some(pilot) = rider {
            let crew = &game.entities[pilot];
            if let Some(canopy) = crew.contained_by() {
                require(
                    game.catalog[game.entities[canopy].type_index].name == "AmericaParachute"
                        && game.entities[canopy].owner == crew.owner
                        && crew.parachute_rider(),
                    "airborne release lost its original rider or canopy",
                )?;
                self.release_position = Some(game.object_position(canopy));
                self.fall = Some(Fall { canopy, pilot });
            }
        }
        Ok(())
    }

    fn crossing_window(&self, [x, y]: [f32; 2], facing: f32) -> bool {
        let [center_x, center_y] = self.site.position();
        (x - center_x).abs() < 25.0
            && (center_y - 45.0..=center_y + 45.0).contains(&y)
            && facing.sin() < -0.8
    }

    fn near_site(&self, point: [f32; 2]) -> bool {
        distance(point, self.site.position()) < 135.0
    }

    pub fn over_surface(&self, game: &Skirmish, id: usize) -> bool {
        let actor = &game.entities[id];
        match self.site {
            Site::Bridge => Some(actor.layer()) == self.layer,
            Site::Water => actor.layer() == 1 && game.ground_height(actor.position) < WATER,
        }
    }

    pub fn completed(&self, game: &Skirmish) -> Result<bool, Box<dyn Error>> {
        let Some(Fall { canopy, pilot }) = self.fall else {
            return Ok(false);
        };
        let rider = &game.entities[pilot];
        if rider.contained_by().is_some() || game.entities[canopy].alive() {
            return Ok(false);
        }
        let correct = self.over_surface(game, pilot)
            && match self.site {
                Site::Bridge => {
                    rider.alive()
                        && rider.selectable(&game.catalog[rider.type_index])
                        && !rider.parachute_rider()
                        && game.height_above_ground(pilot).abs() < 0.1
                }
                Site::Water => {
                    self.retains_water_outcome(game, Fall { canopy, pilot })
                        && game.object_position(canopy)[2]
                            > game.ground_height(game.entities[canopy].position)
                }
            };
        require(
            correct,
            &format!(
                "paid {:?} ejection hit the wrong surface: {}",
                self.site,
                self.details(game)
            ),
        )?;
        Ok(true)
    }

    pub fn retains_water_outcome(&self, game: &Skirmish, Fall { canopy, pilot }: Fall) -> bool {
        let rider = &game.entities[pilot];
        let chute = &game.entities[canopy];
        self.over_surface(game, pilot)
            && self.over_surface(game, canopy)
            && !rider.alive()
            && rider.contained_by().is_none()
            && matches!(rider.death_type(), DeathType::Flooded)
            && !chute.alive()
            && chute.corpse_present()
            && chute.has_drawable()
    }

    pub fn details(&self, game: &Skirmish) -> String {
        let fall = self.fall.map(|fall| {
            [fall.canopy, fall.pilot].map(|id| {
                let actor = &game.entities[id];
                (
                    id,
                    game.object_position(id),
                    actor.layer(),
                    game.height_above_ground(id),
                    actor.health,
                    actor.death_type() as u8,
                )
            })
        });
        format!(
            "{:?} goal {:?} phase {:?}; {}; AA unit {} at {:?}; shots={} last={:?} ejected={:?} fall={fall:?}",
            self.site,
            self.site.position(),
            self.phase,
            aircraft_verification::details(game, self.aircraft),
            self.attacker,
            game.entities[self.attacker].position,
            self.shot_count,
            self.last_shot,
            self.release_position
        )
    }
}

fn distance(first: [f32; 2], second: [f32; 2]) -> f32 {
    Vec2::from_array(first).distance(Vec2::from_array(second))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires original game installation"]
    fn paid_killing_fields_parachutes_release_on_actual_bridge_and_water() {
        let root = generals_assets::game_root().unwrap();
        let map = crate::radar::bridge_destruction_capture::map_name();
        let mut game = crate::playtest::map_fixture::Fixture::load(&root, map)
            .unwrap()
            .into_simulation()
            .unwrap();
        for site in [Site::Bridge, Site::Water] {
            let mut trial = Trial::prepare(&mut game, site).unwrap();
            trial.begin(&mut game).unwrap();
            let mut saw_open = false;
            let mut complete = false;
            for _ in 0..7200 {
                trial
                    .observe(&mut game)
                    .unwrap_or_else(|error| panic!("{error}; {}", trial.details(&game)));
                if let Some(fall) = trial.fall {
                    saw_open |= game.entities[fall.canopy].parachute_opened()
                        && trial.over_surface(&game, fall.canopy);
                }
                if trial.completed(&game).unwrap() {
                    complete = true;
                    break;
                }
                game.step();
            }
            println!("{}", trial.details(&game));
            if complete && matches!(site, Site::Water) {
                let fall = trial.fall.expect("completed water trial has no fall");
                let first_height = game.height_above_ground(fall.canopy);
                for _ in 0..WATER_DROP_FRAMES {
                    game.step();
                }
                assert!(trial.retains_water_outcome(&game, fall));
                assert!(first_height > 0.0);
                assert!(game.height_above_ground(fall.canopy).abs() < 0.1);
            }
            assert!(
                saw_open && complete,
                "{site:?} did not complete an actual canopy and surface sequence"
            );
        }
    }
}
