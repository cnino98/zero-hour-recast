use super::{Mouse, matching, picking::View};
use crate::{
    playtest::{CameraRig, Match},
    playtest_ui,
};
use bevy::prelude::*;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{error::Error, time::Instant};

pub(super) fn load_group_keys(assets: &VirtualAssets) -> Result<[KeyCode; 10], Box<dyn Error>> {
    let file = content::scan(assets.read("data/english/commandmap.ini")?)?;
    let mut keys = [KeyCode::Digit0; 10];
    for (group, key) in keys.iter_mut().enumerate() {
        for (operation, modifier) in [
            ("CREATE", "CTRL"),
            ("SELECT", "NONE"),
            ("ADD", "SHIFT"),
            ("VIEW", "ALT"),
        ] {
            let name = format!("{operation}_TEAM{group}");
            let declaration = file
                .declarations
                .iter()
                .find(|item| item.kind == "CommandMap" && item.name == name)
                .ok_or_else(|| format!("missing CommandMap {name}"))?;
            for (field, expected) in [
                ("Key", format!("KEY_{group}")),
                ("Transition", "DOWN".into()),
                ("Modifiers", modifier.into()),
            ] {
                let value = declaration
                    .lines
                    .iter()
                    .find(|line| {
                        line.depth == Some(0)
                            && line.tokens.first().is_some_and(|token| token == field)
                    })
                    .and_then(|line| line.tokens.get(1));
                if value != Some(&expected) {
                    return Err(format!("unsupported {name} {field}").into());
                }
            }
        }
        *key = [
            KeyCode::Digit0,
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ][group];
    }
    Ok(keys)
}

pub fn keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: ResMut<Mouse>,
    battle: ResMut<Match>,
    rig: ResMut<CameraRig>,
    view: View,
) {
    view.update_selection(
        keys.into_inner(),
        mouse.into_inner(),
        battle.into_inner(),
        rig.into_inner(),
    );
}

impl View<'_, '_> {
    fn update_selection(
        self,
        keys: &ButtonInput<KeyCode>,
        mouse: &mut Mouse,
        battle: &mut Match,
        rig: &mut CameraRig,
    ) {
        mouse.reset_generation(battle.generation);
        if !battle.started || battle.paused || battle.simulation.end_game_at.is_some() {
            return;
        }
        let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
        if u8::from(control) + u8::from(shift) + u8::from(alt) > 1 {
            return;
        }
        if control && keys.just_pressed(KeyCode::KeyF) {
            // CommandXlat forwards CREATE_FORMATION for the current selection.
            let selected = command_units(battle);
            if let Err(error) = battle.simulation.group_create_formation(&selected) {
                eprintln!("Formation command rejected: {error}");
            }
        }
        if !control && !shift && !alt {
            for (key, mode) in mouse.selection.keys.into_iter().zip([
                matching::Mode::Matching,
                matching::Mode::All,
                matching::Mode::Aircraft,
            ]) {
                if keys.just_pressed(key) {
                    matching::keyboard(battle, mouse, &self, &mode);
                }
            }
        }
        for group in 0..10 {
            if !keys.just_pressed(mouse.group_keys[group]) {
                continue;
            }
            mouse.groups[group].retain(|&id| {
                battle
                    .simulation
                    .entities
                    .get(id)
                    .is_some_and(|unit| unit.alive() && unit.owner == 0)
            });
            if control {
                // SelectionTranslator::onMetaCreateTeam includes locally controlled selection.
                let selected: Vec<_> = battle
                    .simulation
                    .entities
                    .iter()
                    .enumerate()
                    .rev()
                    .filter(|(id, unit)| {
                        unit.alive() && unit.owner == 0 && battle.selected.contains(id)
                    })
                    .map(|(id, _)| id)
                    .collect();
                // MSG_CREATE_TEAM changes the hotkey Squad, not current selection.
                drop(battle.command_message_members());
                assign_group(&mut mouse.groups, group, selected);
            } else if alt {
                // onMetaViewTeam targets the last live member, including team zero's fix.
                if let Some(&id) = mouse.groups[group].last() {
                    rig.focus = Vec2::from_array(battle.simulation.entities[id].position) * 0.1;
                }
            } else {
                let double = mouse.clicks.group_pressed(group, Instant::now());
                if double && let Some(&id) = mouse.groups[group].last() {
                    rig.focus = Vec2::from_array(battle.simulation.entities[id].position) * 0.1;
                }
                // onMetaAddTeam clears a selected structure or a recalled structure
                // group before adding, including the pinned SuperHackers fix.
                let clear = has_building_or_enemy(battle)
                    || mouse.groups[group].first().is_some_and(|&id| {
                        battle.simulation.catalog[battle.simulation.entities[id].type_index]
                            .kind
                            .is_building()
                    });
                let perform = !double
                    || (!shift
                        && !mouse.groups[group]
                            .iter()
                            .all(|id| battle.selected.contains(id)));
                if perform {
                    if !shift || clear {
                        battle.clear_drawable_selection();
                    }
                    // MSG_SELECT_TEAM/MSG_ADD_TEAM has one dispatcher boundary;
                    // Player replaces/appends Squad objects without temporary AI groups.
                    drop(battle.command_message_members());
                    if !shift {
                        battle.logic_selection.clear();
                    }
                    battle
                        .logic_selection
                        .extend_from_slice(&mouse.groups[group]);
                }
                if recall(
                    &mut battle.selected,
                    &mouse.groups[group],
                    shift,
                    double,
                    clear,
                ) {
                    mouse.selection_voice(battle);
                }
            }
        }
    }
}

