//! SH 2bfaea60 `ObjectCreationUpgrade` and ordinary `GenericObjectCreationNugget`.
use super::{
    DamageType, DeathType, Entity, Skirmish,
    physical_ocl::{self, transform},
    startup,
};
use crate::collision_geometry::{Body, Geometry, Shape};
use crate::terrain::CellType;
use num_traits::ToPrimitive;
use std::f32::consts::TAU;

/// Callbacks copy immutable template data before they create more objects.
#[derive(Clone)]
pub struct List {
    pub name: String,
    pub objects: Vec<Object>,
}

#[derive(Clone)]
pub struct Object {
    pub names: Vec<String>,
    pub count: i32,
    pub offset: [f32; 3],
    pub disposition: u16,
    pub health: [f32; 2],
    pub lifetime: [u32; 2],
    pub flags: u16,
    pub container: String,
    pub invulnerable_time: u32,
    pub force_intensity: f32,
    pub force_magnitude: [f32; 2],
    pub force_pitch: [f32; 2],
    pub spin_rate: f32,
    pub formation_distances: [f32; 3],
    pub fade_frames: u32,
    pub fade_sound: String,
}

pub(super) struct Fade {
    start: u64,
    duration: u32,
    outward: bool,
}

#[derive(Clone, Copy)]
struct Placement {
    point: [f32; 3],
    transform: Option<([f32; 3], [[f32; 3]; 3])>,
    orientation: f32,
    lifetime_override: u32,
}

pub const REQUIRES_LIVE_PLAYER: u16 = 1;
pub const CONTAIN_INSIDE_SOURCE: u16 = 2;
pub const PRESERVE_LAYER: u16 = 4;
pub const IGNORE_PRIMARY_OBSTACLE: u16 = 8;
pub const INHERITS_VETERANCY: u16 = 16;
pub const SPREAD_FORMATION: u16 = 32;
pub const FADE_IN: u16 = 64;
pub const FADE_OUT: u16 = 128;
pub const DIES_ON_BAD_LAND: u16 = 256;

