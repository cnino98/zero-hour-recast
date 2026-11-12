//! Original stealth infantry and Scout Drone model/animation acceptance.
use super::Animations;
use generals_assets::w3d::Model;
use generals_sim::skirmish::UnitType;
use std::{collections::BTreeSet, error::Error};

impl Animations {
    pub fn verify_jarmen_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        let index = types
            .iter()
            .position(|unit| unit.name == "GLAInfantryJarmenKell")
            .ok_or("missing Jarmen")?;
        let draw = self.draws[index]
            .first()
            .ok_or("missing original Jarmen draw")?;
        for (flags, clip) in [
            (vec![], "uihero_skl.uihero_sta"),
            (vec!["REALLYDAMAGED"], "uihero_skl.uihero_ista"),
            (vec!["MOVING"], "uihero_skl.uihero_rna2"),
            (vec!["MOVING", "REALLYDAMAGED"], "uihero_skl.uihero_irna"),
            (vec!["FIRING_A"], "uihero_skl.uihero_ata"),
            (vec!["FIRING_A", "REALLYDAMAGED"], "uihero_skl.uihero_iata2"),
            (vec!["FIRING_B"], "uihero_skl.uihero_sta"),
        ] {
            let conditions = flags.iter().map(|flag| (*flag).to_owned()).collect();
            let state = &draw.states[draw.select(&conditions)];
            let model = state
                .model
                .and_then(|model| models.get(model))
                .ok_or("Jarmen state not preloaded")?;
            if state.name != "uihero_skn"
                || !state.motions.iter().any(|motion| motion.name == clip)
                || !self.clips.contains_key(clip)
                || !model
                    .meshes
                    .iter()
                    .any(|mesh| mesh.attributes & 0x1000 == 0)
            {
                return Err(format!("Jarmen original art missing for {flags:?}").into());
            }
        }
        Ok(())
    }

    pub fn verify_infiltrator_art(
        &self,
        models: &[Model],
        types: &[UnitType],
    ) -> Result<(), Box<dyn Error>> {
        for (name, normal, damaged) in [
            ("AmericaInfantryPathfinder", "aipfdr_skn", "aipfdr_skn"),
            ("AmericaVehicleScoutDrone", "avscoutdr", "avscoutdr_d"),
            ("ChinaInfantryBlackLotus", "nihero_skn", "nihero_skn"),
        ] {
            let index = types
                .iter()
                .position(|unit| unit.name == name)
                .ok_or_else(|| format!("missing {name}"))?;
            let draw = self.draws[index]
                .first()
                .ok_or("missing infiltrator original draw")?;
            for injury in [false, true] {
                let mut flags = BTreeSet::new();
                if injury {
                    flags.insert("REALLYDAMAGED".into());
                }
                let state = &draw.states[draw.select(&flags)];
                if state.name != if injury { damaged } else { normal } {
                    return Err(format!("{name}: wrong native injury model").into());
                }
                let model = state
                    .model
                    .and_then(|index| models.get(index))
                    .ok_or("missing infiltrator mesh")?;
                if !model
                    .meshes
                    .iter()
                    .any(|mesh| mesh.attributes & 0x1000 == 0)
                {
                    return Err(format!("{name}: wholly hidden original geometry").into());
                }
                if name == "ChinaInfantryBlackLotus" {
                    for (condition, suffix, mode) in [
                        ("MOVING", "rna", "LOOP"),
                        ("UNPACKING", "ata1", "ONCE"),
                        ("FIRING_A", "ata2", "LOOP"),
                        ("PACKING", "ata3", "ONCE"),
                    ] {
                        let mut conditions = flags.clone();
                        conditions.insert(condition.into());
                        let state = &draw.states[draw.select(&conditions)];
                        let clip = format!(
                            "nihero_skl.nihero_{}{suffix}",
                            if injury { "i" } else { "" }
                        );
                        if state.mode != mode
                            || !state.motions.iter().any(|motion| motion.name == clip)
                            || !self.clips.contains_key(&clip)
                        {
                            return Err(format!(
                                "Lotus original {condition} injury={injury} clip missing"
                            )
                            .into());
                        }
                    }
                }
            }
        }
        println!(
            "Original infiltrator art: Pathfinder/Scout/Lotus healthy and injured models and all Lotus finite-power motion clips passed"
        );
        Ok(())
    }
}