fn recall(
    selected: &mut Vec<usize>,
    group: &[usize],
    extend: bool,
    double: bool,
    clear: bool,
) -> bool {
    // The pinned SelectTeam fix reselects missing members on double-tap, but
    // preserves other selected units if all group members are already selected.
    // AddTeam's double-tap only moves the camera, without changing membership.
    if double && (extend || group.iter().all(|id| selected.contains(id))) {
        return false;
    }
    if !extend || clear {
        selected.clear();
    }
    for &id in group {
        if !selected.contains(&id) {
            selected.push(id);
        }
    }
    !extend
}

fn assign_group(groups: &mut [Vec<usize>; 10], group: usize, selected: Vec<usize>) {
    // Player::processCreateTeamGameMessage permits one hotkey squad per object.
    for members in &mut *groups {
        members.retain(|id| !selected.contains(id));
    }
    groups[group] = selected;
}

impl Mouse {
    pub fn apply_replacements(&mut self, battle: &mut Match) {
        let events: Vec<_> = battle
            .simulation
            .presentation
            .replacement_events
            .iter()
            .filter(|event| event.frame == battle.simulation.tick)
            .map(|event| (event.previous, event.next))
            .collect();
        for (previous, next) in events {
            let selected = battle.selected.first() == Some(&previous);
            let local = battle.simulation.entities[next].owner == 0;
            let hotkey = local && self.groups.iter().any(|group| group.contains(&previous));
            if selected {
                battle.selection_create(&[next], true);
            }
            if hotkey {
                drop(battle.command_message_members());
            }
            replacement_selection(
                &mut battle.selected,
                &mut self.groups,
                previous,
                next,
                local,
            );
        }
    }
}

fn replacement_selection(
    selected: &mut Vec<usize>,
    groups: &mut [Vec<usize>; 10],
    previous: usize,
    next: usize,
    local: bool,
) {
    // ReplaceObjectUpgrade sends create-new selected group only for the first
    // selected drawable. Its one-object CREATE_TEAM message replaces that squad.
    if selected.first() == Some(&previous) {
        selected.retain(|&id| id != previous);
        if !selected.contains(&next) {
            selected.insert(0, next);
        }
    } else {
        selected.retain(|&id| id != previous);
    }
    let group = groups
        .iter()
        .position(|members| members.contains(&previous));
    for members in &mut *groups {
        members.retain(|&id| id != previous);
    }
    if local && let Some(group) = group {
        assign_group(groups, group, vec![next]);
    }
}

