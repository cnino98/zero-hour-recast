//! Installed rider profiles, alternate locomotor sets and animation timing.
use super::{blocks, definition, field, locomotor, number, scalar, source_numbers};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{
    LocomotorVariant,
    combat_bike::{RIDER_MODELS, Rider, Rules, rider_status_bit, rider_weapon_flag},
    locomotor_set as set,
};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Option<Rules>, Box<dyn Error>> {
    let behaviors = blocks(&object.lines, 0, "Behavior")?;
    let Some(contain) = behaviors.iter().copied().find(|module| {
        module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name == "RiderChangeContain")
    }) else {
        return Ok(None);
    };
    if field(contain, 1, "ScuttleStatus")
        .map(scalar)
        .transpose()?
        .is_some_and(|name| name != "TOPPLED")
    {
        return Err("unsupported bike ScuttleStatus".into());
    }
    let scuttle_frames = field(contain, 1, "ScuttleDelay")
        .map_or(Ok(0), |line| source_numbers::duration_frames(scalar(line)?))?;
    let steering = behaviors
        .into_iter()
        .find(|module| {
            module[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "AnimationSteeringUpdate")
        })
        .ok_or("bike lacks its steering module")?;
    let steering_frames = field(steering, 1, "MinTransitionTime")
        .map_or(Ok(0), |line| source_numbers::duration_frames(scalar(line)?))?;
    let mut riders = Vec::new();
    for index in 1..=8 {
        let Some(line) = field(contain, 1, &format!("Rider{index}")) else {
            continue;
        };
        let [_, template, model, weapon, status, commands, locomotor] = line.tokens.as_slice()
        else {
            return Err("expected template, model, weapon, status, command set and locomotor in Rider entry".into());
        };
        let model = RIDER_MODELS
            .into_iter()
            .find(|name| *name == model)
            .ok_or("invalid rider model condition")?;
        let weapon = rider_weapon_flag(weapon).ok_or("invalid rider weapon flag")?;
        let status = rider_status_bit(status).ok_or("invalid rider object status")?;
        definition(files, "CommandSet", commands)?;
        let locomotor = match locomotor.as_str() {
            "SET_NORMAL" => set::NORMAL,
            "SET_SLUGGISH" => set::SLUGGISH,
            _ => return Err("untranslated rider locomotor set".into()),
        };
        riders.push(Rider {
            template: template.clone(),
            model,
            weapon,
            status,
            command_set: commands.clone(),
            locomotor,
        });
    }
    let globals = &definition(files, "GameData", "")?.lines;
    let sluggish = locomotor::named_set(&object.lines, "SET_SLUGGISH")?
        .into_iter()
        .map(|name| {
            let lines = &definition(files, "Locomotor", name)?.lines;
            Ok(LocomotorVariant {
                speed: number(lines, 0, "Speed")?,
                locomotor: locomotor::load(lines, globals, &object.lines)?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    if riders.is_empty()
        || sluggish.is_empty()
            && riders
                .iter()
                .any(|profile| profile.locomotor == set::SLUGGISH)
    {
        return Err("bike lacks required rider or locomotor declarations".into());
    }
    Ok(Some(Rules {
        riders,
        sluggish,
        scuttle_frames,
        steering_frames,
    }))
}

#[cfg(test)]
mod tests {
    use crate::{
        cleanup_verification::tests::installed,
        skirmish_verification::{advance_until, construct, friendly_entity, produce_type},
    };
    use generals_sim::skirmish::{DeathType, Faction, Kind, Skirmish};

    fn paid_unit(game: &mut Skirmish, producer: usize, name: &str) -> usize {
        let index = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .unwrap();
        let cost = game.catalog[index].cost;
        advance_until(game, 18_000, "Combat Bike production funds", |game| {
            game.players[0].cash >= cost
        })
        .unwrap();
        produce_type(game, producer, index, 0).unwrap()
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_combat_bike_replaces_rider_and_scuttles_into_original_hulk() {
        let (_, mut battle) = installed();
        let game = &mut battle.simulation;
        game.reset(Faction::Gla, Faction::Usa).unwrap();
        let worker = friendly_entity(game, Kind::Builder, 0).unwrap();
        construct(game, worker, Kind::SupplyCenter, 0).unwrap();
        let barracks = construct(game, worker, Kind::Barracks, 0).unwrap();
        let factory = construct(game, worker, Kind::Factory, 0).unwrap();
        let bike = paid_unit(game, factory, "GLAVehicleCombatBike");
        let [rebel] = game.passengers(bike) else {
            panic!("Combat Bike requires its original Rebel payload");
        };
        let rebel = *rebel;
        let actor = &game.entities[bike];
        let unit = &game.catalog[actor.type_index];
        assert_eq!(
            game.catalog[game.entities[rebel].type_index].name,
            "GLAInfantryRebel"
        );
        assert_eq!(actor.active_weapon(unit).name, "GLARebelBikerMachineGun");
        assert!(game.model_flags(actor, [0.5, 0.1]).contains("RIDER2"));
        let terrorist = paid_unit(game, barracks, "GLAInfantryTerrorist");
        game.enter(terrorist, bike).unwrap();
        advance_until(game, 6000, "Terrorist replaces Combat Bike rider", |game| {
            game.passengers(bike) == [terrorist]
        })
        .unwrap();
        assert!(game.entities[rebel].alive());
        assert!(game.entities[rebel].contained_by().is_none());
        let actor = &game.entities[bike];
        assert_eq!(
            actor.active_weapon(&game.catalog[actor.type_index]).name,
            "TerroristSuicideWeapon"
        );
        assert!(game.model_flags(actor, [0.5, 0.1]).contains("RIDER5"));
        game.stop(bike).unwrap();
        assert!(game.can_scuttle_combat_bike(bike));
        game.evacuate(bike).unwrap();
        advance_until(game, 300, "Combat Bike scuttle dismount", |game| {
            game.entities[terrorist].contained_by().is_none()
        })
        .unwrap();
        assert!(game.entities[bike].alive());
        assert!(game.entities[terrorist].alive());
        assert!(!game.can_enter(terrorist, bike));
        assert!(
            game.model_flags(&game.entities[bike], [0.5, 0.1])
                .contains("TOPPLED")
        );
        advance_until(game, 90, "original toppled Combat Bike hulk", |game| {
            game.entities.iter().any(|actor| {
                actor.exists() && game.catalog[actor.type_index].name == "GLACombatBikeToppledHulk"
            })
        })
        .unwrap();
        assert!(matches!(
            game.entities[bike].death_type(),
            DeathType::Toppled
        ));
    }
}
