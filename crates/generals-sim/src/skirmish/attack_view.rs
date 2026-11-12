//! `Pathfinder::isAttackViewBlockedByObstacle`, SH 2bfaea60, CRC compatibility off.
use super::{Skirmish, movement::obstacles::OwnerId};

impl Skirmish {
    pub(super) fn attack_view_blocked(
        &self,
        attacker: usize,
        victim: Option<usize>,
        attacker_position: [f32; 3],
        victim_position: [f32; 3],
    ) -> bool {
        let actor = &self.entities[attacker];
        let unit = &self.catalog[actor.type_index];
        if !self.ai_rules.attack_uses_line_of_sight
            || !unit
                .kind_of
                .iter()
                .any(|kind| kind == "ATTACK_NEEDS_LINE_OF_SIGHT")
        {
            return false;
        }
        if !unit.kind_of.iter().any(|kind| kind == "IMMOBILE")
            && let Some(weapon) = self.allocated_weapon(attacker, actor.weapon_slot())
        {
            if weapon.contact() {
                return false;
            }
            let mut origin = attacker_position;
            origin[2] += unit.geometry_bounds.max_height;
            let destination = victim.map_or(victim_position, |id| self.target_center(id));
            if self
                .projectile_terrain
                .as_ref()
                .is_some_and(|terrain| !terrain.line_of_sight(origin, destination))
            {
                return true;
            }
        }
        let clamp_cell = |point: [f32; 3]| {
            self.cell_at([
                point[0].clamp(0.0, f32::from(self.grid.width() - 1) * 10.0),
                point[1].clamp(0.0, f32::from(self.grid.height() - 1) * 10.0),
            ])
            .expect("worldToCell clamps to map extent")
        };
        let start = clamp_cell(attacker_position);
        let end = clamp_cell(victim_position);
        let victim_layer = victim.map_or(1, |id| self.entities[id].layer);
        let victim_owner = self
            .cell_at([victim_position[0], victim_position[1]])
            .and_then(|cell| self.attack_obstacle(victim_layer, cell))
            .map(|owner| owner.id);
        let layer = if victim_layer == 1 {
            actor.layer
        } else {
            victim_layer
        };
        let skip = if actor.layer == 1 { 0 } else { 3 };
        let ignored = [
            Some(attacker),
            victim,
            actor.contained_by(),
            self.slaved_parent(attacker),
            victim.and_then(|id| self.slaved_parent(id)),
        ];
        self.grid
            .line_cells(start, end)
            .into_iter()
            .skip(skip)
            .any(|cell| {
                self.attack_obstacle(layer, cell).is_some_and(|owner| {
                    !owner.transparent
                        && Some(owner.id) != victim_owner
                        && !ignored
                            .into_iter()
                            .flatten()
                            .any(|id| owner.id == OwnerId::Actor(id))
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Cell,
        collision_geometry::Shape,
        skirmish::{
            Faction, Kind,
            tests::{fixture_type, game},
        },
    };

    fn attacker(game: &mut Skirmish, contact: bool) -> usize {
        let mut unit = fixture_type(Faction::Gla, Kind::Tank);
        unit.kind_of.push("ATTACK_NEEDS_LINE_OF_SIGHT".into());
        if contact {
            unit.weapon.range = 0.0;
        }
        let index = game.catalog.len();
        game.catalog.push(unit);
        game.ai_rules.attack_uses_line_of_sight = true;
        game.spawn(index, 0, [305.0, 205.0], true)
    }

    fn structure(game: &mut Skirmish, position: [f32; 2], radius: f32) -> usize {
        let mut unit = fixture_type(Faction::Gla, Kind::Factory);
        unit.geometry.shape = Shape::Box;
        unit.geometry.major = radius;
        unit.geometry.minor = radius;
        let index = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(index, 0, position, true);
        game.rebuild_navigation();
        id
    }

    fn blocked(game: &Skirmish, actor: usize) -> bool {
        game.attack_view_blocked(actor, None, [305.0, 205.0, 0.0], [405.0, 205.0, 0.0])
    }

    #[test]
    fn opaque_structure_blocks_until_its_original_transparency_kind_is_set() {
        let mut game = game();
        let actor = attacker(&mut game, false);
        let obstacle = structure(&mut game, [355.0, 205.0], 20.0);
        assert!(blocked(&game, actor));
        let index = game.entities[obstacle].type_index;
        game.catalog[index]
            .kind_of
            .push("CAN_SEE_THROUGH_STRUCTURE".into());
        game.rebuild_navigation();
        assert!(!blocked(&game, actor));
    }

    #[test]
    fn victim_inside_another_obstacle_ignores_only_that_obstacle() {
        let mut game = game();
        let actor = attacker(&mut game, false);
        let enclosing = structure(&mut game, [395.0, 205.0], 20.0);
        let victim = game.spawn(6, 1, [405.0, 205.0], true);
        let target_cell = Cell { x: 40, y: 20 };
        assert!(
            game.attack_obstacle(1, target_cell)
                .is_some_and(|owner| owner.id == OwnerId::Actor(enclosing))
        );
        let attack = |game: &Skirmish| {
            game.attack_view_blocked(
                actor,
                Some(victim),
                [305.0, 205.0, 0.0],
                [405.0, 205.0, 0.0],
            )
        };
        assert!(!attack(&game));
        structure(&mut game, [345.0, 205.0], 10.0);
        assert!(attack(&game));
    }

    #[test]
    fn contact_bypasses_obstacles_but_immobile_attackers_still_check_them() {
        let mut game = game();
        let actor = attacker(&mut game, true);
        structure(&mut game, [355.0, 205.0], 20.0);
        assert!(!blocked(&game, actor));
        let index = game.entities[actor].type_index;
        game.catalog[index].kind_of.push("IMMOBILE".into());
        assert!(blocked(&game, actor));
    }

    #[test]
    fn terrain_uses_weapon_origin_and_skips_immobile_attackers() {
        let mut game = game();
        let actor = attacker(&mut game, false);
        let mut heights = vec![0.0; 80 * 60];
        heights[20 * 80 + 35] = 50.0;
        game.set_projectile_terrain(80, 60, [0.0; 2], heights, 10.0)
            .unwrap();
        assert!(blocked(&game, actor));
        let index = game.entities[actor].type_index;
        game.catalog[index].geometry_bounds.max_height = 150.0;
        assert!(!blocked(&game, actor));
        game.catalog[index].geometry_bounds.max_height = 10.0;
        game.catalog[index].kind_of.push("IMMOBILE".into());
        assert!(!blocked(&game, actor));
    }

    #[test]
    fn global_and_attacker_kind_each_gate_attack_visibility() {
        let mut game = game();
        let actor = attacker(&mut game, false);
        structure(&mut game, [355.0, 205.0], 20.0);
        assert!(blocked(&game, actor));
        game.ai_rules.attack_uses_line_of_sight = false;
        assert!(!blocked(&game, actor));
        game.ai_rules.attack_uses_line_of_sight = true;
        let index = game.entities[actor].type_index;
        game.catalog[index]
            .kind_of
            .retain(|kind| kind != "ATTACK_NEEDS_LINE_OF_SIGHT");
        assert!(!blocked(&game, actor));
    }

    #[test]
    fn elevated_attacker_skips_three_cells_but_not_the_fourth() {
        let mut game = game();
        let actor = attacker(&mut game, false);
        structure(&mut game, [325.0, 205.0], 1.0);
        assert!(blocked(&game, actor));
        game.entities[actor].layer = 0; // Native wall/rooftop layer; absent cells fall through to ground.
        assert!(!blocked(&game, actor));
        structure(&mut game, [335.0, 205.0], 1.0);
        assert!(blocked(&game, actor));
    }

    #[test]
    fn negative_target_has_no_victim_cell_but_still_walks_intervening_obstacles() {
        let mut game = game();
        let actor = attacker(&mut game, false);
        let attack = |game: &Skirmish| {
            game.attack_view_blocked(actor, None, [305.0, 205.0, 0.0], [-15.0, 205.0, 0.0])
        };
        assert!(!attack(&game));
        structure(&mut game, [105.0, 205.0], 10.0);
        assert!(attack(&game));
    }
}
