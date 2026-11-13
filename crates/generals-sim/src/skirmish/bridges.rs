//! `TerrainLogic::Bridge::updateDamageState` and `BridgeBehavior::onDie`.
use super::{DamageType, DeathType, NeutralObject, Skirmish};

mod walls;

/// Native marker-pair bridge object; its center is absolute world XYZ.
pub struct RegularBridge {
    pub layer: u8,
    pub template_name: String,
    pub object: NeutralObject,
}

pub(super) struct Binding {
    layer: u8,
    template_name: String,
    pub(super) id: usize,
    absolute_height: Option<f32>,
}

impl Skirmish {
    pub(super) fn broken_bridge_towards(
        &self,
        actor: usize,
        destination: [f32; 2],
    ) -> Option<usize> {
        let from = self.object_position(actor);
        let [x, y] = destination;
        let goal = [x, y, self.ground_height(destination)];
        let layer_cell = |point: [f32; 3]| {
            let xy = [point[0], point[1]];
            crate::grid::LayerCell {
                cell: self.clipped_path_cell(xy),
                layer: self
                    .movement_terrain
                    .get_layer_for_destination(point, self.ground_height(xy)),
            }
        };
        let (from, to) = (layer_cell(from), layer_cell(goal));
        let entity = &self.entities[actor];
        let surfaces = entity.locomotor_surfaces(&self.catalog[entity.type_index]);
        let navigation = self.navigation_for(actor);
        if navigation.quick_path_exists_layers(surfaces, from, to) {
            return None;
        }
        let layer = navigation.broken_bridge_connecting_zones(
            &self.movement_terrain,
            from,
            to,
            surfaces,
        )?;
        self.bridge_objects
            .iter()
            .find(|binding| binding.layer == layer)
            .map(|binding| binding.id)
    }

    /// Native bridge picking requires a drawable owned by a bridge object.
    #[must_use]
    pub fn pick_bridge(&self, from: [f32; 3], to: [f32; 3]) -> Option<[f32; 3]> {
        self.bridge_objects.iter().rev().find_map(|binding| {
            let Binding { layer, id, .. } = binding;
            (self.entities[*id].exists() && !self.entities[*id].masked())
                .then(|| {
                    self.movement_terrain
                        .bridge_geometry(*layer)?
                        .ray_point(from, to)
                })
                .flatten()
        })
    }

    pub(in crate::skirmish) fn bridge_attack_points(&self, victim: usize) -> Option<[[f32; 3]; 2]> {
        self.entities.get(victim)?;
        let geometry = self.bridge_objects.iter().find_map(|binding| {
            let Binding { layer, id, .. } = binding;
            (*id == victim)
                .then(|| self.movement_terrain.bridge_geometry(*layer))
                .flatten()
        });
        let Some(geometry) = geometry else {
            return Some([self.object_position(victim); 2]);
        };
        let direction = [0, 1, 2].map(|axis| geometry.to[axis] - geometry.from[axis]);
        let squares = direction.map(|value| value * value);
        let length = (squares[0] + squares[1] + squares[2]).sqrt();
        let direction = direction.map(|value| value / length);
        let width = [0, 1, 2].map(|axis| geometry.corners[1][axis] - geometry.corners[0][axis]);
        let squares = width.map(|value| value * value);
        let radius = (squares[0] + squares[1] + squares[2]).sqrt() / 2.0;
        let offsets = direction.map(|value| value * radius);
        Some([
            std::array::from_fn(|axis| geometry.from[axis] + offsets[axis]),
            std::array::from_fn(|axis| geometry.to[axis] - offsets[axis]),
        ])
    }

    pub(in crate::skirmish) fn regular_bridge_height(&self, id: usize) -> Option<f32> {
        self.bridge_objects.iter().find_map(|binding| {
            (binding.id == id)
                .then_some(binding.absolute_height)
                .flatten()
        })
    }

