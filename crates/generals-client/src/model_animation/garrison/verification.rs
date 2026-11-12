//! Installed paid Palace and actual original gun meshes, with no diagnostic UI.
use super::{Gun, Guns, gun_transform};
use crate::{
    control_bar::{CommandAction, UiData},
    playtest::Match,
    playtest_view::{Capture, UnitVisual},
    source_material::SourceMaterial,
};
use bevy::{app::AppExit, mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    carrier: usize,
    riders: Vec<usize>,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match, neutral: bool) -> Result<(), Box<dyn Error>> {
    let (carrier, riders) = if neutral {
        crate::transport_verification::prepare_neutral_display(&mut battle.simulation)?
    } else {
        crate::transport_verification::prepare_palace(&mut battle.simulation)?
    };
    if neutral {
        crate::playtest_ui::verify_structure_inventory(app, battle, carrier, riders[0])?;
        // The genuine exit/reentry above clears the rider's prior order.
        crate::transport_verification::engage_garrison(&mut battle.simulation, carrier, &riders)?;
    }
    battle.select_objects(&[carrier], true);
    let data = app.world().resource::<UiData>();
    let commands = &data.units[battle.simulation.entities[carrier].type_index].commands;
    let count = commands
        .iter()
        .filter(|command| matches!(command.action, CommandAction::ExitContainer(_)))
        .count();
    if count != if neutral { 10 } else { 5 }
        || !commands
            .iter()
            .any(|command| matches!(command.action, CommandAction::Evacuate))
    {
        return Err("original garrison inventory/evacuation buttons missing".into());
    }
    app.insert_resource(Check {
        carrier,
        riders,
        done: false,
    });
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    Ok(())
}

#[derive(bevy::ecs::system::SystemParam)]
struct Inspection<'w, 's> {
    guns: Res<'w, Guns>,
    roots: Query<'w, 's, (&'static Gun, &'static Transform, &'static Children)>,
    parts: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static ViewVisibility,
        ),
    >,
    units: Query<'w, 's, (&'static UnitVisual, &'static Visibility)>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Completion<'w> {
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
    clock: ResMut<'w, Time<Virtual>>,
    time: Res<'w, Time<Real>>,
}

fn inspect(
    battle: Res<Match>,
    inspection: Inspection,
    mut check: ResMut<Check>,
    mut completion: Completion,
) {
    let battle = battle.into_inner();
    let Inspection {
        guns,
        roots,
        parts,
        units,
        meshes,
        materials,
    } = inspection;
    if check.done {
        return;
    }
    if completion.time.elapsed_secs() > 60.0 {
        error!("Original garrison render diagnostic timed out");
        check.done = true;
        completion.exit.write(AppExit::error());
        return;
    }
    let hidden = check.riders.iter().all(|id| {
        units.iter().any(|(unit, visibility)| {
            unit.generation == battle.generation
                && unit.index == *id
                && *visibility == Visibility::Hidden
        })
    });
    if !hidden {
        return;
    }
    for (gun, transform, children) in &roots {
        if gun.generation != battle.generation || gun.carrier != check.carrier || !gun.firing {
            continue;
        }
        let rider = battle
            .simulation
            .garrison_points(gun.carrier)
            .and_then(|points| points[gun.point]);
        let Some(rider) = rider else { continue };
        let expected = gun_transform(&battle.simulation, gun.carrier, gun.point, rider);
        if transform.translation.distance(expected.translation) > 0.001
            || transform.rotation.angle_between(expected.rotation).abs() > 0.001
        {
            error!("Original garrison gun left its source window/facing");
            completion.exit.write(AppExit::error());
            check.done = true;
            return;
        }
        let visible = children
            .iter()
            .filter(|&child| {
                parts.get(child).is_ok_and(|(mesh, material, view)| {
                    let Some(mesh) = meshes.get(&mesh.0) else {
                        return false;
                    };
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        return false;
                    };
                    view.get()
                        && !points.is_empty()
                        && points.iter().flatten().all(|value| value.is_finite())
                        && materials
                            .get(&material.0)
                            .is_some_and(|material| material.texture.is_some())
                })
            })
            .count();
        if visible < guns.models[1].len() {
            continue;
        }
        println!(
            "Garrison drawable: host={} point={} rider={} source_position={:?} angle={} visible_parts={visible}",
            gun.carrier,
            gun.point,
            rider,
            battle.simulation.object_position(rider),
            battle
                .simulation
                .garrison_effect_facing(gun.carrier, gun.point)
                .expect("occupied facing")
        );
        completion.clock.pause();
        completion.capture.finish_scenario();
        check.done = true;
        println!(
            "Rendered garrison {}: original barrel and FIRING_A muzzle models at source firepoint/facing; enclosed infantry hidden and original inventory loaded",
            battle.simulation.catalog[battle.simulation.entities[check.carrier].type_index].name
        );
        break;
    }
}
