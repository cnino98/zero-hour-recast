//! Native footprint registration and `AIUpdate::ignoreObstacle` ownership.
use super::{Footprint, Skirmish};
use crate::{Cell, Grid, grid::LayerCell};
use std::collections::BTreeSet;

mod modification;
mod raster;
pub(in crate::skirmish) use raster::cells;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::skirmish) enum OwnerId {
    Actor(usize),
    Static(usize),
    Fence(usize),
    Neutral(usize),
}

#[derive(Clone, Copy)]
pub(in crate::skirmish) struct Owner {
    pub(in crate::skirmish) id: OwnerId,
    pub(in crate::skirmish) transparent: bool,
    pub(in crate::skirmish) player: usize,
}

struct Claims {
    layers: Vec<Option<LayerClaims>>,
    modifications: Vec<crate::grid::ZoneModification>,
}

struct LayerClaims {
    origin: Cell,
    width: u16,
    height: u16,
    cells: BTreeSet<Cell>,
    owners: Vec<Option<Owner>>,
}

impl LayerClaims {
    fn index(&self, cell: Cell) -> Option<usize> {
        let x = cell.x.checked_sub(self.origin.x)?;
        let y = cell.y.checked_sub(self.origin.y)?;
        (x < self.width && y < self.height)
            .then_some(usize::from(y) * usize::from(self.width) + usize::from(x))
    }
}

impl Claims {
    fn prepare(&mut self, grid: &Grid, terrain: &crate::terrain::TerrainMap) {
        if self.layers.is_empty() {
            self.layers.resize_with(16, || None);
        }
        let bounds = std::iter::once((
            1,
            [
                Cell { x: 0, y: 0 },
                Cell {
                    x: grid.width(),
                    y: grid.height(),
                },
            ],
        ))
        .chain(
            terrain
                .path_layers()
                .filter_map(|layer| terrain.bridge_bounds(layer).map(|bounds| (layer, bounds))),
        );
        for (layer, [origin, maximum]) in bounds {
            let width = maximum.x - origin.x;
            let height = maximum.y - origin.y;
            let slot = &mut self.layers[usize::from(layer)];
            if slot.as_ref().is_none_or(|old| {
                old.origin != origin || old.width != width || old.height != height
            }) {
                *slot = Some(LayerClaims {
                    origin,
                    width,
                    height,
                    cells: BTreeSet::new(),
                    owners: vec![None; usize::from(width) * usize::from(height)],
                });
            }
        }
    }
    fn at(&self, node: LayerCell) -> Option<&Owner> {
        let layer = self.layers.get(usize::from(node.layer))?.as_ref()?;
        layer.owners.get(layer.index(node.cell)?)?.as_ref()
    }
    fn remove(&mut self, node: LayerCell) {
        let layer = self.layers[usize::from(node.layer)]
            .as_mut()
            .expect("registered layer");
        layer.cells.remove(&node.cell);
        let index = layer.index(node.cell).expect("registered cell");
        layer.owners[index] = None;
    }
    fn insert(&mut self, node: LayerCell, owner: Owner) -> bool {
        let layer = self.layers[usize::from(node.layer)]
            .as_mut()
            .expect("registered layer");
        if !layer.cells.insert(node.cell) {
            return false;
        }
        let index = layer.index(node.cell).expect("registered cell");
        layer.owners[index] = Some(owner);
        true
    }
    fn update(&mut self, node: LayerCell, owner: Owner) {
        let layer = self.layers[usize::from(node.layer)]
            .as_mut()
            .expect("registered layer");
        let index = layer.index(node.cell).expect("registered cell");
        layer.owners[index] = Some(owner);
    }
    fn clear(&mut self) {
        self.modifications.clear();
        for layer in self.layers.iter_mut().flatten() {
            layer.cells.clear();
            layer.owners.fill(None);
        }
    }
}

struct FenceRegistration {
    cells: BTreeSet<Cell>,
    owner: Owner,
}

