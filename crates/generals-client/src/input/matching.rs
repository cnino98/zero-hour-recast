//! `InGameUI` mass-selection callbacks and screen-to-map escalation.
use super::{Mouse, picking::View, selection_data::SelectionData};
use crate::playtest::Match;

struct Candidate {
    id: usize,
    template: usize,
    owner: usize,
    mass: bool,
    screen: bool,
}

pub(super) enum Mode {
    Matching,
    All,
    Aircraft,
}

fn candidates(battle: &Match, view: &View) -> Vec<Candidate> {
    battle
        .simulation
        .entities
        .iter()
        .enumerate()
        .rev()
        .map(|(id, unit)| {
            let stats = &battle.simulation.catalog[unit.type_index];
            Candidate {
                id,
                template: unit.type_index,
                owner: unit.owner,
                mass: unit.selectable(stats)
                    && unit.contained_by().is_none()
                    && !stats.kind.is_building(),
                screen: view.project(battle, id).is_some_and(|point| {
                    view.camera
                        .0
                        .logical_viewport_rect()
                        .is_some_and(|rect| rect.contains(point))
                }),
            }
        })
        .collect()
}

pub(super) fn keyboard(battle: &mut Match, mouse: &mut Mouse, view: &View, mode: &Mode) {
    let candidates = candidates(battle, view);
    let clear = !matches!(mode, Mode::Matching)
        && candidates.iter().any(|unit| {
            battle.selected.contains(&unit.id)
                && (!unit.mass || !kind_matches(&mouse.selection.types[unit.template], mode))
        });
    if clear {
        battle.clear_drawable_selection();
    }
    let replace = !matches!(mode, Mode::Matching) && battle.selected.is_empty();
    let added = select(&mut battle.selected, &candidates, &mouse.selection, mode);
    if !added.is_empty() {
        battle.selection_create(&added, replace);
    }
    // Matching messages use MSG_CREATE_SELECTED_GROUP_NO_SOUND; select-all does not.
    if !added.is_empty() && !matches!(mode, Mode::Matching) {
        mouse.selection_voice(battle);
    }
}

pub(super) fn double_click(
    battle: &mut Match,
    mouse: &Mouse,
    view: &View,
    picked: Option<usize>,
    extend: bool,
    across_map: bool,
) -> bool {
    let candidates = candidates(battle, view);
    let Some(messages) = double_selection(
        &mut battle.selected,
        &candidates,
        &mouse.selection,
        picked,
        extend,
        across_map,
    ) else {
        return false;
    };
    if messages.destroy {
        battle.selection_destroy();
    }
    battle.selection_create(&[messages.picked], true);
    if !messages.matching.is_empty() {
        battle.selection_create(&messages.matching, false);
    }
    // END_AREA_SELECTION_HINT is non-network and does not materialize a group.
    if messages.restore_message {
        battle.selection_create(&messages.restored, false);
    }
    true
}

struct DoubleSelection {
    destroy: bool,
    picked: usize,
    matching: Vec<usize>,
    restored: Vec<usize>,
    restore_message: bool,
}

fn double_selection(
    selected: &mut Vec<usize>,
    candidates: &[Candidate],
    data: &SelectionData,
    picked: Option<usize>,
    extend: bool,
    across_map: bool,
) -> Option<DoubleSelection> {
    let picked = picked.and_then(|id| {
        candidates
            .iter()
            .find(|unit| unit.id == id && unit.owner == 0 && unit.mass)
    })?;
    let destroy = !selected.is_empty();
    let previous = if extend { selected.clone() } else { Vec::new() };
    selected.clear();
    selected.push(picked.id);
    let matching = append(
        selected,
        candidates,
        data,
        &[picked.template],
        &Mode::Matching,
        !across_map,
    );
    // Shift saves the old selection, selects the clicked template, then restores
    // only mass-selectable old drawables (not buildings), without another voice.
    let restore_message = extend && !previous.is_empty();
    let mut restored = Vec::new();
    for id in previous {
        if candidates.iter().any(|unit| unit.id == id && unit.mass) {
            restored.push(id);
            if !selected.contains(&id) {
                selected.insert(0, id);
            }
        }
    }
    Some(DoubleSelection {
        destroy,
        picked: picked.id,
        matching,
        restored,
        restore_message,
    })
}

fn select(
    selected: &mut Vec<usize>,
    candidates: &[Candidate],
    data: &SelectionData,
    mode: &Mode,
) -> Vec<usize> {
    let templates: Vec<_> = candidates
        .iter()
        .filter(|unit| unit.owner == 0 && selected.contains(&unit.id))
        .map(|unit| unit.template)
        .collect();
    if matches!(mode, Mode::Matching) && templates.is_empty() {
        return Vec::new();
    }
    // CommandXlat clears incompatible selections before selecting all. Matching
    // keeps its selected seeds, even if they are outside the current screen.
    if !matches!(mode, Mode::Matching)
        && candidates.iter().any(|unit| {
            selected.contains(&unit.id)
                && (!unit.mass || !kind_matches(&data.types[unit.template], mode))
        })
    {
        selected.clear();
    }
    let added = append(selected, candidates, data, &templates, mode, true);
    if added.is_empty() {
        append(selected, candidates, data, &templates, mode, false)
    } else {
        added
    }
}

const fn kind_matches(data: &super::selection_data::TypeSelection, mode: &Mode) -> bool {
    !data.excluded && (!matches!(mode, Mode::Aircraft) || data.aircraft)
}

