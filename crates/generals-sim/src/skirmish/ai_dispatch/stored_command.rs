//! `AICommandParms::store/reconstitute` owns complete arguments across subclass deferral.
use crate::skirmish::{Skirmish, combat::attack::Goal};
#[derive(Clone)]
pub(in crate::skirmish) struct Command {
    pub source: i32,
    pub kind: Kind,
}

#[derive(Clone)]
pub(in crate::skirmish) enum Kind {
    Idle,
    HackInternet,
    MoveAside {
        other: usize,
        depth: u8,
    },
    Hunt,
    Guard([f32; 3]),
    GuardArea(usize),
    AttackArea(Vec<[f32; 2]>),
    Retaliate {
        victim: usize,
        position: [f32; 3],
        shots: u32,
    },
    Busy,
    Move([f32; 3]),
    MoveEvenIfSleeping([f32; 3]),
    MoveObject(usize),
    FollowPath {
        points: Vec<[f32; 3]>,
        ignore: Option<usize>,
        exit_production: bool,
    },
    AppendPath([f32; 3]),
    Face(Goal),
    Attack {
        goal: Goal,
        shots: u32,
        forced: bool,
    },
    AttackMove {
        position: [f32; 3],
        shots: u32,
    },
    Enter(usize),
    Exit(usize),
    Evacuate,
    MoveEvacuate {
        position: [f32; 3],
        exit: bool,
    },
    CombatDrop {
        target: Option<usize>,
        position: [f32; 3],
    },
    Repair(usize),
}

impl Skirmish {
    pub(in crate::skirmish) fn native_subclass_command(
        &mut self,
        id: usize,
        command: Command,
    ) -> bool {
        if self.catalog[self.entities[id].type_index]
            .behavior
            .hacking
            .is_some()
        {
            self.hacker_command(id, command)
        } else if self.entities[id].aircraft.is_some() {
            self.jet_command(id, command)
        } else {
            self.chinook_command(id, command)
        }
    }
    pub(in crate::skirmish) fn replay_stored_command(&mut self, id: usize, command: Command) {
        let source = command.source;
        match command.kind {
            Kind::HackInternet => self.native_hack_internet_from(id, source),
            Kind::MoveAside { other, depth } => self.move_away_from(id, other, depth),
            Kind::Hunt => self.native_hunt_from(id, source),
            Kind::Guard(position) => self.native_guard_from(id, position, source),
            Kind::GuardArea(area) => self.native_guard_area_from(id, area, source),
            Kind::AttackArea(points) => self.native_attack_area_from(id, points, source),
            Kind::Retaliate {
                victim,
                position,
                shots,
            } => self.native_retaliate_from(id, victim, position, shots, source),
            Kind::Idle => self.native_idle_from(id, source),
            Kind::Busy => self.native_busy_from(id, source),
            Kind::MoveEvenIfSleeping(position) => self.move_even_if_sleeping(id, position),
            Kind::Move(position) => self.native_move_from(id, position, source),
            Kind::MoveObject(target) => self.native_move_to_object_from(id, target, source),
            Kind::FollowPath {
                points,
                ignore,
                exit_production,
            } => self.native_follow_path_from(id, points, ignore, source, exit_production),
            Kind::AppendPath(position) => self.native_append_path_from(id, position, source),
            Kind::Face(goal) => self.native_face_from(id, goal, source),
            Kind::Attack {
                goal,
                shots,
                forced,
            } => self.native_attack_goal_from(id, goal, shots, source, forced),
            Kind::AttackMove { position, shots } => {
                self.native_attack_move_from(id, position, shots, source);
            }
            Kind::Enter(carrier) => self.native_enter_from(id, carrier, source),
            Kind::Exit(carrier) => self.native_exit_from(id, carrier, source),
            Kind::Evacuate => self.native_evacuate_from(id, source),
            Kind::MoveEvacuate { position, exit } => {
                self.native_move_evacuate_from(id, position, exit, source);
            }
            Kind::CombatDrop { target, position } => {
                self.native_combat_drop_from(id, target, position, source);
            }
            Kind::Repair(target) => {
                if self.entities[id].aircraft.is_some() {
                    self.native_jet_landing_from(id, target, source, true);
                } else {
                    self.native_chinook_repair_from(id, target, source);
                }
            }
        }
    }
}
