use super::*;
use crate::{
    aircraft_verification::comanche_trial::produce_mig, progression_verification::earn_combat_rank,
};
use generals_sim::skirmish::DamageType;

const POWER: &str = "SuperweaponSpectreGunship";

#[ignore = "requires original game installation"]
#[test]
fn paid_spectre_countermeasures_divert_an_original_mig_missile() -> Result<(), Box<dyn Error>> {
    let mut game = setup();
    game.set_match_options(50_000, 1);
    game.reset(Faction::Usa, Faction::China)?;
    let (army, _) = earn_combat_rank(&mut game, 5)?;
    game.purchase_science(0, "SCIENCE_SpectreGunshipSolo")?;
    let builder = friendly_entity(&game, Kind::Builder, 0)?;
    let airfield_type = template(&game, "AmericaAirfield")?;
    let airfield = construct_type(&mut game, builder, airfield_type, 0)?;
    research(&mut game, airfield, "Upgrade_AmericaCountermeasures")?;
    let center = friendly_entity(&game, Kind::CommandCenter, 0)?;
    let target = scout_open_target(&mut game, center, army[0])?;
    let mig = produce_mig(&mut game)?;
    let spectre = template(&game, "AmericaJetSpectreGunship")?;
    let flare = template(&game, "CountermeasureFlare")?;
    verify_source_rules(&game, spectre)?;

    for _ in 0..5 {
        wait_for_sortie(&mut game, center, mig)?;
        let first = game.entities.len();
        game.cast_power(center, POWER, PowerTarget::Position(target))?;
        let ship = game.entities[first..]
            .iter()
            .position(|actor| actor.owner == 0 && actor.type_index == spectre)
            .map(|index| first + index)
            .ok_or("earned Spectre power did not create its original aircraft")?;
        advance_until(
            &mut game,
            1800,
            "Spectre reaches its original firing orbit",
            |game| game.entities[ship].spectre_doors_open() == Some(true),
        )?;
        let shot = game.attack(mig, ship);
        require(
            shot.is_ok(),
            "the original MiG cannot target the airborne Spectre",
        )?;
        if observe_diversion(&mut game, first, mig, ship, flare)? {
            return Ok(());
        }
    }
    Err("paid Spectre never launched its original flares across five live missile sorties".into())
}

fn verify_source_rules(game: &Skirmish, kind: usize) -> Result<(), Box<dyn Error>> {
    let unit = &game.catalog[kind];
    let armor = unit
        .maintenance
        .armor_upgrade
        .as_ref()
        .ok_or("original Spectre upgrade missing")?;
    let gun = DamageType::Gattling as usize;
    let flares = &unit.behavior.countermeasures[0];
    require(
        armor.trigger == "Upgrade_AmericaCountermeasures"
            && unit.armor[gun].to_bits() == 0.4_f32.to_bits()
            && armor.coefficients[gun].to_bits() == 0.3_f32.to_bits()
            && flares.volley_size == 4
            && flares.volleys == 5,
        "Spectre lost its authored armor or four-flare, five-volley loadout",
    )
}

fn wait_for_sortie(game: &mut Skirmish, center: usize, mig: usize) -> Result<(), Box<dyn Error>> {
    advance_until(
        game,
        7201,
        "original Spectre recharge and MiG rearm",
        |game| {
            game.can_use_power(center, POWER)
                && game.entities[mig].aircraft_phase() == Some("idle")
                && game.height_above_ground(mig) < 10.0
                && game.entities[mig].weapon_rounds_remaining(0) == 2
        },
    )
}

fn observe_diversion(
    game: &mut Skirmish,
    first: usize,
    mig: usize,
    ship: usize,
    flare: usize,
) -> Result<bool, Box<dyn Error>> {
    let mut missiles = Vec::new();
    for _ in 0..1500 {
        game.step();
        for projectile in &game.projectiles {
            if projectile.source != mig {
                continue;
            }
            if projectile.target_object() == Some(ship) && !missiles.contains(&projectile.id) {
                missiles.push(projectile.id);
            }
            if missiles.contains(&projectile.id)
                && projectile.target_object().is_some_and(|id| {
                    id >= first
                        && game.entities[id].owner == 0
                        && game.entities[id].type_index == flare
                })
            {
                let volley = game.entities[first..]
                    .iter()
                    .filter(|actor| actor.owner == 0 && actor.type_index == flare)
                    .count();
                return Ok(volley >= 4);
            }
        }
        if !game.entities[ship].exists() {
            break;
        }
    }
    require(
        !missiles.is_empty(),
        "no paid MiG missile actually targeted the Spectre",
    )?;
    game.stop(mig)?;
    Ok(false)
}