    pub(super) fn spawn_map_bridges(&mut self) -> Result<(), &'static str> {
        let bridge_like: Vec<_> = self
            .layout
            .neutral_objects
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                self.catalog
                    .iter()
                    .find(|unit| unit.name == record.name)
                    .is_some_and(|unit| {
                        unit.kind_of.iter().any(|kind| {
                            matches!(kind.as_str(), "LANDMARK_BRIDGE" | "WALK_ON_TOP_OF_WALL")
                        })
                    })
                    .then_some(index)
            })
            .collect();
        for index in 0..self.layout.regular_bridges.len() {
            self.spawn_regular_bridge(index)?;
        }
        let layers: Vec<_> = self
            .movement_terrain
            .bridge_layers()
            .filter(|layer| {
                !self
                    .layout
                    .regular_bridges
                    .iter()
                    .any(|record| record.layer == *layer)
            })
            .collect();
        let mut layer_index = 0;
        for index in bridge_like {
            let id = self.spawn_neutral_object(index, true)?;
            if self.attack_kind(id, "LANDMARK_BRIDGE") {
                let layer = *layers
                    .get(layer_index)
                    .ok_or("Landmark map bridge lacks its allocated terrain layer")?;
                layer_index += 1;
                self.bridge_objects.push(Binding {
                    layer,
                    template_name: self.catalog[self.entities[id].type_index].name.clone(),
                    id,
                    absolute_height: None,
                });
            }
        }
        if layer_index != layers.len() {
            return Err("Landmark map bridges do not match their allocated terrain layers");
        }
        self.allocate_map_wall_layer();
        self.grid.restore_ground_terrain(&self.movement_terrain);
        self.reclassify_navigation_obstacles();
        Ok(())
    }

    fn spawn_regular_bridge(&mut self, index: usize) -> Result<(), &'static str> {
        let template = self
            .catalog
            .iter()
            .position(|unit| unit.name == "GenericBridge")
            .ok_or("Regular map bridge requires the original GenericBridge template")?;
        let record = &self.layout.regular_bridges[index];
        if self
            .movement_terrain
            .bridge_geometry(record.layer)
            .is_none()
        {
            return Err("Regular map bridge has no allocated terrain layer");
        }
        let layer = record.layer;
        let template_name = record.template_name.clone();
        let [x, y, z] = record.object.position;
        if ![x, y, z].iter().all(|value| value.is_finite()) {
            return Err("Regular map bridge has nonfinite absolute position");
        }
        let facing = record.object.angle;
        let name = record.object.script_name.clone();
        let targetable = record.object.script_targetable;
        let indestructible = record.object.indestructible;
        let selectable = record.object.selectable;
        // Bridge::Bridge creates the GenericBridge with a null team;
        // originalOwner is not one of Object's map-property overrides.
        let id = self.spawn(template, self.neutral_owner(), [x, y], true);
        self.set_object_orientation(id, facing);
        if let Some(value) = selectable
            && value != self.entities[id].selectable(&self.catalog[template])
        {
            self.set_object_selectable(id, value);
        }
        self.set_map_body_controls(id, targetable, indestructible);
        self.bridge_objects.push(Binding {
            layer,
            template_name,
            id,
            absolute_height: Some(z),
        });
        self.register_map_member(id, "team".into());
        if let Some(name) = name {
            self.map_objects.insert(name, id);
        }
        self.update_position_claim(id);
        self.observe_ground_partition(id);
        Ok(())
    }

    pub(super) fn bind_map_bridges(&mut self) {
        self.bridge_objects.retain(|binding| {
            self.movement_terrain
                .bridge_geometry(binding.layer)
                .is_some()
        });
        for (id, actor) in self.entities.iter().enumerate() {
            if self.bridge_objects.iter().any(|binding| binding.id == id)
                || !self.catalog[actor.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "LANDMARK_BRIDGE")
            {
                continue;
            }
            if let Some(layer) = self
                .movement_terrain
                .bridge_layers()
                .filter(|layer| {
                    self.movement_terrain
                        .bridge_contains(*layer, actor.position)
                })
                .last()
            {
                self.bridge_objects.push(Binding {
                    layer,
                    template_name: self.catalog[self.entities[id].type_index].name.clone(),
                    id,
                    absolute_height: None,
                });
            }
        }
    }

    /// Native first bridge at the pixel center, including a broken/absent object.
    #[must_use]
    pub fn radar_bridge_at(
        &self,
        point: [f32; 2],
    ) -> Option<(u8, &str, &crate::terrain::BridgeGeometry)> {
        self.bridge_objects.iter().rev().find_map(|binding| {
            let geometry = self.movement_terrain.bridge_geometry(binding.layer)?;
            geometry.contains(point).then_some((
                binding.layer,
                binding.template_name.as_str(),
                geometry,
            ))
        })
    }

    /// `W3DRadar` checks the chosen bridge object's Body, without trying a lower bridge.
    #[must_use]
    pub fn radar_bridge_working(&self, layer: u8) -> bool {
        self.bridge_objects
            .iter()
            .find(|binding| binding.layer == layer)
            .is_some_and(|binding| {
                self.entities[binding.id].exists() && self.body_damage_state(binding.id) != 3
            })
    }

    /// `Radar::update` consumes the real queued refresh after strictly more than 90 frames.
    pub fn take_radar_terrain_refresh(&mut self) -> bool {
        let queued = self.radar_terrain_refresh_frame;
        if queued == 0 || self.update_frame().wrapping_sub(queued) <= 90 {
            return false;
        }
        self.radar_terrain_refresh_frame = 0;
        true
    }

    pub(super) fn bridge_at_position(&self, point: [f32; 2]) -> Option<(u8, usize)> {
        self.bridge_objects.iter().rev().find_map(|binding| {
            self.movement_terrain
                .bridge_contains(binding.layer, point)
                .then_some((binding.layer, binding.id))
        })
    }

    pub(super) fn bridge_on_die(&mut self, bridge: usize) {
        let Some((layer, _)) = self.bridge_at_position(self.entities[bridge].position) else {
            return;
        };
        let minimum_height = self
            .movement_terrain
            .bridge_minimum_height(layer)
            .expect("bound bridge geometry");
        let origin = self.object_position(bridge);
        let corner = self
            .movement_terrain
            .bridge_geometry(layer)
            .expect("bound bridge geometry")
            .corners[2];
        let dx = corner[0] - origin[0];
        let dy = corner[1] - origin[1];
        let xx = dx * dx;
        let yy = dy * dy;
        let radius = (xx + yy).sqrt();
        let radius_squared = radius * radius;
        // Native FROM_CENTER_2D materializes the range before callback mutations.
        let passengers: Vec<_> = self
            .partition
            .fastest_range_objects([origin[0], origin[1]], radius)
            .into_iter()
            .filter(|&id| {
                let position = self.entities[id].position;
                let dx = position[0] - origin[0];
                let dy = position[1] - origin[1];
                let xx = dx * dx;
                let yy = dy * dy;
                xx + yy < radius_squared
            })
            .collect();
        for id in passengers {
            let actor = &self.entities[id];
            let unit = &self.catalog[actor.type_index];
            if unit
                .kind_of
                .iter()
                .any(|kind| matches!(kind.as_str(), "BRIDGE" | "BRIDGE_TOWER"))
            {
                continue;
            }
            let position = self.object_position(id);
            let ground = self.movement_layer_height(actor.layer, actor.position);
            if position[2] > ground
                || position[2] < minimum_height
                || !self.movement_terrain.bridge_contains(layer, actor.position)
                || actor.layer != layer
            {
                continue;
            }
            let height = position[2];
            let physics = unit.live_physics.is_some();
            self.entities[id].layer = 1;
            if physics {
                self.start_bridge_fall(id, height);
            } else {
                self.kill_object(id, DeathType::Normal);
            }
        }
    }

    pub(super) fn update_bridge_states(&mut self) {
        // TerrainLogic prepends bridges; each bridge changes connectivity and
        // damages its objects before advancing to the next bridge in the list.
        for index in (0..self.bridge_objects.len()).rev() {
            let Binding { layer, id, .. } = self.bridge_objects[index];
            if !self.entities[id].exists() {
                continue;
            }
            let destroyed = self.body_damage_state(id) == 3;
            if !self
                .movement_terrain
                .set_bridge_destroyed(layer, destroyed)
                .expect("bound bridge layer")
            {
                continue;
            }
            self.rebuild_navigation();
            for actor in &mut self.entities {
                if actor.ai.path.raw.iter().any(|node| node.layer == layer) {
                    actor.ai.path.clear();
                    actor.ai.ground_path = None;
                }
            }
            if !destroyed {
                continue;
            }
            // GameLogic's object list is newest first. Check geometry live after
            // each attemptDamage, since death callbacks can affect later objects.
            for id in (0..self.entities.len()).rev() {
                let actor = &self.entities[id];
                if actor.exists()
                    && actor.layer == layer
                    && self.movement_terrain.bridge_interacts(
                        layer,
                        self.object_position(id),
                        self.catalog[actor.type_index].geometry.minor,
                        false,
                    )
                {
                    self.apply_weapon_hit(
                        id,
                        id,
                        999_999.0,
                        DamageType::Falling,
                        DeathType::Splatted,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Cell,
        grid::LayerCell,
        skirmish::{
            Faction, Kind,
            physical_ocl::{LiveRules, MotionRules},
            tests::{fixture_type, game},
        },
        terrain::{BridgeGeometry, TerrainMap},
    };

    fn bridge_game() -> (Skirmish, usize, u8) {
        let mut game = game();
        let mut bridge = fixture_type(Faction::Civilian, Kind::TechBuilding);
        bridge.name = "MonumentTrainBridge".into();
        bridge.kind_of = [
            "STRUCTURE",
            "IMMOBILE",
            "BRIDGE",
            "LANDMARK_BRIDGE",
            "UNATTACKABLE",
        ]
        .map(str::to_owned)
        .to_vec();
        game.catalog.push(bridge);
        let id = game.spawn(game.catalog.len() - 1, 0, [355.0, 205.0], true);
        let mut terrain = TerrainMap::classify(80, 60, [], []).unwrap();
        let layer = terrain
            .add_bridge(
                BridgeGeometry {
                    from: [300.0, 205.0, 80.0],
                    to: [410.0, 205.0, 80.0],
                    corners: [
                        [300.0, 230.0, 80.0],
                        [300.0, 180.0, 80.0],
                        [410.0, 230.0, 80.0],
                        [410.0, 180.0, 80.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        game.configure_movement_terrain(terrain).unwrap();
        game.bind_map_bridges();
        (game, id, layer)
    }

    #[test]
    fn isolated_bridge_on_die_releases_grounded_physics_without_shock_rng() {
        let (mut game, bridge, layer) = bridge_game();
        game.catalog[6].live_physics = Some(LiveRules {
            motion: MotionRules {
                center_of_mass_offset: 0.0,
                mass: 30.0,
                gravity: -1.0,
                aerodynamic_friction: 0.0,
                forward_friction: 0.0,
                lateral_friction: 0.0,
                stiffness: 0.0,
                angular_factor: 1.0,
                allow_bouncing: false,
            },
            resistance: 0.0,
            kill_when_resting: false,
            collision: crate::skirmish::physical_ocl::CollisionRules {
                stiffness: 0.5,
                rubble_height: 0.0,
                allow_force: true,
                weapons: [None, None],
            },
            shock_rotation: [0.05, 0.025, 0.025],
            min_fall_speed: 8.0,
            fall_damage_factor: 1.0,
        });
        let tank = game.spawn(6, 0, [355.0, 205.0], true);
        game.entities[tank].layer = layer;
        game.entities[tank].ai.path.raw = vec![
            LayerCell {
                cell: Cell { x: 35, y: 20 },
                layer,
            },
            LayerCell {
                cell: Cell { x: 40, y: 20 },
                layer,
            },
        ];
        game.update_ground_contacts();
        let random = game.random;
        // Isolate BridgeBehavior::handleObjectsOnBridgeOnDie before TerrainLogic
        // changes the layer; the authored combined callback order is tested below.
        game.bridge_on_die(bridge);
        game.apply_native_damage(
            bridge,
            None,
            game.entities[bridge].max_health,
            DeathType::Normal,
        );
        assert_eq!(game.entities[tank].layer, 1);
        assert!(game.entities[tank].live_physics_active());
        assert_eq!(
            game.entities[tank].physical_height().unwrap().to_bits(),
            80.0_f32.to_bits()
        );
        assert_eq!(game.random, random);
        game.update_bridge_states();
        assert!(game.entities[tank].ai.path.raw.is_empty());
        game.tick += 1;
        game.integrate_live_physics(tank);
        assert!(game.entities[tank].physical_height().unwrap() < 80.0);
        assert!(game.entities[tank].physical_height().unwrap() > 0.0);
        for _ in 0..20 {
            game.tick += 1;
            game.integrate_live_physics(tank);
        }
        assert!(!game.entities[tank].alive());
        assert!(matches!(
            game.entities[tank].damage_state.death_type,
            DeathType::Splatted
        ));
        assert_eq!(game.entities[tank].damage_state.last_attacker, Some(tank));
    }

    #[test]
    fn destroyed_geometry_remains_pickable_and_restart_restores_connections_before_spawn() {
        let (mut game, bridge, layer) = bridge_game();
        let hit = game
            .pick_bridge([355.0, 205.0, 500.0], [355.0, 205.0, -100.0])
            .unwrap();
        assert_eq!(hit[2].to_bits(), 80.0_f32.to_bits());
        game.apply_native_damage(
            bridge,
            None,
            game.entities[bridge].max_health,
            DeathType::Normal,
        );
        game.update_bridge_states();
        assert!(
            game.pick_bridge([355.0, 205.0, 500.0], [355.0, 205.0, -100.0])
                .is_some()
        );
        game.movement_terrain.reset_bridges();
        assert!(
            game.movement_terrain
                .bridge_height(layer, [355.0, 205.0])
                .is_some()
        );
    }
}

#[cfg(test)]
mod regular_tests;
