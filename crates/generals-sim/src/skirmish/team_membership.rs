//! Player prototype order, Team instance order, and intrusive Object membership order.
//! Player.cpp:addTeamToList/iterateObjects; Team.cpp constructors; Object.cpp:setOrRestoreTeam.

struct Prototype {
    owner: usize,
    name: String,
    instances: Vec<usize>,
    attack_common_target: bool,
    singleton: bool,
}

struct Team {
    prototype: usize,
    members: Vec<usize>,
    common_attack_target: Option<usize>,
    active: bool,
    created: bool,
}

pub(super) struct Membership {
    prototypes: Vec<Prototype>,
    teams: Vec<Team>,
    objects: Vec<Option<usize>>,
    defaults: Vec<Option<usize>>,
}

impl Membership {
    pub(super) const fn new() -> Self {
        Self {
            prototypes: Vec::new(),
            teams: Vec::new(),
            objects: Vec::new(),
            defaults: Vec::new(),
        }
    }

    pub(super) fn add_prototype(&mut self, owner: usize, name: String) -> usize {
        let id = self.prototypes.len();
        self.prototypes.push(Prototype {
            owner,
            name,
            instances: Vec::new(),
            attack_common_target: false,
            singleton: false,
        });
        id
    }

    pub(in crate::skirmish) fn set_prototype_singleton(
        &mut self,
        prototype: usize,
        singleton: bool,
    ) {
        self.prototypes[prototype].singleton = singleton;
    }

    pub(in crate::skirmish) fn set_prototype_common_target(
        &mut self,
        prototype: usize,
        enabled: bool,
    ) {
        self.prototypes[prototype].attack_common_target = enabled;
    }

    pub(in crate::skirmish) fn attacks_common_target(&self, team: usize) -> bool {
        self.prototypes[self.teams[team].prototype].attack_common_target
    }

    pub(in crate::skirmish) fn common_target(&self, team: usize) -> Option<usize> {
        self.teams[team].common_attack_target
    }

    pub(in crate::skirmish) fn set_common_target(&mut self, team: usize, target: Option<usize>) {
        self.teams[team].common_attack_target = target;
    }

    pub(super) fn create_team(&mut self, prototype: usize) -> usize {
        let id = self.teams.len();
        self.teams.push(Team {
            prototype,
            members: Vec::new(),
            common_attack_target: None,
            active: false,
            created: false,
        });
        self.prototypes[prototype].instances.insert(0, id);
        id
    }

    /// `Team::setActive` marks creation only on the inactive-to-active transition.
    pub(in crate::skirmish) fn activate(&mut self, team: usize) {
        let state = &mut self.teams[team];
        if !state.active {
            state.created = true;
            state.active = true;
        }
    }

    pub(in crate::skirmish) fn active(&self, team: usize) -> bool {
        self.teams[team].active
    }

    /// `Team::updateState` clears `m_created` before executing the callback.
    pub(in crate::skirmish) fn take_created(&mut self, team: usize) -> bool {
        let state = &mut self.teams[team];
        if !state.active {
            return false;
        }
        std::mem::take(&mut state.created)
    }

    pub(super) fn set_default(&mut self, owner: usize, team: usize) {
        assert_eq!(self.prototypes[self.teams[team].prototype].owner, owner);
        self.defaults
            .resize(self.defaults.len().max(owner + 1), None);
        self.defaults[owner] = Some(team);
    }

    pub(in crate::skirmish) fn team_owner(&self, team: usize) -> usize {
        self.prototypes[self.teams[team].prototype].owner
    }

    pub(super) fn default_team(&self, owner: usize) -> usize {
        self.defaults[owner].expect("Player::setDefaultTeam before object creation")
    }

    pub(in crate::skirmish) fn find_team(&mut self, owner: usize, name: &str) -> Option<usize> {
        let prototype = self
            .prototypes
            .iter()
            .position(|prototype| prototype.owner == owner && prototype.name == name)?;
        if let Some(team) = self.prototypes[prototype].instances.first().copied() {
            return Some(team);
        }
        if self.prototypes[prototype].singleton {
            None
        } else {
            Some(self.create_team(prototype))
        }
    }

    pub(super) fn named_team(&self, owner: usize, name: &str) -> Option<usize> {
        self.prototypes
            .iter()
            .find(|prototype| prototype.owner == owner && prototype.name == name)
            .and_then(|prototype| prototype.instances.first().copied())
    }

    pub(super) fn team_name(&self, team: usize) -> &str {
        &self.prototypes[self.teams[team].prototype].name
    }

    pub(super) fn team_objects(&self, team: usize) -> &[usize] {
        &self.teams[team].members
    }

    pub(super) fn team_of(&self, object: usize) -> Option<usize> {
        self.objects.get(object).copied().flatten()
    }

    pub(super) fn set_team(&mut self, object: usize, team: Option<usize>) {
        self.objects
            .resize(self.objects.len().max(object + 1), None);
        if self.objects[object] == team {
            return;
        }
        if let Some(old) = self.objects[object] {
            self.teams[old].members.retain(|&member| member != object);
        }
        self.objects[object] = team;
        if let Some(new) = team {
            self.teams[new].members.insert(0, object);
        }
    }

    pub(super) fn player_objects(&self, owner: usize) -> Vec<usize> {
        self.prototypes
            .iter()
            .filter(|prototype| prototype.owner == owner)
            .flat_map(|prototype| &prototype.instances)
            .flat_map(|&team| self.teams[team].members.iter().copied())
            .collect()
    }
}

impl crate::skirmish::Skirmish {
    pub(in crate::skirmish) fn activate_object_team(&mut self, object: usize) {
        if let Some(team) = self.team_membership.team_of(object) {
            self.team_membership.activate(team);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Membership;

    #[test]
    fn prototype_append_instance_prepend_and_reassignment_match_player_iteration() {
        let mut order = Membership::new();
        let default = order.add_prototype(0, "teamplayer0".into());
        let first = order.create_team(default);
        order.set_default(0, first);
        let attack = order.add_prototype(0, "Attack".into());
        let old_attack = order.create_team(attack);
        let new_attack = order.create_team(attack);
        for (object, team) in [(0, first), (1, old_attack), (2, first), (3, new_attack)] {
            order.set_team(object, Some(team));
        }
        assert_eq!(order.player_objects(0), [2, 0, 3, 1]);
        order.set_team(0, Some(first));
        assert_eq!(order.player_objects(0), [2, 0, 3, 1]);
        order.set_team(2, Some(old_attack));
        assert_eq!(order.player_objects(0), [0, 3, 2, 1]);
        order.set_team(3, None);
        assert_eq!(order.player_objects(0), [0, 2, 1]);
        assert_eq!(
            order.named_team(0, "teamplayer0"),
            Some(order.default_team(0))
        );
    }
}
