//! `Object::setDisabledUntil(HACKED)` / `clearDisabled`, with native mask edges.
use crate::skirmish::{
    Skirmish,
    hacker_disable::{Effect, Event},
    object_sounds::Action,
};

const HACKED: u32 = 1 << 1;
const POWER_LOSS: u32 = (1 << 1) | (1 << 2) | (1 << 6) | (1 << 10);

impl Skirmish {
    /// Native immutable `MiscAudio` bindings, installed before Object construction.
    pub fn configure_disabled_sounds(&mut self, sounds: [String; 4]) {
        self.disabled_sounds = sounds;
    }

    /// Native forever UNMANNED setter. AI commands/team changes belong to callers.
    pub(in crate::skirmish) fn set_unmanned(&mut self, id: usize) {
        let old_mask = self.object_disabled_mask(id);
        let drone = self.attack_kind(id, "DRONE");
        if !drone {
            self.presentation
                .weapon_fx
                .push(crate::skirmish::WeaponFxEvent::Pilot(
                    self.presentation.pilot_events.len(),
                ));
            self.presentation
                .pilot_events
                .push(crate::skirmish::crew::PilotEvent {
                    frame: self.tick,
                    object: id,
                    position: self.object_position(id),
                });
        }
        if !self.entities[id].unmanned() {
            self.native_unmanned_power_pause(id);
            self.entities[id].crew.unmanned = true;
            self.remember_disabled_observer_mask(id);
            if let Some(rider) = self.native_disabled_rider(id) {
                self.set_unmanned(rider);
            }
            if self.uses_spawn_weapons(id) {
                for child in self.spawned_children(id).to_vec() {
                    if self.entities[child].exists() {
                        if self.entities[child].ai.registration.is_some() {
                            self.ai_idle_from_ai(child);
                        }
                        self.set_unmanned(child);
                    }
                }
            }
        }
        if !drone {
            let dead_man_switch = self.catalog[self.entities[id].type_index]
                .weapon_set_rules
                .replacements
                .iter()
                .any(|set| set.flags & 8 != 0);
            if dead_man_switch {
                if let Some(source) = self.entities[id]
                    .damage_state
                    .retained_source
                    .filter(|&source| self.entities[source].exists())
                {
                    self.score_kill(source, id);
                }
                self.kill_object(id, crate::skirmish::DeathType::Normal);
            } else {
                self.clear_pilot_veterancy(id);
                self.undo_first_pilot_healing(id);
            }
        }
        if old_mask == 0 {
            self.native_disabled_edge(id, true, false);
        }
        self.remember_disabled_observer_mask(id);
    }

    fn undo_first_pilot_healing(&mut self, id: usize) {
        if let Some(index) = self.catalog[self.entities[id].type_index]
            .behavior_modules
            .iter()
            .position(|module| module.family == "AutoHealBehavior")
        {
            let state = &mut self.entities[id].maintenance.auto_heal[0];
            state.active = false;
            state.soonest = 0;
            self.update_objects[self.entities[id].update_owner].upgrade_executed[index] = false;
        }
    }

    pub(in crate::skirmish) fn set_hacked_until(&mut self, id: usize, frame: u32) {
        let old_mask = self.object_disabled_mask(id);
        if old_mask & POWER_LOSS == 0 {
            self.hacked_audio(id, false);
        }
        if self.entities[id].hacked_until != u64::from(frame) {
            let active = frame > self.update_frame();
            if old_mask & HACKED == 0 {
                self.native_hacked_power_change(id, true);
            }
            self.entities[id].hacked_until = u64::from(frame);
            if active {
                self.entities[id].disabled_status |= HACKED;
            } else {
                self.entities[id].disabled_status &= !HACKED;
            }
            if self.object_disabled(id) && self.entities[id].drawable.present {
                self.entities[id].drawable.tint_status |= 1;
            }
            self.remember_disabled_observer_mask(id);
            if let Some(rider) = self.native_disabled_rider(id) {
                self.set_hacked_until(rider, frame);
            }
            if self.uses_spawn_weapons(id) {
                for child in self.spawned_children(id).to_vec() {
                    if self.entities[child].exists() {
                        if self.entities[child].ai.registration.is_some() {
                            self.ai_idle_from_ai(child);
                        }
                        self.set_hacked_until(child, frame);
                    }
                }
            }
        }
        // Native edgeCase is captured before the write and does not test the
        // new mask: frame<=now still invokes onDisabledEdge(true).
        if old_mask == 0 {
            self.native_disabled_edge(id, true, false);
        }
        self.remember_disabled_observer_mask(id);
    }

