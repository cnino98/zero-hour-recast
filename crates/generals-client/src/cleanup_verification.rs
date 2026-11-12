//! Paid source Ambulance/SCUD setup for the cleanup command and rendered acceptance.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub struct Setup {
    pub actor: usize,
    pub object: usize,
    pub field: u64,
    pub position: [f32; 3],
    pub health: f32,
    pub expires: u64,
}

pub struct Progress {
    frame: Option<u64>,
    shots: usize,
    last_shot: Option<u64>,
    peak_projectiles: usize,
    peak_stream_points: usize,
    minimum_health: f32,
    removed: Option<u64>,
    cleanup_impact: Option<u64>,
    projectiles: std::collections::BTreeSet<u64>,
}

impl Progress {
    pub const fn new(health: f32) -> Self {
        Self {
            frame: None,
            shots: 0,
            last_shot: None,
            peak_projectiles: 0,
            peak_stream_points: 0,
            minimum_health: health,
            removed: None,
            cleanup_impact: None,
            projectiles: std::collections::BTreeSet::new(),
        }
    }

    pub fn observe(&mut self, game: &Skirmish, setup: &Setup) {
        if self.frame == Some(game.tick) {
            return;
        }
        self.frame = Some(game.tick);
        for shot in game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == setup.actor && shot.target == Some(setup.object))
        {
            // Cleanup fires on the hazard's physical Object. Match its launch
            // callback to the weapon that fired on that Object this frame.
            for event in &game.presentation.weapon_fx {
                if let generals_sim::skirmish::WeaponFxEvent::Projectile(event) = event
                    && matches!(
                        event.stage,
                        generals_sim::skirmish::ProjectileFxStage::Created
                    )
                    && event.source == shot.source
                    && event.weapon_type == shot.weapon_type
                    && event.weapon_slot == shot.weapon_slot
                    && event.weapon_index == shot.weapon_index
                {
                    self.projectiles.insert(event.projectile);
                }
            }
        }
        let shots = game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == setup.actor)
            .count();
        self.shots += shots;
        if shots > 0 {
            self.last_shot = Some(game.tick);
        }
        self.peak_projectiles = self.peak_projectiles.max(
            game.projectiles
                .iter()
                .filter(|projectile| projectile.source == setup.actor)
                .count(),
        );
        let points = game
            .projectile_streams
            .iter()
            .filter(|stream| stream.source == setup.actor)
            .map(|stream| game.projectile_stream_points(stream).len())
            .max()
            .unwrap_or(0);
        self.peak_stream_points = self.peak_stream_points.max(points);
        if let Some(field) = game.hazards.iter().find(|field| field.id == setup.field) {
            self.minimum_health = self.minimum_health.min(field.health(&game.entities));
        } else if self.removed.is_none() {
            self.removed = Some(game.tick);
        }
        if !game.entities[setup.object].exists()
            && game.presentation.object_fx_events.iter().any(|event| {
                event.object == setup.object
                    && event.position.map(f32::to_bits) == setup.position.map(f32::to_bits)
            })
            && game.presentation.weapon_fx.iter().any(|event| {
                matches!(event, generals_sim::skirmish::WeaponFxEvent::Projectile(event)
                if event.source == setup.actor
                    && matches!(event.stage, generals_sim::skirmish::ProjectileFxStage::Detonation))
            })
        {
            self.cleanup_impact = Some(game.tick);
        }
    }

    pub fn removed_by_damage(&self, setup: &Setup) -> bool {
        self.shots > 0
            && self
                .removed
                .is_some_and(|frame| frame < setup.expires && self.cleanup_impact == Some(frame))
    }

    pub const fn projectile_ids(&self) -> &std::collections::BTreeSet<u64> {
        &self.projectiles
    }

    pub fn details(&self, game: &Skirmish, setup: &Setup) -> String {
        let actor = &game.entities[setup.actor];
        let unit = &game.catalog[actor.type_index];
        let field = game.hazards.iter().find(|field| field.id == setup.field);
        let projectiles: Vec<_> = game
            .projectiles
            .iter()
            .filter(|projectile| projectile.source == setup.actor)
            .take(3)
            .map(|projectile| {
                (
                    projectile.id,
                    projectile.hazard_target,
                    projectile.position,
                    projectile.destination,
                )
            })
            .collect();
        format!(
            "frame={} field={} position={:?} field_health={:?} minimum_health={} expires={} removed={:?} cleanup_impact={:?} ready={} cleaning={:?} weapon={} range={} turret=({},{}) shots={} last_shot={:?} peak_projectiles={} peak_stream_points={} projectiles={:?}; {}",
            game.tick,
            setup.field,
            setup.position,
            field.map(|field| field.health(&game.entities)),
            self.minimum_health,
            setup.expires,
            self.removed,
            self.cleanup_impact,
            game.cleanup_ready(setup.actor),
            actor.cleaning_area(),
            actor.active_weapon(unit).name,
            actor.active_weapon(unit).range,
            actor.ai.turret.angle,
            actor.ai.turret.pitch,
            self.shots,
            self.last_shot,
            self.peak_projectiles,
            self.peak_stream_points,
            projectiles,
            generals_sim::skirmish::diagnostics::movement_details(game, setup.actor),
        )
    }
}