pub(super) fn region(
    battle: &mut Match,
    area: Rect,
    extend: bool,
    view: &View,
    mouse: &mut Mouse,
) -> bool {
    // W3DView::iterateDrawablesInRegion projects drawable origins, not mesh bounds.
    let mut selected: Vec<_> = battle
        .simulation
        .entities
        .iter()
        .enumerate()
        .rev()
        .filter(|(id, unit)| {
            let stats = &battle.simulation.catalog[unit.type_index];
            unit.selectable(stats)
                && !unit.masked()
                && !unit.unselectable()
                && !unit.drawable_effectively_hidden()
                && (stats.selectable || !stats.kind_of.iter().any(|kind| kind == "FORCEATTACKABLE"))
                && battle.simulation.logically_visible(0, *id)
                && matches!(
                    battle.simulation.visibility_status(0, *id),
                    generals_sim::skirmish::visibility::Status::Clear
                        | generals_sim::skirmish::visibility::Status::Partial
                )
        })
        .filter(|(id, _)| {
            view.project(battle, *id).is_some_and(|center| {
                area.contains(center)
                    && view
                        .camera
                        .0
                        .logical_viewport_rect()
                        .is_some_and(|rect| rect.contains(center))
                    && !playtest_ui::captures_pointer(
                        battle,
                        &view.window,
                        center,
                        &view.ui,
                        &view.promotions,
                    )
            })
        })
        .map(|(id, _)| id)
        .collect();
    let ours = selected
        .iter()
        .filter(|&&id| battle.simulation.entities[id].owner == 0)
        .count();
    if ours > 0 {
        selected.retain(|&id| {
            let unit = &battle.simulation.entities[id];
            unit.owner == 0
                && (ours == 1
                    || !battle.simulation.catalog[unit.type_index]
                        .kind
                        .is_building())
        });
    } else if selected.len() != 1 {
        return false;
    }
    mouse.clicks.manual_selection();
    if selected.is_empty() {
        let replace = !extend || has_building_or_enemy(battle);
        // SelectionXlat still creates a new empty group when a rectangle contains
        // multiple owned structures; Shift preserves an existing mobile group.
        if !extend || has_building_or_enemy(battle) {
            battle.clear_drawable_selection();
        }
        battle.selection_create(&[], replace);
        return false;
    }
    let first = &battle.simulation.entities[selected[0]];
    let replace = !extend
        || first.owner != 0
        || battle.simulation.catalog[first.type_index]
            .kind
            .is_building()
        || has_building_or_enemy(battle);
    battle.select_objects(&selected, replace);
    true
}

pub(super) fn command_units(battle: &mut Match) -> Vec<usize> {
    battle.command_message_members()
}

pub(super) fn has_building_or_enemy(battle: &Match) -> bool {
    battle.selected.iter().any(|&id| {
        battle.simulation.entities.get(id).is_some_and(|unit| {
            unit.owner != 0
                || battle.simulation.catalog[unit.type_index]
                    .kind
                    .is_building()
        })
    })
}

impl Match {
    /// `MSG_CREATE_SELECTED_GROUP[_NO_SOUND]`. Preserve every ordered argument,
    /// including repeats, independently of the GUI's drawable list.
    pub(crate) fn selection_create(&mut self, added: &[usize], replace: bool) {
        self.logic_selection =
            self.simulation
                .create_selection(0, &self.logic_selection, added, replace);
    }
    /// `MSG_REMOVE_FROM_SELECTED_GROUP`, not a per-frame final-vector comparison.
    pub(crate) fn selection_remove(&mut self, removed: &[usize]) {
        self.logic_selection = self
            .simulation
            .remove_selection(0, &self.logic_selection, removed);
    }
    /// `MSG_DESTROY_SELECTED_GROUP`, including already queued client messages.
    pub(crate) fn selection_destroy(&mut self) {
        self.simulation.destroy_selection(0, &self.logic_selection);
        self.logic_selection.clear();
    }
    /// `InGameUI::deselectAllDrawables` emits Destroy only if drawables were selected.
    pub(crate) fn clear_drawable_selection(&mut self) {
        if !self.selected.is_empty() {
            self.selection_destroy();
        }
        self.selected.clear();
    }
    /// Ordinary GUI/direct-controller selection message path. Scripted acceptance
    /// uses this same event boundary, never an implicit group at each unit command.
    pub(crate) fn select_objects(&mut self, added: &[usize], replace: bool) {
        if replace {
            self.clear_drawable_selection();
        }
        self.selection_create(added, replace);
        for &id in added {
            if self.logic_selection.contains(&id) && !self.selected.contains(&id) {
                self.selected.push(id);
            }
        }
    }
    /// One admitted network/logic message, before any per-member operation.
    /// The returned `AIGroup` order is the native last-occurrence order, including
    /// source live/add/foreign-owner filtering, rather than GUI order.
    pub(crate) fn command_message_members(&mut self) -> Vec<usize> {
        self.simulation
            .materialize_selection_group(0, &self.logic_selection)
    }
}