    pub(in crate::skirmish) fn clear_hacked_disable(&mut self, id: usize) -> bool {
        if !self.entities[id].hacked() {
            return false;
        }
        let old_mask = self.object_disabled_mask(id);
        if old_mask & (POWER_LOSS & !HACKED) == 0 {
            self.hacked_audio(id, true);
        }
        self.native_hacked_power_change(id, false);
        // Native clear propagates to its rider only for the forever deadline.
        if self.entities[id].hacked_until == u64::from(u32::MAX)
            && let Some(rider) = self.native_disabled_rider(id)
        {
            self.clear_hacked_disable(rider);
        }
        if self.uses_spawn_weapons(id) {
            for child in self.spawned_children(id).to_vec() {
                if self.entities[child].exists() {
                    self.clear_hacked_disable(child);
                }
            }
        }
        self.entities[id].hacked_until = 0;
        self.entities[id].disabled_status &= !HACKED;
        self.clear_native_disabled_tint(id);
        self.remember_disabled_observer_mask(id);
        if self.object_disabled_mask(id) == 0 {
            self.native_disabled_edge(id, false, true);
        }
        self.remember_disabled_observer_mask(id);
        true
    }

    pub(in crate::skirmish) fn native_disabled_edge(
        &mut self,
        id: usize,
        disabled: bool,
        was_disabled: bool,
    ) {
        let kind = self.entities[id].type_index;
        let callbacks: Vec<_> = self.catalog[kind]
            .behavior_modules
            .iter()
            .map(|module| module.family.clone())
            .collect();
        let mut spy = 0;
        for family in callbacks {
            match family.as_str() {
                "SpyVisionUpdate" => {
                    self.native_spy_disabled_edge(id, spy, disabled);
                    spy += 1;
                }
                "DozerAIUpdate" | "WorkerAIUpdate" => self.dozer_disabled_edge(id, disabled),
                _ => {}
            }
        }
        // These are observer baselines, not callback state; a zero-duration
        // native edge must not invent a later inverse edge from this cache.
        self.entities[id].disabled_observed = self.object_disabled_mask(id);
        self.native_radar_disabled_edge(id, disabled);
        self.on_disabled_economy_power(id, disabled, was_disabled);
        self.remember_disabled_observer_mask(id);
    }

    fn remember_disabled_observer_mask(&mut self, id: usize) {
        let mask = self.object_disabled_mask(id);
        self.entities[id].disabled_observed = mask;
        self.remember_radar_disabled_mask(id, mask != 0);
    }

    fn native_disabled_rider(&self, id: usize) -> Option<usize> {
        match self.catalog[self.entities[id].type_index].contain_family() {
            Some("OverlordContain") => self.entities[id].containment.passengers.first().copied(),
            Some("HelixContain") => self.entities[id]
                .attachment_child()
                .filter(|&rider| self.entities[rider].exists()),
            _ => None,
        }
    }

    fn hacked_audio(&mut self, id: usize, enabled: bool) {
        let kinds = &self.catalog[self.entities[id].type_index].kind_of;
        let base = if kinds.iter().any(|kind| kind == "STRUCTURE") {
            0
        } else if kinds.iter().any(|kind| kind == "VEHICLE") {
            2
        } else {
            return;
        };
        self.presentation.disable_events.push(Event {
            object: id,
            frame: self.tick,
            effect: if enabled {
                Effect::Reenabled { target: id }
            } else {
                Effect::DisabledEdge { target: id }
            },
        });
        let name = self.disabled_sounds[base + usize::from(enabled)].clone();
        self.object_sound(id, name, Action::PlayAtPositionUnowned);
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn observe_native_disabled_tint(
        &mut self,
        id: usize,
        old: u32,
        new: u32,
    ) {
        let exceptions = (1 << 3) | (1 << 5) | (1 << 11);
        if new & !old & !exceptions != 0 && self.entities[id].drawable.present {
            self.entities[id].drawable.tint_status |= 1;
        }
        if old & !new != 0 {
            self.clear_native_disabled_tint(id);
        }
    }

    fn clear_native_disabled_tint(&mut self, id: usize) {
        if self.entities[id].drawable.present
            && self.object_disabled_mask(id) & !((1 << 3) | (1 << 5) | (1 << 11)) == 0
        {
            self.entities[id].drawable.tint_status &= !1;
        }
    }
}

impl crate::skirmish::Entity {
    #[must_use]
    pub const fn drawable_disabled_tint(&self) -> bool {
        self.drawable.tint_status & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Faction, Kind, special_power, tests};

