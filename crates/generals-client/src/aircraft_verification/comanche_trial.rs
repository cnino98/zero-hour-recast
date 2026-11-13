//! Paid `MiG` missile exposures use the original Comanche evasion and helipad repair.
use super::{construct, construct_type, details, friendly_entity, produce_type, template};
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

pub fn produce_mig(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    construct(game, builder, Kind::SupplyCenter, 1)?;
    let kind = template(game, "ChinaAirfield")?;
    let airfield = construct_type(game, builder, kind, 1)?;
    let kind = template(game, "ChinaJetMIG")?;
    produce_type(game, airfield, kind, 1)
}

enum Phase {
    Ready,
    Missile,
    Outcome { serial: u64, frame: u64 },
    Repair,
}

pub struct Exposure {
    aircraft: usize,
    defender: usize,
    airfield: usize,
    phase: Phase,
    first: usize,
    attempts: u8,
    pub diverted: bool,
}

impl Exposure {
    pub const fn new(aircraft: usize, defender: usize, airfield: usize) -> Self {
        Self {
            aircraft,
            defender,
            airfield,
            phase: Phase::Ready,
            first: 0,
            attempts: 0,
            diverted: false,
        }
    }

    pub fn observe(&mut self, game: &mut Skirmish) -> Result<(), String> {
        if self.diverted {
            return Ok(());
        }
        self.advance(game).map_err(|error| {
            format!(
                "{error}; attempt={} tick={}; Comanche attacker={:?}; {}; MiG attacker={:?}; {}",
                self.attempts,
                game.tick,
                game.entities[self.aircraft].last_attacker(),
                details(game, self.aircraft),
                game.entities[self.defender].last_attacker(),
                details(game, self.defender),
            )
        })
    }

    fn advance(&mut self, game: &mut Skirmish) -> Result<(), &'static str> {
        if !game.entities[self.aircraft].alive() || !game.entities[self.defender].alive() {
            return Err("paid MiG or Comanche lost during missile exposure");
        }
        match self.phase {
            Phase::Ready => {
                let mig = &game.entities[self.defender];
                if mig.aircraft_phase() == Some("idle")
                    && game.height_above_ground(self.defender) < 10.0
                    && mig.weapon_rounds_remaining(0) == 2
                {
                    self.approach(game)?;
                }
            }
            Phase::Missile => self.observe_launch(game)?,
            Phase::Outcome { serial, frame } => {
                self.diverted = game.projectiles.iter().any(|missile| {
                    missile.id == serial
                        && missile.target_object().is_some_and(|id| {
                            game.catalog[game.entities[id].type_index].name == "CountermeasureFlare"
                        })
                });
                if !self.diverted && game.tick >= frame + 180 {
                    if self.attempts == 12 {
                        return Err("no missile diverted in twelve paid MiG sorties");
                    }
                    let aircraft = &game.entities[self.aircraft];
                    if aircraft.health.to_bits() == aircraft.max_health.to_bits() {
                        self.phase = Phase::Ready;
                    } else {
                        game.materialize_selection_group(0, &[self.aircraft]);
                        game.repair(self.aircraft, self.airfield)?;
                        self.phase = Phase::Repair;
                    }
                }
            }
            Phase::Repair => {
                let aircraft = &game.entities[self.aircraft];
                if aircraft.health.to_bits() == aircraft.max_health.to_bits()
                    && aircraft.aircraft_phase() == Some("idle")
                    && game.height_above_ground(self.aircraft) > 10.0
                {
                    self.phase = Phase::Ready;
                }
            }
        }
        Ok(())
    }

    fn approach(&mut self, game: &mut Skirmish) -> Result<(), &'static str> {
        let home = game.entities[self.airfield].position;
        let other = game.entities[self.defender].position;
        let [dx, dy] = [other[0] - home[0], other[1] - home[1]];
        let distance = dx.hypot(dy).max(1.0);
        // Keep the Comanche away from the MiG's ground taxi. Its original
        // weapon set cannot fire on an airborne vehicle.
        game.move_to(
            self.aircraft,
            [
                (dx / distance).mul_add(250.0, home[0]),
                (dy / distance).mul_add(250.0, home[1]),
            ],
        )?;
        game.attack(self.defender, self.aircraft)?;
        self.first = game.entities.len();
        self.attempts += 1;
        self.phase = Phase::Missile;
        Ok(())
    }

    fn observe_launch(&mut self, game: &mut Skirmish) -> Result<(), &'static str> {
        if let Some(projectile) = game.projectiles.iter().find(|projectile| {
            projectile.source == self.defender
                && projectile.target_object() == Some(self.aircraft)
                && projectile.object.is_some_and(|id| id >= self.first)
        }) {
            self.phase = Phase::Outcome {
                serial: projectile.id,
                frame: game.tick,
            };
            // JetAI Stop sends the MiG back to its runway, cancelling further
            // fire without removing the missile already in flight.
            game.stop(self.defender)?;
        }
        Ok(())
    }
}
