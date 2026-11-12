//! `AISkirmishPlayer::buildAIBaseDefenseStructure` circle and priority placement.
use super::{Building, OpponentFaction, OpponentRules, State, enemies};
use crate::skirmish::{
    Skirmish,
    movement::{bounding_radius, distance_squared},
};
use std::f32::consts::PI;

pub(super) struct Placement {
    radius: f32,
    front: u32,
    flank: u32,
    angles: [[f32; 2]; 3],
}
impl Placement {
    pub(super) fn new(buildings: &[Building], data: &OpponentFaction, center: [f32; 2]) -> Self {
        let mut radius_squared = 0.0_f32;
        for (building, source) in buildings.iter().zip(&data.buildings) {
            if building.name.is_empty() {
                continue;
            }
            let radius = source.bounding_radius * 0.4;
            let x = (building.position[0] - center[0]).abs() + radius;
            let y = (building.position[1] - center[1]).abs() + radius;
            let y_squared = y * y;
            radius_squared = radius_squared.max(x * x + y_squared);
        }
        Self {
            radius: radius_squared.sqrt(),
            front: 0,
            flank: 0,
            angles: [[0.0; 2]; 3],
        }
    }

    fn angle(&mut self, flank: bool, step: f32) -> f32 {
        let (sector, selector) = if flank {
            (if self.flank & 1 == 1 { 1 } else { 2 }, self.flank >> 1)
        } else {
            (0, self.front)
        };
        if selector & 1 == 1 {
            self.angles[sector][1] -= step;
            self.angles[sector][1]
        } else {
            let value = self.angles[sector][0];
            self.angles[sector][0] += step;
            value
        }
    }
}

pub(super) fn request(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    name: &str,
    flank: bool,
) -> bool {
    let Some(kind) = game.catalog.iter().position(|unit| unit.name == name) else {
        return false;
    };
    let unit = &game.catalog[kind];
    loop {
        let label = if flank {
            if state.defenses.flank & 1 == 1 {
                "Flank"
            } else {
                "Backdoor"
            }
        } else {
            "Center"
        };
        let Some(goal) = approach(game, rules, state, label, flank) else {
            return true;
        };
        let offset = [
            goal[0] - state.base_center[0],
            goal[1] - state.base_center[1],
        ];
        let y_squared = offset[1] * offset[1];
        let x_squared = offset[0] * offset[0];
        let length = (x_squared + y_squared).sqrt();
        let direction = if length > 0.0 {
            [offset[0] / length, offset[1] / length]
        } else {
            [0.0; 2]
        };
        let distance = state.defenses.radius + rules.defense_extra_distance;
        let offset = direction.map(|value| value * distance);
        let circumference = 2.0 * PI * distance;
        let radius = bounding_radius(&unit.footprint([0.0; 2], 0.0));
        let step = 2.0 * PI * (radius * 4.0 / circumference);
        let angle = state.defenses.angle(flank, step);
        if angle > PI / 3.0 {
            return true;
        }
        let sin = angle.sin();
        let cos = angle.cos();
        let y_sin = offset[1] * sin;
        let x_sin = offset[0] * sin;
        let x_cos = offset[0] * cos;
        let y_cos = offset[1] * cos;
        let point = [
            state.base_center[0] + (x_cos - y_sin),
            state.base_center[1] + (y_cos + x_sin),
        ];
        let legal = game.can_place_at_angle(kind, point, unit.placement_angle, true);
        if flank {
            state.defenses.flank += 1;
        } else {
            state.defenses.front += 1;
        }
        if legal {
            state.buildings.insert(
                0,
                Building {
                    name: name.into(),
                    position: point,
                    angle: unit.placement_angle,
                    remaining: 1,
                    automatic: false,
                    priority: true,
                    object: None,
                    destroyed_at: None,
                    supply: None,
                },
            );
            return true;
        }
    }
}

