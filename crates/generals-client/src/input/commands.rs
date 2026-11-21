//! `GameLogic` command-list admission between scripts and object updates.
use super::{CommandMode, Mouse};
use crate::{control_bar::CommandAction, playtest::Match};
use bevy::prelude::{Vec2, Vec3};
use generals_sim::skirmish::PowerTarget;

#[derive(Default)]
pub struct Queue {
    pending: Vec<Message>,
    pub(super) members: Vec<usize>,
    pub(crate) drawables: Vec<usize>,
    pub(crate) executing: bool,
}

struct Message {
    generation: u32,
    owner: usize,
    selected: Vec<(usize, usize)>,
    waypoint: bool,
    force: bool,
    kind: Kind,
}

pub enum Kind {
    Action(CommandAction),
    Targeted {
        mode: CommandMode,
        point: Vec3,
        target: Option<usize>,
    },
    Power {
        caster: usize,
        name: String,
        target: PowerTarget,
        specific: bool,
    },
    Force {
        target: Option<usize>,
        point: Option<Vec3>,
    },
    Move {
        point: Vec3,
        waypoints: bool,
    },
    Guard(Vec3),
    Rally {
        object: usize,
        point: Vec3,
    },
    Attack(usize),
    Override(Vec3),
    Context {
        mode: CommandMode,
        target: usize,
    },
    Board(usize),
    ExitContainer(usize),
    ActorContext(ActorOrder),
    Build {
        unit: usize,
        point: Vec2,
        angle: f32,
    },
    Research(String),
    CancelConstruction,
    CancelUpgrade(String),
    CancelProduction(i32),
    Science(String),
    Formation,
    Select {
        added: Vec<usize>,
        replace: bool,
    },
    Recall {
        members: Vec<usize>,
        replace: bool,
    },
    Deselect(Vec<usize>),
    DestroySelection,
    SelectionBoundary,
}

pub enum ActorOrder {
    Repair(usize),
    Attack(usize),
    AssistBuild(usize),
    Deliver(usize),
    Harvest(usize),
    Rally(Vec3),
}

impl Queue {
    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.members.clear();
        self.drawables.clear();
    }
}

pub fn enqueue(battle: &mut Match, mouse: Option<&Mouse>, kind: Kind) {
    let snapshot = |ids: &[usize]| {
        ids.iter()
            .filter_map(|&id| {
                battle
                    .simulation
                    .entities
                    .get(id)
                    .map(|actor| (id, actor.owner))
            })
            .collect()
    };
    let message = Message {
        generation: battle.generation,
        // The implemented local CommandXlat controls player zero; observer input is gated by the UI.
        owner: 0,
        selected: snapshot(&battle.selected),
        waypoint: mouse.is_some_and(|mouse| mouse.waypoint_mode),
        force: mouse.is_some_and(Mouse::force_attack_mode),
        kind,
    };
    battle.commands.pending.push(message);
}

pub(super) fn target_admitted(
    battle: &mut Match,
    mode: &CommandMode,
    point: Vec3,
    target: Option<usize>,
) -> bool {
    match *mode {
        CommandMode::Hijack => {
            return target.is_some_and(|target| super::hijacker::can_target(battle, target));
        }
        CommandMode::Sabotage => {
            return target.is_some_and(|target| super::sabotage::can_target(battle, target));
        }
        CommandMode::Capture => {
            return target.is_some_and(|target| super::capture::can_target(battle, target));
        }
        CommandMode::Burton(command) => {
            return target.is_some_and(|target| super::burton::can_target(battle, command, target));
        }
        CommandMode::BombTruckDisguise => {
            return target.is_some_and(|target| super::bomb_truck::can_target(battle, target));
        }
        CommandMode::SnipeVehicle(slot) => {
            return target.is_some_and(|target| super::pilot::can_target(battle, target, slot));
        }
        CommandMode::CarBomb => {
            return target.is_some_and(|target| super::car_bomb::can_target(battle, target, false));
        }
        _ => {}
    }
    let on_map = battle
        .simulation
        .command_point_on_map(point.truncate().to_array());
    for index in 0..battle.selected.len() {
        let id = battle.selected[index];
        if !super::locally_controlled(battle, id) {
            continue;
        }
        let game = &mut battle.simulation;
        let accepted = match *mode {
            CommandMode::Cleanup => on_map && game.cleanup_ready(id),
            CommandMode::FireWeapon(slot) => on_map && game.location_weapon_ready(id, slot),
            CommandMode::ClearMines(slot) => on_map && game.mine_clearing_ready(id, slot),
            CommandMode::HelixBomb => on_map && game.helix_bomb_ready(id),
            CommandMode::CombatDrop => {
                point.is_finite()
                    && game.combat_drop_ready(id)
                    && target.is_none_or(|target| game.can_combat_drop_into(id, target))
            }
            CommandMode::Hack(ability) => {
                target.is_some_and(|target| game.can_hack(id, target, ability))
            }
            CommandMode::DisableBuilding => target.is_some_and(|target| {
                game.visible_to(0, target) && game.can_disable_building(id, target)
            }),
            CommandMode::LaserLock => target.is_some_and(|target| {
                game.visible_to(0, target) && game.can_laser_lock(id, target)
            }),
            CommandMode::AttackMove
            | CommandMode::Guard
            | CommandMode::GuardFlying
            | CommandMode::Rally => point.is_finite() && game.entities[id].complete(),
            _ => false,
        };
        if accepted {
            return true;
        }
    }
    false
}

