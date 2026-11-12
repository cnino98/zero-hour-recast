//! Paid original Helix production, attachments, passengers and physical napalm bombs.
use crate::{
    playtest::MAP_NAME,
    skirmish_data,
    skirmish_verification::{
        advance_until, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::{research, template},
    terrain::Terrain,
    world_data,
};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::{error::Error, path::Path};

pub mod crash;

pub struct Setup {
    pub helicopters: [usize; 3],
    pub passengers: [usize; 5],
    pub target: usize,
    pub defenders: [usize; 2],
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let map = MapData::parse(&assets.read(MAP_NAME)?.bytes)?;
    let mut data = skirmish_data::load(&assets, &map.objects)?;
    let settings = world_data::camera_settings(&assets, root)?;
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )?;
    let (layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )?;
    let mut animations = crate::model_animation::load(&assets, &mut data.models, &data.types)?;
    animations.bind_logical_bones(&mut data.types);
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations)?;
    let options = root.join("UserData/Options.ini");
    drop(crate::control_bar::load(&assets, &data.types, &options)?);
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    verify(&mut game)
}

fn funded(game: &mut Skirmish, amount: u32) -> Result<(), Box<dyn Error>> {
    advance_until(
        game,
        18_000,
        "original supplies funding Helix commands",
        |game| game.players[0].cash >= amount,
    )
}

fn build(game: &mut Skirmish, builder: usize, name: &str) -> Result<usize, Box<dyn Error>> {
    let index = template(game, name)?;
    funded(game, game.catalog[index].cost)?;
    construct_type(game, builder, index, 0)
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    build(game, builder, "ChinaPowerPlant")?;
    build(game, builder, "ChinaSupplyCenter")?;
    let airfield = build(game, builder, "ChinaAirfield")?;
    let kind = template(game, "ChinaVehicleHelix")?;
    let mut helicopters = [0; 3];
    for (index, id) in helicopters.iter_mut().enumerate() {
        funded(game, game.catalog[kind].cost)?;
        *id = produce_type(game, airfield, kind, 0)?;
        require(
            game.entities[*id].producer() == Some(airfield),
            "Helix lost paid airfield producer",
        )?;
        let home = game.entities[airfield].position;
        let offset = [0.0, 150.0, 300.0][index];
        game.move_to(*id, [home[0] + 180.0, home[1] + offset])?;
    }
    for (id, upgrade) in helicopters.into_iter().zip([
        "Upgrade_ChinaHelixBattleBunker",
        "Upgrade_ChinaHelixGattlingCannon",
        "Upgrade_ChinaHelixPropagandaTower",
    ]) {
        research(game, id, upgrade)?;
        let child = game.entities[id]
            .attachment_child()
            .ok_or("Helix attachment was not created")?;
        require(
            game.entities[child].owner == 0 && !game.entities[child].masked(),
            "Helix portable attachment is hidden or foreign",
        )?;
        require(
            game.passengers(id).is_empty(),
            "Helix attachment occupied a real passenger slot",
        )?;
    }
    let barracks = build(game, builder, "ChinaBarracks")?;
    let infantry = template(game, "ChinaInfantryTankHunter")?;
    let mut passengers = [0; 5];
    for id in &mut passengers {
        funded(game, game.catalog[infantry].cost)?;
        *id = produce_type(game, barracks, infantry, 0)?;
        game.enter(*id, helicopters[0])?;
    }
    advance_until(
        game,
        3600,
        "five paid passengers entering the original Helix",
        |game| {
            passengers
                .iter()
                .all(|&id| game.entities[id].contained_by() == Some(helicopters[0]))
        },
    )?;
    require(
        game.passengers(helicopters[0]) == passengers,
        "Helix inventory lost original entry order",
    )?;
    require(
        passengers
            .iter()
            .all(|&id| game.entities[id].masked() && game.passenger_can_fire(id)),
        "Helix Bunker did not enclose and arm its infantry",
    )?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    // Complete later research infrastructure before the live-fire trial.
    build(game, builder, "ChinaWarFactory")?;
    let defenders = crash::prepare(game, target)?;
    stage_bombing_field(game, airfield, helicopters)?;
    Ok(Setup {
        helicopters,
        passengers,
        target,
        defenders,
    })
}

