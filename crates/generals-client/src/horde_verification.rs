//! Bounded paid original Horde/Nationalism acceptance, separate from full regressions.
use crate::{
    control_bar::UiData,
    horde_decal::Inspection,
    input::Mouse,
    playtest::{CameraRig, Match},
    playtest_ui::{dispatch_command, selected_command},
    playtest_view::Capture,
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
};
use bevy::{app::AppExit, prelude::*};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

struct Group {
    units: Vec<usize>,
    center: usize,
    research: usize,
    focus: [f32; 2],
}

fn prepare_group(game: &mut Skirmish) -> Result<Group, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    construct(game, builder, Kind::Factory, 0)?;
    let propaganda = template(game, "ChinaPropagandaCenter")?;
    let center = construct_type(game, builder, propaganda, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let redguard = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaInfantryRedguard")
        .ok_or("Red Guard absent")?;
    let hunter = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaInfantryTankHunter")
        .ok_or("Tank Hunter absent")?;
    let base = game.entities[barracks].position;
    let focus = [base[0] - 250.0, base[1] - 180.0];
    let mut units = Vec::new();
    for (index, offset) in [
        [0.0, 0.0],
        [12.0, 0.0],
        [-12.0, 0.0],
        [0.0, 12.0],
        [0.0, -12.0],
    ]
    .into_iter()
    .enumerate()
    {
        let id = produce_type(
            game,
            barracks,
            if index == 4 { hunter } else { redguard },
            0,
        )?;
        game.move_to(id, [focus[0] + offset[0], focus[1] + offset[1]])?;
        advance_until(
            game,
            3000,
            "paid infantry reaching horde formation",
            |game| matches!(game.entities[id].ai.order, Order::Idle),
        )?;
        units.push(id);
    }
    advance_until(
        game,
        1500,
        "mixed original infantry forming a horde",
        |game| units.iter().all(|&id| game.entities[id].in_horde()),
    )?;
    let research = game.catalog[game.entities[center].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == "Upgrade_Nationalism")
        .ok_or("Propaganda Center lacks original Nationalism command")?;
    for &id in &units {
        require(
            game.entities[id].horde_bonuses() == 1,
            "unresearched horde has wrong weapon conditions",
        )?;
    }
    Ok(Group {
        units,
        center,
        research,
        focus,
    })
}

fn wait_research(game: &mut Skirmish, group: &Group) -> Result<(), Box<dyn Error>> {
    advance_until(
        game,
        2400,
        "paid Nationalism research and native horde refresh",
        |game| {
            game.has_upgrade(0, "Upgrade_Nationalism")
                && group
                    .units
                    .iter()
                    .all(|&id| game.entities[id].horde_bonuses() == 3)
        },
    )?;
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let group = prepare_group(game)?;
    let cost = game.catalog[game.entities[group.center].type_index]
        .maintenance
        .research[group.research]
        .cost;
    let before = game.players[0].cash;
    game.queue_upgrade(group.center, group.research)?;
    require(
        game.players[0].cash.checked_add(cost) == Some(before),
        "Nationalism did not deduct its original cost",
    )?;
    wait_research(game, &group)?;
    let separated = group.units[4];
    game.move_to(separated, [group.focus[0] - 200.0, group.focus[1]])?;
    advance_until(
        game,
        1500,
        "authored classic horde leave behavior",
        |game| !game.entities[separated].in_horde(),
    )?;
    require(
        game.entities[separated].horde_bonuses() == 2,
        "authored Action=HORDE lost its source Nationalism condition outside the group",
    )?;
    println!(
        "Installed Horde: five normally paid mixed Red Guards/Tank Hunter, original proximity status, paid Nationalism and authored classic separated-member condition passed"
    );
    Ok(())
}

#[derive(Resource)]
struct Check {
    group: Group,
    phase: u8,
    began: u64,
    research_began: Option<u64>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let group = prepare_group(&mut battle.simulation)?;
    battle.selected.clone_from(&group.units);
    let began = battle.simulation.tick;
    app.insert_resource(Check {
        group,
        phase: 0,
        began,
        research_began: None,
    });
    Ok(())
}

pub fn register(app: &mut App) {
    let focus = app.world().resource::<Check>().group.focus;
    app.world_mut().resource_mut::<CameraRig>().focus = Vec2::from_array(focus) * 0.1;
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn record(mut check: ResMut<Check>, mut battle: ResMut<Match>, mut exit: MessageWriter<AppExit>) {
    if !matches!(check.phase, 4 | 5) {
        return;
    }
    if check.phase == 4 {
        if let Err(error) = require(
            !battle.simulation.entities[check.group.center]
                .queue
                .is_empty(),
            "original Nationalism command did not queue paid research",
        ) {
            check.phase = 3;
            error!("Original Horde acceptance: {error}");
            exit.write(AppExit::error());
            return;
        }
        check.research_began = Some(battle.simulation.tick);
        check.phase = 5;
    }
    if battle.simulation.has_upgrade(0, "Upgrade_Nationalism")
        && check
            .group
            .units
            .iter()
            .all(|&id| battle.simulation.entities[id].horde_bonuses() == 3)
    {
        battle.select_objects(&check.group.units, true);
        check.research_began = None;
        check.phase = 2;
    } else if check
        .research_began
        .is_some_and(|frame| battle.simulation.tick >= frame + 2400)
    {
        check.phase = 3;
        error!("timed out waiting for paid Nationalism research and native horde refresh");
        exit.write(AppExit::error());
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct Presentation<'w, 's> {
    art: Inspection<'w, 's>,
    ui: Res<'w, UiData>,
}

fn inspect(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    presentation: Presentation,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == 3 || !capture.ready_for_scenario() {
        return;
    }
    let result = presentation.inspect_phase(&mut check, &mut battle, &mut mouse);
    match result {
        Ok(true) => {
            time.pause();
            capture.finish_scenario();
            check.phase = 3;
            println!(
                "Rendered Horde: original rotated EXHorde infantry decals, mixed paid formation, source Nationalism command, EXHorde_UP replacement and elapsed fade passed"
            );
        }
        Ok(false) => {}
        Err(error) => {
            check.phase = 3;
            error!("Original Horde acceptance: {error}");
            exit.write(AppExit::error());
        }
    }
}

impl Presentation<'_, '_> {
    fn inspect_phase(
        self,
        check: &mut Check,
        battle: &mut Match,
        mouse: &mut Mouse,
    ) -> Result<bool, Box<dyn Error>> {
        if battle.simulation.tick > check.began + 4000 {
            return Err("native Horde acceptance timed out".into());
        }
        if check.phase == 1 {
            let command = (1..=14)
                .find_map(|slot| {
                    selected_command(battle, &self.ui, slot)
                        .filter(|command| command.name == "Command_UpgradeChinaNationalism")
                })
                .ok_or("original Nationalism button missing")?;
            dispatch_command(battle, mouse, command.action);
            check.phase = 4;
            return Ok(false);
        }
        if matches!(check.phase, 4 | 5) {
            return Ok(false);
        }
        for &id in &check.group.units {
            if self
                .art
                .inspect(battle, id)?
                .is_none_or(|alpha| alpha < 0.9)
            {
                return Ok(false);
            }
        }
        if check.phase == 0 {
            battle.select_objects(&[check.group.center], true);
            check.phase = 1;
            Ok(false)
        } else {
            Ok(true)
        }
    }
}
