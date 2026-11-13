//! SH `GameClient` cached shroud state, refreshed when `W3DScene` renders a clear drawable.
use crate::{playtest::Match, playtest_view::UnitVisual};
use bevy::prelude::*;
use generals_sim::skirmish::{Skirmish, visibility::Status};
use std::collections::BTreeMap;

#[derive(Resource)]
pub struct DrawVisibility {
    generation: Option<u32>,
    observer: usize,
    clear: BTreeMap<usize, (u64, u64)>,
}
impl DrawVisibility {
    pub(crate) const fn new() -> Self {
        Self {
            generation: None,
            observer: 0,
            clear: BTreeMap::new(),
        }
    }
    pub(crate) fn obscured(&self, battle: &Match, id: usize) -> bool {
        self.obscured_at(
            battle,
            id,
            !matches!(
                battle
                    .simulation
                    .visibility_status(battle.observer_player.unwrap_or(0), id),
                Status::Clear | Status::Partial
            ),
        )
    }
    pub(crate) fn obscured_at(&self, battle: &Match, id: usize, obscured: bool) -> bool {
        if !obscured {
            return false;
        }
        let game = &battle.simulation;
        let observer = battle.observer_player.unwrap_or(0);
        let clear = (self.generation == Some(battle.generation) && self.observer == observer)
            .then(|| {
                self.clear
                    .get(&id)
                    .filter(|(birth, _)| *birth == game.entities[id].drawable_birth())
                    .map(|&(_, frame)| frame)
            })
            .flatten();
        expired(clear, game.tick, game.entities[id].is_effectively_dead())
    }
}

fn obscured(game: &Skirmish, id: usize, observer: usize, clear: Option<u64>) -> bool {
    if matches!(
        game.visibility_status(observer, id),
        Status::Clear | Status::Partial
    ) {
        return false;
    }
    expired(clear, game.tick, game.entities[id].is_effectively_dead())
}

fn expired(clear: Option<u64>, frame: u64, dead: bool) -> bool {
    let grace = if dead { 150 } else { 60 };
    clear.is_none_or(|clear| frame >= clear.saturating_add(grace))
}

pub fn hidden(cache: Option<&DrawVisibility>, battle: &Match, id: usize) -> bool {
    let actor = &battle.simulation.entities[id];
    actor.drawable_effectively_hidden()
        || actor.masked()
        || cache.map_or_else(
            || {
                obscured(
                    &battle.simulation,
                    id,
                    battle.observer_player.unwrap_or(0),
                    None,
                )
            },
            |cache| cache.obscured(battle, id),
        )
}

pub fn record(
    mut cache: ResMut<DrawVisibility>,
    battle: Option<Res<Match>>,
    parts: Query<(&ChildOf, &ViewVisibility), With<super::AnimatedPart>>,
    visuals: Query<&UnitVisual>,
    projectiles: Query<&crate::projectile_render::ProjectileVisual>,
) {
    let Some(battle) = battle else { return };
    let observer = battle.observer_player.unwrap_or(0);
    if cache.generation != Some(battle.generation) || cache.observer != observer {
        cache.clear.clear();
        cache.generation = Some(battle.generation);
        cache.observer = observer;
    }
    if !battle.started {
        return;
    }
    let game = &battle.simulation;
    cache.clear.retain(|&id, (birth, _)| {
        game.entities
            .get(id)
            .is_some_and(|actor| actor.has_drawable() && actor.drawable_birth() == *birth)
    });
    let shots: BTreeMap<_, _> = game
        .projectiles
        .iter()
        .map(|shot| (shot.id, shot))
        .collect();
    for (parent, visible) in &parts {
        if !visible.get() {
            continue;
        }
        if let Ok(visual) = projectiles.get(parent.parent())
            && visual.generation == battle.generation
            && let Some(projectile) = shots.get(&visual.id)
            && let Some(id) = clear_projectile(game, projectile, observer)
        {
            cache
                .clear
                .insert(id, (game.entities[id].drawable_birth(), game.tick));
        }
        let Ok(visual) = visuals.get(parent.parent()) else {
            continue;
        };
        if visual.generation != battle.generation || !visible.get() {
            continue;
        }
        let actor = &game.entities[visual.index];
        if !actor.drawable_effectively_hidden()
            && !actor.masked()
            && matches!(
                game.visibility_status(observer, visual.index),
                Status::Clear
            )
        {
            cache
                .clear
                .insert(visual.index, (actor.drawable_birth(), game.tick));
        }
    }
}

fn clear_projectile(
    game: &Skirmish,
    projectile: &generals_sim::skirmish::Projectile,
    observer: usize,
) -> Option<usize> {
    let id = projectile.object?;
    let actor = &game.entities[id];
    if !actor.has_drawable() || actor.masked() || actor.drawable_effectively_hidden() {
        return None;
    }
    let (position, axes) = projectile.effect_transform(&game.entities)?;
    let body = generals_sim::collision_geometry::Body {
        position,
        angle: axes[0][1].atan2(axes[0][0]),
        geometry: &game.catalog[actor.type_index].geometry,
    };
    matches!(game.visibility_footprint(observer, &body), Status::Clear).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendered_clear_grace_uses_logic_frames_and_extends_only_effective_death() {
        assert!(expired(None, 0, false));
        assert!(!expired(Some(100), 159, false));
        assert!(expired(Some(100), 160, false));
        assert!(!expired(Some(100), 249, true));
        assert!(expired(Some(100), 250, true));
    }
}

#[cfg(test)]
mod installed_tests {
    use super::*;

    #[test]
    #[ignore = "requires original game installation"]
    fn grace_does_not_cross_drawable_incarnation_map_or_observer() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let (id, actor) = battle
            .simulation
            .entities
            .iter()
            .enumerate()
            .find(|(_, actor)| actor.owner == 1 && actor.alive())
            .unwrap();
        let birth = actor.drawable_birth();
        assert!(matches!(
            battle.simulation.visibility_status(0, id),
            Status::Shrouded | Status::Fogged
        ));
        let mut cache = DrawVisibility::new();
        cache.generation = Some(battle.generation);
        cache.clear.insert(id, (birth, battle.simulation.tick));
        assert!(!cache.obscured(&battle, id));
        cache.clear.insert(id, (birth + 1, battle.simulation.tick));
        assert!(cache.obscured(&battle, id));
        cache.clear.insert(id, (birth, battle.simulation.tick));
        battle.generation += 1;
        assert!(cache.obscured(&battle, id));
        cache.generation = Some(battle.generation);
        let mut app = App::new();
        app.insert_resource(cache)
            .insert_resource(battle)
            .add_systems(Update, record);
        app.world_mut().resource_mut::<Match>().observer_player = Some(1);
        app.update();
        assert!(app.world().resource::<DrawVisibility>().clear.is_empty());
    }
}
