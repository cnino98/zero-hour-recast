//! Shared source slot resolution before map-world construction.
use super::{Faction, PlayerSlot};
use crate::skirmish::{movement::distance_squared, startup};

#[cfg(test)]
mod tests;

pub struct ResolvedSlots {
    pub factions: Vec<Faction>,
    pub colors: Vec<Option<usize>>,
    pub start_positions: Vec<Option<usize>>,
    pub random: [u32; 6],
}

/// Resolve source side/color/start choices using the admitted player palette.
/// The input choices are retained independently for restart; no world is loaded.
/// # Errors
/// Rejects mismatched slots, invalid playable selections and overlapping starts.
pub fn resolve_player_slots(
    slots: &[PlayerSlot],
    seed: u32,
    color_count: usize,
    starts: &[[f32; 2]],
) -> Result<ResolvedSlots, &'static str> {
    if slots.len() != starts.len() || slots.first().is_none_or(|slot| !slot.occupied) {
        return Err("Player slots do not match map capacity");
    }
    let mut taken = vec![false; slots.len()];
    for slot in slots {
        if slot.faction == Some(Faction::Civilian)
            || slot.team.is_some_and(|team| team > 3)
            || slot.color.is_some_and(|color| color >= color_count)
        {
            return Err("Invalid playable slot selection");
        }
        if !slot.occupied {
            continue;
        }
        if let Some(start) = slot.start {
            if start >= taken.len() || taken[start] {
                return Err("Player starts overlap or exceed map capacity");
            }
            taken[start] = true;
        }
    }

    let mut random = startup::seed_random(seed);
    let (factions, colors) = resolve_factions_and_colors(slots, seed, color_count, &mut random);
    let start_positions = resolve_start_positions(slots, starts, &mut random, taken);
    Ok(ResolvedSlots {
        factions,
        colors,
        start_positions,
        random,
    })
}

fn resolve_factions_and_colors(
    slots: &[PlayerSlot],
    seed: u32,
    color_count: usize,
    random: &mut [u32; 6],
) -> (Vec<Faction>, Vec<Option<usize>>) {
    let mut colors: Vec<_> = slots.iter().map(|slot| slot.color).collect();
    // checkForDuplicateColors: reverse slot order preserves the first owner.
    for owner in (0..slots.len()).rev() {
        if !slots[owner].occupied {
            continue;
        }
        if let Some(color) = colors[owner] {
            colors[owner] = None;
            if !colors.contains(&Some(color)) {
                colors[owner] = Some(color);
            }
        }
    }
    // populateRandomSideAndColor interleaves side and color resolution per
    // occupied slot. The discarded draws use the ORIGINAL base seed.
    let color_maximum = u32::try_from(color_count - 1).expect("validated source player palette");
    let mut factions = Vec::with_capacity(slots.len());
    for owner in 0..slots.len() {
        let selected = slots[owner].faction;
        if !slots[owner].occupied {
            factions.push(selected.unwrap_or(Faction::Civilian));
            continue;
        }
        let faction = selected.unwrap_or_else(|| {
            for _ in 0..seed % 7 {
                startup::random_between(random, 1);
            }
            let index = usize::try_from(startup::random_between(random, 1000) % 3)
                .expect("three original base factions");
            [Faction::Usa, Faction::China, Faction::Gla][index]
        });
        factions.push(faction);
        if colors[owner].is_some() {
            continue;
        }
        loop {
            let color = usize::try_from(startup::random_between(random, color_maximum))
                .expect("source color index");
            if !colors.contains(&Some(color)) {
                colors[owner] = Some(color);
                break;
            }
        }
    }
    (factions, colors)
}

fn resolve_start_positions(
    slots: &[PlayerSlot],
    starts: &[[f32; 2]],
    random: &mut [u32; 6],
    mut taken: Vec<bool>,
) -> Vec<Option<usize>> {
    let mut start_positions: Vec<_> = slots
        .iter()
        .map(|slot| slot.start.filter(|_| slot.occupied))
        .chain([None])
        .collect();
    let mut any_picked = taken.iter().any(|&value| value);
    let mut team_position: [Option<usize>; 4] = [None; 4];
    for owner in 0..slots.len() {
        let slot = &slots[owner];
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
                let distance = teammate.map_or_else(
                    || {
                        taken
                            .iter()
                            .enumerate()
                            .filter(|(_, occupied)| **occupied)
                            .map(|(other, _)| {
                                distance_squared(starts[position], starts[other]).sqrt()
                            })
                            .sum()
                    },
                    |teammate| distance_squared(starts[position], starts[teammate]).sqrt(),
                );
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
                let candidate =
                    usize::try_from(startup::random_between(random, maximum)).unwrap_or(0);
                if !taken[candidate] {
                    break candidate;
                }
            }
        };
        start_positions[owner] = Some(chosen);
        taken[chosen] = true;
        any_picked = true;
        if let Some(team) = team {
            team_position[team].get_or_insert(chosen);
        }
    }
    start_positions
}