fn stage_bombing_field(
    game: &mut Skirmish,
    airfield: usize,
    helicopters: [usize; 3],
) -> Result<(), Box<dyn Error>> {
    let factory = template(game, "ChinaWarFactory")?;
    let storm = template(game, "BlackNapalmFirestormSmall")?;
    let radius = game.catalog[storm]
        .fire
        .as_ref()
        .unwrap()
        .storm
        .as_ref()
        .unwrap()
        .geometry
        .final_size[1];
    let site = choose_bombing_field(game, factory, game.entities[airfield].position, radius)?;
    for (id, offset) in helicopters.into_iter().zip([-100.0, 0.0, 100.0]) {
        let goal = [site[0] + offset, site[1]];
        game.move_to(id, goal).map_err(|error| {
            format!(
                "Helix{id} formation move failed from {:?} to {goal:?} at field {site:?}: {error}",
                game.entities[id].position
            )
        })?;
    }
    advance_until(
        game,
        3600,
        "paid Helix formation at clear bombing field",
        |game| {
            helicopters
                .iter()
                .zip([-100.0, 0.0, 100.0])
                .all(|(&id, offset)| {
                    let actor = &game.entities[id];
                    let dx = actor.position[0] - (site[0] + offset);
                    let dy = actor.position[1] - site[1];
                    actor.alive()
                        && matches!(actor.ai.order, generals_sim::skirmish::Order::Idle)
                        && !actor.moving()
                        && dy.mul_add(dy, dx * dx) < 100.0 * 100.0
                        && clear_bombing_point(game, actor.position, radius)
                })
        },
    )
    .map_err(|error| {
        format!(
            "{error}; {}",
            formation_state(game, helicopters, site, radius)
        )
    })?;
    println!(
        "  Helix live-fire field {site:?}, original max storm radius {radius}; three actual completed move orders outside structures and AA"
    );
    Ok(())
}

fn choose_bombing_field(
    game: &Skirmish,
    factory: usize,
    home: [f32; 2],
    radius: f32,
) -> Result<[f32; 2], Box<dyn Error>> {
    // Visibility uses ceil(navigation extent / cell size). Excluding its last
    // cell gives a conservative interior bound; clipped construction footprints
    // alone do not prove that a movement destination is on-map.
    let interior = game
        .visibility_dimensions()
        .map(|cells| f32::from(cells.saturating_sub(1)) * game.visibility_cell_size());
    let mut candidates = Vec::new();
    let grid = |limit| {
        std::iter::successors(Some(100.0), |value| Some(value + 100.0))
            .take_while(move |&value| value < limit)
    };
    for y in grid(interior[1]) {
        for x in grid(interior[0]) {
            let site = [x, y];
            if bombing_goals(site).into_iter().all(|goal| {
                goal.into_iter()
                    .zip(interior)
                    .all(|(coordinate, limit)| coordinate >= 0.0 && coordinate < limit)
                    && game.can_place(factory, goal)
            }) {
                candidates.push(site);
            }
        }
    }
    // Stable sorting preserves row/column order for equal-distance candidates.
    let distance = |point: [f32; 2]| {
        let dx = point[0] - home[0];
        let dy = point[1] - home[1];
        dy.mul_add(dy, dx * dx)
    };
    candidates.sort_by(|&left, &right| distance(left).total_cmp(&distance(right)));
    if let Some(&site) = candidates.iter().find(|&&site| {
        bombing_goals(site)
            .into_iter()
            .all(|goal| clear_bombing_point(game, goal, radius))
    }) {
        return Ok(site);
    }
    Err(format!(
        "no clear Helix bombing field: home={home:?}, conservative interior={interior:?}, original storm radius={radius}, terrain-compatible sites={}, {}",
        candidates.len(), bombing_clearance_diagnostic(game, &candidates, radius)
    ).into())
}

const fn bombing_goals(site: [f32; 2]) -> [[f32; 2]; 4] {
    // Three side-by-side aircraft fit the original camera's wide viewport.
    // The final point independently checks the commanded bombing approach.
    [
        [site[0] - 100.0, site[1]],
        site,
        [site[0] + 100.0, site[1]],
        [site[0] + 100.0, site[1]],
    ]
}

