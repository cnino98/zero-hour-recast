//! Paid original science, jet sorties and Bunker Buster garrison evacuation.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
        scout_construction_site,
    },
    strategy_verification::{research, template},
};
use generals_sim::skirmish::{Faction, Kind, ProjectileFxStage, Skirmish, WeaponFxEvent};
use std::error::Error;

pub struct Setup {
    pub airfield: usize,
    pub aircraft: usize,
    pub target: usize,
    pub occupants: Vec<usize>,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let airfield_type = template(game, "AmericaAirfield")?;
    let airfield = construct_type(game, builder, airfield_type, 0)?;
    let fighter = template(game, "AmericaJetStealthFighter")?;
    let cash = game.players[0].cash;
    require(
        !game.prerequisites_met(0, fighter)
            && game.train(airfield, fighter).is_err()
            && game.players[0].cash == cash,
        "Stealth Fighter bypassed its unpaid original science",
    )?;
    let points = game.players[0].science_points;
    game.purchase_science(0, "SCIENCE_StealthFighter")?;
    require(
        game.players[0].science_points + 1 == points,
        "Stealth Fighter science did not spend one promotion point",
    )?;
    let aircraft = produce_type(game, airfield, fighter, 0)?;
    let enemy_builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy_builder, Kind::PowerPlant, 1)?;
    construct(game, enemy_builder, Kind::SupplyCenter, 1)?;
    let barracks = construct(game, enemy_builder, Kind::Barracks, 1)?;
    let bunker = template(game, "ChinaBunker")?;
    let target = isolated_bunker(game, enemy_builder, bunker)?;
    let hunter = template(game, "ChinaInfantryTankHunter")?;
    let mut occupants = Vec::new();
    for _ in 0..2 {
        let id = produce_type(game, barracks, hunter, 1)?;
        game.enter(id, target)?;
        advance_until(
            game,
            7200,
            "paid Tank Hunter entering original bunker",
            |game| game.entities[id].contained_by() == Some(target),
        )?;
        occupants.push(id);
    }
    let point = game.entities[target].position;
    game.move_to(builder, [point[0], point[1] - 190.0])?;
    advance_until(
        game,
        7200,
        "paid dozer scouting Stealth Fighter target",
        |game| game.logically_visible(0, target),
    )?;
    advance_until(game, 300, "Stealth Fighter innate cloak", |game| {
        game.entities[aircraft].stealthed()
    })?;
    Ok(Setup {
        airfield,
        aircraft,
        target,
        occupants,
    })
}

pub fn approach(game: &mut Skirmish, id: usize, target: usize) -> Result<(), Box<dyn Error>> {
    let point = game.entities[target].position;
    game.move_to(id, [point[0], point[1] - 700.0])?;
    advance_until(
        game,
        7200,
        "Stealth Fighter taxi and airborne staging",
        |game| {
            let actor = &game.entities[id];
            actor.aircraft_phase() == Some("idle")
                && game.height_above_ground(id) > 30.0
                && (actor.position[0] - point[0]).hypot(actor.position[1] - point[1] + 700.0)
                    < 200.0
        },
    )
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    sortie(game, &setup)?;
    require(
        setup.occupants.iter().all(|&id| {
            game.entities[id].alive() && game.entities[id].contained_by() == Some(setup.target)
        }),
        "unupgraded missile busted original bunker occupants",
    )?;
    research(game, setup.airfield, "Upgrade_AmericaBunkerBusters")?;
    sortie(game, &setup)?;
    require(
        game.passengers(setup.target).is_empty()
            && setup.occupants.iter().all(|&id| !game.entities[id].alive()),
        "paid Bunker Buster failed original occupant damage and force-exit",
    )?;
    println!(
        "Paid Stealth Fighter: original science/production, innate cloak, two physical missiles, reveal/recloak, landing/rearm and paid Bunker Buster occupants passed at {}",
        game.tick
    );
    Ok(())
}

fn sortie(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let id = setup.aircraft;
    approach(game, id, setup.target)?;
    let health = game.entities[setup.target].health;
    let mut minimum = health;
    let mut shots = 0;
    let mut projectile = false;
    let mut revealed = false;
    let mut landing = false;
    let mut trace = SortieTrace {
        fighter: id,
        target: setup.target,
        seen: Vec::new(),
        ended: Vec::new(),
        hidden: Vec::new(),
        records: Vec::new(),
        previous: game.object_position(id),
        structures: game
            .entities
            .iter()
            .enumerate()
            .filter(|(_, actor)| game.catalog[actor.type_index].kind.is_building())
            .map(|(id, actor)| (id, actor.health))
            .collect(),
    };
    game.attack(id, setup.target)?;
    for _ in 0..9000 {
        game.step();
        trace.record(game);
        let actor = &game.entities[id];
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == id)
            .count();
        projectile |= game.projectiles.iter().any(|projectile| {
            projectile.source == id && projectile.render_transform(&game.entities).is_some()
        });
        revealed |= shots > 0 && !actor.stealthed();
        landing |= matches!(
            actor.aircraft_phase(),
            Some("landing" | "reload ammunition")
        );
        minimum = minimum.min(game.entities[setup.target].health);
        if shots > 0
            && actor.aircraft_phase() == Some("idle")
            && actor.weapon_rounds_remaining(0) == 2
            && game.height_above_ground(id) < 30.0
        {
            require(
                shots == 2 && projectile && revealed && landing && actor.stealthed(),
                "Stealth Fighter omitted original missile clip, firing reveal, recloak or rearm",
            )?;
            if minimum >= health {
                return Err(trace.failure(game, health, minimum).into());
            }
            return Ok(());
        }
    }
    Err(format!("Stealth Fighter sortie timed out shots={shots} projectile={projectile} reveal={revealed} landing={landing}; {}", crate::aircraft_verification::details(game, id)).into())
}