impl Entity {
    /// Drawable opacity from the OCL's independent, frame-based fade.
    ///
    /// # Panics
    /// The interior elapsed time is bounded by the original 32-bit duration.
    #[must_use]
    pub fn ocl_fade_opacity(&self, frame: u64) -> f32 {
        self.drawable.ocl_fade.as_ref().map_or(1.0, |fade| {
            let elapsed = frame.saturating_sub(fade.start);
            let amount = if elapsed >= u64::from(fade.duration) {
                1.0
            } else {
                let elapsed = u32::try_from(elapsed).expect("bounded OCL fade elapsed time");
                super::progression::integer_real(elapsed)
                    / super::progression::integer_real(fade.duration)
            };
            if fade.outward { 1.0 - amount } else { amount }
        })
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn apply_object_creation_upgrade(&mut self, id: usize, ordinal: usize) {
        let list = self.catalog[self.entities[id].type_index]
            .behavior
            .object_creation[ordinal]
            .clone();
        if let Some(list) = list {
            self.create_generic_objects(id, None, &list);
        }
    }

    // GenericObjectCreationNugget::create(Object*, Object*) ignores secondary.
    pub(in crate::skirmish) fn create_generic_objects(
        &mut self,
        source: usize,
        _secondary: Option<usize>,
        list: &List,
    ) {
        let _ = self.create_generic_objects_from_source(source, list);
    }

    pub(super) fn create_generic_objects_from_source(
        &mut self,
        source: usize,
        list: &List,
    ) -> Option<usize> {
        let placement = Placement {
            point: self.object_position(source),
            transform: Some(self.object_transform(source)),
            orientation: self.entities[source].facing,
            lifetime_override: 0,
        };
        self.create_generic_list(source, list, placement)
    }

    pub(in crate::skirmish) fn create_generic_objects_at(
        &mut self,
        source: usize,
        list: &List,
        primary: [f32; 3],
        angle: Option<f32>,
        lifetime_frames: u32,
    ) -> Option<usize> {
        self.create_generic_list(
            source,
            list,
            Placement {
                point: primary,
                transform: None,
                orientation: angle.unwrap_or(0.0),
                lifetime_override: lifetime_frames,
            },
        )
    }

    fn create_generic_list(
        &mut self,
        source: usize,
        list: &List,
        placement: Placement,
    ) -> Option<usize> {
        let mut first = None;
        for nugget in &list.objects {
            let object = self.create_generic_nugget(source, nugget, placement, |_| {});
            first = first.or(object);
        }
        first
    }

    pub(super) fn create_ordinary_death_nugget(
        &mut self,
        source: usize,
        nugget: &Object,
        point: [f32; 3],
        source_transform: bool,
        created: &mut Vec<usize>,
    ) {
        self.create_generic_nugget(
            source,
            nugget,
            Placement {
                point,
                transform: source_transform.then(|| self.object_transform(source)),
                orientation: if source_transform {
                    self.entities[source].facing
                } else {
                    0.0
                },
                lifetime_override: 0,
            },
            |id| created.push(id),
        );
    }

    fn create_generic_nugget(
        &mut self,
        source: usize,
        nugget: &Object,
        placement: Placement,
        mut created: impl FnMut(usize),
    ) -> Option<usize> {
        if nugget.names.is_empty()
            || nugget.flags & REQUIRES_LIVE_PLAYER != 0
                && !self.player_active(self.entities[source].owner)
        {
            return None;
        }
        let placement = if placement.transform.is_some() {
            Placement {
                point: self.object_position(source),
                transform: Some(self.object_transform(source)),
                orientation: self.entities[source].facing,
                ..placement
            }
        } else {
            placement
        };
        // Native creates the enclosing object before choosing any child.
        let container = self
            .catalog
            .iter()
            .position(|unit| unit.name == nugget.container)
            .filter(|_| !nugget.container.is_empty())
            .map(|kind| {
                let id = self.new_object(kind, self.entities[source].owner, true);
                self.entities[id].producer = Some(source);
                id
            });
        let mut first = container;
        if let Some(id) = container {
            created(id);
        }
        for _ in 0..nugget.count {
            let limit = u32::try_from(nugget.names.len() - 1).expect("native OCL name count");
            let choice =
                usize::try_from(self.random_between(limit)).expect("native OCL name index");
            let Some(kind) = self
                .catalog
                .iter()
                .position(|unit| unit.name == nugget.names[choice])
            else {
                continue;
            };
            let id = self.new_object(kind, self.entities[source].owner, true);
            first = first.or(Some(id));
            created(id);
            self.entities[id].producer = Some(source);
            if nugget.flags & PRESERVE_LAYER != 0
                && container.is_none()
                && self.entities[source].layer != 1
            {
                self.entities[id].layer = self.entities[source].layer;
            }
            if let Some(container) = container
                && self.catalog[self.entities[container].type_index]
                    .contain_family()
                    .is_some()
                && self.native_container_admits(id, container, true)
            {
                self.add_to_contain(id, container);
            }
            let mut child_placement = placement;
            if nugget.flags & SPREAD_FORMATION != 0 {
                let [inner_min, inner_max, outer] = nugget.formation_distances;
                let inner = startup::random_real(&mut self.random, inner_min, inner_max);
                child_placement.point =
                    self.search_ocl_position(placement.point, inner, outer, true);
            }
            self.configure_generic_object(id, source, nugget, &child_placement);
            self.fade_generic_object(id, source, nugget);
        }
        if let Some(container) = container {
            self.configure_generic_object(container, source, nugget, &placement);
        }
        first
    }

    fn configure_generic_object(
        &mut self,
        id: usize,
        source: usize,
        nugget: &Object,
        placement: &Placement,
    ) {
        self.entities[id].producer = Some(source);
        let lifetime = if placement.lifetime_override > 0 {
            [placement.lifetime_override; 2]
        } else {
            nugget.lifetime
        };
        if lifetime[1] > 0 && !self.entities[id].behaviors.lifetime_die_frames.is_empty() {
            self.set_lifetime_range(id, 0, lifetime);
        }
        let offset = placement.transform.map_or(nugget.offset, |(_, basis)| {
            transform::vector(basis, nugget.offset)
        });
        let point = std::array::from_fn(|axis| placement.point[axis] + offset[axis]);
        if nugget.flags & IGNORE_PRIMARY_OBSTACLE != 0 && self.object_physics_mass(id).is_some() {
            self.entities[id].set_physics_ignored_object(Some(source));
        }
        let percent = startup::random_real(&mut self.random, nugget.health[0], nugget.health[1]);
        self.set_ocl_initial_health(id, percent);
        if self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .any(|module| {
                matches!(
                    module.family.as_str(),
                    "SlavedUpdate" | "MobMemberSlavedUpdate"
                )
            })
        {
            self.enslave_spawn(id, source);
        }
        if nugget.flags & INHERITS_VETERANCY != 0
            && self.catalog[self.entities[id].type_index]
                .veterancy
                .trainable
        {
            self.set_hijacker_veterancy(id, self.entities[source].veterancy, false);
            self.transfer_crew_name(source, id);
        }
        if nugget.invulnerable_time > 0 {
            self.entities[id].helpers.defection.undetected = true;
            self.start_defection_timer(id, nugget.invulnerable_time, false);
        }
        self.place_generic_object(id, source, nugget, placement, point);
        if nugget.flags & CONTAIN_INSIDE_SOURCE != 0 {
            if self.catalog[self.entities[source].type_index]
                .contain_family()
                .is_some()
                && self.native_container_admits(id, source, true)
            {
                self.add_to_contain(id, source);
                if self.entities[source].has_drawable()
                    && self.entities[source].drawable_effectively_hidden()
                {
                    self.set_drawable_hidden(id, true);
                }
            } else {
                self.destroy_maintenance_object(id);
            }
        }
        if nugget.flags & DIES_ON_BAD_LAND != 0 {
            self.check_ocl_landing(id);
        }
    }

    fn place_generic_object(
        &mut self,
        id: usize,
        source: usize,
        nugget: &Object,
        placement: &Placement,
        mut point: [f32; 3],
    ) {
        if nugget.disposition & physical_ocl::INHERIT_VELOCITY != 0
            && self.object_physics_mass(source).is_some()
            && self.object_physics_mass(id).is_some()
        {
            self.physical_force(id, self.entities[source].physics_velocity());
        }
        if nugget.disposition & physical_ocl::LIKE_EXISTING != 0 {
            if let Some((position, basis)) = placement.transform {
                self.set_object_transform(id, position, basis);
            } else {
                self.set_object_orientation(id, placement.orientation);
            }
            self.set_object_position(id, point);
            if self.height_above_ground(source) > 0.0 && self.object_physics_mass(id).is_some() {
                self.entities[id].set_physics_allow_fall(true);
            }
            if self.attack_kind(id, "STRUCTURE") {
                self.flatten_object_terrain(id);
                let mut adjusted = self.object_position(id);
                // Native samples the unoffset source position after flattening.
                adjusted[2] = self.ground_height([placement.point[0], placement.point[1]]);
                self.set_object_position(id, adjusted);
                self.restore_object_obstacle(id);
            }
        }
        if nugget.disposition & super::physical_ocl::ON_GROUND != 0 {
            let ground = self.ground_height([point[0], point[1]]);
            let layer = self.movement_terrain.get_highest_layer_for_destination(
                [point[0], point[1], 99999.0],
                ground,
                false,
            );
            let yaw = startup::random_real(&mut self.random, 0.0, TAU);
            self.set_object_orientation(id, yaw);
            point[2] =
                self.movement_terrain
                    .destination_layer_height(layer, [point[0], point[1]], ground);
            if layer != 1 {
                point[2] += 1.0;
            }
            self.entities[id].layer = layer;
            self.set_object_position(id, point);
        }
        if nugget.disposition & physical_ocl::SEND_OUT != 0 {
            // ObjectCreationList.cpp1089: independent of the airborne force flags.
            // Ground height changes, but the inherited pathfinding layer does not.
            let yaw = startup::random_real(&mut self.random, 0.0, TAU);
            self.set_object_orientation(id, yaw);
            point[2] = self.ground_height([point[0], point[1]]);
            self.set_object_position(id, point);
            if self.object_physics_mass(id).is_some() {
                self.entities[id].clear_physics_extra_friction();
                let horizontal = 4.0 * nugget.force_intensity;
                let force = [
                    startup::random_real(&mut self.random, -horizontal, horizontal),
                    startup::random_real(&mut self.random, -horizontal, horizontal),
                    0.0,
                ];
                self.physical_force(id, force);
            }
        }
        if nugget.disposition
            & (physical_ocl::RANDOM_FORCE | physical_ocl::SEND_FLYING | physical_ocl::SEND_UP)
            != 0
        {
            if let Some((position, basis)) = placement.transform {
                self.set_object_transform(id, position, basis);
            }
            self.set_object_position(id, point);
            if self.object_physics_mass(id).is_some() {
                self.entities[id].prepare_ocl_physics();
                // Native derives an omitted spin rate from the authored intensity.
                let spin = if nugget.spin_rate < 0.0 {
                    (std::f32::consts::PI / 32.0) * nugget.force_intensity
                } else {
                    nugget.spin_rate
                };
                let rates =
                    std::array::from_fn(|_| startup::random_real(&mut self.random, -spin, spin));
                let force = self.generic_ocl_force(nugget);
                self.physical_force(id, force);
                self.set_object_transform(
                    id,
                    self.object_position(id),
                    transform::yaw(placement.orientation),
                );
                self.entities[id].set_physics_angular_rates(rates);
            }
        }
    }

    fn generic_ocl_force(&mut self, nugget: &Object) -> [f32; 3] {
        if nugget.disposition & (physical_ocl::SEND_FLYING | physical_ocl::SEND_UP) != 0 {
            let flying = nugget.disposition & physical_ocl::SEND_FLYING != 0;
            let horizontal = if flying { 4.0 } else { 2.0 } * nugget.force_intensity;
            let vertical = if flying { 3.0 } else { 4.0 } * nugget.force_intensity;
            [
                startup::random_real(&mut self.random, -horizontal, horizontal),
                startup::random_real(&mut self.random, -horizontal, horizontal),
                startup::random_real(
                    &mut self.random,
                    vertical * if flying { 0.33 } else { 0.75 },
                    vertical,
                ),
            ]
        } else {
            physical_ocl::random_force(&mut self.random, nugget.force_pitch, nugget.force_magnitude)
        }
    }

    fn fade_generic_object(&mut self, id: usize, source: usize, nugget: &Object) {
        for flag in [FADE_IN, FADE_OUT] {
            if nugget.flags & flag != 0 {
                if !nugget.fade_sound.is_empty() {
                    self.position_sound(
                        source,
                        nugget.fade_sound.clone(),
                        self.object_position(source),
                    );
                }
                self.entities[id].drawable.ocl_fade = Some(Fade {
                    start: self.tick,
                    duration: nugget.fade_frames,
                    outward: flag == FADE_OUT,
                });
            }
        }
    }

    fn check_ocl_landing(&mut self, id: usize) {
        let [x, y, z] = self.object_position(id);
        let layer = self.entities[id].layer;
        let ground = self.ground_height([x, y]);
        if layer == 1
            && self
                .movement_terrain
                .parachute_water_height([x, y], ground)
                .is_some_and(|water| z <= water + 10.0)
        {
            self.attempt_health_damage(id, None, 999_999.0, DamageType::Water, DeathType::Flooded);
        }
        if self.entities[id].is_off_map()
            || self.ocl_cell_type([x, y], layer).is_none_or(|kind| {
                matches!(
                    kind,
                    CellType::Cliff | CellType::Water | CellType::Impassable
                )
            })
        {
            self.kill_object(id, DeathType::Normal);
        }
    }

    fn ocl_cell_type(&self, point: [f32; 2], layer: u8) -> Option<CellType> {
        let cell = self.cell_at(point)?;
        if layer == 1 {
            return self.movement_terrain.cell(cell).map(|data| data.kind);
        }
        self.movement_terrain
            .bridge_cell(layer, cell)
            .map(|data| data.kind)
    }

    pub(in crate::skirmish) fn adjust_ocl_position_to_passable(
        &mut self,
        center: [f32; 3],
        radius: f32,
    ) -> [f32; 3] {
        self.search_ocl_position(center, 0.0, radius, false)
    }

    // PartitionManager::findPositionAround starts with one sample on the first
    // ring, then alternates positive and negative angles on wider rings.
    fn search_ocl_position(
        &mut self,
        center: [f32; 3],
        inner: f32,
        outer: f32,
        highest_layer: bool,
    ) -> [f32; 3] {
        if self.cell_at([center[0], center[1]]).is_none() {
            return center;
        }
        let start = startup::random_angle(&mut self.random);
        let mut distance = inner;
        let mut spacing = TAU;
        loop {
            if distance > outer {
                break;
            }
            let samples = ((TAU / spacing) / 2.0)
                .ceil()
                .to_u32()
                .expect("native OCL scan samples");
            for sample in 0..samples {
                let offset = spacing * super::progression::integer_real(sample);
                for angle in [start + offset, start - offset]
                    .into_iter()
                    .take(if sample == 0 { 1 } else { 2 })
                {
                    let x = distance * angle.cos();
                    let y = distance * angle.sin();
                    let point = [x + center[0], y + center[1]];
                    if let Some(result) = self.try_ocl_position(point, angle, highest_layer) {
                        return result;
                    }
                }
            }
            distance += 5.0;
            spacing = (5.0 / (distance + 1.0)) * (TAU / 6.0);
        }
        center
    }

    fn try_ocl_position(
        &self,
        point: [f32; 2],
        angle: f32,
        highest_layer: bool,
    ) -> Option<[f32; 3]> {
        let ground = self.ground_height(point);
        let mut height = ground;
        let mut layer = 1;
        if highest_layer {
            layer = self.movement_terrain.get_highest_layer_for_destination(
                [point[0], point[1], 99_999.0],
                ground,
                false,
            );
            if layer != 1 {
                height = self
                    .movement_terrain
                    .destination_layer_height(layer, point, ground)
                    + 1.0;
            }
        }
        let kind = self.ocl_cell_type(point, layer)?;
        if matches!(kind, CellType::Impassable)
            || (!highest_layer && !matches!(kind, CellType::Clear))
            || (layer == 1
                && (matches!(kind, CellType::Cliff)
                    || self
                        .movement_terrain
                        .parachute_water_height(point, ground)
                        .is_some()))
        {
            return None;
        }
        let position = [point[0], point[1], height];
        let probe = Body {
            geometry: &Geometry {
                shape: Shape::Sphere,
                is_small: true,
                major: 5.0,
                minor: 5.0,
                height: 5.0,
            },
            position,
            angle,
        };
        self.native_potential_collisions(&probe, true)
            .is_empty()
            .then_some(position)
    }

    fn set_ocl_initial_health(&mut self, id: usize, percent: f32) {
        if !self.catalog[self.entities[id].type_index]
            .body_kind
            .is_active()
        {
            return;
        }
        let percent = (percent * 100.0)
            .to_i32()
            .expect("native OCL health percent");
        let actor = &self.entities[id];
        let fraction = percent.to_f32().expect("native OCL health percent") / 100.0;
        let requested = fraction * actor.damage_state.initial_health;
        self.change_body_health(id, requested - actor.health);
    }
}

#[cfg(test)]
mod tests {
    use super::{CONTAIN_INSIDE_SOURCE, List, Object, PRESERVE_LAYER, REQUIRES_LIVE_PLAYER};
    use crate::skirmish::{Faction, Kind, Skirmish, physical_ocl, tests};

