//! Installed Helix research dependencies. Native ability data uses the common parser.
#[cfg(test)]
mod tests;

pub(super) fn supported(name: &str) -> bool {
    matches!(
        name,
        "Upgrade_ChinaHelixGattlingCannon"
            | "Upgrade_ChinaHelixPropagandaTower"
            | "Upgrade_ChinaHelixBattleBunker"
            | "Upgrade_HelixNapalmBomb"
    )
}