    fn power() -> special_power::Rules {
        special_power::Rules {
            tag: "NativeHackedPower".into(),
            name: "FixturePower".into(),
            power_type: "SPECIAL_SPECTRE_GUNSHIP".into(),
            required_science: String::new(),
            timer: special_power::Timer {
                reload_frames: 60,
                shared: false,
                public: false,
            },
            view_range: 0.0,
            view_frames: 0,
            view_template: None,
            initiate_sound: String::new(),
            target_sound: String::new(),
            module_initiate_sound: String::new(),
            academy_classification: "ACT_NONE".into(),
            update_starts_attack: true,
            startup: special_power::Startup {
                paused: false,
                ready: false,
            },
            script_only: false,
        }
    }

    #[test]
    fn past_deadline_keeps_native_pause_and_energy_edges_without_setting_the_mask() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::Usa, Kind::PowerPlant);
        unit.energy = 20;
        unit.special_powers.push(power());
        tests::register_behavior(&mut unit, "SpecialAbility", 0x0100);
        unit.behavior_modules.last_mut().unwrap().tag = "NativeHackedPower".into();
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [200.0; 2], true);
        game.update_power();
        let before = game.players[0].energy.native[0];
        game.tick = 10;
        let progress = game.special_power_percent(id, "FixturePower");
        game.set_hacked_until(id, 5);
        assert!(!game.entities[id].hacked());
        assert_eq!(game.entities[id].hacked_until, 5);
        assert_eq!(game.players[0].energy.native[0], before - 20);
        // Identical deadline suppresses propagation/pause, but edgeCase was
        // false-mask before both calls: native onDisabledEdge runs twice.
        game.set_hacked_until(id, 5);
        assert_eq!(game.players[0].energy.native[0], before - 40);
        game.tick = 100;
        game.expire_hacked();
        assert!(!game.clear_hacked_disable(id));
        assert!(!game.special_power_ready(id, "FixturePower"));
        assert_eq!(
            game.special_power_percent(id, "FixturePower").to_bits(),
            progress.to_bits()
        );
        game.update_power();
        assert_eq!(game.players[0].energy.native[0], before - 40);
    }

    #[test]
    fn native_disabled_tint_survives_past_hacked_rewrite_until_actual_clear() {
        let mut game = tests::game();
        let kind = game.type_for(0, Kind::Tank).unwrap();
        let id = game.spawn(kind, 0, [200.0; 2], true);
        game.set_hacked_until(id, 100);
        assert!(game.entities[id].drawable_disabled_tint());
        game.tick = 10;
        game.set_hacked_until(id, 5);
        assert!(!game.entities[id].hacked());
        assert!(game.entities[id].drawable_disabled_tint());
        assert!(!game.clear_hacked_disable(id));
        assert!(game.entities[id].drawable_disabled_tint());
        // clearDisabled(HELD) also clears the tint once only exception bits remain.
        game.set_disabled_held(id);
        game.clear_disabled_held(id);
        assert!(!game.entities[id].drawable_disabled_tint());
        game.set_hacked_until(id, 6);
        assert!(!game.entities[id].drawable_disabled_tint());
        game.set_disabled_held(id);
        game.set_hacked_until(id, 7);
        // A past HACKED setter still sets tint when another disabled bit is set.
        assert!(!game.entities[id].hacked());
        assert!(game.entities[id].drawable_disabled_tint());
        game.clear_disabled_held(id);
        assert!(!game.entities[id].drawable_disabled_tint());
    }

    #[test]
    fn actual_overlord_literal_rider_receives_changed_deadline_and_forever_clear_only() {
        let (mut game, old) = crate::skirmish::containment::tests::setup(0.0, 0.0);
        let carrier = crate::skirmish::containment::tests::respawn_contain_family(
            &mut game,
            old,
            "OverlordContain",
        );
        let rider = game.spawn(5, 0, [300.0; 2], true);
        game.add_to_contain(rider, carrier);
        assert_eq!(game.entities[rider].contained_by(), Some(carrier));
        game.set_hacked_until(carrier, 100);
        assert!(game.entities[carrier].hacked());
        assert!(game.entities[rider].hacked());
        assert_eq!(game.entities[rider].hacked_until, 100);
        game.clear_hacked_disable(rider);
        game.set_hacked_until(carrier, 100);
        assert!(!game.entities[rider].hacked());
        game.set_hacked_until(carrier, 101);
        assert!(game.entities[rider].hacked());
        game.clear_hacked_disable(carrier);
        assert!(game.entities[rider].hacked());
        game.set_hacked_until(carrier, u32::MAX);
        game.clear_hacked_disable(carrier);
        assert!(!game.entities[rider].hacked());
    }

    #[test]
    fn disabled_sound_gate_is_shared_across_emp_and_hacked_and_queued_at_the_native_call() {
        let mut game = tests::game();
        game.configure_disabled_sounds([
            "BuildingOff".into(),
            "BuildingOn".into(),
            "VehicleOff".into(),
            "VehicleOn".into(),
        ]);
        let kind = game.type_for(0, Kind::Tank).unwrap();
        let id = game.spawn(kind, 0, [200.0; 2], true);
        game.entities[id].disabled_status |= 1 << 2;
        game.observe_spy_vision_disabled(id);
        let before = game.presentation.object_sounds.len();
        game.set_hacked_until(id, 100);
        game.clear_hacked_disable(id);
        assert_eq!(game.presentation.object_sounds.len(), before);
        game.entities[id].disabled_status &= !(1 << 2);
        game.observe_spy_vision_disabled(id);
        game.set_hacked_until(id, 100);
        game.clear_hacked_disable(id);
        let sounds = &game.presentation.object_sounds[before..];
        assert_eq!(
            sounds
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["VehicleOff", "VehicleOn"]
        );
        assert!(
            sounds
                .iter()
                .all(|event| matches!(event.action, Action::PlayAtPositionUnowned))
        );
    }
    #[test]
    fn repeated_unmanned_keeps_audio_and_xp_effects_but_only_pauses_once_and_undoes_first_healer() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::Usa, Kind::Tank);
        unit.veterancy.trainable = true;
        unit.veterancy.required = [0, 100, 200, 400];
        unit.special_powers.push(power());
        tests::register_behavior(&mut unit, "SpecialAbility", 0x0100);
        unit.behavior_modules.last_mut().unwrap().tag = "NativeHackedPower".into();
        for _ in 0..2 {
            let mut heal = tests::self_heal(1.0, 30, 0, &[]);
            heal.starts_active = true;
            unit.maintenance.auto_heal.push(heal);
            tests::register_update(&mut unit, "AutoHealBehavior");
        }
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [200.0; 2], true);
        game.ai_busy_from_ai(id);
        game.set_veterancy(id, 3);
        game.tick = 10;
        let progress = game.special_power_percent(id, "FixturePower");
        game.set_unmanned(id);
        assert!(!game.entities[id].ai_idle()); // Object setter itself never issues AI_IDLE.
        assert!(!game.entities[id].drawable_disabled_tint());
        assert!(!game.entities[id].maintenance.auto_heal[0].active);
        assert!(game.entities[id].maintenance.auto_heal[1].active);
        assert!(!game.native_upgrade_executed(id, "AutoHealBehavior", 0));
        assert!(game.native_upgrade_executed(id, "AutoHealBehavior", 1));
        game.set_veterancy(id, 3);
        game.give_self_upgrade_instance(id, "AutoHealBehavior", 0);
        game.tick = 20;
        game.set_unmanned(id);
        assert_eq!(game.entities[id].experience, 0);
        assert_eq!(game.entities[id].veterancy, 0);
        assert_eq!(game.presentation.pilot_events.len(), 2);
        assert!(!game.native_upgrade_executed(id, "AutoHealBehavior", 0));
        assert!(game.native_upgrade_executed(id, "AutoHealBehavior", 1));
        assert_eq!(
            game.special_power_percent(id, "FixturePower").to_bits(),
            progress.to_bits()
        );
        game.entities[id].crew.unmanned = false;
        game.observe_spy_vision_disabled(id);
        game.tick = 30;
        assert!(game.special_power_percent(id, "FixturePower") > progress);
    }

    #[test]
    fn unmanned_deadman_credits_previous_damage_source_then_uses_native_null_source_kill() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::Gla, Kind::Tank);
        unit.weapon_set_rules
            .replacements
            .push(crate::skirmish::WeaponTemplateSet {
                flags: 8,
                weapons: [Some(0), None, None],
                share_lock: false,
            });
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 1, [200.0; 2], true);
        let sniper = game.spawn(5, 0, [300.0; 2], true);
        game.tick = 10;
        game.record_body_damage(id, Some(sniper), crate::skirmish::DamageType::Explosion);
        game.tick = 11;
        // BodyModule::clearLastAttacker changes the clearable attacker, not
        // getLastDamageInfo()->in.sourceID used by the dead-man switch.
        game.entities[id].damage_state.last_attacker = None;
        let kills = game.players[0].statistics.units_destroyed;
        game.set_unmanned(id);
        assert!(!game.entities[id].alive());
        assert_eq!(game.players[0].statistics.units_destroyed, kills + 1);
        assert!(game.entities[id].damage_state.current_source.is_none());
        // ActiveBody retains a recent real combat record while onDie receives
        // the null source of Object::kill. Neither is a fabricated self-hit.
        assert!(matches!(
            game.entities[id].damage_state.damage_type,
            crate::skirmish::DamageType::Explosion
        ));
        assert!(matches!(
            game.entities[id].damage_state.death_type,
            crate::skirmish::DeathType::Normal
        ));
        assert_eq!(game.entities[id].damage_state.retained_source, Some(sniper));
    }
}
