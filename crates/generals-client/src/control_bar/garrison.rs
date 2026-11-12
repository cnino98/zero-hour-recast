//! `ControlBarStructureInventory`: source hardcoded inventory/Stop/Evacuate slots.
use super::{Declaration, Error};

pub(super) fn command_slots(
    set: Option<&Declaration>,
    structure_inventory: bool,
) -> Result<Vec<(usize, &str)>, Box<dyn Error>> {
    if structure_inventory {
        let mut commands: Vec<_> = (1..=10)
            .map(|slot| (slot, "Command_StructureExit"))
            .collect();
        commands.extend([(11, "Command_Stop"), (12, "Command_Evacuate")]);
        return Ok(commands);
    }
    let mut commands = Vec::new();
    for line in set.into_iter().flat_map(|set| &set.lines) {
        if line.depth != Some(0) || line.tokens.len() != 2 {
            continue;
        }
        let Ok(slot) = line.tokens[0].parse::<usize>() else {
            continue;
        };
        if !(1..=14).contains(&slot) {
            return Err("command slot outside the original 14 buttons".into());
        }
        commands.push((slot, line.tokens[1].as_str()));
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_garrison_uses_native_ten_inventory_slots_and_fixed_stop_evacuate() {
        let commands = command_slots(None, true).unwrap();
        assert_eq!(commands.len(), 12);
        assert!(
            commands[..10]
                .iter()
                .enumerate()
                .all(|(index, &(slot, name))| slot == index + 1 && name == "Command_StructureExit")
        );
        assert_eq!(commands[10], (11, "Command_Stop"));
        assert_eq!(commands[11], (12, "Command_Evacuate"));
        assert!(command_slots(None, false).unwrap().is_empty());
    }
}
