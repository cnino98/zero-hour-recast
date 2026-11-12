//! `HighlanderBody::attemptDamage`: clamp the raw amount before `ActiveBody` processing.
use crate::skirmish::{BodyKind, DamageType, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn body_damage_input(
        &self,
        id: usize,
        damage: DamageType,
        amount: f32,
    ) -> f32 {
        let actor = &self.entities[id];
        if matches!(
            self.catalog[actor.type_index].body_kind,
            BodyKind::Highlander
        ) && !matches!(damage, DamageType::Unresistable)
        {
            amount.min(actor.health - 1.0)
        } else {
            amount
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{DeathType, Faction, Kind, tests};

    fn fixture(armor: f32) -> (Skirmish, usize) {
        let mut game = tests::game();
        let mut tree = tests::fixture_type(Faction::Civilian, Kind::Effect);
        tree.name = "HighlanderTree".into();
        tree.body_kind = BodyKind::Highlander;
        tree.max_health = 50.0;
        tree.initial_health = 50.0;
        tree.armor = [armor; crate::skirmish::DAMAGE_TYPE_COUNT];
        let kind = game.catalog.len();
        game.catalog.push(tree);
        let id = game.spawn(kind, game.neutral_owner(), [400.0, 400.0], true);
        (game, id)
    }

    #[test]
    fn ordinary_damage_clamps_before_armor_and_unresistable_topple_still_kills() {
        let (mut game, tree) = fixture(0.5);
        game.apply_weapon_hit(tree, 1, 1000.0, DamageType::Flame, DeathType::Burned);
        assert_eq!(game.entities[tree].health.to_bits(), 25.5_f32.to_bits());
        assert!(game.entities[tree].alive());
        game.apply_native_damage(tree, None, 999_999.0, DeathType::Toppled);
        assert!(game.entities[tree].is_effectively_dead());
        assert!(matches!(
            game.entities[tree].damage_state.death_type,
            DeathType::Toppled
        ));
    }

    #[test]
    fn vulnerability_is_applied_after_the_raw_limit_and_can_still_kill() {
        let (mut game, tree) = fixture(2.0);
        game.attempt_health_damage(tree, None, 1000.0, DamageType::Flame, DeathType::Burned);
        assert!(game.entities[tree].is_effectively_dead());
    }

    #[test]
    fn healing_uses_the_same_derived_input_limit_without_clamping_direct_health_changes() {
        let (mut game, tree) = fixture(1.0);
        game.change_body_health(tree, -40.0);
        game.apply_weapon_hit(tree, 1, 1000.0, DamageType::Healing, DeathType::Normal);
        assert_eq!(game.entities[tree].health.to_bits(), 19.0_f32.to_bits());
        game.change_body_health(tree, -18.5);
        assert_eq!(game.entities[tree].health.to_bits(), 0.5_f32.to_bits());
        assert!(game.entities[tree].alive());
    }
}
