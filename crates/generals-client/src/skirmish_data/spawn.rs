//! Original one-shot tunnel defenders and recurring Stinger spawn stations.
use super::{blocks, field, scalar};
use generals_assets::{content::Declaration, w3d::Model};
use generals_sim::skirmish::{
    UnitType,
    spawn::{Point, Rules},
};
use std::error::Error;

pub(super) fn load(object: &Declaration) -> Result<Vec<Rules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let mut rules = Vec::new();
    for module in modules {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "SpawnBehavior")
        {
            continue;
        }
        let yes = |key: &str| -> Result<bool, Box<dyn Error>> {
            match field(module, 1, key).map(scalar).transpose()? {
                None => Ok(false),
                Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
                Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
                Some(_) => Err(format!("Invalid SpawnBehavior boolean {key}").into()),
            }
        };
        let templates = module
            .iter()
            .filter(|line| {
                line.depth == Some(1)
                    && line
                        .tokens
                        .first()
                        .is_some_and(|key| key == "SpawnTemplateName")
            })
            .flat_map(|line| line.tokens[1..].iter().cloned())
            .collect::<Vec<_>>();
        if templates.is_empty() {
            return Err("empty spawn templates".into());
        }
        rules.push(Rules {
            templates,
            count: field(module, 1, "SpawnNumber")
                .map(scalar)
                .transpose()?
                .map(str::parse::<i32>)
                .transpose()?
                .unwrap_or(0),
            conditions: super::death_callbacks::conditions(module)?,
            replace_frames: i32::from_ne_bytes(
                field(module, 1, "SpawnReplaceDelay")
                    .map_or(Ok(0), |line| {
                        super::source_numbers::duration_frames(scalar(line)?)
                    })?
                    .to_ne_bytes(),
            ),
            flags: [
                ("OneShot", Rules::ONE_SHOT),
                ("SpawnedRequireSpawner", Rules::REQUIRE_SPAWNER),
                ("SlavesHaveFreeWill", Rules::FREE_WILL),
                ("ExitByBudding", Rules::EXIT_BY_BUDDING),
                ("AggregateHealth", Rules::AGGREGATE_HEALTH),
                ("CanReclaimOrphans", Rules::RECLAIM_ORPHANS),
            ]
            .into_iter()
            .try_fold(0, |flags, (key, bit)| {
                Ok::<_, Box<dyn Error>>(flags | if yes(key)? { bit } else { 0 })
            })?,
            initial_burst: field(module, 1, "InitialBurst")
                .map(scalar)
                .transpose()?
                .map(str::parse::<i32>)
                .transpose()?
                .unwrap_or(0),
        });
    }
    Ok(rules)
}

pub(super) fn bind(
    object: &Declaration,
    model: &Model,
    unit: &mut UnitType,
) -> Result<(), Box<dyn Error>> {
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "SpawnPointProductionExitUpdate")
        {
            continue;
        }
        let prefix = field(module, 1, "SpawnPointBoneName")
            .map(scalar)
            .transpose()?
            .unwrap_or("");
        let mut points = Vec::new();
        for index in 1..=10 {
            let name = format!("{prefix}{index:02}");
            let Some(position) = model.bone_position(&name) else {
                break;
            };
            let axes = model
                .bone_axes(&name)
                .ok_or("missing spawn point bone basis")?;
            points.push(Point {
                position,
                basis: std::array::from_fn(|row| std::array::from_fn(|column| axes[column][row])),
            });
        }
        unit.behavior.spawn_exits.push(points);
    }
    Ok(())
}