fn bombing_clearance(
    actor: &generals_sim::skirmish::Entity,
    unit: &generals_sim::skirmish::UnitType,
    radius: f32,
) -> f32 {
    // Reserve the 100-unit formation arrival tolerance around the full firestorm
    // and building bounds. Only an anti-air weapon requires the standoff used
    // for the separate Patriot trial.
    let footprint = unit.geometry_bounds.sphere_radius;
    let blast = radius + footprint + 100.0;
    (0..3)
        .filter_map(|slot| unit.weapon_template(actor.weapon_index(slot)))
        .filter(|weapon| weapon.damage > 0.0 && weapon.anti_mask & 1 != 0)
        .map(|weapon| weapon.range + footprint + 300.0)
        .fold(blast, f32::max)
}

pub fn clear_bombing_point(game: &Skirmish, point: [f32; 2], radius: f32) -> bool {
    game.entities
        .iter()
        .filter(|actor| !actor.masked() && game.catalog[actor.type_index].kind.is_building())
        .all(|actor| {
            let unit = &game.catalog[actor.type_index];
            let clearance = bombing_clearance(actor, unit, radius);
            let dx = point[0] - actor.position[0];
            let dy = point[1] - actor.position[1];
            dy.mul_add(dy, dx * dx) > clearance * clearance
        })
}

fn bombing_clearance_diagnostic(game: &Skirmish, sites: &[[f32; 2]], radius: f32) -> String {
    let mut best: Option<(f32, String)> = None;
    for &site in sites {
        let mut limiting: Option<(f32, [f32; 2], usize, f32, f32)> = None;
        for goal in bombing_goals(site) {
            for (id, actor) in game.entities.iter().enumerate() {
                let unit = &game.catalog[actor.type_index];
                if actor.masked() || !unit.kind.is_building() {
                    continue;
                }
                let required = bombing_clearance(actor, unit, radius);
                let distance = (goal[0] - actor.position[0]).hypot(goal[1] - actor.position[1]);
                let margin = distance - required;
                if limiting.as_ref().is_none_or(|&(old, ..)| margin < old) {
                    limiting = Some((margin, goal, id, distance, required));
                }
            }
        }
        if let Some((margin, goal, id, distance, required)) = limiting
            && best.as_ref().is_none_or(|&(old, ..)| margin > old)
        {
            let actor = &game.entities[id];
            best = Some((
                margin,
                format!(
                    "best site={site:?}, limiting goal={goal:?}, structure={id} {} at {:?}, distance={distance}, required={required}, clearance margin={margin}",
                    game.catalog[actor.type_index].name, actor.position
                ),
            ));
        }
    }
    best.map_or_else(
        || "no terrain-compatible site with a limiting structure".to_owned(),
        |(_, diagnostic)| diagnostic,
    )
}

fn formation_state(
    game: &Skirmish,
    helicopters: [usize; 3],
    site: [f32; 2],
    radius: f32,
) -> String {
    helicopters.into_iter().zip([-100.0, 0.0, 100.0]).map(|(id, offset)| {
        let actor = &game.entities[id];
        let goal = [site[0] + offset, site[1]];
        let violations: Vec<_> = game.entities.iter().filter_map(|building| {
            let unit = &game.catalog[building.type_index];
            if building.masked() || !unit.kind.is_building() { return None; }
            let required = bombing_clearance(building, unit, radius);
            let distance = (actor.position[0] - building.position[0])
                .hypot(actor.position[1] - building.position[1]);
            (distance <= required).then_some((&unit.name, distance, required))
        }).take(6).collect();
        format!("Helix{id} HP={} XYZ={:?} goal={goal:?} delta={} Idle={} moving={} clearance={violations:?}",
            actor.health, game.object_position(id),
            (actor.position[0] - goal[0]).hypot(actor.position[1] - goal[1]),
            matches!(actor.ai.order, generals_sim::skirmish::Order::Idle), actor.moving())
    }).collect::<Vec<_>>().join("; ")
}

pub fn arm_bomb(game: &mut Skirmish, helicopter: usize) -> Result<(), Box<dyn Error>> {
    research(game, helicopter, "Upgrade_HelixNapalmBomb")?;
    let ready = game
        .helix_bomb_ready_frame(helicopter)
        .ok_or("Helix bomb upgrade did not unpause recharge")?;
    require(
        ready == game.tick + 300 && !game.helix_bomb_ready(helicopter),
        "Helix bomb skipped its original first recharge",
    )?;
    advance_until(game, 301, "first original Helix napalm recharge", |game| {
        game.helix_bomb_ready(helicopter)
    })
}

