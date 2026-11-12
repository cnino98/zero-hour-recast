//! Native original-art check for the researched Scorpion launcher and live missile.
use crate::{
    model_animation::{AnimatedPart, Animations},
    playtest::Match,
    playtest_view::{Capture, UnitVisual},
    projectile_render::ProjectileVisual,
    skirmish_verification::{advance_until, friendly_entity, require},
};
use bevy::prelude::*;
use generals_sim::skirmish::Kind;
use std::error::Error;

#[derive(Resource)]
struct Check {
    scorpion: usize,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (scorpion, _) =
        crate::weapon_verification::research::prepare_scorpion(&mut battle.simulation)?;
    let target = friendly_entity(&battle.simulation, Kind::Builder, 1)?;
    battle.simulation.attack(scorpion, target)?;
    advance_until(
        &mut battle.simulation,
        18_000,
        "Scorpion researched missile launch",
        |game| {
            game.projectiles.iter().any(|projectile| {
                projectile.source == scorpion
                    && projectile.weapon_slot == 1
                    && projectile.render_transform(&game.entities).is_some()
            })
        },
    )?;
    battle.simulation.stop(scorpion)?;
    for _ in 0..4 {
        battle.simulation.step();
    }
    require(
        battle
            .simulation
            .projectiles
            .iter()
            .any(|projectile| projectile.source == scorpion && projectile.weapon_slot == 1),
        "Scorpion missile expired before native view",
    )?;
    battle.select_objects(&[scorpion], true);
    app.insert_resource(Check {
        scorpion,
        done: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    Ok(())
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    meshes: Query<
        'w,
        's,
        (
            &'static AnimatedPart,
            &'static Visibility,
            &'static ViewVisibility,
        ),
    >,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    visible: Query<'w, 's, &'static ViewVisibility>,
    animations: Res<'w, Animations>,
}

fn inspect(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    view: View,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.done || !capture.ready_for_scenario() {
        return;
    }
    let result = verify(&check, battle.into_inner(), view);
    check.done = true;
    match result {
        Ok(()) => {
            capture.finish_scenario();
            println!(
                "Rendered Scorpion research: original rocket rack/turret, depleted clip hiding and original missile mesh passed"
            );
        }
        Err(error) => {
            error!("Researched weapon native verification: {error}");
            exit.write(AppExit::error());
        }
    }
}

fn verify(check: &Check, battle: &Match, view: View) -> Result<(), Box<dyn Error>> {
    let View {
        visuals,
        meshes,
        projectiles,
        visible,
        animations,
    } = view;
    let scorpion = check.scorpion;
    let (_, children) = visuals
        .iter()
        .find(|(visual, _)| visual.index == scorpion && visual.generation == battle.generation)
        .ok_or("researched Scorpion drawable absent")?;
    let mut rack = false;
    let mut hidden_round = false;
    for child in children {
        let Ok((part, visibility, seen)) = meshes.get(*child) else {
            continue;
        };
        let Some(name) = animations
            .mesh_name(part.model, part.binding.mesh)
            .and_then(|name| name.rsplit('.').next())
        else {
            continue;
        };
        rack |= name.eq_ignore_ascii_case("MissleRack01") && seen.get();
        hidden_round |= name.eq_ignore_ascii_case("WeaponA01") && *visibility == Visibility::Hidden;
    }
    require(
        rack && hidden_round,
        "original Scorpion rack or fired missile hide state missing",
    )?;
    require(
        projectiles.iter().any(|(visual, children)| {
            battle.simulation.projectiles.iter().any(|projectile| {
                projectile.id == visual.id
                    && projectile.source == scorpion
                    && projectile.weapon_slot == 1
            }) && children
                .iter()
                .any(|child| visible.get(child).is_ok_and(|visible| visible.get()))
        }),
        "original Scorpion missile mesh is not visible",
    )?;
    Ok(())
}