pub(super) fn mob_rules(
    files: &[generals_assets::content::ContentFile],
    object: &Declaration,
) -> Result<Vec<generals_sim::skirmish::spawn::MobRules>, Box<dyn Error>> {
    let modules = blocks(&object.lines, 0, "Behavior")?;
    let mut result = Vec::new();
    for module in modules {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|family| family != "MobMemberSlavedUpdate")
        {
            continue;
        }
        let globals = &super::definition(files, "GameData", "")?.lines;
        let locomotor =
            |set: &str| -> Result<(generals_sim::skirmish::Locomotor, f32), Box<dyn Error>> {
                let name = super::selected_pair(&object.lines, 0, "Locomotor", set)?
                    .ok_or("mob lacks original locomotor set")?;
                let lines = &super::definition(files, "Locomotor", name)?.lines;
                Ok((
                    super::locomotor::load(lines, globals, &object.lines)?,
                    super::number(lines, 0, "Speed")?,
                ))
            };
        let (wander, wander_speed) = locomotor("SET_WANDER")?;
        let (panic, panic_speed) = locomotor("SET_PANIC")?;
        let integer = |key: &str, default| -> Result<i32, Box<dyn Error>> {
            field(module, 1, key).map_or(Ok(default), |line| {
                Ok(i32::from_ne_bytes(
                    super::source_numbers::unsigned(scalar(line)?)?.to_ne_bytes(),
                ))
            })
        };
        result.push(generals_sim::skirmish::spawn::MobRules {
            catch_up: integer("MustCatchUpRadius", 50)?,
            no_catch_up: integer("NoNeedToCatchUpRadius", 25)?,
            squirrelliness: field(module, 1, "Squirrelliness").map_or(Ok(0.0), |line| {
                Ok::<_, Box<dyn Error>>(scalar(line)?.parse::<f32>()?)
            })?,
            crisis_calls: field(module, 1, "CatchUpCrisisBailTime")
                .map_or(Ok(999_999), |line| {
                    super::source_numbers::unsigned(scalar(line)?)
                })?,
            wander,
            wander_speed,
            panic,
            panic_speed,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(body: &str) -> generals_assets::content::ContentFile {
        generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "spawn.ini".into(), origin: "SpawnBehavior installed contracts".into(),
            bytes: format!("Object GLAStingerSite\nBehavior=SpawnPointProductionExitUpdate Tag\nSpawnPointBoneName=SpawnPoint\nEnd\nBehavior=SpawnBehavior Spawn\n{body}\nEnd\nEnd\n").into_bytes(),
        }).unwrap()
    }
    #[test]
    fn replacement_delay_uses_native_unsigned_prefix_narrowing_and_real_ceil() {
        for (token, expected) in [("101ms", 4), ("4294967296ms", 0), ("-1", 128_849_016)] {
            let file = source(&format!(
                "SpawnTemplateName=Soldier\nSpawnReplaceDelay={token}"
            ));
            assert_eq!(
                load(&file.declarations[0]).unwrap()[0].replace_frames,
                expected
            );
        }
    }
    #[test]
    fn recurring_stinger_defaults_and_native_millisecond_conversion() {
        let file = source(
            "SpawnNumber=3\nSpawnTemplateName=GLAInfantryStingerSoldier\nSpawnReplaceDelay=30000\nSpawnedRequireSpawner=Yes",
        );
        let rules = load(&file.declarations[0]).unwrap().remove(0);
        assert_eq!(rules.count, 3);
        assert_eq!(rules.replace_frames, 900);
        assert!(!rules.has(Rules::ONE_SHOT));
        assert!(!rules.has(Rules::FREE_WILL));
        assert!(rules.has(Rules::REQUIRE_SPAWNER));
        let file = source("SpawnNumber=3\nSpawnTemplateName=Soldier\nCanReclaimOrphans=Yes");
        assert!(load(&file.declarations[0]).unwrap()[0].has(Rules::RECLAIM_ORPHANS));
    }
    #[test]
    fn installed_mob_repeated_template_fields_preserve_order_and_budding_contract() {
        let file = source(
            "SpawnNumber=10\nSpawnTemplateName=Pistol01\nSpawnTemplateName=Rock02\nSpawnTemplateName=Molotov02\nSpawnTemplateName=Pistol03\nSpawnTemplateName=Rock04\nSpawnTemplateName=Molotov02\nSpawnTemplateName=Pistol05\nExitByBudding=Yes\nAggregateHealth=Yes\nInitialBurst=5\nSpawnReplaceDelay=30000",
        );
        let rules = load(&file.declarations[0]).unwrap().remove(0);
        assert_eq!(
            rules.templates,
            [
                "Pistol01",
                "Rock02",
                "Molotov02",
                "Pistol03",
                "Rock04",
                "Molotov02",
                "Pistol05"
            ]
        );
        assert_eq!(rules.count, 10);
        assert_eq!(rules.initial_burst, 5);
        assert_eq!(rules.replace_frames, 900);
        assert!(rules.has(Rules::EXIT_BY_BUDDING) && rules.has(Rules::AGGREGATE_HEALTH));
        assert!(!rules.has(Rules::REQUIRE_SPAWNER));
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_member_zero_build_time_retains_authored_value_and_rejects_paid_zero() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        let fixture = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "mob.ini".into(), origin: "native Mob data defaults and Int parsing".into(),
            bytes: b"Object MobDefaults\nLocomotor=SET_WANDER AngryMobWanderLocomotor\nLocomotor=SET_PANIC AngryMobPanicLocomotor\nBehavior=PhysicsBehavior TestPhysics\nMass=5.0\nEnd\nBehavior=MobMemberSlavedUpdate First\nEnd\nBehavior=MobMemberSlavedUpdate Second\nMustCatchUpRadius=-40units\nNoNeedToCatchUpRadius=-15\nSquirrelliness=2\nCatchUpCrisisBailTime=4294967296\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let parsed = mob_rules(&files, &fixture.declarations[0]).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            (
                parsed[0].catch_up,
                parsed[0].no_catch_up,
                parsed[0].crisis_calls
            ),
            (50, 25, 999_999)
        );
        assert_eq!(
            (
                parsed[1].catch_up,
                parsed[1].no_catch_up,
                parsed[1].crisis_calls
            ),
            (-40, -15, 0)
        );
        assert_eq!(
            parsed[1].squirrelliness.to_bits(),
            2.0_f32.to_bits(),
            "clamp belongs to onObjectCreated"
        );
        let object =
            super::super::definition(&files, "Object", "GLAInfantryAngryMobPistol01").unwrap();
        let mut unit = super::super::unit_type(
            &files,
            object,
            generals_sim::skirmish::Faction::Gla,
            generals_sim::skirmish::Kind::Infantry,
        )
        .unwrap();
        assert_eq!(unit.build_seconds.to_bits(), 0.0_f32.to_bits());
        assert!(super::super::validate_health(&unit).is_ok());
        unit.behavior.mobs.clear();
        assert!(super::super::validate_health(&unit).is_err());
    }
}