fn append(
    selected: &mut Vec<usize>,
    candidates: &[Candidate],
    data: &SelectionData,
    templates: &[usize],
    mode: &Mode,
    screen: bool,
) -> Vec<usize> {
    let mut added = Vec::new();
    for unit in candidates {
        if !unit.mass || unit.owner != 0 || (screen && !unit.screen) || selected.contains(&unit.id)
        {
            continue;
        }
        let stats = &data.types[unit.template];
        let matches = if matches!(mode, Mode::Matching) {
            templates
                .iter()
                .any(|template| stats.equivalent.contains(template))
        } else {
            kind_matches(stats, mode)
        };
        if matches && (data.maximum == 0 || selected.len() < data.maximum) {
            // InGameUI::selectDrawable prepends newly selected drawables.
            selected.insert(0, unit.id);
            added.push(unit.id);
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::{Candidate, Mode, double_selection, select};
    use crate::input::selection_data::{SelectionData, TypeSelection};
    use bevy::prelude::KeyCode;

    fn data() -> SelectionData {
        SelectionData {
            keys: [KeyCode::KeyE, KeyCode::KeyQ, KeyCode::KeyW],
            maximum: 0,
            types: vec![
                TypeSelection {
                    excluded: false,
                    aircraft: false,
                    equivalent: vec![0, 4],
                },
                TypeSelection {
                    excluded: true,
                    aircraft: false,
                    equivalent: vec![1],
                },
                TypeSelection {
                    excluded: false,
                    aircraft: true,
                    equivalent: vec![2],
                },
                TypeSelection {
                    excluded: false,
                    aircraft: false,
                    equivalent: vec![3],
                },
                TypeSelection {
                    excluded: false,
                    aircraft: false,
                    equivalent: vec![0, 4],
                },
            ],
        }
    }

    fn units() -> Vec<Candidate> {
        vec![
            Candidate {
                id: 0,
                template: 0,
                owner: 0,
                mass: true,
                screen: true,
            },
            Candidate {
                id: 1,
                template: 4,
                owner: 0,
                mass: true,
                screen: true,
            },
            Candidate {
                id: 2,
                template: 0,
                owner: 0,
                mass: true,
                screen: false,
            },
            Candidate {
                id: 3,
                template: 0,
                owner: 1,
                mass: true,
                screen: true,
            },
            Candidate {
                id: 4,
                template: 1,
                owner: 0,
                mass: true,
                screen: true,
            },
            Candidate {
                id: 5,
                template: 2,
                owner: 0,
                mass: true,
                screen: false,
            },
            Candidate {
                id: 6,
                template: 3,
                owner: 0,
                mass: false,
                screen: true,
            },
        ]
    }

    #[test]
    fn double_click_restores_already_selected_arguments_even_when_drawables_do_not_change() {
        let mut selected = vec![1, 0];
        let messages =
            double_selection(&mut selected, &units(), &data(), Some(0), true, false).unwrap();
        assert_eq!(selected, [1, 0]);
        assert!(messages.destroy);
        assert_eq!(messages.picked, 0);
        assert_eq!(messages.matching, [1]);
        assert!(messages.restore_message);
        assert_eq!(messages.restored, [1, 0]);
    }

    #[test]
    fn matching_message_arguments_keep_candidate_order_while_gui_prepends() {
        let mut selected = vec![0];
        let mut candidates = units();
        for unit in &mut candidates {
            unit.screen = true;
        }
        let added = select(&mut selected, &candidates, &data(), &Mode::All);
        assert_eq!(added, [1, 2, 5]);
        assert_eq!(selected, [5, 2, 1, 0]);
    }

    #[test]
    fn matching_expands_screen_then_map_without_enemy_or_empty_selection_fallback() {
        let mut selected = vec![0];
        assert_eq!(
            select(&mut selected, &units(), &data(), &Mode::Matching).len(),
            1
        );
        assert_eq!(selected, [1, 0]);
        assert_eq!(
            select(&mut selected, &units(), &data(), &Mode::Matching).len(),
            1
        );
        assert_eq!(selected, [2, 1, 0]);
        for selected in [&mut Vec::new(), &mut vec![3]] {
            assert_eq!(
                select(selected, &units(), &data(), &Mode::Matching).len(),
                0
            );
        }
    }

    #[test]
    fn empty_screen_expands_to_map_and_select_all_excludes_workers_and_buildings() {
        let mut units = units();
        for unit in &mut units {
            unit.screen = false;
        }
        let mut selected = vec![4, 6];
        assert_eq!(select(&mut selected, &units, &data(), &Mode::All).len(), 4);
        assert_eq!(selected, [5, 2, 1, 0]);
        assert_eq!(
            select(&mut selected, &units, &data(), &Mode::Aircraft).len(),
            1
        );
        assert_eq!(selected, [5]);
    }

    #[test]
    fn double_click_scope_is_explicit_and_shift_restores_only_mobile_selection() {
        let mut selected = vec![4, 6];
        assert!(double_selection(&mut selected, &units(), &data(), Some(0), true, false).is_some());
        assert_eq!(selected, [4, 1, 0]);
        assert!(
            double_selection(&mut selected, &units(), &data(), Some(0), false, false).is_some()
        );
        assert_eq!(selected, [1, 0]);
        assert!(double_selection(&mut selected, &units(), &data(), Some(0), false, true).is_some());
        assert_eq!(selected, [2, 1, 0]);
        assert!(double_selection(&mut selected, &units(), &data(), Some(3), false, true).is_none());
        assert!(double_selection(&mut selected, &units(), &data(), Some(6), false, true).is_none());
        assert_eq!(selected, [2, 1, 0]);
    }

    #[test]
    fn source_selection_cap_limits_matching_but_shift_restores_previous_members() {
        let mut rules = data();
        rules.maximum = 2;
        let mut selected = vec![4];
        assert!(double_selection(&mut selected, &units(), &rules, Some(0), true, true).is_some());
        assert_eq!(selected, [4, 1, 0]);
    }
}