struct SortieTrace {
    fighter: usize,
    target: usize,
    seen: Vec<u64>,
    ended: Vec<u64>,
    hidden: Vec<u64>,
    records: Vec<String>,
    previous: [f32; 3],
    structures: Vec<(usize, f32)>,
}

impl SortieTrace {
    fn record(&mut self, game: &Skirmish) {
        let fighter = game.object_position(self.fighter);
        let delta: [f32; 3] = std::array::from_fn(|axis| fighter[axis] - self.previous[axis]);
        self.previous = fighter;
        for projectile in game
            .projectiles
            .iter()
            .filter(|projectile| projectile.source == self.fighter)
        {
            let born = !self.seen.contains(&projectile.id);
            if born {
                self.seen.push(projectile.id);
            }
            let hidden = projectile.render_transform(&game.entities).is_none()
                && !self.hidden.contains(&projectile.id);
            if hidden {
                self.hidden.push(projectile.id);
            }
            if (born || hidden || game.tick.is_multiple_of(30)) && self.records.len() < 64 {
                self.records.push(format!("tick={} missile={} age={} pos={:?} visible={} fighter={fighter:?} frame_delta={delta:?}",
                    game.tick, projectile.id, game.tick - projectile.created_frame(&game.entities).unwrap_or(game.tick),
                    projectile.effect_transform(&game.entities).map(|transform| transform.0), projectile.render_transform(&game.entities).is_some()));
            }
        }
        for event in &game.presentation.weapon_fx {
            if let WeaponFxEvent::Projectile(event) = event
                && event.source == self.fighter
                && matches!(
                    event.stage,
                    ProjectileFxStage::Detonation | ProjectileFxStage::Lasered
                )
            {
                self.records.push(format!(
                    "tick={} missile={} detonation={} pos={:?} target_pos={:?} target_health={}",
                    game.tick,
                    event.projectile,
                    matches!(event.stage, ProjectileFxStage::Detonation),
                    event.position,
                    game.object_position(self.target),
                    game.entities[self.target].health
                ));
            }
        }
        for (id, health) in &mut self.structures {
            let actor = &game.entities[*id];
            if actor.health < *health {
                self.records.push(format!(
                    "tick={} building={} id={id} pos={:?} health={health}->{}",
                    game.tick,
                    game.catalog[actor.type_index].name,
                    game.object_position(*id),
                    actor.health
                ));
            }
            *health = actor.health;
        }
        for &id in &self.seen {
            if !self.ended.contains(&id)
                && !game
                    .projectiles
                    .iter()
                    .any(|projectile| projectile.id == id)
            {
                self.ended.push(id);
                self.records.push(format!(
                    "tick={} missile={id} removed target_health={}",
                    game.tick, game.entities[self.target].health
                ));
            }
        }
    }

    fn failure(&self, game: &Skirmish, health: f32, minimum: f32) -> String {
        let target = &game.entities[self.target];
        let unit = &game.catalog[target.type_index];
        let weapon = &game.catalog[game.entities[self.fighter].type_index].weapon;
        format!(
            "Stealth Fighter missile did not damage its target: tick={} health={health} minimum={minimum} target={} owner={} pos={:?} armor={} minimum_health={} radius={:?} damage_type={} damage={} geometry_center={} sphere_radius={}; trace={:?}",
            game.tick,
            unit.name,
            target.owner,
            game.object_position(self.target),
            unit.armor[weapon.damage_type as usize],
            unit.minimum_health,
            weapon
                .area
                .as_ref()
                .map(|area| [area.primary_radius, area.secondary_radius]),
            weapon.damage_type as usize,
            weapon.damage,
            unit.geometry_bounds.center_height,
            unit.geometry_bounds.sphere_radius,
            self.records
        )
    }
}

fn isolated_bunker(
    game: &mut Skirmish,
    builder: usize,
    kind: usize,
) -> Result<usize, Box<dyn Error>> {
    let center = friendly_entity(game, Kind::CommandCenter, 1)?;
    let base = game.entities[center].position;
    let point = [
        [700.0, -300.0],
        [-700.0, -300.0],
        [0.0, -700.0],
        [500.0, -700.0],
        [-500.0, -700.0],
        [500.0, 300.0],
        [-500.0, 300.0],
        [1000.0, -500.0],
        [-1000.0, -500.0],
        [0.0, -1000.0],
    ]
    .into_iter()
    .filter_map(|offset| game.placement_near(kind, [base[0] + offset[0], base[1] + offset[1]]))
    .find(|point| {
        game.entities
            .iter()
            .filter(|actor| {
                let unit = &game.catalog[actor.type_index];
                actor.alive()
                    && !unit.no_collide()
                    && unit.kind_of.iter().any(|kind| kind == "STRUCTURE")
            })
            .all(|actor| {
                let bounds = &game.catalog[actor.type_index].geometry_bounds;
                (actor.position[0] - point[0]).hypot(actor.position[1] - point[1])
                    > 300.0 + bounds.sphere_radius
            })
    })
    .ok_or("no legal Bunker site with an unobstructed original missile approach")?;
    scout_construction_site(game, builder, kind, point)?;
    let cash = game.players[1].cash;
    let target = game.build(builder, kind, point)?;
    require(
        cash.checked_sub(game.catalog[kind].cost) == Some(game.players[1].cash)
            && !game.entities[target].complete(),
        "isolated Bunker skipped original paid construction",
    )?;
    advance_until(game, 7200, "paid isolated Bunker construction", |game| {
        game.entities[target].complete()
    })?;
    Ok(target)
}