fn drop_bomb(
    game: &mut Skirmish,
    helicopter: usize,
    target: [f32; 2],
    upgraded: bool,
) -> Result<(), Box<dyn Error>> {
    let start = game.entities.len();
    game.helix_bomb(helicopter, target)?;
    advance_until(
        game,
        1800,
        "original physical Helix napalm release",
        |game| {
            game.entities[helicopter]
                .helix_bomb_object()
                .is_some_and(|id| id >= start)
        },
    )?;
    let bomb = game.entities[helicopter].helix_bomb_object().unwrap();
    // PhysicsBehavior's constructor wakes at current frame + UPDATE_SLEEP_NONE
    // (one), so a new bomb retains its source creation XYZ until the next frame.
    require(
        game.object_position(bomb).map(f32::to_bits)
            == game.object_position(helicopter).map(f32::to_bits),
        "Helix bomb integrated before its source creation phase completed",
    )?;
    require(
        game.catalog[game.entities[bomb].type_index].name == "NapalmBomb"
            && game.object_position(bomb)[2]
                > game.ground_height(game.entities[bomb].position) + 1.0,
        "Helix did not release its real airborne NapalmBomb",
    )?;
    // An ordinary player Stop must leave the persistent physical object alive.
    game.stop(helicopter)?;
    require(
        game.entities[bomb].alive(),
        "Stopping the Helix destroyed its persistent bomb",
    )?;
    let forward_z = game.entities[bomb]
        .physical_basis()
        .ok_or("missing NapalmBomb physics")?[2][0];
    game.step();
    require(
        game.entities[bomb]
            .physical_basis()
            .is_some_and(|basis| basis[2][0] < forward_z),
        "falling NapalmBomb did not pitch its forward axis down on its first physics update",
    )?;
    let storm = if upgraded {
        "BlackNapalmFirestormSmall"
    } else {
        "FirestormSmall"
    };
    advance_until(
        game,
        900,
        "physical bomb death and original FireOCL",
        |game| {
            !game.entities[bomb].alive()
                && game.entities[start..].iter().any(|actor| {
                    // Original InactiveBody fire has zero health while its
                    // normal-phase geometry/damage modules remain active.
                    !actor.masked()
                        && actor.fire_radius().is_some()
                        && game.catalog[actor.type_index].name == storm
                })
        },
    )?;
    let field = game.entities[start..]
        .iter()
        .find(|actor| actor.fire_radius().is_some() && game.catalog[actor.type_index].name == storm)
        .ok_or("missing live Helix firestorm")?;
    let storm_radius = game.catalog[field.type_index]
        .fire
        .as_ref()
        .unwrap()
        .storm
        .as_ref()
        .unwrap()
        .geometry
        .final_size[1];
    require(
        clear_bombing_point(game, field.position, storm_radius),
        "actual Helix firestorm overlaps the protected building or anti-air perimeter",
    )?;
    let weapon = if upgraded {
        "BlackNapalmBombWeapon"
    } else {
        "NapalmBombWeapon"
    };
    require(
        game.presentation.shots.iter().any(|shot| {
            shot.source == bomb
                && game.catalog[game.entities[bomb].type_index]
                    .weapon_template(shot.weapon_index)
                    .is_some_and(|rules| rules.name == weapon)
        }),
        "Napalm bomb did not choose its source upgrade-dependent death weapon",
    )?;
    println!(
        "  Helix physical {weapon} and {storm} passed at {}",
        game.tick
    );
    Ok(())
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    game.exit_container(setup.passengers[0])?;
    advance_until(game, 2400, "Helix landing and passenger exit", |game| {
        game.entities[setup.passengers[0]].contained_by().is_none()
    })?;
    game.enter(setup.passengers[0], setup.helicopters[0])?;
    advance_until(game, 2400, "Helix passenger reboarding", |game| {
        game.entities[setup.passengers[0]].contained_by() == Some(setup.helicopters[0])
    })?;
    arm_bomb(game, setup.helicopters[1])?;
    let bomber = setup.helicopters[1];
    let staging = game.entities[bomber].position;
    let bomb_target = [staging[0] + 100.0, staging[1]];
    drop_bomb(game, bomber, bomb_target, false)?;
    let factory_type = template(game, "ChinaWarFactory")?;
    let factory = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && actor.type_index == factory_type && actor.complete())
        .ok_or("paid research factory was lost before Black Napalm")?;
    research(game, factory, "Upgrade_ChinaBlackNapalm")?;
    advance_until(
        game,
        600,
        "Helix napalm recharge after first drop",
        |game| game.helix_bomb_ready(setup.helicopters[1]),
    )?;
    drop_bomb(game, bomber, bomb_target, true)?;
    verify_weapons(game, &setup)?;
    crash::verify(game, &setup)?;
    println!(
        "Paid Helix: three original attachments, five enclosed armed infantry, landing exit/reentry, normal and Black Napalm physical bombs passed at {}",
        game.tick
    );
    Ok(())
}

