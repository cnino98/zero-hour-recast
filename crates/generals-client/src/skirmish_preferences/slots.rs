//! `GameInfo::buildGameInfoAsciiString` and `ParseAsciiStringToGameInfo` slot records.
use super::{OpponentSlot, Slot, faction, faction_number, index, number};
use std::{collections::BTreeMap, fmt::Write};

pub(super) fn parse(value: &str) -> Result<(Vec<Slot>, String), &'static str> {
    let mut fields = BTreeMap::new();
    for field in value.split(';').filter(|field| !field.is_empty()) {
        let (key, value) = field.split_once('=').ok_or("missing game option value")?;
        if !matches!(
            key,
            "US" | "M" | "MC" | "MS" | "SD" | "C" | "SR" | "SC" | "O" | "S"
        ) || value.is_empty()
        {
            return Err("unknown or empty game option");
        }
        fields.insert(key, value);
    }
    for key in ["M", "MC", "MS", "SD", "C", "S"] {
        if !fields.contains_key(key) {
            return Err("incomplete game options");
        }
    }
    let raw: Vec<_> = fields["S"]
        .split(':')
        .filter(|value| !value.is_empty())
        .collect();
    if raw.len() != 8 {
        return Err("expected eight native slots");
    }
    let mut name = String::new();
    let slots = raw
        .into_iter()
        .enumerate()
        .map(|(owner, value)| {
            let fields: Vec<_> = value.split(',').collect();
            let (opponent, selection) = match fields.as_slice() {
                [human, _, _, accepted, color, faction, start, team, _]
                    if owner == 0 && human.starts_with('H') && accepted.len() == 2 =>
                {
                    human[1..].clone_into(&mut name);
                    (OpponentSlot::Open, [*color, *faction, *start, *team])
                }
                [kind, color, faction, start, team] if owner != 0 => {
                    let opponent = match *kind {
                        "CE" => OpponentSlot::Easy,
                        "CM" => OpponentSlot::Medium,
                        "CH" => OpponentSlot::Hard,
                        _ => return Err("invalid native AI slot"),
                    };
                    (opponent, [*color, *faction, *start, *team])
                }
                ["O"] if owner != 0 => return Ok(Slot::new(OpponentSlot::Open)),
                ["X"] if owner != 0 => return Ok(Slot::new(OpponentSlot::Closed)),
                _ => return Err("invalid represented slot record"),
            };
            Ok(Slot {
                faction: faction(selection[1])?,
                opponent,
                color: index(selection[0], 8)?,
                start: index(selection[2], 8)?,
                team: index(selection[3], 4)?.map(|value| u8::try_from(value).expect("four teams")),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((slots, name))
}

pub(super) fn encode(
    slots: &[Slot],
    name: &str,
    map: &str,
    seed: u32,
    starting_cash: u32,
    superweapon_restricted: bool,
) -> String {
    // Only the existing map is loaded. CRC/size are the native unknown-cache
    // defaults; these metadata do not select or reload a map in this port.
    let portable = map.replace('\\', "/");
    let directory = portable
        .rsplit_once('/')
        .map_or(portable.as_str(), |(parent, _)| parent);
    let mut value = format!(
        "M=03{directory};MC=0;MS=0;SD={seed};C=100;SR={};SC={starting_cash};O=Y;S=",
        u32::from(superweapon_restricted)
    );
    for owner in 0..8 {
        let opponent = slots
            .get(owner)
            .map_or(OpponentSlot::Closed, |slot| slot.opponent);
        if owner == 0 || opponent.difficulty().is_some() {
            if owner == 0 {
                write!(value, "H{name},0,0,TT,").expect("string write");
            } else {
                value.push_str(match opponent {
                    OpponentSlot::Easy => "CE,",
                    OpponentSlot::Medium => "CM,",
                    OpponentSlot::Hard => "CH,",
                    _ => unreachable!("occupied AI"),
                });
            }
            write!(
                value,
                "{},{},{},{}",
                number(slots[owner].color),
                faction_number(slots[owner].faction),
                number(slots[owner].start),
                number(slots[owner].team.map(usize::from))
            )
            .expect("string write");
            if owner == 0 {
                value.push_str(",0");
            }
            value.push(':');
        } else {
            value.push_str(if matches!(opponent, OpponentSlot::Open) {
                "O:"
            } else {
                "X:"
            });
        }
    }
    value.push(';');
    value
}
