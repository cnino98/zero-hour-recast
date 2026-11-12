//! Original Stinger Site, held soldiers, native missile, command icons and firing audio.
use crate::{
    control_bar::{CommandAction, UiData},
    input::Mouse,
    model_animation::AnimatedPart,
    playtest::{CameraRig, Match},
    playtest_ui::{self, Action, PromotionIcon, selected_command},
    playtest_view::{Capture, UnitVisual},
    projectile_render::ProjectileVisual,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
    source_material::SourceMaterial,
    stinger_verification,
};
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: stinger_verification::Setup,
    began: Option<u64>,
    phase: u8,
    sounds: [bool; 2],
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = stinger_verification::prepare_quiet(&mut battle.simulation)?;
    battle.select_objects(&[setup.site], true);
    app.insert_resource(Check {
        setup,
        began: None,
        phase: 0,
        sounds: [false; 2],
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let position = app.world().resource::<Match>().simulation.entities[check.setup.site].position;
    app.world_mut().resource_mut::<CameraRig>().focus = Vec2::from_array(position) * 0.1;
    app.add_systems(PreUpdate, inspect_audio);
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if check.began.is_some() && check.phase != 5 {
        let events = std::array::from_fn::<_, 4, _>(|index| {
            if index == 3 {
                (check.setup.site, "RPGTrooperVoiceUpgradeCamoNetting")
            } else {
                (check.setup.soldiers[index], "StingerMissileWeapon")
            }
        });
        let played = audio.played(events);
        check.sounds[0] |= played[..3].iter().any(|played| *played);
        check.sounds[1] |= played[3];
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    data: Res<'w, UiData>,
    visuals: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    meshes: Query<
        'w,
        's,
        (
            &'static AnimatedPart,
            &'static ViewVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
    >,
    materials: Res<'w, Assets<SourceMaterial>>,
    projectiles: Query<'w, 's, (&'static ProjectileVisual, &'static Children)>,
    visible: Query<'w, 's, &'static ViewVisibility>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static ComputedNode,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    icons: Res<'w, Assets<PromotionIcon>>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == 5 || !capture.ready_for_scenario() {
        return;
    }
    let Some(began) = check.began else {
        check.began = Some(battle.simulation.tick);
        return;
    };
    let result = require(
        battle.simulation.tick - began < 3600,
        "Stinger camouflage/combat native acceptance timed out",
    )
    .and_then(|()| view.advance(&mut check, &mut battle, &mut mouse, &mut time, &mut capture));
    if let Err(error) = result {
        error!(
            "Stinger phase {}, audio={:?}, site concealed={}",
            check.phase,
            check.sounds,
            stinger_verification::camouflage::concealed(&battle.simulation, check.setup.site)
        );
        check.phase = 5;
        error!("Stinger Site native acceptance: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn advance(
        self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
        time: &mut Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<(), Box<dyn Error>> {
        let setup = &check.setup;
        match check.phase {
            0 => {
                self.verify_buttons(battle)?;
                let index =
                    stinger_verification::camouflage::research(&battle.simulation, setup.site)?;
                let action = selected_command(battle, &self.data, 12)
                    .ok_or("missing CamoNetting control")?
                    .action;
                require(
                    matches!(action, CommandAction::Research(value) if value == index),
                    "CamoNetting icon has no original research action",
                )?;
                let cash = battle.simulation.players[0].cash;
                playtest_ui::dispatch_command(battle, mouse, action);
                require(
                    battle.simulation.players[0].cash == cash - 500,
                    "native CamoNetting control did not pay its original price",
                )?;
                check.phase = 1;
                time.unpause();
            }
            1 => {
                if check.sounds[1]
                    && stinger_verification::camouflage::concealed(&battle.simulation, setup.site)
                    && self.models(battle, setup, true)
                {
                    time.pause();
                    capture.intermediate("stinger-camouflage.png");
                    check.phase = 2;
                }
            }
            2 => {
                stinger_verification::approach(&mut battle.simulation, setup)?;
                check.phase = 3;
                time.unpause();
            }
            3 => {
                if stinger_verification::ready_to_attack(&battle.simulation, setup) {
                    stinger_verification::engage(&mut battle.simulation, setup)?;
                    check.phase = 4;
                }
            }
            4 => {
                if check.sounds[0]
                    && self.models(battle, setup, false)
                    && self.missile(battle, setup)
                {
                    require(
                        !battle.simulation.entities[setup.site].stealthed(),
                        "Stinger remained concealed during native soldier attack",
                    )?;
                    check.phase = 5;
                    time.pause();
                    capture.finish_scenario();
                    println!(
                        "Original Stinger Site: actual paid camouflage control, research audio, site/soldier cloak materials, attack reveal, visible missile and firing audio passed"
                    );
                }
            }
            _ => unreachable!("completed Stinger diagnostic"),
        }
        Ok(())
    }

    fn models(&self, battle: &Match, setup: &stinger_verification::Setup, cloaked: bool) -> bool {
        for id in std::iter::once(&setup.site).chain(&setup.soldiers) {
            let visible = self.visuals.iter().any(|(visual, children)| {
                visual.index == *id
                    && visual.generation == battle.generation
                    && children.iter().any(|child| {
                        self.meshes.get(child).is_ok_and(|(_, visible, material)| {
                            visible.get()
                                && (!cloaked
                                    || self.materials.get(&material.0).is_some_and(|material| {
                                        material.parameters.opacity > 0.0
                                            && material.parameters.opacity < 1.0
                                            && material.alpha_mode == AlphaMode::Blend
                                    }))
                        })
                    })
            });
            if !visible {
                return false;
            }
        }
        true
    }

    fn verify_buttons(&self, battle: &Match) -> Result<(), Box<dyn Error>> {
        for (slot, name) in [
            (12, "Command_UpgradeGLACamoNetting"),
            (13, "Command_Stop"),
            (14, "Command_Sell"),
        ] {
            let command = selected_command(battle, &self.data, slot)
                .ok_or("Stinger Site original command missing")?;
            require(command.name == name, "Stinger Site command set differs")?;
            let image = &command.image;
            let expected = self.data.icon(image);
            require(
                self.buttons.iter().any(|(action, node, material)| {
                    matches!(action, Action::Slot(index) if *index == slot)
                        && node.size().min_element() > 0.0
                        && self.icons.get(&material.0).is_some_and(|icon| {
                            icon.image == expected.image && icon.uv_rect == expected.uv_rect
                        })
                }),
                "Stinger Site original inventory/command icon not painted",
            )?;
        }
        Ok(())
    }

    fn missile(&self, battle: &Match, setup: &stinger_verification::Setup) -> bool {
        let game = &battle.simulation;
        self.projectiles.iter().any(|(visual, children)| {
            visual.generation == battle.generation
                && game
                    .projectiles
                    .iter()
                    .find(|projectile| projectile.id == visual.id)
                    .is_some_and(|projectile| setup.soldiers.contains(&projectile.source))
                && children
                    .iter()
                    .any(|child| self.visible.get(child).is_ok_and(|visible| visible.get()))
        })
    }
}
