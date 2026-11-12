//! `OpenContain::doLoadSound/doUnloadSound`, `SuperHackers` 2bfaea60.
use super::Skirmish;

pub(super) struct AudioState {
    pub(super) enabled: bool,
    pub(super) frames: [u64; 2],
}

pub enum SoundHook {
    EnterSound,
    ExitSound,
    SoundEnter,
    SoundExit,
    SoundFalling,
}

pub struct Sound {
    pub frame: u64,
    pub object: usize,
    pub type_index: usize,
    pub hook: SoundHook,
    pub position: [f32; 3],
}

impl Skirmish {
    pub(in crate::skirmish) fn queue_container_sound(&mut self, carrier: usize, loading: bool) {
        let state = &mut self.entities[carrier].containment;
        let index = usize::from(!loading);
        if (loading && !state.audio.enabled) || state.audio.frames[index] == self.tick {
            return;
        }
        state.audio.frames[index] = self.tick;
        let hook = if loading {
            SoundHook::EnterSound
        } else {
            SoundHook::ExitSound
        };
        self.push_container_sound(carrier, hook, self.object_position(carrier));
    }

    pub(in crate::skirmish) fn container_enter_callback(&mut self, carrier: usize) {
        if self.entities[carrier].containment.audio.enabled {
            self.push_container_sound(
                carrier,
                SoundHook::SoundEnter,
                self.object_position(carrier),
            );
        }
    }

    pub(in crate::skirmish) fn container_removal_sound(
        &mut self,
        carrier: usize,
        rider: usize,
        normal: bool,
    ) {
        let tunnel = self.is_tunnel(carrier);
        if normal && !tunnel {
            self.queue_container_sound(carrier, false);
        }
        let position = self.object_position(carrier);
        let rider_position = if normal && !tunnel {
            position
        } else {
            // TunnelContain invokes the base callbacks before moving the rider
            // from its previous contained position to the requested endpoint.
            self.object_position(rider)
        };
        self.push_container_sound(carrier, SoundHook::SoundExit, position);
        self.push_container_sound(rider, SoundHook::SoundFalling, rider_position);
        // TunnelContain::onRemoving plays its coalesced sound after the base callbacks.
        if tunnel
            && self.entities[carrier].containment.tunnel.is_none()
            && self.entities[carrier].containment.cave.is_none()
        {
            self.queue_container_sound(carrier, false);
        }
    }

    fn push_container_sound(&mut self, object: usize, hook: SoundHook, position: [f32; 3]) {
        self.presentation.container_sounds.push(Sound {
            frame: self.tick,
            object,
            type_index: self.entities[object].type_index,
            hook,
            position,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::SoundHook;
    use crate::skirmish::containment::tests::setup;

    #[test]
    fn container_sound_callbacks_coalesce_each_direction_and_skip_initial_payload() {
        let (mut game, carrier) = setup(0.0, 0.0);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.board(rider, carrier);
        assert_eq!(game.presentation.container_sounds.len(), 1);
        assert!(matches!(
            game.presentation.container_sounds[0].hook,
            SoundHook::SoundEnter
        ));
        // The template callback has no frame guard; the module deadline starts at zero.
        game.remove_passenger(rider, carrier);
        game.presentation.container_sounds.clear();
        game.tick = 1;
        game.entities[carrier].containment.audio.enabled = false;
        game.board(rider, carrier);
        assert!(game.presentation.container_sounds.is_empty());
        game.entities[carrier].containment.audio.enabled = true;
        game.remove_passenger(rider, carrier);
        game.board(rider, carrier);
        game.remove_passenger(rider, carrier);
        game.board(rider, carrier);
        assert_eq!(game.presentation.container_sounds.len(), 8);
        assert!(matches!(
            game.presentation.container_sounds[0].hook,
            SoundHook::ExitSound
        ));
        assert!(matches!(
            game.presentation.container_sounds[4].hook,
            SoundHook::EnterSound
        ));
        assert_eq!(game.presentation.container_sounds[4].object, carrier);
        assert_eq!(game.presentation.container_sounds[2].object, rider);
        assert_eq!(
            game.presentation.container_sounds[4]
                .position
                .map(f32::to_bits),
            game.object_position(carrier).map(f32::to_bits)
        );
        game.tick = 2;
        game.remove_passenger(rider, carrier);
        assert_eq!(game.presentation.container_sounds.len(), 11);
        // SpawnBehavior's free-will door exit never added this object to the list.
        let free = game.spawn(5, 0, [355.0, 255.0], true);
        game.tick = 3;
        game.exit_spawned_from_container(free, carrier);
        assert_eq!(game.presentation.container_sounds.len(), 11);
    }

    #[test]
    fn container_damage_removal_does_not_play_unload_for_killed_cargo() {
        let (mut game, carrier) = setup(0.0, 1.0);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.tick = 1;
        game.board(rider, carrier);
        game.presentation.container_sounds.clear();
        game.apply_hit(carrier, carrier, game.entities[carrier].max_health);
        assert!(!game.entities[rider].alive());
        assert_eq!(game.presentation.container_sounds.len(), 2);
        assert!(matches!(
            game.presentation.container_sounds[0].hook,
            SoundHook::SoundExit
        ));
        assert!(matches!(
            game.presentation.container_sounds[1].hook,
            SoundHook::SoundFalling
        ));
    }

    #[test]
    fn tunnel_callback_keeps_rider_position_before_remote_endpoint_relocation() {
        let (mut game, carrier) = setup(0.0, 0.0);
        let rider = game.spawn(5, 0, [455.0, 355.0], true);
        let index = game.entities[carrier].type_index;
        game.catalog[index].contain.as_mut().unwrap().tunnel =
            Some(crate::skirmish::containment::TunnelRules {
                full_heal_frames: 150.0,
                conditions: crate::skirmish::death_callbacks::Conditions {
                    death_mask: u32::MAX,
                    veterancy_mask: 15,
                    exempt_status: 0,
                    required_status: 0,
                },
            });
        game.tick = 1;
        let before = game.object_position(rider);
        let endpoint = game.object_position(carrier);
        game.container_removal_sound(carrier, rider, true);
        assert_eq!(game.presentation.container_sounds.len(), 3);
        assert!(matches!(
            game.presentation.container_sounds[0].hook,
            SoundHook::SoundExit
        ));
        assert!(matches!(
            game.presentation.container_sounds[1].hook,
            SoundHook::SoundFalling
        ));
        assert!(matches!(
            game.presentation.container_sounds[2].hook,
            SoundHook::ExitSound
        ));
        assert_eq!(
            game.presentation.container_sounds[1]
                .position
                .map(f32::to_bits),
            before.map(f32::to_bits)
        );
        assert_eq!(
            game.presentation.container_sounds[2]
                .position
                .map(f32::to_bits),
            endpoint.map(f32::to_bits)
        );
    }
}