pub(super) fn issue(battle: &mut Match, mouse: &mut Mouse, kind: Kind) {
    if battle.commands.executing {
        execute(battle, mouse, 0, kind);
    } else {
        enqueue(battle, Some(mouse), kind);
    }
}

/// The ordinary client and installed controllers share this command boundary.
pub fn step(battle: &mut Match, mouse: &mut Mouse) {
    if !battle.started || battle.paused {
        return;
    }
    battle.simulation.sync_drawable_selection(&battle.selected);
    if !battle.simulation.begin_step() {
        battle.commands.clear();
        return;
    }
    drain(battle, mouse);
    battle.simulation.finish_step();
    // ReplaceObjectUpgrade reads the first selected drawable before its destruction.
    mouse.apply_replacements(battle);
    super::apply_spectre_selections(battle);
}

fn drain(battle: &mut Match, mouse: &mut Mouse) {
    let messages = std::mem::take(&mut battle.commands.pending);
    battle.commands.executing = true;
    for message in messages {
        if message.generation != battle.generation
            || !battle.simulation.player_active(message.owner)
        {
            continue;
        }
        let retain = |members: Vec<(usize, usize)>| {
            members
                .into_iter()
                .filter_map(|(id, owner)| {
                    battle
                        .simulation
                        .entities
                        .get(id)
                        .is_some_and(|actor| actor.exists() && actor.owner == owner)
                        .then_some(id)
                })
                .collect()
        };
        let selected = retain(message.selected);
        let group = battle.simulation.begin_command_group(message.owner);
        battle.commands.members.clone_from(&group.members);
        let squad = battle.simulation.selected_objects(message.owner).to_vec();
        battle.commands.drawables = selected;
        let current_selected = std::mem::replace(&mut battle.selected, group.members.clone());
        let current_squad = std::mem::replace(&mut battle.logic_selection, squad);
        let current_placement = battle.placing.take();
        let current_mode = std::mem::replace(&mut mouse.pending_command, CommandMode::Normal);
        let current_waypoint = std::mem::replace(&mut mouse.waypoint_mode, message.waypoint);
        let current_force = std::mem::replace(&mut mouse.placement_modifiers.active, message.force);
        execute(battle, mouse, message.owner, message.kind);
        battle.simulation.finish_command_group(group);
        battle.commands.members.clear();
        battle.commands.drawables.clear();
        battle.placing = current_placement;
        battle.selected = current_selected;
        battle.logic_selection = current_squad;
        mouse.pending_command = current_mode;
        mouse.waypoint_mode = current_waypoint;
        mouse.placement_modifiers.active = current_force;
    }
    battle.logic_selection = battle.simulation.selected_objects(0).to_vec();
    battle.commands.executing = false;
}

fn execute(battle: &mut Match, mouse: &mut Mouse, owner: usize, kind: Kind) {
    match kind {
        Kind::Action(action) => execute_action(battle, mouse, action),
        Kind::Targeted {
            mode,
            point,
            target,
        } => {
            mouse.pending_command = mode;
            super::targeted_order_world(battle, mouse, point, target);
        }
        Kind::Power {
            caster,
            name,
            target,
            specific,
        } => execute_power(battle, owner, caster, &name, &target, specific),
        Kind::Force { target, point } => super::attack::force_order(battle, target, point, mouse),
        Kind::Move { point, waypoints } => {
            super::move_group(battle, point, waypoints);
        }
        Kind::Guard(point) => {
            let members = battle.command_message_members();
            let _ = battle
                .simulation
                .group_guard_world(&members, point.to_array(), false);
        }
        Kind::Rally { object, point } => {
            drop(battle.command_message_members());
            if super::locally_controlled(battle, object) {
                let _ = battle.simulation.set_rally_point(object, point.to_array());
            }
        }
        Kind::Attack(target) => {
            let members = battle.command_message_members();
            let _ = battle
                .simulation
                .group_attack_objects(&members, target, false);
        }
        Kind::Override(point) => execute_override(battle, owner, point),
        Kind::Context { mode, target } => execute_context(battle, mouse, &mode, target),
        Kind::Board(target) => {
            super::board_context(battle, mouse, target);
        }
        Kind::ExitContainer(passenger) => {
            if let Some(host) = single_member(battle)
                && super::locally_controlled(battle, passenger)
            {
                let _ = battle.simulation.exit_container_at(passenger, host);
            }
        }
        Kind::ActorContext(order) => execute_actor_order(battle, &order),
        Kind::Build { unit, point, angle } => {
            if let Some(builder) = single_member(battle)
                && let Err(error) =
                    battle
                        .simulation
                        .build_facing(builder, unit, point.to_array(), angle)
            {
                eprintln!("Building placement rejected: {error}");
            }
        }
        Kind::Research(upgrade) => execute_research(battle, &upgrade),
        Kind::CancelConstruction => {
            if let Some(building) = single_member(battle) {
                let _ = battle.simulation.cancel_construction(owner, building);
            }
        }
        Kind::CancelUpgrade(upgrade) => cancel_upgrade(battle, owner, &upgrade),
        Kind::CancelProduction(id) => {
            if let Some(producer) = single_member(battle) {
                let _ = battle.simulation.cancel_production(owner, producer, id);
            }
        }
        Kind::Science(science) => {
            drop(battle.command_message_members());
            if let Err(error) = battle.simulation.purchase_science(owner, &science) {
                eprintln!("Science purchase rejected: {error}");
            }
        }
        Kind::Formation => {
            let members = battle.command_message_members();
            let _ = battle.simulation.group_create_formation(&members);
        }
        Kind::Select { added, replace } => {
            battle.selection_create(&added, replace);
        }
        Kind::Recall { members, replace } => battle
            .simulation
            .apply_selection_recall(owner, &members, replace),
        Kind::Deselect(removed) => battle.selection_remove(&removed),
        Kind::DestroySelection => battle.selection_destroy(),
        Kind::SelectionBoundary => {
            drop(battle.command_message_members());
        }
    }
}

