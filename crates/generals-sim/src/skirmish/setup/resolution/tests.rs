//! Before-extraction algorithms retained only as a comparison oracle.
use super::*;
use crate::skirmish::{PlayerStart, Skirmish};

impl Skirmish {
    fn previous_assign_factions_and_colors(&mut self) -> Vec<Faction> {
        self.player_colors = self.slots.iter().map(|slot| slot.color).collect();
        // checkForDuplicateColors: reverse slot order preserves the first owner.
        for owner in (0..self.slots.len()).rev() {
            if !self.slots[owner].occupied {
                continue;
            }
            if let Some(color) = self.player_colors[owner] {
                self.player_colors[owner] = None;
                if !self.player_colors.contains(&Some(color)) {
                    self.player_colors[owner] = Some(color);
                }
            }
        }
        // populateRandomSideAndColor interleaves side and color resolution per
        // occupied slot. The discarded draws use the ORIGINAL base seed.
        let color_maximum = self.progression.as_ref().map_or(7, |rules| {
            u32::try_from(rules.multiplayer_colors.len() - 1)
                .expect("validated source player palette")
        });
        let mut factions = Vec::with_capacity(self.slots.len());
        for owner in 0..self.slots.len() {
            let selected = self.slots[owner].faction;
            if !self.slots[owner].occupied {
                factions.push(selected.unwrap_or(Faction::Civilian));
                continue;
            }
            let faction = selected.unwrap_or_else(|| {
                for _ in 0..self.layout.random_seed % 7 {
                    self.random_between(1);
                }
                let index = usize::try_from(self.random_between(1000) % 3)
                    .expect("three original base factions");
                [Faction::Usa, Faction::China, Faction::Gla][index]
            });
            factions.push(faction);
            if self.player_colors[owner].is_some() {
                continue;
            }
            loop {
                let color = usize::try_from(self.random_between(color_maximum))
                    .expect("source color index");
                if !self.player_colors.contains(&Some(color)) {
                    self.player_colors[owner] = Some(color);
                    break;
                }
            }
        }
        factions
    }

    fn previous_assign_start_positions(&mut self, mut taken: Vec<bool>) {
        self.start_positions = self
            .slots
            .iter()
            .map(|slot| slot.start.filter(|_| slot.occupied))
            .chain([None])
            .collect();
        let mut any_picked = taken.iter().any(|&value| value);
        let mut team_position: [Option<usize>; 4] = [None; 4];
        for owner in 0..self.slots.len() {
            let slot = &self.slots[owner];
            if !slot.occupied || slot.start.is_some() {
                continue;
            }
            let team = slot.team.map(usize::from);
            let teammate = team.and_then(|index| team_position[index]);
            let chosen = if any_picked {
                let mut best = None;
                let mut best_distance = 0.0;
                for position in 0..taken.len() {
                    if taken[position] {
                        continue;
                    }
                    let distance = if let Some(teammate) = teammate {
                        distance_squared(
                            self.layout.starts[position].position,
                            self.layout.starts[teammate].position,
                        )
                        .sqrt()
                    } else {
                        taken
                            .iter()
                            .enumerate()
                            .filter(|(_, occupied)| **occupied)
                            .map(|(other, _)| {
                                distance_squared(
                                    self.layout.starts[position].position,
                                    self.layout.starts[other].position,
                                )
                                .sqrt()
                            })
                            .sum()
                    };
                    if best.is_none()
                        || if teammate.is_some() {
                            distance < best_distance
                        } else {
                            distance > best_distance
                        }
                    {
                        best = Some(position);
                        best_distance = distance;
                    }
                }
                best.expect("Validated occupied slots fit map starts")
            } else {
                let maximum = u32::try_from(taken.len() - 1).unwrap_or(0);
                loop {
                    let candidate = usize::try_from(self.random_between(maximum)).unwrap_or(0);
                    if !taken[candidate] {
                        break candidate;
                    }
                }
            };
            self.start_positions[owner] = Some(chosen);
            taken[chosen] = true;
            any_picked = true;
            if let Some(team) = team {
                team_position[team].get_or_insert(chosen);
            }
        }
    }
}

