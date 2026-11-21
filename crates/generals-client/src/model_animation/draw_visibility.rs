//! SH `GameClient` cached shroud state, refreshed when `W3DScene` renders a clear drawable.
use crate::{playtest::Match, playtest_view::UnitVisual, projectile_render::ProjectileVisual};
use bevy::{mesh::MeshTag, prelude::*};
use generals_sim::skirmish::{Projectile, Skirmish, visibility::Status};
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

#[derive(bevy::ecs::system::SystemParam)]
pub struct DrawableRoots<'w, 's> {
    actors: Query<'w, 's, &'static UnitVisual>,
    projectiles: Query<'w, 's, &'static ProjectileVisual>,
}

impl DrawableRoots<'_, '_> {
    pub fn clear<'a>(
        &self,
        battle: &'a Match,
        root: Entity,
        projectile: impl FnOnce(u64) -> Option<&'a Projectile>,
    ) -> Option<usize> {
        let game = &battle.simulation;
        let observer = battle.observer_player.unwrap_or(0);
        if let Ok(visual) = self.actors.get(root)
            && visual.generation == battle.generation
        {
            let actor = &game.entities[visual.index];
            (!actor.drawable_effectively_hidden()
                && !actor.masked()
                && matches!(
                    game.visibility_status(observer, visual.index),
                    Status::Clear
                ))
            .then_some(visual.index)
        } else if let Ok(visual) = self.projectiles.get(root)
            && visual.generation == battle.generation
            && let Some(projectile) = projectile(visual.id)
        {
            clear_projectile(game, projectile, observer)
        } else {
            None
        }
    }
}

pub fn record(
    mut commands: Commands,
    mut cache: ResMut<DrawVisibility>,
    battle: Option<Res<Match>>,
    parts: Query<(Entity, &ChildOf, &ViewVisibility, Option<&MeshTag>), With<super::AnimatedPart>>,
    actors: Query<&'static UnitVisual>,
    projectiles: Query<&'static ProjectileVisual>,
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
    let roots = DrawableRoots {
        actors,
        projectiles,
    };
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
    for (part, parent, visible, tag) in &parts {
        if !visible.get() {
            continue;
        }
        let clear = roots.clear(&battle, parent.parent(), |id| shots.get(&id).copied());
        if let Some(id) = clear {
            cache
                .clear
                .insert(id, (game.entities[id].drawable_birth(), game.tick));
        }
        // The tag belongs to the mesh instance. A neighboring actor using the
        // same asset can still receive the native Partial/fog shroud pass.
        let desired = u32::from(clear.is_some());
        if tag.map_or(0, |tag| tag.0) != desired {
            commands.entity(part).insert(MeshTag(desired));
        }
    }
}

fn clear_projectile(game: &Skirmish, projectile: &Projectile, observer: usize) -> Option<usize> {
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
    use crate::{
        china_power_verification::{MISSILE_POWER, prepare_launcher},
        model_animation::AnimatedPart,
        model_render::PartBinding,
        skirmish_verification::{advance_until, friendly_entity},
    };
    use generals_sim::skirmish::{Kind, PowerTarget, visibility::VisibilityRules};
    use std::sync::Arc;

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

    #[test]
    #[ignore = "requires original game installation"]
    fn clear_actor_and_projectile_bind_instance_only_shroud_exemption() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let (launcher, _, _) = prepare_launcher(game).unwrap();
        let local = friendly_entity(game, Kind::CommandCenter, 0).unwrap();
        let remote = friendly_entity(game, Kind::CommandCenter, 1).unwrap();
        let target = game.object_position(local);
        advance_until(game, 10_801, "nuclear launcher ready", |game| {
            game.can_use_power_at_location(launcher, MISSILE_POWER, target)
        })
        .unwrap();
        game.cast_power(launcher, MISSILE_POWER, PowerTarget::Position3D(target))
            .unwrap();
        let clear_shot = |game: &Skirmish, shot: &Projectile| {
            shot.source == launcher
                && shot.render_transform(&game.entities).is_some()
                && clear_projectile(game, shot, 0).is_some()
        };
        advance_until(game, 90, "clear native projectile", |game| {
            game.projectiles.iter().any(|shot| clear_shot(game, shot))
        })
        .unwrap();
        let projectile = game
            .projectiles
            .iter()
            .find(|shot| clear_shot(game, shot))
            .unwrap()
            .id;
        assert!(matches!(game.visibility_status(0, local), Status::Clear));
        assert!(matches!(
            game.visibility_status(0, remote),
            Status::Shrouded
        ));

        let generation = battle.generation;
        let mut app = App::new();
        app.insert_resource(DrawVisibility::new())
            .insert_resource(battle)
            .add_systems(PostUpdate, record);
        let world = app.world_mut();
        let local_root = world
            .spawn(UnitVisual {
                index: local,
                generation,
            })
            .id();
        let remote_root = world
            .spawn(UnitVisual {
                index: remote,
                generation,
            })
            .id();
        let projectile_root = world
            .spawn(ProjectileVisual {
                id: projectile,
                generation,
            })
            .id();
        let local_part = model_part(world, local_root);
        let remote_part = model_part(world, remote_root);
        let projectile_part = model_part(world, projectile_root);
        let projected = world.spawn((ChildOf(local_root), Mesh3d::default())).id();
        app.update();
        assert_eq!(app.world().get::<MeshTag>(local_part), Some(&MeshTag(1)));
        assert_eq!(
            app.world().get::<MeshTag>(projectile_part),
            Some(&MeshTag(1))
        );
        assert_eq!(app.world().get::<MeshTag>(remote_part), None);
        assert_eq!(app.world().get::<MeshTag>(projected), None);

        set_local_footprint(&mut app, local, &Status::Partial);
        app.update();
        assert_eq!(app.world().get::<MeshTag>(local_part), Some(&MeshTag(0)));
        assert_eq!(app.world().get::<MeshTag>(projected), None);
        set_local_footprint(&mut app, local, &Status::Clear);
        app.update();
        assert_eq!(app.world().get::<MeshTag>(local_part), Some(&MeshTag(1)));
        assert_eq!(app.world().get::<MeshTag>(remote_part), None);
    }

    fn model_part(world: &mut World, parent: Entity) -> Entity {
        world
            .spawn((
                ChildOf(parent),
                Mesh3d::default(),
                ViewVisibility::VISIBLE,
                AnimatedPart {
                    model: 0,
                    binding: Arc::new(PartBinding {
                        linear_mapper: None,
                        mesh: 0,
                        pass: 0,
                        vertices: Vec::new(),
                    }),
                },
            ))
            .id()
    }

    fn set_local_footprint(app: &mut App, local: usize, status: &Status) {
        let game = &mut app.world_mut().resource_mut::<Match>().simulation;
        for unit in &mut game.catalog {
            unit.sight.clearing_range = 0.0;
            unit.sight.reveal_to_all_range = 0.0;
        }
        let kind = game.entities[local].type_index;
        for distance in (20_u16..=240).step_by(20) {
            game.catalog[kind].sight.clearing_range = f32::from(distance);
            game.configure_visibility(VisibilityRules {
                cell_size: 20.0,
                unlook_persist_frames: 0,
                use_shroud: true,
            })
            .unwrap();
            if std::mem::discriminant(game.visibility_status(0, local))
                == std::mem::discriminant(status)
            {
                return;
            }
        }
        panic!("original Command Center never reached the requested footprint status");
    }
}