fn approach(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &State,
    label: &str,
    flank: bool,
) -> Option<[f32; 2]> {
    let label = format!("{label}{}", state.start + 1);
    rules
        .map
        .waypoints
        .iter()
        .filter(|way| {
            way.labels
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&label))
        })
        .min_by(|left, right| {
            distance_squared(left.position, state.base_center)
                .total_cmp(&distance_squared(right.position, state.base_center))
        })
        .map(|way| way.position)
        .or_else(|| (!flank).then(|| enemies::structure_center(game, state.enemy.unwrap_or(0))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::opponent::{AiBuilding, AiWaypoint, OpponentDifficulty};

    fn fixture() -> (Skirmish, OpponentRules, State, usize) {
        let mut game = crate::skirmish::tests::game();
        let kind = game
            .catalog
            .iter()
            .position(|unit| unit.kind == crate::skirmish::Kind::Barracks)
            .unwrap();
        game.catalog[kind].name = "DefenseFixture".into();
        game.catalog[kind].geometry.major = 5.0;
        game.catalog[kind].geometry.minor = 5.0;
        let mut rules = super::super::tests::rules(Vec::new());
        rules.factions[0].defense_template = "DefenseFixture".into();
        let mut state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
        state.base_center = [300.0, 300.0];
        state.defenses.radius = 100.0;
        rules.map.waypoints.push(AiWaypoint {
            id: 1,
            position: [500.0, 300.0],
            labels: vec![format!("Center{}", state.start + 1)],
            links: vec![],
            bidirectional: false,
        });
        (game, rules, state, kind)
    }

    #[test]
    fn front_requests_alternate_exact_circle_offsets_and_prepend_single_build_orders() {
        let (game, rules, mut state, kind) = fixture();
        for _ in 0..3 {
            assert!(request(&game, &rules, &mut state, "DefenseFixture", false));
        }
        assert_eq!(state.buildings.len(), 3);
        assert_eq!(state.defenses.front, 3);
        let distance = 120.0_f32;
        let angle = 2.0 * PI * (game.catalog[kind].geometry.major * 4.0 / (2.0 * PI * distance));
        let along = distance * angle.cos();
        let x = along + 300.0;
        let y = distance * angle.sin();
        assert_eq!(
            state.buildings[2].position.map(f32::to_bits),
            [420.0_f32, 300.0].map(f32::to_bits)
        );
        assert_eq!(
            state.buildings[1].position.map(f32::to_bits),
            [x, 300.0 - y].map(f32::to_bits)
        );
        assert_eq!(
            state.buildings[0].position.map(f32::to_bits),
            [x, 300.0 + y].map(f32::to_bits)
        );
        assert!(
            state.buildings.iter().all(|building| building.priority
                && building.remaining == 1
                && !building.automatic)
        );
    }

    #[test]
    fn missing_flank_is_a_noop_and_flanks_begin_with_backdoor_then_left_route() {
        let (game, mut rules, mut state, _) = fixture();
        assert!(request(&game, &rules, &mut state, "DefenseFixture", true));
        assert!(state.buildings.is_empty());
        assert_eq!(state.defenses.flank, 0);
        for (name, point) in [("Backdoor", [300.0, 100.0]), ("Flank", [300.0, 500.0])] {
            rules.map.waypoints.push(AiWaypoint {
                id: 2,
                position: point,
                labels: vec![format!("{name}{}", state.start + 1)],
                links: vec![],
                bidirectional: false,
            });
        }
        assert!(request(&game, &rules, &mut state, "DefenseFixture", true));
        assert!(request(&game, &rules, &mut state, "DefenseFixture", true));
        assert_eq!(
            state.buildings[1].position.map(f32::to_bits),
            [300.0_f32, 180.0].map(f32::to_bits)
        );
        assert_eq!(
            state.buildings[0].position.map(f32::to_bits),
            [300.0_f32, 420.0].map(f32::to_bits)
        );
    }

    #[test]
    fn base_circle_includes_unbuilt_unrepresented_authored_geometry() {
        let (_, mut rules, mut state, _) = fixture();
        rules.factions[0].buildings = [([0.0, 0.0], 10.0), ([100.0, 80.0], 30.0)]
            .into_iter()
            .map(|(position, bounding_radius)| AiBuilding {
                name: "Unrepresented".into(),
                bounding_radius,
                position,
                angle: 0.0,
                rebuilds: 0,
                automatic: false,
            })
            .collect();
        state.buildings = rules.factions[0]
            .buildings
            .iter()
            .map(|entry| Building {
                name: entry.name.clone(),
                position: entry.position,
                angle: 0.0,
                remaining: 1,
                automatic: false,
                priority: false,
                object: None,
                destroyed_at: None,
                supply: None,
            })
            .collect();
        let placement = Placement::new(&state.buildings, &rules.factions[0], [50.0, 40.0]);
        let radius_squared = 6548.0_f32; // (50+12)^2 + (40+12)^2
        assert_eq!(placement.radius.to_bits(), radius_squared.sqrt().to_bits());
    }
}
