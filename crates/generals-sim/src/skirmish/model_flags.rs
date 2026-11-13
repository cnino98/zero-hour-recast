//! Represented native model-condition flags shared by draw and containment.
//! `OpenContain::monitorConditionChanges` compares the full drawable flags,
//! including weapon transitions, even when they select the same model.
use super::{Entity, Skirmish, docking_flags};
use std::collections::BTreeSet;

impl Skirmish {
    #[must_use]
    pub fn model_flags(&self, unit: &Entity, thresholds: [f32; 2]) -> BTreeSet<String> {
        let physical_active_body = self.catalog[unit.type_index]
            .physical
            .as_ref()
            .is_some_and(|rules| rules.active_body);
        let mut flags = entity_flags(
            unit,
            physical_active_body,
            matches!(
                self.catalog[unit.type_index].body_kind,
                super::BodyKind::Inactive
            ),
            thresholds,
            self.tick,
        );
        flags.extend(unit.radar.conditions().into_iter().map(str::to_owned));
        flags.extend(unit.container_conditions().into_iter().map(str::to_owned));
        flags.extend(unit.combat_bike.rider_model.map(str::to_owned));
        flags.extend(unit.combat_bike.steering.map(str::to_owned));
        if unit.freefall_disabled() {
            flags.insert("FREEFALL".into());
        }
        if unit.helpers.special.cheering {
            flags.insert("SPECIAL_CHEERING".into());
        }
        flags.extend(unit.aircraft_conditions().into_iter().map(str::to_owned));
        if unit.spectre_afterburners() {
            flags.insert("JETAFTERBURNER".into());
        }
        if let Some(open) = unit
            .delivery_doors_open()
            .or_else(|| unit.spectre_doors_open())
        {
            flags.insert(
                if open {
                    "DOOR_1_OPENING"
                } else {
                    "DOOR_1_CLOSING"
                }
                .into(),
            );
        }
        flags.extend(
            crate::skirmish::parking::doors::condition_names(unit.production_door_flags)
                .into_iter()
                .map(str::to_owned),
        );
        if unit.loaded()
            && self.catalog[unit.type_index]
                .contain
                .as_ref()
                .is_some_and(|rules| rules.garrison.is_some())
        {
            flags.insert("GARRISONED".into());
        }
        if self.players[unit.owner].power < 0
            && self.catalog[unit.type_index]
                .kind_of
                .iter()
                .any(|name| name == "POWERED")
        {
            flags.insert("DISABLED".into());
        }
        if unit
            .drawable
            .model_conditions
            .has_flag(super::ModelConditions::CAPTURED)
        {
            flags.insert("CAPTURED".into());
        }
        flags
    }
}