fn execute_override(battle: &mut Match, owner: usize, point: Vec3) {
    for id in battle.command_message_members() {
        if super::locally_controlled(battle, id) {
            battle
                .simulation
                .override_special_power_destination(id, point.to_array(), owner);
        }
    }
}

fn execute_action(battle: &mut Match, mouse: &mut Mouse, action: CommandAction) {
    match action {
        CommandAction::Produce(product) => {
            if let Some(producer) = single_member(battle) {
                let _ = battle.simulation.train(producer, product);
            }
        }
        CommandAction::BattlePlan(plan) => {
            for id in battle.command_message_members() {
                battle.simulation.choose_battle_plan(id, plan);
            }
        }
        action => crate::playtest_ui::execute_command(battle, mouse, action),
    }
}

fn execute_research(battle: &mut Match, upgrade: &str) {
    for producer in battle.command_message_members() {
        let kind = battle.simulation.entities[producer].type_index;
        if let Some(index) = battle.simulation.catalog[kind]
            .maintenance
            .research
            .iter()
            .position(|entry| entry.name == upgrade)
        {
            let _ = battle.simulation.queue_upgrade(producer, index);
        }
    }
}

fn cancel_upgrade(battle: &mut Match, owner: usize, upgrade: &str) {
    if let Some(producer) = single_member(battle) {
        let kind = battle.simulation.entities[producer].type_index;
        if let Some(index) = battle.simulation.catalog[kind]
            .maintenance
            .research
            .iter()
            .position(|entry| entry.name == upgrade)
        {
            let _ = battle.simulation.cancel_upgrade(owner, producer, index);
        }
    }
}

fn single_member(battle: &mut Match) -> Option<usize> {
    let members = battle.command_message_members();
    members.first().copied()
}

fn execute_power(
    battle: &mut Match,
    owner: usize,
    caster: usize,
    name: &str,
    target: &PowerTarget,
    specific: bool,
) {
    let members = battle.command_message_members();
    if specific
        && let Some(actor) = battle
            .simulation
            .entities
            .get(caster)
            .filter(|actor| actor.exists())
    {
        if actor.owner == owner {
            let _ = battle
                .simulation
                .cast_power_from_provider(caster, name, target.clone());
        }
    } else {
        for id in members {
            let _ = battle.simulation.cast_power(id, name, target.clone());
        }
    }
}

fn execute_context(battle: &mut Match, mouse: &mut Mouse, mode: &CommandMode, target: usize) {
    match *mode {
        CommandMode::Hijack => {
            super::hijacker::issue(battle, mouse, target);
        }
        CommandMode::CarBomb => {
            super::car_bomb::issue(battle, mouse, target, true);
        }
        CommandMode::Sabotage => {
            super::sabotage::issue(battle, mouse, target);
        }
        CommandMode::Capture => {
            super::capture::issue(battle, mouse, target);
        }
        CommandMode::Hack(ability) => {
            super::hacker::issue_finite_context(battle, mouse, target, ability);
        }
        CommandMode::DisableBuilding => {
            super::hacker::issue(battle, mouse, target, true);
        }
        _ => unreachable!("translated context command"),
    }
}

fn execute_actor_order(battle: &mut Match, order: &ActorOrder) {
    for id in battle.command_message_members() {
        let game = &mut battle.simulation;
        let result = match *order {
            ActorOrder::Repair(target) => game.repair(id, target),
            ActorOrder::Attack(target) => game.attack(id, target),
            ActorOrder::AssistBuild(target) => game.assist_build(id, target),
            ActorOrder::Deliver(target) => game.deliver_supplies(id, target),
            ActorOrder::Harvest(supply) => game.harvest(id, supply),
            ActorOrder::Rally(point) => game.set_rally_point(id, point.to_array()),
        };
        if let Err(error) = result {
            eprintln!("Context order rejected: {error}");
        }
    }
}

#[cfg(test)]
mod tests;
