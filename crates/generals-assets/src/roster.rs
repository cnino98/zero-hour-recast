//! Source-derived skirmish faction eligibility, without GUI widget operations.
//!
//! Port of `PopulatePlayerTemplateComboBox` (GUIUtil.cpp:234–279) and
//! `ChallengeGenerals::getGeneralByTemplateName` from `SuperHackers` commit
//! 2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed. Source copyright Electronic Arts
//! Inc. 2025, GPL-3.0-or-later. Random/observer UI entries are not factions.
use std::collections::BTreeSet;

/// Already resolved fields consumed by the source faction selection loop.
pub struct PlayerTemplate {
    pub name: String,
    pub side: String,
    pub starting_building: String,
    pub old_faction: bool,
}

/// Challenge persona fields used to restrict ordinary skirmish selection.
pub struct GeneralPersona {
    pub player_template: String,
    pub starts_enabled: bool,
}

/// First source selection condition excluding a faction, or eligibility.
#[derive(Debug, Eq, PartialEq)]
pub enum Eligibility {
    Selectable,
    NoStartingBuilding,
    NotOldFaction,
    LockedGeneral,
    DuplicateSide,
}

/// Evaluate the source selection loop in player-template order.
///
/// `PlayableSide` is deliberately absent: this GUI source path does not consult
/// it. Persona names match without ASCII case; seen-side keys retain case, as in
/// the original `std::set<AsciiString>`. Only accepted sides enter that set.
#[must_use]
pub fn skirmish_eligibility(
    players: &[PlayerTemplate],
    personas: &[GeneralPersona],
    old_factions_only: bool,
) -> Vec<Eligibility> {
    let mut seen = BTreeSet::new();
    players
        .iter()
        .map(|player| {
            if player.starting_building.is_empty() {
                Eligibility::NoStartingBuilding
            } else if old_factions_only && !player.old_faction {
                Eligibility::NotOldFaction
            } else if personas
                .iter()
                .find(|persona| persona.player_template.eq_ignore_ascii_case(&player.name))
                .is_some_and(|persona| !persona.starts_enabled)
            {
                Eligibility::LockedGeneral
            } else if !seen.insert(&player.side) {
                Eligibility::DuplicateSide
            } else {
                Eligibility::Selectable
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Eligibility, GeneralPersona, PlayerTemplate, skirmish_eligibility};

    fn player(name: &str, side: &str, building: &str, old: bool) -> PlayerTemplate {
        PlayerTemplate {
            name: name.into(),
            side: side.into(),
            starting_building: building.into(),
            old_faction: old,
        }
    }

    #[test]
    fn locked_general_does_not_hide_a_later_unlocked_side() {
        let players = [
            player("Boss", "China", "CommandCenter", false),
            player("China", "China", "CommandCenter", true),
            player("Observer", "Observer", "", false),
            player("DuplicateChina", "China", "CommandCenter", false),
        ];
        let personas = [GeneralPersona {
            player_template: "BOSS".into(),
            starts_enabled: false,
        }];
        assert_eq!(
            skirmish_eligibility(&players, &personas, false),
            [
                Eligibility::LockedGeneral,
                Eligibility::Selectable,
                Eligibility::NoStartingBuilding,
                Eligibility::DuplicateSide,
            ]
        );
    }

    #[test]
    fn old_faction_filter_and_case_sensitive_side_identity_follow_source() {
        let players = [
            player("General", "China", "CommandCenter", false),
            player("Base", "China", "CommandCenter", true),
            player("DistinctCase", "china", "CommandCenter", true),
        ];
        assert_eq!(
            skirmish_eligibility(&players, &[], true),
            [
                Eligibility::NotOldFaction,
                Eligibility::Selectable,
                Eligibility::Selectable
            ]
        );
    }
}