fn entity_flags(
    unit: &Entity,
    physical_active_body: bool,
    inactive_body: bool,
    thresholds: [f32; 2],
    tick: u64,
) -> BTreeSet<String> {
    let ratio = if unit.max_health > 0.0 {
        unit.health / unit.max_health
    } else {
        1.0
    };
    let mut flags: BTreeSet<String> = [
        (
            "RUBBLE",
            unit.drawable.model_conditions.has_flag(1 << 5)
                || !inactive_body
                    && !unit.alive()
                    && !unit.post_collapse()
                    && !unit.crate_present()
                    && (!unit.corpse_present() || unit.helicopter_corpse())
                    && (!unit.physical_present() || physical_active_body),
        ),
        ("POST_COLLAPSE", unit.post_collapse()),
        ("DYING", unit.corpse_present()),
        ("DAMAGED", ratio <= thresholds[0] && ratio > thresholds[1]),
        ("REALLYDAMAGED", ratio <= thresholds[1] && ratio > 0.0),
        ("MOVING", unit.moving()),
        ("PREORDER", unit.drawable.model_conditions.preordered),
        (
            "PRONE",
            unit.drawable
                .model_conditions
                .has_flag(crate::skirmish::ModelConditions::PRONE),
        ),
        (
            "PANICKING",
            unit.drawable
                .model_conditions
                .has_flag(crate::skirmish::ModelConditions::PANICKING),
        ),
        (
            "RAPPELLING",
            unit.drawable
                .model_conditions
                .has_flag(crate::skirmish::ModelConditions::RAPPELLING),
        ),
        ("AFLAME", unit.aflame()),
        ("SECOND_LIFE", unit.battle_bus_grounded()),
        ("STUNNED_FLAILING", unit.physics_flailing()),
        (
            "STUNNED",
            unit.physics_stunned() && !unit.physics_flailing(),
        ),
        ("SMOLDERING", unit.smoldering()),
        ("TURRET_ROTATE", unit.ai.turret.rotating),
        ("ATTACKING", unit.attacking()),
        ("FRONTCRUSHED", unit.damage_state.front_crushed),
        ("BACKCRUSHED", unit.damage_state.back_crushed),
        ("CARRYING", unit.cargo > 0),
        ("DOCKING", unit.docking_flags & docking_flags::DOCKING != 0),
        (
            "DOCKING_BEGINNING",
            unit.docking_flags & docking_flags::BEGINNING != 0,
        ),
        (
            "DOCKING_ACTIVE",
            unit.docking_flags & docking_flags::ACTIVE != 0,
        ),
        (
            "DOCKING_ENDING",
            unit.docking_flags & docking_flags::ENDING != 0,
        ),
    ]
    .into_iter()
    .filter(|(_, active)| *active)
    .map(|(name, _)| name.into())
    .collect();
    append_construction_conditions(unit, &mut flags);
    append_death_conditions(unit, &mut flags);
    flags.extend(
        [
            unit.continuous_fire_condition(),
            unit.death_condition(),
            unit.drone_condition(),
            unit.battle_plan_condition(),
        ]
        .into_iter()
        .flatten()
        .map(str::to_owned),
    );
    flags.extend(unit.upgrade_conditions(tick).into_iter().map(str::to_owned));
    append_weapon_conditions(unit, &mut flags);
    for (name, bit) in [
        ("FIRING_A", 36),
        ("PACKING", 73),
        ("UNPACKING", 74),
        ("DEPLOYED", 75),
        ("RAISING_FLAG", 87),
    ] {
        if unit.drawable.model_conditions.has_flag(1 << bit) {
            flags.insert(name.into());
        }
    }
    flags
}

fn append_construction_conditions(unit: &Entity, flags: &mut BTreeSet<String>) {
    flags.extend(
        [
            ("ACTIVELY_CONSTRUCTING", unit.actively_constructing()),
            ("AWAITING_CONSTRUCTION", unit.awaiting_construction()),
            ("PARTIALLY_CONSTRUCTED", unit.partially_constructed()),
            ("SOLD", unit.sold_model()),
            (
                "ACTIVELY_BEING_CONSTRUCTED",
                unit.actively_being_constructed(),
            ),
        ]
        .into_iter()
        .filter(|(_, active)| *active)
        .map(|(name, _)| name.into()),
    );
}

fn append_death_conditions(unit: &Entity, flags: &mut BTreeSet<String>) {
    for (name, flag) in [
        ("TOPPLED", super::ModelConditions::TOPPLED),
        ("BURNED", super::ModelConditions::BURNED),
        ("PARACHUTING", super::ModelConditions::PARACHUTING),
        ("FREEFALL", super::ModelConditions::FREEFALL),
        (
            "EXPLODED_FLAILING",
            super::ModelConditions::EXPLODED_FLAILING,
        ),
        (
            "EXPLODED_BOUNCING",
            super::ModelConditions::EXPLODED_BOUNCING,
        ),
    ] {
        if unit.drawable.model_conditions.has_flag(flag) {
            flags.insert(name.into());
        }
    }
}

fn append_weapon_conditions(unit: &Entity, flags: &mut BTreeSet<String>) {
    if unit.weapon_set_upgraded() {
        flags.insert("WEAPONSET_PLAYER_UPGRADE".into());
    }
    if unit.salvage_level() != 0 {
        flags.insert(
            if unit.salvage_level() == 1 {
                "WEAPONSET_CRATEUPGRADE_ONE"
            } else {
                "WEAPONSET_CRATEUPGRADE_TWO"
            }
            .into(),
        );
    }

    for (slot, condition) in unit.weapon_set.status.condition.iter().enumerate() {
        if let Some(condition) = condition {
            flags.insert((*condition).into());
            flags.insert(["USING_WEAPON_A", "USING_WEAPON_B", "USING_WEAPON_C"][slot].into());
        }
    }
}