    fn nugget() -> Object {
        Object {
            names: vec!["OclFixtureChild".into()],
            count: 1,
            offset: [7.0, 3.0, 2.0],
            disposition: physical_ocl::LIKE_EXISTING,
            health: [0.25; 2],
            lifetime: [0; 2],
            flags: PRESERVE_LAYER,
            container: String::new(),
            invulnerable_time: 0,
            force_intensity: 0.0,
            force_magnitude: [0.0; 2],
            force_pitch: [0.0; 2],
            spin_rate: -1.0,
            formation_distances: [0.0; 3],
            fade_frames: 0,
            fade_sound: String::new(),
        }
    }

    fn setup(objects: Vec<Object>, null_first: bool) -> (Skirmish, usize, usize) {
        let mut game = tests::game();
        let mut child = tests::fixture_type(Faction::Usa, Kind::Infantry);
        child.name = "OclFixtureChild".into();
        let child_type = game.catalog.len();
        game.catalog.push(child);
        let mut parent = tests::fixture_type(Faction::Usa, Kind::Barracks);
        if null_first {
            parent.behavior.object_creation.push(None);
            tests::register_behavior(&mut parent, "ObjectCreationUpgrade", 0x80);
            parent.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Create".into()];
        }
        parent.behavior.object_creation.push(Some(List {
            name: "OclFixture".into(),
            objects,
        }));
        tests::register_behavior(&mut parent, "ObjectCreationUpgrade", 0x80);
        parent.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Create".into()];
        let kind = game.catalog.len();
        game.catalog.push(parent);
        let source = game.spawn(kind, 0, [350.0, 250.0], true);
        (game, source, child_type)
    }

