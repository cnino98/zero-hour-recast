//! Paid Chemical Suits research, original `EXChemSuit` artwork and visible recipients.
use super::{Decal, HordeDecals};
use crate::{
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    usa_research_verification,
};
use bevy::prelude::*;
use std::{error::Error, time::Instant};

#[derive(Resource)]
struct Check {
    infantry: Vec<usize>,
    done: bool,
    began: Instant,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = usa_research_verification::prepare(&mut battle.simulation)?;
    battle.started = true;
    battle.selected.clone_from(&setup.infantry);
    app.insert_resource(Check {
        infantry: setup.infantry,
        done: false,
        began: Instant::now(),
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let first = game.entities[check.infantry[0]].position;
    let last = game.entities[*check.infantry.last().expect("paid infantry")].position;
    app.world_mut().resource_mut::<CameraRig>().focus =
        (Vec2::from_array(first) + Vec2::from_array(last)) * 0.05;
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect(
    mut check: ResMut<Check>,
    data: Res<HordeDecals>,
    decals: Query<(&Decal, &ViewVisibility)>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<crate::source_material::SourceMaterial>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let data = data.into_inner();
    let meshes = meshes.into_inner();
    let materials = materials.into_inner();
    if check.done {
        return;
    }
    if check.began.elapsed().as_secs() >= 45 {
        eprintln!("Chemical Suits native check: no visible original decals within 45 seconds");
        check.done = true;
        exit.write(AppExit::error());
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    for &id in &check.infantry {
        let Some((decal, visible)) = decals.iter().find(|(decal, _)| decal.object == id) else {
            return;
        };
        let valid = visible.get()
            && decal.texture == 4
            && meshes
                .get(&decal.mesh)
                .is_some_and(|mesh| mesh.count_vertices() > 0)
            && materials.get(&decal.material).is_some_and(|material| {
                material.texture.as_ref() == data.handles.get(4)
                    && material.parameters.opacity.to_bits() == 1.0_f32.to_bits()
            });
        if !valid {
            eprintln!("Chemical Suits native check: original decal not visible for infantry {id}");
            check.done = true;
            exit.write(AppExit::error());
            return;
        }
    }
    println!(
        "Chemical Suits: paid existing/future infantry display original EXChemSuit terrain decals"
    );
    check.done = true;
    capture.finish_scenario();
}