const STARTS: [[f32; 2]; 8] = [
    [140.0, 140.0],
    [660.0, 140.0],
    [660.0, 460.0],
    [140.0, 460.0],
    [400.0, 100.0],
    [700.0, 300.0],
    [400.0, 500.0],
    [100.0, 300.0],
];

fn slots(count: usize, pattern: u8) -> Vec<PlayerSlot> {
    (0..count)
        .map(|owner| {
            let mut slot = PlayerSlot {
                faction: None,
                occupied: true,
                difficulty: None,
                team: None,
                start: None,
                color: None,
            };
            match pattern {
                1 => {
                    if owner % 2 == 0 {
                        slot.faction = Some(Faction::China);
                        slot.color = Some(owner);
                    }
                    slot.team = Some(u8::try_from(owner % 2).unwrap());
                    if owner == 0 {
                        slot.start = Some(count - 1);
                    }
                    if owner == count - 1 {
                        slot.occupied = false;
                    }
                }
                2 => {
                    slot.color = Some(2);
                    slot.team = Some(u8::try_from(owner % 4).unwrap());
                    if owner == 2 {
                        slot.occupied = false;
                    }
                }
                3 => {
                    slot.faction = Some(Faction::Gla);
                    slot.start = Some(owner);
                    slot.color = (owner % 2 == 0).then_some(owner);
                }
                _ => {}
            }
            slot
        })
        .collect()
}

#[test]
fn presentation_resolution_matches_previous_slot_order_and_exact_rng_state() {
    let mut previous = crate::skirmish::tests::game();
    for count in [2, 4, 8] {
        previous.layout.starts = STARTS[..count]
            .iter()
            .copied()
            .map(|position| PlayerStart {
                position,
                rally: None,
            })
            .collect();
        for seed in [0, 1, 6, 7, 31, u32::MAX] {
            for pattern in 0..4 {
                let slots = slots(count, pattern);
                let resolved = resolve_player_slots(&slots, seed, 8, &STARTS[..count]).unwrap();
                previous.layout.random_seed = seed;
                previous.slots = slots;
                previous.random = startup::seed_random(seed);
                let factions = previous.previous_assign_factions_and_colors();
                let mut taken = vec![false; count];
                for slot in previous.slots.iter().filter(|slot| slot.occupied) {
                    if let Some(start) = slot.start {
                        taken[start] = true;
                    }
                }
                previous.previous_assign_start_positions(taken);
                assert!(
                    resolved.factions == factions,
                    "factions count={count} seed={seed} pattern={pattern}"
                );
                assert_eq!(
                    resolved.colors, previous.player_colors,
                    "colors count={count} seed={seed} pattern={pattern}"
                );
                assert_eq!(
                    resolved.start_positions, previous.start_positions,
                    "starts count={count} seed={seed} pattern={pattern}"
                );
                assert_eq!(
                    resolved.random, previous.random,
                    "RNG count={count} seed={seed} pattern={pattern}"
                );
                assert_eq!(resolved.factions.len(), count);
                assert_eq!(resolved.colors.len(), count);
                assert_eq!(resolved.start_positions.len(), count + 1);
                assert_eq!(resolved.start_positions[count], None);
            }
        }
    }
}

#[test]
fn presentation_resolution_preserves_validation_order_before_any_selection() {
    let mut slots = slots(4, 0);
    slots[0].faction = Some(Faction::Civilian);
    assert_eq!(
        resolve_player_slots(&slots, 31, 8, &STARTS[..2]).err(),
        Some("Player slots do not match map capacity")
    );
    assert_eq!(
        resolve_player_slots(&slots, 31, 8, &STARTS[..4]).err(),
        Some("Invalid playable slot selection")
    );
    slots[0].faction = None;
    slots[0].start = Some(1);
    slots[1].start = Some(1);
    slots[2].color = Some(8);
    assert_eq!(
        resolve_player_slots(&slots, 31, 8, &STARTS[..4]).err(),
        Some("Player starts overlap or exceed map capacity")
    );
    slots[1].start = None;
    slots[2].occupied = false;
    assert_eq!(
        resolve_player_slots(&slots, 31, 8, &STARTS[..4]).err(),
        Some("Invalid playable slot selection")
    );
}