    #[test]
    fn location_creation_keeps_world_offset_instead_of_rotating_with_source() {
        let (mut game, source, child_kind) = setup(Vec::new(), false);
        game.set_object_orientation(source, 2.0);
        let object = nugget();
        let list = List {
            name: "Located".into(),
            objects: vec![object],
        };
        let child = game
            .create_generic_objects_at(source, &list, [600.0, 400.0, 7.0], Some(1.0), 0)
            .unwrap();
        assert_eq!(game.entities[child].type_index, child_kind);
        assert_eq!(game.entities[child].producer, Some(source));
        assert_eq!(
            game.object_position(child).map(f32::to_bits),
            [607.0_f32, 403.0, 9.0].map(f32::to_bits)
        );
        assert_eq!(game.entities[child].facing.to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn ambush_spreads_outside_the_click_and_fades_for_ninety_frames() {
        let (mut game, source, child_kind) = setup(Vec::new(), false);
        let mut object = nugget();
        object.count = 4;
        object.offset = [0.0; 3];
        object.disposition = physical_ocl::ON_GROUND;
        object.flags = super::SPREAD_FORMATION | super::FADE_IN | super::DIES_ON_BAD_LAND;
        object.formation_distances = [20.0, 30.0, 400.0];
        object.fade_frames = 90;
        let list = List {
            name: "Ambush".into(),
            objects: vec![object],
        };
        let center = [500.0, 400.0, 0.0];
        let first = game
            .create_generic_objects_at(source, &list, center, None, 0)
            .unwrap();
        let mut positions = Vec::new();
        for id in first..first + 4 {
            let actor = &game.entities[id];
            assert_eq!(actor.type_index, child_kind);
            assert!(actor.alive());
            let position = game.object_position(id);
            let distance = (position[0] - center[0]).hypot(position[1] - center[1]);
            assert!((19.99..=400.01).contains(&distance));
            assert!(!positions.contains(&position.map(f32::to_bits)));
            positions.push(position.map(f32::to_bits));
            assert_eq!(
                actor.ocl_fade_opacity(game.tick).to_bits(),
                0.0_f32.to_bits()
            );
            assert_eq!(
                actor.ocl_fade_opacity(game.tick + 45).to_bits(),
                0.5_f32.to_bits()
            );
            assert_eq!(
                actor.ocl_fade_opacity(game.tick + 90).to_bits(),
                1.0_f32.to_bits()
            );
        }
    }

    #[test]
    fn ordered_create_grants_run_after_all_object_created_callbacks() {
        use crate::skirmish::veterancy::StartingVeterancy;

        fn birth_idle_offset(random: &mut [u32; 6]) -> u32 {
            // AIUpdate.cpp296 constructs NORMAL's Locomotor before onObjectCreated.
            // Locomotor.cpp673–675 samples angle, increment and direction.
            super::startup::random_real(
                random,
                -std::f32::consts::PI / 6.0,
                std::f32::consts::PI / 6.0,
            );
            super::startup::random_real(random, 0.8, 1.2);
            super::startup::random_between(random, 1);
            // AIStates.cpp691–693 constructs three child Idle machines;
            // AIUpdate.cpp630 then enters primary Idle.
            let offsets: [u32; 4] =
                std::array::from_fn(|_| super::startup::random_between(random, 60));
            offsets[3]
        }

        let mut game = tests::game();
        let mut child = tests::fixture_type(Faction::Usa, Kind::Infantry);
        child.name = "OclFixtureChild".into();
        child.veterancy.trainable = true;
        child.veterancy.required = [0, 100, 200, 300];
        let child_kind = game.catalog.len();
        game.catalog.push(child);
        let mut parent = tests::fixture_type(Faction::Usa, Kind::Infantry);
        parent.veterancy.trainable = true;
        parent.veterancy.required = [0, 100, 200, 300];
        parent.veterancy.starting = vec![
            StartingVeterancy {
                science: None,
                level: 1,
            },
            StartingVeterancy {
                science: None,
                level: 2,
            },
        ];
        tests::register_behavior(&mut parent, "VeterancyGainCreate", 8);
        let first_grant = parent.behavior_modules.pop().unwrap();
        // The original Pilot also authors its first grant before its AI module.
        parent.behavior_modules.insert(0, first_grant);
        let mut object = nugget();
        object.flags |= super::INHERITS_VETERANCY;
        parent.behavior.object_creation.push(Some(List {
            name: "VeteranBirth".into(),
            objects: vec![object],
        }));
        tests::register_behavior(&mut parent, "ObjectCreationUpgrade", 0x80);
        parent.behavior_modules.last_mut().unwrap().upgrade.triggers =
            vec!["Upgrade_Veterancy_VETERAN".into()];
        tests::register_behavior(&mut parent, "VeterancyGainCreate", 8);
        let parent_kind = game.catalog.len();
        game.catalog.push(parent);
        let mut after_birth = game.random;
        let parent_offset = birth_idle_offset(&mut after_birth);
        // ObjectCreationList.cpp1343 selects its sole name with [0, 0].
        // RandomValue.cpp267 consumes no draw under RETAIL_COMPATIBLE_CRC=0.
        let child_offset = birth_idle_offset(&mut after_birth);
        let parent = game.spawn(parent_kind, 0, [350.0, 250.0], true);
        let children: Vec<_> = game
            .entities
            .iter()
            .enumerate()
            .filter_map(|(id, actor)| {
                (actor.type_index == child_kind && actor.producer == Some(parent)).then_some(id)
            })
            .collect();
        assert_eq!(children.len(), 1);
        let child = children[0];
        assert!(game.entities[parent].ai.primary.initialized);
        assert!(game.entities[child].ai.primary.initialized);
        assert_eq!(game.entities[parent].ai.idle.sleep_offset, parent_offset);
        assert_eq!(game.entities[child].ai.idle.sleep_offset, child_offset);
        assert_eq!(game.random, after_birth);
        // Both CREATEs still run in authored order. The first grant creates a
        // Veteran child; the second raises only the parent to Elite.
        assert_eq!(game.entities[parent].veterancy, 2);
        assert_eq!(game.entities[child].veterancy, 1);
        assert_eq!(game.entities[child].experience, 100);
        assert!(game.native_upgrade_executed(parent, "ObjectCreationUpgrade", 0));
    }

    #[test]
    fn container_is_created_before_children_and_negative_count_still_configures_it() {
        for count in [-1, 1] {
            let mut object = nugget();
            object.count = count;
            object.container = "OclContainer".into();
            object.invulnerable_time = 6;
            object.flags |= super::INHERITS_VETERANCY;
            let (mut game, source, child_type) = setup(vec![object], false);
            let mut carrier = tests::fixture_type(Faction::Usa, Kind::Vehicle);
            carrier.name = "OclContainer".into();
            carrier.contain = Some(crate::skirmish::containment::tests::rules(0.0, 0.0));
            tests::register_update(&mut carrier, "OpenContain");
            let carrier_kind = game.catalog.len();
            game.catalog.push(carrier);
            game.catalog[child_type].veterancy.trainable = true;
            game.catalog[child_type].veterancy.required = [0, 100, 200, 300];
            game.map_objects.insert("EjectingAircraft".into(), source);
            game.set_hijacker_veterancy(source, 2, false);
            game.presentation.promotion_events.clear();
            let first = game.entities.len();
            game.grant_native_object_upgrade(source, "Create");
            assert_eq!(game.entities[first].type_index, carrier_kind);
            assert_eq!(game.entities[first].producer, Some(source));
            assert_eq!(
                game.object_position(first).map(f32::to_bits),
                [357.0_f32, 253.0, 2.0].map(f32::to_bits)
            );
            assert!(game.entities[first].helpers.defection.undetected);
            assert!(game.presentation.promotion_events.is_empty());
            if count < 0 {
                assert_eq!(game.entities.len(), first + 1);
                assert!(game.passengers(first).is_empty());
                assert_eq!(game.named_map_object("EjectingAircraft"), Some(source));
            } else {
                let child = first + 1;
                assert_eq!(game.entities.len(), first + 2);
                assert_eq!(game.entities[child].type_index, child_type);
                assert_eq!(game.entities[child].contained_by(), Some(first));
                assert_eq!(game.entities[child].producer, Some(source));
                assert_eq!(game.entities[child].veterancy, 2);
                assert_eq!(game.entities[child].experience, 200);
                assert_eq!(game.named_map_object("EjectingAircraft"), Some(child));
                assert!(game.entities[child].helpers.defection.undetected);
            }
            game.tick = 5;
            game.run_object_updates();
            assert!(game.entities[first].helpers.defection.undetected);
            game.tick = 6;
            game.run_object_updates();
            assert!(!game.entities[first].helpers.defection.undetected);
        }
    }

    #[test]
    fn random_force_keeps_native_constructor_rng_and_applies_impulse_once() {
        let mut object = nugget();
        object.offset = [0.0; 3];
        object.disposition = physical_ocl::RANDOM_FORCE;
        object.force_magnitude = [10.0, 12.0];
        object.force_pitch = [50.0_f32.to_radians(), 60.0_f32.to_radians()];
        object.spin_rate = 0.0;
        let list = List {
            name: "PilotEjection".into(),
            objects: vec![object],
        };
        let (mut game, source, child_type) = setup(Vec::new(), false);
        game.catalog[child_type].live_physics = Some(physical_ocl::LiveRules {
            motion: physical_ocl::MotionRules {
                center_of_mass_offset: 0.0,
                mass: 2.0,
                gravity: -0.1,
                stiffness: 0.5,
                forward_friction: 0.15,
                lateral_friction: 0.15,
                aerodynamic_friction: 0.0,
                angular_factor: 2.0,
                allow_bouncing: false,
            },
            resistance: 0.25,
            kill_when_resting: false,
            collision: physical_ocl::CollisionRules {
                stiffness: 0.5,
                rubble_height: 0.0,
                allow_force: true,
                weapons: [None, None],
            },
            shock_rotation: [0.0; 3],
            min_fall_speed: 99.0,
            fall_damage_factor: 1.0,
        });
        tests::register_update(&mut game.catalog[child_type], "PhysicsBehavior");
        game.set_object_position(source, [350.0, 250.0, 100.0]);
        let random_before = game.random;
        let control = game.new_object(child_type, 0, true);
        let mut expected_random = game.random;
        // Equal health/spin bounds and a single template consume no CRC0 RNG.
        let force = physical_ocl::random_force(
            &mut expected_random,
            list.objects[0].force_pitch,
            list.objects[0].force_magnitude,
        );
        game.destroy_maintenance_object(control);
        game.process_update_destruction();
        game.random = random_before;
        let child = game.entities.len();
        game.create_generic_objects(source, None, &list);
        assert_eq!(game.random, expected_random);
        let state = game.entities[child].live_physics.as_ref().unwrap();
        assert_eq!(
            state.motion.acceleration.map(f32::to_bits),
            force.map(|value| (value * 0.5).to_bits())
        );
        assert_eq!(
            state.motion.velocity.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
        assert_eq!(
            state.motion.rates.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
        assert!(state.motion.bouncing);
        assert_eq!(
            game.object_position(child).map(f32::to_bits),
            [350.0_f32, 250.0, 100.0].map(f32::to_bits)
        );
        game.tick = 1;
        game.run_object_updates();
        let state = game.entities[child].live_physics.as_ref().unwrap();
        assert_eq!(
            state.motion.acceleration.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
        let vertical_impulse = force[2] * 0.5;
        assert_eq!(
            state.motion.velocity[2].to_bits(),
            (vertical_impulse - 0.1).to_bits()
        );
    }

    #[test]
    fn null_upgrade_preserves_ordinal_and_equal_health_range_consumes_no_extra_rng() {
        let mut object = nugget();
        object.count = 2;
        let (mut game, source, child_type) = setup(vec![object], true);
        let (mut control, _, control_child) = setup(Vec::new(), false);
        // Native singleton name selection and equal health bounds draw no RNG
        // with CRC0. Both actual child constructors still consume their own RNG.
        control.new_object(control_child, 0, true);
        control.new_object(control_child, 0, true);
        let first = game.entities.len();
        game.grant_native_object_upgrade(source, "Create");
        assert_eq!(game.entities.len(), first + 2);
        assert_eq!(game.random, control.random);
        for child in first..first + 2 {
            assert_eq!(game.entities[child].type_index, child_type);
            assert_eq!(game.entities[child].producer, Some(source));
            assert_eq!(
                game.object_position(child).map(f32::to_bits),
                [357.0_f32, 253.0, 2.0].map(f32::to_bits)
            );
            assert_eq!(game.entities[child].health.to_bits(), 25.0_f32.to_bits());
        }
        assert!(game.native_upgrade_executed(source, "ObjectCreationUpgrade", 0));
        assert!(game.native_upgrade_executed(source, "ObjectCreationUpgrade", 1));
        let random = game.random;
        game.activate_upgrade_modules(source);
        assert_eq!(game.entities.len(), first + 2);
        assert_eq!(game.random, random);
    }

    #[test]
    fn failed_containment_queues_created_object_for_real_deferred_destruction() {
        let mut object = nugget();
        object.flags |= CONTAIN_INSIDE_SOURCE;
        let (mut game, source, _) = setup(vec![object], false);
        let child = game.entities.len();
        game.grant_native_object_upgrade(source, "Create");
        assert_eq!(game.entities.len(), child + 1);
        assert_eq!(game.entities[child].producer, Some(source));
        assert!(game.entities[child].exists());
        assert!(game.update_objects[game.entities[child].update_owner].destroy_requested);
        game.process_update_destruction();
        assert!(!game.entities[child].exists());
    }

    #[test]
    fn inactive_player_suppresses_required_live_nugget_before_selection_or_construction() {
        let mut object = nugget();
        object.flags |= REQUIRES_LIVE_PLAYER;
        let (mut game, source, _) = setup(vec![object], false);
        game.defeated[0] = true;
        assert!(!game.player_active(0));
        let count = game.entities.len();
        let random = game.random;
        game.grant_native_object_upgrade(source, "Create");
        assert_eq!(game.entities.len(), count);
        assert_eq!(game.random, random);
        assert!(game.native_upgrade_executed(source, "ObjectCreationUpgrade", 0));
    }

    #[test]
    fn named_team_is_visible_to_create_triggered_replacement_constructor() {
        let mut game = tests::game();
        let prototype = game
            .team_membership
            .add_prototype(0, "NamedConstructorTeam".into());
        let team = game.team_membership.create_team(prototype);
        let mut replacement = tests::fixture_type(Faction::Usa, Kind::Barracks);
        replacement.name = "NamedTeamReplacement".into();
        let replacement_type = game.catalog.len();
        game.catalog.push(replacement);
        let mut original = tests::fixture_type(Faction::Usa, Kind::Barracks);
        original
            .behavior
            .replacement_names
            .push("NamedTeamReplacement".into());
        tests::register_behavior(&mut original, "ReplaceObjectUpgrade", 0x80);
        original
            .behavior_modules
            .last_mut()
            .unwrap()
            .upgrade
            .triggers = vec!["Replace".into()];
        original
            .behavior
            .radar_grants
            .push(crate::skirmish::radar_update::Grant {
                name: Some("Replace".into()),
                player: false,
                on_created: true,
                academy_radar: false,
            });
        tests::register_behavior(&mut original, "GrantUpgradeCreate", 0x8);
        let original_type = game.catalog.len();
        game.catalog.push(original);
        let old = game.new_object_on_team(original_type, 0, Some(team));
        let event = game
            .presentation
            .replacement_events
            .last()
            .expect("CREATE-triggered replacement");
        assert_eq!(event.previous, old);
        let next = event.next;
        assert_eq!(game.entities[next].type_index, replacement_type);
        assert_eq!(game.team_membership.team_of(old), Some(team));
        assert_eq!(game.team_membership.team_of(next), Some(team));
        assert!(game.update_objects[game.entities[old].update_owner].destroy_requested);
    }

    #[test]
    fn like_existing_structure_flattens_before_source_height_and_path_registration() {
        let mut object = nugget();
        object.offset = [60.0, 0.0, 40.0];
        let (mut game, source, child_type) = setup(vec![object], false);
        let mut child = tests::fixture_type(Faction::Usa, Kind::TechBuilding);
        child.name = "OclFixtureChild".into();
        child.geometry.shape = crate::collision_geometry::Shape::Box;
        child.geometry.major = 10.0;
        child.geometry.minor = 10.0;
        game.catalog[child_type] = child;
        let mut heights = vec![12.5; 81 * 61];
        heights[25 * 81 + 41] = 2.5;
        game.set_projectile_terrain(81, 61, [0.0; 2], heights, 40.0)
            .unwrap();
        let first = game.entities.len();
        game.grant_native_object_upgrade(source, "Create");
        assert_eq!(
            game.ground_height([420.0, 250.0]).to_bits(),
            2.5_f32.to_bits()
        );
        // Placement is offset, but source intentionally samples original source XY for Z.
        assert_eq!(
            game.object_position(first).map(f32::to_bits),
            [410.0_f32, 250.0, 12.5].map(f32::to_bits)
        );
        assert!(
            game.navigation
                .is_blocked(game.cell_at([410.0, 250.0]).unwrap())
        );
    }

    #[test]
    fn ground_aligned_uses_native_orientation_setter_on_sloping_child() {
        let mut object = nugget();
        object.disposition = physical_ocl::ON_GROUND;
        let (mut game, source, child_type) = setup(vec![object], false);
        game.catalog[child_type]
            .kind_of
            .push("STICK_TO_TERRAIN_SLOPE".into());
        let heights = (0..61)
            .flat_map(|_| (0..81_u16).map(|x| f32::from(x) * 0.625))
            .collect();
        game.set_projectile_terrain(81, 61, [-20.0; 2], heights, 40.0)
            .unwrap();
        let first = game.entities.len();
        game.grant_native_object_upgrade(source, "Create");
        let basis = game.object_transform(first).1;
        assert!(
            basis[0][2] < 0.0,
            "Thing orientation uses terrain normal, not a pure yaw matrix"
        );
        assert_eq!(
            game.object_position(first)[2].to_bits(),
            game.ground_height(game.entities[first].position).to_bits()
        );
    }
}