pub fn ambulances(game: &mut Skirmish) -> Result<[usize; 2], Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::Gla)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let medic = template(game, "AmericaVehicleMedic")?;
    let first = produce_type(game, factory, medic, 0)?;
    let second = produce_type(game, factory, medic, 0)?;
    Ok([first, second])
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let [actor, _] = ambulances(game)?;
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::SupplyCenter, 1)?;
    let factory = construct(game, builder, Kind::Factory, 1)?;
    construct_type(game, builder, template(game, "GLAPalace")?, 1)?;
    game.purchase_science(1, "SCIENCE_ScudLauncher")?;
    let scud = produce_type(game, factory, template(game, "GLAVehicleScudLauncher")?, 1)?;
    game.switch_weapon(scud, 1)?;
    let target = friendly_entity(game, Kind::CommandCenter, 0)?;
    let center = game.entities[target].position;
    let origin = game.entities[scud].position;
    let delta = [origin[0] - center[0], origin[1] - center[1]];
    let length = delta[0].hypot(delta[1]);
    let point = [
        (delta[0] / length).mul_add(340.0, center[0]),
        (delta[1] / length).mul_add(340.0, center[1]),
    ];
    game.move_to(scud, point)?;
    advance_until(game, 18_000, "paid SCUD reaching cleanup fixture", |game| {
        let position = game.entities[scud].position;
        (position[0] - point[0]).hypot(position[1] - point[1]) < 25.0
    })?;
    game.stop(scud)?;
    game.attack(scud, target)?;
    advance_until(game, 3000, "original SCUD toxin impact", |game| {
        game.hazards.iter().any(|field| field.source == scud)
    })?;
    game.stop(scud)?;
    let field = game
        .hazards
        .iter()
        .find(|field| field.source == scud)
        .ok_or("SCUD toxin field disappeared before cleanup setup")?;
    Ok(Setup {
        actor,
        object: field.object,
        field: field.id,
        position: field.position,
        health: field.health(&game.entities),
        expires: field.expires,
    })
}

#[cfg(test)]
pub mod tests {
    use crate::{
        playtest::{MAP_NAME, Match},
        skirmish_data,
        terrain::Terrain,
        world_data,
    };
    use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
    use generals_sim::skirmish::Faction;

    pub fn installed() -> (VirtualAssets, Match) {
        let (assets, battle, _) = installed_with_animations();
        (assets, battle)
    }

    pub fn installed_with_animations() -> (VirtualAssets, Match, crate::model_animation::Animations)
    {
        installed_with_layout(|_| {})
    }

    pub fn installed_with_layout(
        configure: impl FnOnce(&mut generals_sim::skirmish::MapLayout),
    ) -> (VirtualAssets, Match, crate::model_animation::Animations) {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let map = MapData::parse(&assets.read(MAP_NAME).unwrap().bytes).unwrap();
        let mut data = skirmish_data::load(&assets, &map.objects).unwrap();
        let settings = world_data::camera_settings(&assets, root).unwrap();
        let terrain = Terrain::new(
            map.heightmap,
            map.tiles,
            settings.adjust_cliffs,
            settings.three_way_blends,
        )
        .unwrap();
        let (mut layout, _) = world_data::load(
            &assets,
            &map.objects,
            &terrain,
            &data.types,
            &mut data.models,
        )
        .unwrap();
        configure(&mut layout);
        let movement = terrain
            .classify_movement(&map.triggers, &map.objects)
            .unwrap();
        let animations =
            crate::model_animation::load(&assets, &mut data.models, &data.types).unwrap();
        let projectiles =
            crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations)
                .unwrap();
        let mut simulation = data.into_simulation(movement, layout).unwrap();
        terrain
            .configure_projectiles(&mut simulation, projectiles.partition_cell_size)
            .unwrap();
        (
            assets,
            Match {
                game_speed: 30,
                observer_player: None,
                observer_players: Vec::new(),
                simulation,
                selected: Vec::new(),
                logic_selection: Vec::new(),
                placing: None,
                started: true,
                paused: false,
                promotions_open: false,
                generation: 0,
                opponent_difficulty: None,
                opponent_faction: Faction::Gla,
            },
            animations,
        )
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_cleanup_removes_the_original_scud_field_before_its_natural_expiry() {
        let (_, mut battle) = installed();
        let game = &mut battle.simulation;
        let setup = super::prepare(game).unwrap();
        let mut progress = super::Progress::new(setup.health);
        game.clean_area(setup.actor, [setup.position[0], setup.position[1]])
            .unwrap();
        while game.tick < setup.expires && !progress.removed_by_damage(&setup) {
            game.step();
            progress.observe(game, &setup);
        }
        assert!(
            progress.removed_by_damage(&setup),
            "{}",
            progress.details(game, &setup)
        );
        assert!(
            progress.shots > 0 && progress.peak_projectiles > 0 && progress.peak_stream_points > 1,
            "{}",
            progress.details(game, &setup)
        );
    }
}