pub(in crate::skirmish) struct Obstacles {
    map: Vec<Option<Registration>>,
    fences: Vec<Option<FenceRegistration>>,
    neutral: Vec<Option<Registration>>,
    neutral_owners: Vec<Option<usize>>,
    objects: Vec<Option<Registration>>,
    removed_objects: BTreeSet<usize>,
    claims: Claims,
}

struct Registration {
    footprint: Footprint,
    rubble: bool,
    cells: BTreeSet<LayerCell>,
    owner: Owner,
}

impl Obstacles {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            map: Vec::new(),
            fences: Vec::new(),
            neutral: Vec::new(),
            neutral_owners: Vec::new(),
            objects: Vec::new(),
            removed_objects: BTreeSet::new(),
            claims: Claims {
                layers: Vec::new(),
                modifications: Vec::new(),
            },
        }
    }

    pub(in crate::skirmish) fn clear(&mut self) {
        self.map.clear();
        self.fences.clear();
        self.neutral.clear();
        self.neutral_owners.clear();
        self.objects.clear();
        self.removed_objects.clear();
        self.claims.clear();
    }

    pub(in crate::skirmish) fn take_zone_modifications(
        &mut self,
    ) -> Vec<crate::grid::ZoneModification> {
        std::mem::take(&mut self.claims.modifications)
    }

    pub(in crate::skirmish) fn at(&self, cell: Cell) -> Option<&Owner> {
        self.claims.at(LayerCell { cell, layer: 1 })
    }

    pub(in crate::skirmish) fn layer_cells(&self) -> impl Iterator<Item = LayerCell> + '_ {
        self.claims
            .layers
            .iter()
            .enumerate()
            .skip(2)
            .filter_map(|(layer, claims)| claims.as_ref().map(|claims| (layer, claims)))
            .flat_map(|(layer, claims)| {
                claims.cells.iter().map(move |cell| LayerCell {
                    cell: *cell,
                    layer: u8::try_from(layer).expect("bounded layer"),
                })
            })
    }

    pub(in crate::skirmish) fn obstacle_owners(
        &self,
    ) -> impl Iterator<Item = (LayerCell, &Owner)> + '_ {
        self.claims
            .layers
            .iter()
            .enumerate()
            .skip(1)
            .filter_map(|(layer, claims)| claims.as_ref().map(|claims| (layer, claims)))
            .flat_map(|(layer, claims)| {
                claims.cells.iter().map(move |cell| LayerCell {
                    cell: *cell,
                    layer: u8::try_from(layer).expect("bounded layer"),
                })
            })
            .filter_map(|node| {
                self.claims
                    .at(node)
                    .filter(|owner| !matches!(owner.id, OwnerId::Fence(_)))
                    .map(|owner| (node, owner))
            })
    }

    pub(in crate::skirmish) fn fence_cells(&self) -> impl Iterator<Item = LayerCell> + '_ {
        self.claims
            .layers
            .iter()
            .enumerate()
            .skip(1)
            .filter_map(|(layer, claims)| claims.as_ref().map(|claims| (layer, claims)))
            .flat_map(|(layer, claims)| {
                claims.cells.iter().map(move |cell| LayerCell {
                    cell: *cell,
                    layer: u8::try_from(layer).expect("bounded layer"),
                })
            })
            .filter(|&node| {
                self.claims
                    .at(node)
                    .is_some_and(|owner| matches!(owner.id, OwnerId::Fence(_)))
            })
    }

    pub(in crate::skirmish) fn owner_details(&self, cell: Cell) -> String {
        self.at(cell).map_or_else(
            || "no registered owner".into(),
            |owner| match owner.id {
                OwnerId::Actor(id) => format!("entity {id}"),
                OwnerId::Static(index) => format!("original map obstacle {index}"),
                OwnerId::Fence(index) => format!("original map fence {index}"),
                OwnerId::Neutral(index) => format!("original neutral reservation {index}"),
            },
        )
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn attack_obstacle(&self, layer: u8, cell: Cell) -> Option<&Owner> {
        let layer = if self.movement_terrain.bridge_cell(layer, cell).is_some() {
            layer
        } else {
            1
        };
        self.navigation_obstacles
            .claims
            .at(LayerCell { cell, layer })
    }

    pub(in crate::skirmish) fn registered_obstacle_cells(&mut self) -> BTreeSet<Cell> {
        self.navigation_obstacles
            .claims
            .prepare(&self.grid, &self.movement_terrain);
        // Pathfinder::newMap classifies existing objects newest first; GameLogic
        // prepends the regular and landmark bridges created before ordinary objects.
        for index in (0..self.bridge_objects.len()).rev() {
            self.register_object_obstacle(self.bridge_objects[index].id);
        }
        // GameLogic::startGame registers ordinary objects in map-file order.
        // Reserve neutral footprints before their later simulation IDs exist;
        // adoption below preserves those exact claims without reinsertion.
        for index in 0..self.layout.obstacle_order.len() {
            match self.layout.obstacle_order[index] {
                crate::skirmish::MapObstacle::Static { index, transparent } => {
                    self.register_map_obstacle(index, Some(transparent));
                }
                crate::skirmish::MapObstacle::Fence { index, transparent } => {
                    self.register_fence(index, Some(transparent));
                }
                crate::skirmish::MapObstacle::Neutral(index) => {
                    self.register_neutral_obstacle(index);
                }
            }
        }
        for index in 0..self.layout.fences.len() {
            self.register_fence(index, None);
        }
        for index in 0..self.layout.obstacles.len() {
            self.register_map_obstacle(index, None);
        }
        for index in 0..self.layout.neutral_objects.len() {
            self.register_neutral_obstacle(index);
        }
        for id in 0..self.entities.len() {
            self.register_object_obstacle(id);
        }
        self.navigation_obstacles.claims.layers[1]
            .as_ref()
            .expect("ground claims")
            .cells
            .clone()
    }

    pub(in crate::skirmish) fn reclassify_navigation_obstacles(&mut self) {
        // Installing the real map layer replaces the constructor's flat layer.
        // Keep neutral-to-entity identity while replaying original map order.
        self.navigation_obstacles.map.clear();
        self.navigation_obstacles.fences.clear();
        self.navigation_obstacles.neutral.clear();
        self.navigation_obstacles.objects.clear();
        self.navigation_obstacles.claims.clear();
        self.rebuild_navigation();
    }

    /// Explicit `Pathfinder::removeObjectFromPathfindMap` before deferred destruction.
    pub(in crate::skirmish) fn unregister_object_obstacle(&mut self, id: usize) {
        // Zero Hour's blast-crater footprints survive even explicit removal.
        if self.attack_kind(id, "BLAST_CRATER") {
            return;
        }
        self.remove_wall_footprint(id);
        self.navigation_obstacles.removed_objects.insert(id);
        if let Some(old) = self
            .navigation_obstacles
            .objects
            .get_mut(id)
            .and_then(Option::take)
        {
            remove_registration(
                &mut self.navigation_obstacles.claims,
                &mut self.movement_terrain,
                &self.grid,
                &old,
            );
        }
        self.rebuild_navigation();
    }

    /// Explicit `Pathfinder::addObjectToPathfindMap` after removal or containment.
    pub(in crate::skirmish) fn restore_object_obstacle(&mut self, id: usize) {
        self.navigation_obstacles.removed_objects.remove(&id);
        self.register_object_obstacle(id);
        self.rebuild_navigation();
    }

    pub(in crate::skirmish) fn register_object_obstacle(&mut self, id: usize) {
        // Rebuilding cached navigation must not reinsert an explicitly removed
        // Object while GameLogic still retains it on the destruction list.
        if self.navigation_obstacles.removed_objects.contains(&id) {
            return;
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        // Zero Hour preserves an already classified blast crater after removal.
        if !actor.alive()
            && unit.kind_of.iter().any(|kind| kind == "BLAST_CRATER")
            && self
                .navigation_obstacles
                .objects
                .get(id)
                .is_some_and(Option::is_some)
        {
            return;
        }
        let footprint = self
            .static_obstacle(id)
            .then(|| (self.object_footprint(id), !actor.alive()));
        register(
            &mut self.navigation_obstacles.objects,
            &mut self.navigation_obstacles.claims,
            id,
            footprint,
            &self.grid,
            &mut self.movement_terrain,
            Owner {
                id: OwnerId::Actor(id),
                player: actor.owner,
                transparent: unit
                    .kind_of
                    .iter()
                    .any(|kind| kind == "CAN_SEE_THROUGH_STRUCTURE"),
            },
        );
    }

    pub(in crate::skirmish) fn object_footprint(&self, id: usize) -> Footprint {
        let actor = &self.entities[id];
        let geometry = self.body_geometry(id);
        Footprint {
            position: actor.position,
            angle: actor.facing,
            major: geometry.major,
            minor: geometry.minor,
            shape: geometry.shape,
        }
    }

    pub(in crate::skirmish) fn reclassify_rubble_obstacle(&mut self, id: usize) {
        self.remove_wall_footprint(id);
        // setGeometryInfoZ precedes the native remove/add. Sphere removal
        // therefore samples its new radius, including the source's old outer claims.
        let footprint = self.object_footprint(id);
        if let Some(old) = self
            .navigation_obstacles
            .objects
            .get_mut(id)
            .and_then(Option::as_mut)
        {
            old.footprint = footprint;
        }
        self.register_object_obstacle(id);
    }

    fn register_fence(&mut self, index: usize, transparent: Option<bool>) {
        let obstacles = &mut self.navigation_obstacles;
        if obstacles.fences.len() <= index {
            obstacles.fences.resize_with(index + 1, || None);
        }
        if let Some(registration) = &mut obstacles.fences[index] {
            if let Some(transparent) = transparent {
                registration.owner.transparent = transparent;
            }
            for cell in &registration.cells {
                obstacles.claims.update(
                    LayerCell {
                        cell: *cell,
                        layer: 1,
                    },
                    registration.owner,
                );
            }
            return;
        }
        let owner = Owner {
            id: OwnerId::Fence(index),
            player: self.layout.starts.len(),
            transparent: transparent.unwrap_or(false),
        };
        let sampled = raster::fence_cells(&self.layout.fences[index], &self.grid);
        let cells: BTreeSet<_> = sampled
            .iter()
            .copied()
            .filter(|cell| {
                self.movement_terrain.accepts_obstacle(*cell)
                    && obstacles.claims.insert(
                        LayerCell {
                            cell: *cell,
                            layer: 1,
                        },
                        owner,
                    )
            })
            .collect();
        if !cells.is_empty() {
            let fence = &self.layout.fences[index];
            let bounds = modification::bounds(fence.position, &sampled, true);
            obstacles
                .claims
                .modifications
                .push(crate::grid::ZoneModification {
                    update_bounds: Some(bounds),
                    cells: cells
                        .iter()
                        .map(|&cell| crate::grid::ZoneCellChange {
                            cell,
                            kind: crate::terrain::CellType::Obstacle,
                            fence: true,
                            reset_zone: true,
                        })
                        .collect(),
                    post_cells: Vec::new(),
                    pinched: Vec::new(),
                    force_dirty: true,
                });
        }
        obstacles.fences[index] = Some(FenceRegistration { cells, owner });
    }

    fn register_map_obstacle(&mut self, index: usize, transparent: Option<bool>) {
        let footprint = Some(self.layout.obstacles[index].clone());
        let transparent = transparent.unwrap_or_else(|| {
            self.navigation_obstacles
                .map
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(|registration| registration.owner.transparent)
        });
        let player = self
            .layout
            .obstacle_teams
            .get(index)
            .map_or_else(|| self.neutral_owner(), |team| self.placed_map_team(team).0);
        register(
            &mut self.navigation_obstacles.map,
            &mut self.navigation_obstacles.claims,
            index,
            footprint.map(|footprint| (footprint, false)),
            &self.grid,
            &mut self.movement_terrain,
            Owner {
                id: OwnerId::Static(index),
                player,
                transparent,
            },
        );
    }

    fn register_neutral_obstacle(&mut self, index: usize) {
        if let Some(id) = self
            .navigation_obstacles
            .neutral_owners
            .get(index)
            .copied()
            .flatten()
        {
            self.register_object_obstacle(id);
            return;
        }
        let record = &self.layout.neutral_objects[index];
        let footprint = self
            .catalog
            .iter()
            .find(|unit| unit.name == record.name)
            .and_then(|unit| {
                let ground = self.ground_height([record.position[0], record.position[1]]);
                let height = if record.position[2] == 0.0 {
                    unit.preferred_height
                } else {
                    (ground + record.position[2]) - ground
                };
                admitted(unit, false, height, unit.geometry.is_small)
                    .then(|| unit.footprint([record.position[0], record.position[1]], record.angle))
            });
        let transparent = self
            .catalog
            .iter()
            .find(|unit| unit.name == record.name)
            .is_some_and(|unit| {
                unit.kind_of
                    .iter()
                    .any(|kind| kind == "CAN_SEE_THROUGH_STRUCTURE")
            });
        let player = self.placed_map_team(&record.original_team).0;
        register(
            &mut self.navigation_obstacles.neutral,
            &mut self.navigation_obstacles.claims,
            index,
            footprint.map(|footprint| (footprint, false)),
            &self.grid,
            &mut self.movement_terrain,
            Owner {
                id: OwnerId::Neutral(index),
                player,
                transparent,
            },
        );
    }

    pub(in crate::skirmish) fn registered_neutral_object(&self, index: usize) -> Option<usize> {
        self.navigation_obstacles
            .neutral_owners
            .get(index)
            .copied()
            .flatten()
    }

    pub(in crate::skirmish) fn adopt_neutral_obstacle(&mut self, index: usize, id: usize) {
        let obstacles = &mut self.navigation_obstacles;
        obstacles
            .neutral_owners
            .resize(self.layout.neutral_objects.len(), None);
        obstacles.neutral_owners[index] = Some(id);
        if obstacles.objects.len() <= id {
            obstacles.objects.resize_with(id + 1, || None);
        }
        obstacles.objects[id] = obstacles.neutral.get_mut(index).and_then(Option::take);
        if let Some(registration) = &mut obstacles.objects[id] {
            registration.owner.id = OwnerId::Actor(id);
            for cell in &registration.cells {
                obstacles.claims.update(*cell, registration.owner);
            }
        }
    }

    fn static_obstacle(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        (actor.alive() || self.retained_rubble(id))
            && !actor.containment.nested_removed
            && admitted(
                unit,
                self.object_disabled(id),
                self.height_above_ground(id),
                self.body_geometry(id).is_small,
            )
    }

    pub(in crate::skirmish) fn ignored_navigation(&self, id: usize) -> Option<Grid> {
        let ignored = self.entities[id].ai.ignored_producer?;
        let registration = self.navigation_obstacles.objects.get(ignored)?.as_ref()?;
        // PathfindCell stores one obstacle ID. A later overlapping footprint
        // neither owns this cell nor prevents ignoring its original owner.
        Some(
            self.navigation_for(id)
                .without_layer_obstacles(registration.cells.iter().copied()),
        )
    }
}

fn admitted(unit: &crate::skirmish::UnitType, disabled: bool, height: f32, is_small: bool) -> bool {
    let has = |name: &str| unit.kind_of.iter().any(|kind| kind == name);
    has("STRUCTURE")
        && (has("IMMOBILE") || disabled)
        && !["MINE", "PROJECTILE", "BRIDGE_TOWER"].into_iter().any(has)
        && !is_small
        && (height <= 10.0 || has("BLAST_CRATER"))
}

fn register(
    registrations: &mut Vec<Option<Registration>>,
    claims: &mut Claims,
    id: usize,
    footprint: Option<(Footprint, bool)>,
    grid: &Grid,
    terrain: &mut crate::terrain::TerrainMap,
    owner: Owner,
) {
    if registrations.len() <= id {
        registrations.resize_with(id + 1, || None);
    }
    let previous = &mut registrations[id];
    if matches!((&previous, &footprint), (Some(old), Some((new, rubble))) if same(&old.footprint, new) && old.rubble == *rubble)
    {
        for cell in &previous.as_ref().expect("unchanged registration").cells {
            claims.update(*cell, owner);
        }
        previous.as_mut().expect("unchanged registration").owner = owner;
        return;
    }
    if let Some(old) = previous.take() {
        remove_registration(claims, terrain, grid, &old);
    }
    let Some((footprint, rubble)) = footprint else {
        return;
    };
    let sampled = cells(&footprint, grid);
    let mut changes = Vec::new();
    let mut claimed = BTreeSet::new();
    for &cell in &sampled {
        let node = LayerCell { cell, layer: 1 };
        if !terrain.accepts_obstacle(cell) || claims.at(node).is_some() {
            continue;
        }
        let kind = if rubble {
            terrain.add_path_rubble(cell);
            crate::terrain::CellType::Rubble
        } else {
            claims.insert(node, owner);
            claimed.insert(node);
            crate::terrain::CellType::Obstacle
        };
        changes.push(crate::grid::ZoneCellChange {
            cell,
            kind,
            fence: false,
            reset_zone: true,
        });
    }
    let bounds = modification::bounds(footprint.position, &sampled, false);
    let pinched = modification::finish(claims, terrain, grid, bounds, false);
    claims.modifications.push(crate::grid::ZoneModification {
        update_bounds: Some(bounds),
        cells: changes,
        post_cells: pinched.0,
        pinched: pinched.1,
        force_dirty: true,
    });
    *previous = Some(Registration {
        footprint,
        rubble,
        cells: claimed,
        owner,
    });
}

fn remove_registration(
    claims: &mut Claims,
    terrain: &mut crate::terrain::TerrainMap,
    grid: &Grid,
    old: &Registration,
) {
    let sampled = cells(&old.footprint, grid);
    let mut changes = Vec::new();
    for &cell in &sampled {
        let node = LayerCell { cell, layer: 1 };
        // removeObstacle clears ownerless RUBBLE before checking the supplied ID.
        let rubble = terrain.remove_path_rubble(cell);
        let owned = claims
            .at(node)
            .is_some_and(|owner| owner.id == old.owner.id);
        if owned {
            claims.remove(node);
            terrain.clear_path_obstacle(cell);
        }
        if rubble || owned {
            changes.push(crate::grid::ZoneCellChange {
                cell,
                kind: crate::terrain::CellType::Clear,
                fence: false,
                reset_zone: owned,
            });
        }
    }
    let bounds = modification::bounds(old.footprint.position, &sampled, false);
    let pinched = modification::finish(claims, terrain, grid, bounds, true);
    claims.modifications.push(crate::grid::ZoneModification {
        update_bounds: Some(bounds),
        cells: changes,
        post_cells: pinched.0,
        pinched: pinched.1,
        force_dirty: true,
    });
}

fn same(first: &Footprint, second: &Footprint) -> bool {
    first.position.map(f32::to_bits) == second.position.map(f32::to_bits)
        && first.angle.to_bits() == second.angle.to_bits()
        && first.major.to_bits() == second.major.to_bits()
        && first.minor.to_bits() == second.minor.to_bits()
        && std::mem::discriminant(&first.shape) == std::mem::discriminant(&second.shape)
}