#[cfg(test)]
mod tests {
    use super::{assign_group, recall, replacement_selection};

    #[ignore = "requires original game installation"]
    #[test]
    fn mob_child_selection_emits_missing_slaves_then_nexus_before_group_move() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let setup = crate::angry_mob_verification::prepare(&mut battle.simulation).unwrap();
        let children = battle.simulation.spawned_children(setup.nexus).to_vec();
        let picked = children[3];
        battle.select_objects(&[picked], true);
        let selected = battle.selected.clone();
        battle.simulation.sync_drawable_selection(&selected);
        battle.simulation.step();
        crate::input::apply_spectre_selections(&mut battle);
        let mut expected = vec![picked];
        expected.extend(children.iter().copied().filter(|&id| id != picked));
        expected.push(setup.nexus);
        assert_eq!(battle.logic_selection, expected);
        assert_eq!(battle.selected.first(), Some(&setup.nexus));
        assert_eq!(super::command_units(&mut battle), expected);
        let unchanged = battle.logic_selection.clone();
        let selected = battle.selected.clone();
        battle.simulation.sync_drawable_selection(&selected);
        battle.simulation.step();
        crate::input::apply_spectre_selections(&mut battle);
        assert_eq!(
            battle.logic_selection, unchanged,
            "complete selection emits no repeat arguments"
        );
        let destination =
            bevy::prelude::Vec3::from_array(battle.simulation.object_position(setup.nexus))
                + bevy::prelude::Vec3::new(80.0, 0.0, 0.0);
        assert!(crate::input::move_group(&mut battle, destination, false));
        assert!(matches!(
            battle.simulation.entities[setup.nexus].ai.order,
            generals_sim::skirmish::Order::Move(_)
        ));
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_replacement_event_moves_the_logic_squad_to_the_new_object() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let setup = crate::fake_building_verification::prepare(&mut battle.simulation).unwrap();
        let old = setup.fakes[0];
        let fake = battle.simulation.entities[setup.fakes[1]].type_index;
        let real = battle
            .simulation
            .type_for(0, generals_sim::skirmish::Kind::SupplyCenter)
            .unwrap();
        let funds = battle.simulation.catalog[fake]
            .cost
            .max(battle.simulation.catalog[real].cost);
        crate::skirmish_verification::advance_until(
            &mut battle.simulation,
            18_000,
            "ordinary supply income for live-commandset admission checks",
            |game| game.players[0].cash >= funds,
        )
        .unwrap();
        assert!(
            battle
                .simulation
                .construction_admission(setup.worker, fake)
                .is_ok()
        );
        crate::fake_building_verification::switch(&mut battle.simulation, setup.worker, false)
            .unwrap();
        assert!(
            battle
                .simulation
                .construction_admission(setup.worker, fake)
                .is_err()
        );
        assert!(
            battle
                .simulation
                .construction_admission(setup.worker, real)
                .is_ok()
        );
        crate::fake_building_verification::switch(&mut battle.simulation, setup.worker, true)
            .unwrap();
        assert!(
            battle
                .simulation
                .construction_admission(setup.worker, fake)
                .is_ok()
        );
        battle.select_objects(&[old], true);
        let root = &generals_assets::game_root().unwrap();
        let mut mouse = crate::input::load(
            &assets,
            &root.join("UserData/Options.ini"),
            &battle.simulation.catalog,
        )
        .unwrap();
        mouse.groups[3] = vec![old];
        let next = crate::fake_building_verification::convert(&mut battle.simulation, old).unwrap();
        mouse.apply_replacements(&mut battle);
        assert_eq!(battle.selected, [next]);
        assert_eq!(battle.logic_selection, [next]);
        assert_eq!(mouse.groups[3], [next]);
        assert_eq!(battle.command_message_members(), [next]);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn repeated_selection_arguments_reorder_command_members_without_duplicate_drawables() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        battle
            .simulation
            .reset(
                generals_sim::skirmish::Faction::Usa,
                generals_sim::skirmish::Faction::Gla,
            )
            .unwrap();
        let a = battle
            .simulation
            .entities
            .iter()
            .position(|actor| {
                actor.owner == 0
                    && battle.simulation.catalog[actor.type_index].kind
                        == generals_sim::skirmish::Kind::Builder
            })
            .unwrap();
        let producer = battle
            .simulation
            .entities
            .iter()
            .position(|actor| {
                actor.owner == 0
                    && battle.simulation.catalog[actor.type_index].kind
                        == generals_sim::skirmish::Kind::CommandCenter
            })
            .unwrap();
        let b = crate::skirmish_verification::produce(
            &mut battle.simulation,
            producer,
            generals_sim::skirmish::Kind::Builder,
            0,
        )
        .unwrap();
        battle.select_objects(&[a, b], true);
        battle.select_objects(&[a], false);
        assert_eq!(battle.selected, [a, b]);
        assert_eq!(battle.logic_selection, [a, b, a]);
        assert_eq!(battle.command_message_members(), [b, a]);
        battle.selection_remove(&[a]);
        battle.selected.retain(|&id| id != a);
        assert_eq!(battle.logic_selection, [b]);
        assert_eq!(battle.command_message_members(), [b]);
        battle.clear_drawable_selection();
        assert!(battle.logic_selection.is_empty());
        assert!(battle.selected.is_empty());
    }

    #[test]
    fn replacement_preserves_source_first_selection_and_replaces_whole_hotkey_squad() {
        let mut selected = vec![12, 7];
        let mut groups = std::array::from_fn(|_| Vec::new());
        groups[3] = vec![7, 12, 9];
        groups[4] = vec![15];
        replacement_selection(&mut selected, &mut groups, 12, 24, true);
        assert_eq!(selected, [24, 7]);
        assert_eq!(groups[3], [24]);
        assert_eq!(groups[4], [15]);

        selected = vec![7, 24];
        replacement_selection(&mut selected, &mut groups, 24, 30, true);
        assert_eq!(selected, [7]);
        assert_eq!(groups[3], [30]);
        replacement_selection(&mut selected, &mut groups, 30, 35, false);
        assert!(groups[3].is_empty());
        assert_eq!(selected, [7]);
    }

    #[test]
    fn double_tap_reselects_missing_group_members_but_preserves_complete_selection() {
        let mut selected = vec![2];
        assert!(recall(&mut selected, &[2, 3], false, true, false));
        assert_eq!(selected, [2, 3]);
        selected.push(4);
        assert!(!recall(&mut selected, &[2, 3], false, true, false));
        assert_eq!(selected, [2, 3, 4]);
        assert!(!recall(&mut selected, &[7], true, true, true));
        assert_eq!(selected, [2, 3, 4]);
        assert!(!recall(&mut selected, &[7], true, false, true));
        assert_eq!(selected, [7]);
    }

    #[test]
    fn assigning_hotkey_group_moves_members_out_of_previous_groups() {
        let mut groups: [Vec<usize>; 10] = std::array::from_fn(|_| Vec::new());
        groups[1] = vec![5, 3];
        groups[2] = vec![7];
        assign_group(&mut groups, 2, vec![5, 9]);
        assert_eq!(groups[1], [3]);
        assert_eq!(groups[2], [5, 9]);
        assign_group(&mut groups, 2, Vec::new());
        assert!(groups[2].is_empty());
        assert_eq!(groups[1], [3]);
    }
}
