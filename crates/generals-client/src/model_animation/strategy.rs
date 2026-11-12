//! Installed Strategy Center deployment, armor walls, cannon and radar art.
use super::{Animations, Players};
use generals_assets::w3d::Model;
use generals_sim::skirmish::UnitType;
use generals_sim::skirmish::battle_plan::Plan;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn strategy_active_model(
        &self,
        players: &Players,
        type_index: usize,
        plan: Plan,
    ) -> Option<usize> {
        let expected = match plan {
            Plan::Bombardment => "abstrategy_a8",
            Plan::HoldTheLine => "abstrategy_a7",
            Plan::SearchAndDestroy => "abstrategy_a3",
        };
        self.draws
            .get(type_index)?
            .iter()
            .zip(&players.0)
            .find_map(|(draw, player)| {
                let state = draw.states.get(player.state)?;
                (state.name == expected)
                    .then_some(state.render_model)
                    .flatten()
            })
    }

    pub fn verify_strategy_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let index = types
            .iter()
            .position(|unit| unit.name == "AmericaStrategyCenter")
            .ok_or("missing Strategy Center catalog entry")?;
        if self.draws[index].len() != 9 {
            return Err("Strategy Center lost an original draw module".into());
        }
        for (door, deploying, active) in [
            (1, "abstrategy_a1", "abstrategy_a8"),
            (2, "abstrategy_a7", "abstrategy_a7"),
            (3, "abstrategy_a2", "abstrategy_a3"),
        ] {
            for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
                for (status, model, mode) in [
                    ("OPENING", deploying, Some("ONCE")),
                    ("CLOSING", deploying, Some("ONCE_BACKWARDS")),
                    ("WAITING_TO_CLOSE", active, None),
                ] {
                    let mut flags = BTreeSet::from([format!("DOOR_{door}_{status}")]);
                    flags.extend(damage.map(str::to_owned));
                    let state = self.draws[index]
                        .iter()
                        .map(|draw| &draw.states[draw.select(&flags)])
                        .find(|state| state.name == model && state.model.is_some())
                        .ok_or_else(|| {
                            format!(
                                "Strategy Center {flags:?} did not select loaded original {model}"
                            )
                        })?;
                    if let Some(mode) = mode
                        && (state.mode != mode
                            || state
                                .motions
                                .iter()
                                .all(|motion| !self.clips.contains_key(&motion.name)))
                    {
                        return Err(format!(
                            "Strategy Center {flags:?} lost {mode} deployment animation"
                        )
                        .into());
                    }
                }
            }
        }
        for damage in [None, Some("DAMAGED"), Some("REALLYDAMAGED")] {
            for door in [2, 3] {
                let mut flags = BTreeSet::from([format!("DOOR_{door}_WAITING_TO_CLOSE")]);
                flags.extend(damage.map(str::to_owned));
                let cannon = self.draws[index]
                    .iter()
                    .map(|draw| &draw.states[draw.select(&flags)])
                    .find(|state| state.name == "abstrategy_a8")
                    .and_then(|state| state.model)
                    .and_then(|model| models.get(model))
                    .ok_or("missing dormant Strategy Center cannon model")?;
                if cannon
                    .meshes
                    .iter()
                    .any(|mesh| mesh.attributes & 0x1000 == 0)
                {
                    return Err("dormant Strategy Center cannon descendants remain visible".into());
                }
            }
            let weapon = self
                .firing_state(index, damage, 0)
                .ok_or("missing Strategy Center cannon state")?;
            if weapon.name != "abstrategy_a8" || weapon.model.is_none() {
                return Err(
                    "Strategy Center projectile binding did not select deployed cannon".into(),
                );
            }
        }
        println!(
            "Original Strategy Center art: all nine draw modules, three deployment/packing animations and active cannon/walls/radar models verified"
        );
        Ok(())
    }
}