fn verify_weapons(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let target = game.entities[setup.target].position;
    for id in &setup.helicopters[..2] {
        game.move_to(*id, [target[0], target[1] - 160.0])?;
    }
    let cannon = game.entities[setup.helicopters[1]]
        .attachment_child()
        .ok_or("missing Helix cannon")?;
    advance_until(
        game,
        3600,
        "Helix scouting into actual passenger and attachment firing range",
        |game| {
            game.logically_visible(0, setup.target)
                && setup
                    .passengers
                    .iter()
                    .all(|&id| game.weapon_target_reachable(id, setup.target))
                && game.weapon_target_reachable(cannon, setup.target)
        },
    )
    .map_err(|error| format!("{error}; {}", helix_combat_state(game, setup, cannon)))?;
    for id in &setup.helicopters[..2] {
        game.attack(*id, setup.target)?;
    }
    let start = helix_combat_state(game, setup, cannon);
    let mut fired = [false; 3];
    for _ in 0..2400 {
        game.step();
        for (index, source) in [setup.helicopters[1], cannon].into_iter().enumerate() {
            fired[index] |= game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == source);
        }
        fired[2] |= game
            .presentation
            .shots
            .iter()
            .any(|shot| setup.passengers.contains(&shot.source));
        if fired.into_iter().all(|value| value) {
            break;
        }
    }
    if !fired.into_iter().all(|value| value) {
        return Err(format!(
            "original Helix fire missing: host/cannon/passenger={fired:?}; start: {start}; end: {}",
            helix_combat_state(game, setup, cannon)
        )
        .into());
    }
    println!(
        "  Helix host minigun, actual Gattling attachment and enclosed Tank Hunter fired at the real enemy at {}",
        game.tick
    );
    Ok(())
}

fn helix_combat_state(game: &Skirmish, setup: &Setup, cannon: usize) -> String {
    let target = &game.entities[setup.target];
    let mut state = vec![format!(
        "tick={} target={} HP={} XYZ={:?} visible={}",
        game.tick,
        setup.target,
        target.health,
        game.object_position(setup.target),
        game.logically_visible(0, setup.target)
    )];
    for id in setup.helicopters[..2]
        .iter()
        .copied()
        .chain(std::iter::once(cannon))
        .chain(setup.passengers.iter().copied())
    {
        let actor = &game.entities[id];
        let unit = &game.catalog[actor.type_index];
        let weapon = actor.active_weapon(unit);
        let order = match actor.ai.order {
            generals_sim::skirmish::Order::Idle => "Idle".to_owned(),
            generals_sim::skirmish::Order::Attack(id) => format!("Attack({id})"),
            generals_sim::skirmish::Order::Move(goal) => format!("Move({goal:?})"),
            _ => "other".to_owned(),
        };
        state.push(format!(
            "; {id} {} HP={} XYZ={:?} distance={} moving={} order={order} held={:?} last_attacker={:?} weapon={} range={} can_target={} reachable={}",
            unit.name, actor.health, game.object_position(id),
            (actor.position[0] - target.position[0]).hypot(actor.position[1] - target.position[1]),
            actor.moving(), actor.contained_by(), actor.last_attacker(), weapon.name, weapon.range,
            game.weapon_can_target(id, setup.target), game.weapon_target_reachable(id, setup.target)
        ));
    }
    state.join("; ")
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_helix_attachments_passengers_and_both_physical_bombs() {
        let root = generals_assets::game_root().unwrap();
        super::run(&root).unwrap();
    }
}
