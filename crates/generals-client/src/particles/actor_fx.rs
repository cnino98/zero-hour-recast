//! Native object FX callbacks retain their callback-time pose, owner and shroud.
use super::{
    Particles, Pose, Template,
    effects::{Nugget, parse_list, rng},
};
use crate::skirmish_data::{blocks, field, scalar};
use generals_assets::{
    content::{self, ContentFile},
    virtual_assets::VirtualAssets,
};
use generals_sim::skirmish::{Skirmish, UnitType};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

pub(super) fn names(types: &[UnitType]) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    for unit in types {
        for rules in &unit.behavior.toppling {
            names.insert(rules.topple_fx.as_str());
            names.insert(rules.bounce_fx.as_str());
        }
        names.extend(unit.combat_drop_kill_fx.as_deref());
        names.extend(
            unit.behavior
                .veterancy_crates
                .iter()
                .map(|rules| rules.execute_fx.as_str()),
        );
        names.extend(
            unit.neutral_tech
                .tech_buildings
                .iter()
                .filter_map(|rule| rule.pulse_fx.as_deref()),
        );
        names.extend(
            unit.behavior
                .sticky_bombs
                .iter()
                .filter_map(|rules| rules.geometry_fx.as_deref()),
        );
        names.extend(
            unit.behavior_modules
                .iter()
                .map(|module| module.upgrade.fx.as_str()),
        );
        for buster in &unit.behavior.bunker_busters {
            names.insert(buster.detonation_fx.as_str());
            names.insert(buster.crash_fx.as_str());
        }
        if let Some(rules) = unit.collision_rules() {
            names.extend(
                rules
                    .weapons
                    .iter()
                    .flatten()
                    .flat_map(|weapon| weapon.effects.iter().map(String::as_str)),
            );
        }
        names.extend(
            unit.behavior
                .firestorms
                .iter()
                .map(|rules| rules.effect.as_str()),
        );
        names.extend(
            unit.behavior
                .damage_weapons
                .iter()
                .map(|rules| rules.upgrade_fx.as_str()),
        );
        if let Some(storm) = unit.fire.as_ref().and_then(|fire| fire.storm.as_ref()) {
            names.insert(storm.effect.as_str());
        }
        if let Some(rules) = &unit.behavior.car_bomb_converter {
            names.insert(rules.effect.as_str());
        }
        names.extend(
            unit.damage_fx
                .iter()
                .filter_map(|set| set.effects.as_ref())
                .flat_map(|rules| rules.entries.iter().flatten())
                .flat_map(|entry| [entry.minor.as_str(), entry.major.as_str()]),
        );
        death_names(unit, &mut names);
        names.extend(
            generals_sim::skirmish::physical_ocl::lists(unit)
                .flat_map(|list| &list.nuggets)
                .filter(|nugget| nugget.debris_mass.is_some())
                .map(|nugget| nugget.draw.final_fx.as_str()),
        );
    }
    names.retain(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"));
    names
}

fn death_names<'a>(unit: &'a UnitType, names: &mut BTreeSet<&'a str>) {
    if let Some(rules) = &unit.death_lifecycle {
        for death in &rules.slow {
            names.extend(death.fx.iter().flatten().map(String::as_str));
            if let generals_sim::skirmish::death_lifecycle::Derived::BattleBus(bus) = &death.derived
            {
                names.insert(bus.start_fx.as_str());
                names.insert(bus.ground_fx.as_str());
            }
        }
    }
    names.extend(
        unit.behavior
            .bone_fx
            .iter()
            .flat_map(|rules| &rules.slots)
            .flatten()
            .filter_map(|slot| slot.fx.as_ref())
            .map(|fx| fx.name.as_str()),
    );
    for rules in &unit.behavior.death_fx {
        names.insert(rules.effect.as_str());
        names.insert(rules.upgrade_fx.as_str());
    }
    names.extend(
        unit.behavior
            .death_weapons
            .iter()
            .map(|rules| rules.upgrade_fx.as_str()),
    );
    if let Some(rules) = &unit.structure_death {
        names.extend(
            rules
                .phases
                .iter()
                .flat_map(|phase| phase.effects.iter().map(String::as_str)),
        );
        if let generals_sim::skirmish::structure_death::Module::Topple(topple) = &rules.module {
            names.extend([
                topple.start_effect.as_str(),
                topple.delay_effect.as_str(),
                topple.done_effect.as_str(),
                topple.crushing_effect.as_str(),
            ]);
            names.extend(topple.angles.iter().map(|(_, name)| name.as_str()));
        }
    }
    for helicopter in unit.helicopter_death_rules() {
        names.extend(helicopter.phases.iter().map(|phase| phase.effect.as_str()));
    }
    names.extend(
        unit.behavior
            .instant_death
            .iter()
            .flat_map(|rules| &rules.effects)
            .filter_map(Option::as_deref),
    );
    for jet in unit.jet_death_rules() {
        names.extend(jet.phases.iter().map(|phase| phase.effect.as_str()));
    }
}

pub(super) fn load(
    files: &[ContentFile],
    types: &[UnitType],
    templates: &[Template],
    lists: &mut Vec<Vec<Nugget>>,
) -> Result<BTreeMap<String, usize>, Box<dyn Error>> {
    let mut bindings = BTreeMap::new();
    for name in names(types) {
        let list = parse_list(
            files,
            {
                let fx = crate::skirmish_data::fx_definition(files, name)?
                    .ok_or_else(|| format!("missing FXList {name}"))?;
                &fx.lines
            },
            templates,
            lists,
            true,
        )?;
        bindings.insert(name.into(), lists.len());
        lists.push(list);
    }
    Ok(bindings)
}

pub fn sounds(
    assets: &VirtualAssets,
    types: &[UnitType],
) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/fxlist.ini")?)?];
    let mut bindings = BTreeMap::new();
    for name in names(types) {
        sound_list(&files, name, true, &mut bindings)?;
    }
    Ok(bindings)
}

pub(super) fn load_position_effects(
    assets: &VirtualAssets,
    names: &BTreeSet<String>,
    templates: &[Template],
    effects: &mut super::effects::LoadedEffects,
) -> Result<(), Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/fxlist.ini")?)?];
    for name in names {
        if effects.actor_fx.contains_key(name) {
            continue;
        }
        let list = parse_list(
            &files,
            {
                let fx = crate::skirmish_data::fx_definition(&files, name)?
                    .ok_or_else(|| format!("missing FXList {name}"))?;
                &fx.lines
            },
            templates,
            &mut effects.lists,
            true,
        )?;
        effects.actor_fx.insert(name.clone(), effects.lists.len());
        effects.lists.push(list);
    }
    Ok(())
}

fn sound_list(
    files: &[ContentFile],
    name: &str,
    object_form: bool,
    bindings: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Box<dyn Error>> {
    if name.eq_ignore_ascii_case("NONE") {
        return Ok(());
    }
    let fx = crate::skirmish_data::fx_definition(files, name)?
        .ok_or_else(|| format!("missing FXList {name}"))?;
    let mut sounds = Vec::new();
    for nugget in blocks(&fx.lines, 0, "Sound")? {
        if let Some(line) = field(nugget, 1, "Name") {
            sounds.push(scalar(line)?.into());
        }
    }
    for nugget in blocks(&fx.lines, 0, "FXListAtBonePos")? {
        if !object_form {
            return Err("nested bone FX receives a position rather than an object".into());
        }
        let child = field(nugget, 1, "FX")
            .map(scalar)
            .transpose()?
            .unwrap_or("NONE");
        // Retain child audio dependencies separately. Repetition and ordering
        // depend on actual current drawable bones, not the loader's name list.
        sound_list(files, child, false, bindings)?;
    }
    bindings.insert(name.into(), sounds);
    Ok(())
}

impl Particles {
    pub(crate) fn debris_final_fx(
        &mut self,
        battle: &crate::playtest::Match,
        name: &str,
        position: [f32; 3],
        basis: [[f32; 3]; 3],
        animations: &mut crate::model_animation::Animations,
    ) {
        self.begin(battle.generation, battle.simulation.tick);
        if name.is_empty() || name.eq_ignore_ascii_case("NONE") {
            return;
        }
        let viewer = battle.observer_player.unwrap_or(0);
        if !matches!(
            battle
                .simulation
                .visibility_cell(viewer, [position[0], position[1]]),
            generals_sim::skirmish::visibility::Status::Clear
        ) {
            return;
        }
        let pose = Pose {
            position,
            axes: std::array::from_fn(|column| std::array::from_fn(|row| basis[row][column])),
        };
        let index = self.actor_fx[name];
        self.fire_from(
            index,
            (&pose, None),
            &[],
            &|point| battle.simulation.ground_height(point),
            0.0,
            &mut rng::source(animations, battle.generation),
        );
    }

    pub fn tree_position_event(
        &mut self,
        game: &Skirmish,
        generation: u32,
        name: &str,
        position: [f32; 3],
        clear: bool,
        animations: &mut crate::model_animation::Animations,
    ) {
        self.begin(generation, game.tick);
        if name.is_empty() || name.eq_ignore_ascii_case("NONE") || !clear {
            return;
        }
        let index = self.actor_fx[name];
        self.fire_from(
            index,
            (&Pose::at(position), None),
            &[],
            &|point| game.ground_height(point),
            0.0,
            &mut rng::source(animations, generation),
        );
    }

    pub fn actor_event(
        &mut self,
        battle: &crate::playtest::Match,
        index: usize,
        players: Option<&crate::model_animation::Players>,
        audio: &mut crate::source_audio::SourceAudio,
        animations: &mut crate::model_animation::Animations,
    ) {
        let game = &battle.simulation;
        let event = &game.presentation.object_fx_events[index];
        let viewer = battle.observer_player.unwrap_or(0);
        self.begin(battle.generation, game.tick);
        if !actor_visible(event, viewer) {
            return;
        }
        let pose = actor_pose(event);
        let bones = if event.oriented_object {
            players.map_or_else(Vec::new, |players| {
                self.lists[self.actor_fx[&event.effect]]
                    .iter()
                    .enumerate()
                    .filter_map(|(index, nugget)| {
                        let Nugget::AtBones { bone, .. } = nugget else {
                            return None;
                        };
                        let poses = players
                            .effect_bones(bone)
                            .into_iter()
                            .map(|bone| {
                                let offset = pose.rotate(bone.translation);
                                Pose {
                                    position: std::array::from_fn(|axis| {
                                        pose.position[axis] + offset[axis]
                                    }),
                                    axes: bone.axes.map(|axis| pose.rotate(axis)),
                                }
                            })
                            .collect();
                        Some((index, poses))
                    })
                    .collect()
            })
        } else {
            Vec::new()
        };
        self.fire_actor(
            event,
            viewer,
            &bones,
            &|point| game.ground_height(point),
            &mut super::effects::Output {
                random: &mut rng::source(animations, battle.generation),
                sound: &mut |name, position, object| {
                    if object {
                        audio.object_fx_sound(event, name, viewer);
                    } else {
                        audio.fx(name, position);
                    }
                },
                clear: &|position| {
                    matches!(
                        game.visibility_cell(viewer, [position[0], position[1]]),
                        generals_sim::skirmish::visibility::Status::Clear
                    )
                },
            },
        );
    }

    pub fn stop_structure_systems(&mut self, systems: &[u64]) {
        for &system in systems {
            self.destroy(system);
        }
    }

    pub fn structure_particle(
        &mut self,
        battle: &crate::playtest::Match,
        event: &generals_sim::skirmish::object_fx::Event,
        template: &str,
        local: [f32; 3],
        animations: &mut crate::model_animation::Animations,
    ) -> u64 {
        self.begin(battle.generation, event.frame);
        let template = self
            .templates
            .iter()
            .position(|entry| entry.name == template)
            .expect("validated structure particle");
        let parent = Pose {
            position: event.position,
            axes: std::array::from_fn(|column| std::array::from_fn(|row| event.basis[row][column])),
        };
        let local = Pose::at(local);
        let pose = super::model_states::compose(&parent, &local);
        let system = self.create(
            template,
            pose,
            None,
            &mut super::random(animations, battle.generation),
        );
        let emitter = self
            .emitters
            .last_mut()
            .expect("created structure particle");
        emitter.object_attachment = Some((event.object, local));
        system
    }

    pub fn structure_position_event(
        &mut self,
        game: &Skirmish,
        generation: u32,
        event: &generals_sim::skirmish::object_fx::Event,
        viewer: usize,
        animations: &mut crate::model_animation::Animations,
    ) {
        self.begin(generation, game.tick);
        if event.clear_cells & (1 << viewer) == 0
            || event.effect.is_empty()
            || event.effect.eq_ignore_ascii_case("NONE")
        {
            return;
        }
        let index = self.actor_fx[&event.effect];
        let pose = Pose {
            position: event.position,
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        };
        // The ordered Structure dispatcher already submitted ownerless sounds.
        let previous_sounds = self.sounds.len();
        self.fire_from(
            index,
            (&pose, None),
            &[],
            &|point| game.ground_height(point),
            0.0,
            &mut rng::source(animations, generation),
        );
        self.sounds.truncate(previous_sounds);
    }

    fn fire_actor(
        &mut self,
        event: &generals_sim::skirmish::object_fx::Event,
        viewer: usize,
        bones: &[(usize, Vec<Pose>)],
        ground: &impl Fn([f32; 2]) -> f32,
        output: &mut super::effects::Output<'_>,
    ) {
        if !actor_visible(event, viewer) {
            return;
        }
        let pose = actor_pose(event);
        self.fire_ordered(
            self.actor_fx[&event.effect],
            &super::effects::Callback {
                pose: &pose,
                object: event
                    .oriented_object
                    .then_some((event.object, event.primary_geometry_radius)),
                secondary: event.secondary_position,
            },
            bones,
            ground,
            event.override_radius,
            output,
        );
    }
}

const fn actor_visible(event: &generals_sim::skirmish::object_fx::Event, viewer: usize) -> bool {
    (if event.oriented_object {
        event.visible_players
    } else {
        event.clear_cells
    }) & (1 << viewer)
        != 0
}

fn actor_pose(event: &generals_sim::skirmish::object_fx::Event) -> Pose {
    Pose {
        position: event.position,
        axes: if event.oriented_object {
            std::array::from_fn(|column| std::array::from_fn(|row| event.basis[row][column]))
        } else {
            Pose::at([0.0; 3]).axes
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::virtual_assets::Asset;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_damage_fx_armor_sets_load_each_native_minor_and_major_callback() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let types = &battle.simulation.catalog;
        let particles = super::super::load(&assets, types).unwrap();
        let sounds = sounds(&assets, types).unwrap();
        for (unit_name, expected) in [
            ("ChinaVehicleDozer", "FX_DamageTankStruck"),
            ("GLAInfantryWorker", "FX_DamageInfantryStruck"),
        ] {
            let unit = types.iter().find(|unit| unit.name == unit_name).unwrap();
            assert!(
                unit.damage_fx
                    .iter()
                    .filter_map(|set| set.effects.as_ref())
                    .any(|rules| rules
                        .entries
                        .iter()
                        .flatten()
                        .any(|entry| entry.minor == expected || entry.major == expected))
            );
            assert!(particles.actor_fx.contains_key(expected));
            assert!(sounds.contains_key(expected));
        }
        for unit in types {
            for entry in unit
                .damage_fx
                .iter()
                .filter_map(|set| set.effects.as_ref())
                .flat_map(|rules| rules.entries.iter().flatten())
            {
                for effect in [&entry.minor, &entry.major] {
                    if !effect.is_empty() && !effect.eq_ignore_ascii_case("NONE") {
                        assert!(
                            particles.actor_fx.contains_key(effect),
                            "{}: {effect}",
                            unit.name
                        );
                        assert!(sounds.contains_key(effect), "{}: {effect}", unit.name);
                    }
                }
            }
        }
    }

    fn ricochet_fixture() -> (Particles, generals_sim::skirmish::object_fx::Event) {
        let files = [content::scan(Asset {
            path: "fx.ini".into(), origin: "native object ricochet".into(),
            bytes: b"FXList Hit\nParticleSystem\nName=Dust\nRicochet=Yes\nOrientToObject=Yes\nOffset=X:2 Y:0 Z:3\nEnd\nParticleSystem\nName=Dust\nRicochet=Yes\nOffset=X:2 Y:0 Z:3\nEnd\nParticleSystem\nName=Dust\nRicochet=Yes\nOrientToObject=Yes\nAttachToObject=Yes\nOffset=X:2 Y:0 Z:3\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("Dust")]);
        let list = parse_list(
            &files,
            &files[0].declarations[0].lines,
            &particles.templates,
            &mut particles.lists,
            true,
        )
        .unwrap();
        particles
            .actor_fx
            .insert("Hit".into(), particles.lists.len());
        particles.lists.push(list);
        let event = generals_sim::skirmish::object_fx::Event {
            frame: 3,
            object: 8,
            type_index: 2,
            owner: 1,
            position: [10.0, 20.0, 30.0],
            basis: [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: 0.0,
            override_radius: 0.0,
            visible_players: 2,
            clear_cells: 2,
            effect: "Hit".into(),
        };
        (particles, event)
    }

    #[test]
    fn ricochet_uses_secondary_xy_only_and_preserves_parent_attachment_and_rng_order() {
        let identity = Pose::at([0.0; 3]).axes;
        let parent_axes = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
        for (object, secondary, expected_position, expected_axes) in [
            (true, Some([5.0, 20.0, 200.0]), [12.0, 20.0, 33.0], identity),
            (true, None, [7.0, 20.0, 32.0], parent_axes),
            (
                false,
                Some([10.0, 15.0, 200.0]),
                [12.0, 20.0, 33.0],
                identity,
            ),
        ] {
            let (mut particles, mut event) = ricochet_fixture();
            event.oriented_object = object;
            event.secondary_position = secondary;
            let mut draws = Vec::new();
            particles.fire_actor(
                &event,
                1,
                &[],
                &|_| 0.0,
                &mut super::super::effects::Output {
                    random: &mut rng::real_only(|low, high| {
                        draws.push([low, high].map(f32::to_bits));
                        low
                    }),
                    sound: &mut |_, _, _| panic!("no sound nugget"),
                    clear: &|_| true,
                },
            );
            assert_eq!(particles.emitters.len(), 3);
            for emitter in &particles.emitters[..2] {
                assert_eq!(
                    emitter.transform.position.map(f32::to_bits),
                    expected_position.map(f32::to_bits)
                );
            }
            assert_eq!(
                particles.emitters[0]
                    .transform
                    .axes
                    .map(|axis| axis.map(f32::to_bits)),
                expected_axes.map(|axis| axis.map(f32::to_bits))
            );
            assert_eq!(
                particles.emitters[1]
                    .transform
                    .axes
                    .map(|axis| axis.map(f32::to_bits)),
                identity.map(|axis| axis.map(f32::to_bits))
            );
            let attached = &particles.emitters[2];
            if object && secondary.is_some() {
                let (owner, local) = attached.object_attachment.as_ref().unwrap();
                assert_eq!(*owner, event.object);
                assert_eq!(
                    local.position.map(f32::to_bits),
                    [0.0_f32; 3].map(f32::to_bits)
                );
                assert_eq!(
                    attached.transform.position.map(f32::to_bits),
                    event.position.map(f32::to_bits)
                );
                assert_eq!(
                    attached.transform.axes.map(|axis| axis.map(f32::to_bits)),
                    parent_axes.map(|axis| axis.map(f32::to_bits))
                );
            } else if !object {
                assert!(attached.object_attachment.is_none());
            }
            let mut expected_draws = vec![[0.0_f32; 2].map(f32::to_bits); 4];
            expected_draws.push([0.0, std::f32::consts::TAU].map(f32::to_bits));
            assert_eq!(draws, expected_draws.repeat(3));
        }
    }

    #[test]
    fn object_ricochet_at_45_degrees_composes_under_the_actual_pitched_parent() {
        let (mut particles, mut event) = ricochet_fixture();
        event.secondary_position = Some([5.0, 15.0, 200.0]);
        particles.fire_actor(
            &event,
            1,
            &[],
            &|_| 0.0,
            &mut super::super::effects::Output {
                random: &mut rng::real_only(|low, _| low),
                sound: &mut |_, _, _| panic!("no sound nugget"),
                clear: &|_| true,
            },
        );
        let diagonal = std::f32::consts::FRAC_1_SQRT_2;
        let displacement = 2.0 * diagonal;
        let position = [10.0 + displacement, 20.0 + displacement, 33.0];
        let local_axes = [
            [diagonal, diagonal, 0.0],
            [-diagonal, diagonal, 0.0],
            [0.0, 0.0, 1.0],
        ];
        assert_eq!(particles.emitters.len(), 3);
        for emitter in &particles.emitters[..2] {
            assert_eq!(
                emitter.transform.position.map(f32::to_bits),
                position.map(f32::to_bits)
            );
        }
        assert_eq!(
            particles.emitters[0]
                .transform
                .axes
                .map(|axis| axis.map(f32::to_bits)),
            local_axes.map(|axis| axis.map(f32::to_bits))
        );
        assert_eq!(
            particles.emitters[1]
                .transform
                .axes
                .map(|axis| axis.map(f32::to_bits)),
            Pose::at([0.0; 3]).axes.map(|axis| axis.map(f32::to_bits))
        );
        let attached = &particles.emitters[2];
        let (owner, local) = attached.object_attachment.as_ref().unwrap();
        assert_eq!(*owner, event.object);
        assert_eq!(
            local.position.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
        assert_eq!(
            local.axes.map(|axis| axis.map(f32::to_bits)),
            local_axes.map(|axis| axis.map(f32::to_bits))
        );
        assert_eq!(
            attached.transform.position.map(f32::to_bits),
            event.position.map(f32::to_bits)
        );
        let world_axes = [
            [0.0, diagonal, diagonal],
            [0.0, diagonal, -diagonal],
            [-1.0, 0.0, 0.0],
        ];
        assert_eq!(
            attached.transform.axes.map(|axis| axis.map(f32::to_bits)),
            world_axes.map(|axis| axis.map(f32::to_bits))
        );
    }

    fn ordered_bone_fixture() -> (Particles, generals_sim::skirmish::object_fx::Event) {
        let files = [content::scan(Asset {
            path: "fx.ini".into(),
            origin: "FXList object and positional bone callbacks".into(),
            bytes: b"FXList Parent\nSound\nName=Before\nEnd\nLightPulse\nRadius=30\nRadiusAsPercentOfObjectSize=150%\nEnd\nFXListAtBonePos\nFX=Child\nBoneName=FIREPOINT\nEnd\nSound\nName=After\nEnd\nEnd\nFXList Child\nParticleSystem\nName=Dust\nUseCallersRadius=Yes\nEnd\nSound\nName=ChildSound\nEnd\nLightPulse\nRadius=35\nRadiusAsPercentOfObjectSize=200%\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let mut bindings = BTreeMap::new();
        sound_list(&files, "Parent", true, &mut bindings).unwrap();
        assert_eq!(bindings["Parent"], ["Before", "After"]);
        assert_eq!(bindings["Child"], ["ChildSound"]);
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("Dust")]);
        let parent = parse_list(
            &files,
            &files[0].declarations[0].lines,
            &particles.templates,
            &mut particles.lists,
            true,
        )
        .unwrap();
        particles
            .actor_fx
            .insert("Parent".into(), particles.lists.len());
        particles.lists.push(parent);
        let event = generals_sim::skirmish::object_fx::Event {
            frame: 3,
            object: 8,
            type_index: 2,
            owner: 1,
            position: [10.0, 20.0, 30.0],
            basis: generals_sim::skirmish::physical_ocl::transform::yaw(0.0),
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: 100.0,
            override_radius: 17.0,
            visible_players: 2,
            clear_cells: 0,
            effect: "Parent".into(),
        };
        (particles, event)
    }

    #[test]
    fn ordered_object_and_bone_nuggets_keep_sound_context_radius_and_child_shroud() {
        let (mut particles, event) = ordered_bone_fixture();
        let bones = [(
            2,
            vec![
                Pose::at([40.0, 50.0, 60.0]),
                Pose::at([70.0, 80.0, 90.0]),
                Pose::at([100.0, 110.0, 120.0]),
            ],
        )];
        let mut sounds = Vec::new();
        particles.fire_actor(
            &event,
            1,
            &bones,
            &|_| 0.0,
            &mut super::super::effects::Output {
                random: &mut rng::real_only(|low, _| low),
                sound: &mut |name, point, object| sounds.push((name.to_owned(), point, object)),
                clear: &|point| point[0] < 100.0,
            },
        );
        assert_eq!(
            sounds
                .iter()
                .map(|sound| sound.0.as_str())
                .collect::<Vec<_>>(),
            ["Before", "ChildSound", "ChildSound", "After"]
        );
        assert_eq!(
            sounds.iter().map(|sound| sound.2).collect::<Vec<_>>(),
            [true, false, false, true]
        );
        assert_eq!(
            sounds[1].1.map(f32::to_bits),
            [40.0_f32, 50.0, 60.0].map(f32::to_bits)
        );
        assert_eq!(
            sounds[2].1.map(f32::to_bits),
            [70.0_f32, 80.0, 90.0].map(f32::to_bits)
        );
        assert_eq!(particles.emitters.len(), 2);
        assert!(
            particles
                .emitters
                .iter()
                .all(|emitter| emitter.radius.is_none())
        );
        assert_eq!(
            particles
                .pulses
                .lights()
                .iter()
                .map(|light| light.outer_radius.to_bits())
                .collect::<Vec<_>>(),
            [36.0_f32, 36.0, 151.0].map(f32::to_bits)
        );
        assert!(particles.sounds.is_empty());
        sounds.clear();
        particles.fire_actor(
            &event,
            1,
            &[],
            &|_| 0.0,
            &mut super::super::effects::Output {
                random: &mut rng::real_only(|low, _| low),
                sound: &mut |name, _, _| sounds.push((name.to_owned(), [0.0; 3], false)),
                clear: &|_| true,
            },
        );
        assert_eq!(
            sounds
                .iter()
                .map(|sound| sound.0.as_str())
                .collect::<Vec<_>>(),
            ["Before", "After"]
        );
        assert_eq!(
            particles.emitters.len(),
            2,
            "missing bones emit no child systems"
        );
    }

    struct AnimatedDebris {
        app: bevy::prelude::App,
        actor: usize,
        root: bevy::prelude::Entity,
        position: [f32; 3],
        basis: [[f32; 3]; 3],
    }

    fn landing_particles() -> Particles {
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("Dust")]);
        let files = [content::scan(generals_assets::virtual_assets::Asset { path: "fx.ini".into(), origin: "actual debris final callback transform contract".into(), bytes: b"FXList Landing\nParticleSystem\nName=Dust\nOffset=X:2 Y:0 Z:3\nOrientToObject=Yes\nEnd\nEnd\n".to_vec() }).unwrap()];
        let fx = parse_list(
            &files,
            &files[0].declarations[0].lines,
            &particles.templates,
            &mut particles.lists,
            true,
        )
        .unwrap();
        particles
            .actor_fx
            .insert("Landing".into(), particles.lists.len());
        particles.lists.push(fx);
        particles
    }

    fn animated_debris() -> AnimatedDebris {
        use bevy::prelude::*;
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let audio = crate::source_audio::load(
            &assets,
            &battle.simulation.catalog,
            std::path::Path::new(""),
        )
        .unwrap();
        let (debris, mut unit, _) =
            crate::skirmish_data::physical_ocl::tests::installed_outpost(&assets);
        let mut list = crate::skirmish_data::physical_ocl::tests::technical_list(&assets);
        list.nuggets[0].draw.keep_color = false;
        // Grounded draw-transform fixture: retain the original model/animations,
        // but author no launch impulse before the public lifecycle advances.
        list.nuggets[0].force_magnitude = [0.0; 2];
        list.nuggets[0].draw.final_fx = "Landing".into();
        unit.death_lifecycle.as_mut().unwrap().slow[0].objects[0].push(
            generals_sim::skirmish::death_lifecycle::Spawn::Physical(list.clone()),
        );
        if !battle
            .simulation
            .catalog
            .iter()
            .any(|unit| unit.name == debris.name)
        {
            battle.simulation.catalog.push(debris);
        }
        battle.simulation.catalog.push(unit);
        let source = battle
            .simulation
            .entities
            .iter()
            .position(|actor| actor.owner == 0)
            .unwrap();
        let actor = battle
            .simulation
            .create_physical_ocl(source, &list)
            .unwrap()[0];
        battle.simulation.step();
        let (position, basis) = battle.simulation.object_transform(actor);
        let mut models = Vec::new();
        let prepared =
            crate::physical_debris::load(&assets, &battle.simulation.catalog, &mut models).unwrap();
        let mut animations = crate::model_animation::Animations::effect_fixture(19);
        let mut meshes = Assets::<Mesh>::default();
        let geometry = crate::playtest_view::animation_fixture::geometry(&models, &mut meshes);
        animations.retain_models(models, 19);
        let particles = landing_particles();
        let generation = battle.generation;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .insert_resource(meshes)
            .insert_resource(geometry)
            .insert_resource(battle)
            .insert_resource(prepared)
            .insert_resource(animations)
            .insert_resource(audio)
            .insert_resource(particles)
            .insert_resource(crate::model_animation::Clock::new())
            .init_resource::<crate::structure_view::Structures>()
            .add_systems(
                Update,
                (
                    crate::physical_debris::update,
                    crate::physical_debris::animation::update,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                crate::physical_debris::transform
                    .after(bevy::transform::TransformSystems::Propagate),
            );
        let root = app
            .world_mut()
            .spawn((
                crate::playtest_view::UnitVisual {
                    index: actor,
                    generation,
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        AnimatedDebris {
            app,
            actor,
            root,
            position,
            basis,
        }
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn debris_mesh_uses_instance_transform_but_actual_final_callback_keeps_object_pose() {
        use bevy::prelude::*;
        let mut fixture = animated_debris();
        fixture.app.update();
        let original = *fixture
            .app
            .world()
            .get::<GlobalTransform>(fixture.root)
            .unwrap();
        fixture
            .app
            .world_mut()
            .resource_mut::<crate::playtest::Match>()
            .simulation
            .entities[fixture.actor]
            .set_client_instance_matrix([
                [0.0, -1.0, 0.0, 13.0],
                [1.0, 0.0, 0.0, 7.0],
                [0.0, 0.0, 1.0, 5.0],
            ]);
        for _ in 0..4 {
            fixture.app.update();
        }
        let mesh = *fixture
            .app
            .world()
            .get::<GlobalTransform>(fixture.root)
            .unwrap();
        assert_ne!(
            original.to_matrix().to_cols_array().map(f32::to_bits),
            mesh.to_matrix().to_cols_array().map(f32::to_bits)
        );
        let particles = fixture.app.world().resource::<Particles>();
        assert_eq!(particles.emitters.len(), 1);
        let offset = [2.0, 0.0, 3.0];
        let expected: [f32; 3] = std::array::from_fn(|axis| {
            let products: [f32; 3] =
                std::array::from_fn(|column| fixture.basis[axis][column] * offset[column]);
            fixture.position[axis] + ((products[0] + products[1]) + products[2])
        });
        assert_eq!(
            particles.emitters[0].transform.position.map(f32::to_bits),
            expected.map(f32::to_bits)
        );
        assert_eq!(
            particles.emitters[0]
                .transform
                .axes
                .map(|axis| axis.map(f32::to_bits)),
            std::array::from_fn::<_, 3, _>(|column| std::array::from_fn::<_, 3, _>(|row| fixture
                .basis[row][column]
                .to_bits()))
        );
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn debris_final_fx_uses_actual_oriented_position_and_null_source_after_clear_cell_gate() {
        use crate::model_animation::Animations;
        let (_, battle) = crate::cleanup_verification::tests::installed();
        let actor = battle
            .simulation
            .entities
            .iter()
            .position(|actor| actor.owner == 0)
            .unwrap();
        let position = battle.simulation.object_position(actor);
        let basis =
            generals_sim::skirmish::physical_ocl::transform::yaw(std::f32::consts::FRAC_PI_2);
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("Dust")]);
        let files = [content::scan(Asset { path: "fx.ini".into(), origin: "W3DDebrisDraw final doFXPos contract".into(), bytes: b"FXList Landing\nParticleSystem\nName=Dust\nOffset=X:2 Y:0 Z:3\nOrientToObject=Yes\nEnd\nSound\nName=LandingSound\nEnd\nEnd\n".to_vec() }).unwrap()];
        let list = parse_list(
            &files,
            &files[0].declarations[0].lines,
            &particles.templates,
            &mut particles.lists,
            true,
        )
        .unwrap();
        particles
            .actor_fx
            .insert("Landing".into(), particles.lists.len());
        particles.lists.push(list);
        let mut animations = Animations::effect_fixture(19);
        let mut unchanged = Animations::effect_fixture(19);
        particles.debris_final_fx(
            &battle,
            "Landing",
            [-1000.0, -1000.0, 0.0],
            basis,
            &mut animations,
        );
        assert!(particles.emitters.is_empty() && particles.sounds.is_empty());
        assert_eq!(
            animations.effect_random(battle.generation).to_bits(),
            unchanged.effect_random(battle.generation).to_bits()
        );
        particles.debris_final_fx(&battle, "Landing", position, basis, &mut animations);
        assert_eq!(particles.emitters.len(), 1);
        let emitter = &particles.emitters[0];
        assert!(emitter.object_attachment.is_none() && emitter.attachment.is_none());
        assert_eq!(
            emitter.transform.position.map(f32::to_bits),
            [position[0], position[1] + 2.0, position[2] + 3.0].map(f32::to_bits)
        );
        assert_eq!(particles.sounds.len(), 1);
        assert_eq!(particles.sounds[0].0, "LandingSound");
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn tree_position_fx_keeps_sound_and_position_but_shrouded_callbacks_consume_no_rng() {
        use crate::model_animation::Animations;
        use generals_sim::skirmish::visibility::Status;

        let (_, battle) = crate::cleanup_verification::tests::installed();
        let game = &battle.simulation;
        let actor = game
            .entities
            .iter()
            .position(|actor| actor.owner == 0)
            .unwrap();
        let position = game.object_position(actor);
        assert!(matches!(
            game.visibility_cell(0, [position[0], position[1]]),
            Status::Clear
        ));
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("Dust")]);
        let files = [content::scan(Asset {
            path: "fx.ini".into(),
            origin: "tree position FX regression".into(),
            bytes: b"FXList Topple\nParticleSystem\nName=Dust\nOffset=X:2 Y:0 Z:3\nEnd\nSound\nName=ToppleSound\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        let list = parse_list(
            &files,
            &files[0].declarations[0].lines,
            &particles.templates,
            &mut particles.lists,
            true,
        )
        .unwrap();
        particles
            .actor_fx
            .insert("Topple".into(), particles.lists.len());
        particles.lists.push(list);
        let mut animations = Animations::effect_fixture(19);
        let mut unchanged = Animations::effect_fixture(19);
        let hidden = [-1000.0, -1000.0, 0.0];
        assert!(matches!(
            game.visibility_cell(0, [hidden[0], hidden[1]]),
            Status::Shrouded
        ));
        particles.tree_position_event(game, 0, "Topple", hidden, false, &mut animations);
        particles.tree_position_event(game, 0, "NONE", position, true, &mut animations);
        assert!(particles.emitters.is_empty());
        assert!(particles.sounds.is_empty());
        // Native integer RNG takes signed Int bounds; retain a positive,
        // representable inclusive range while checking the unchanged stream.
        for _ in 0..4 {
            assert_eq!(
                animations.effect_integer(0, [0, 0x7fff_fffe]),
                unchanged.effect_integer(0, [0, 0x7fff_fffe])
            );
        }
        particles.tree_position_event(game, 0, "Topple", position, true, &mut animations);
        assert_eq!(particles.emitters.len(), 1);
        assert!(particles.emitters[0].object_attachment.is_none());
        assert_eq!(
            particles.emitters[0].transform.position.map(f32::to_bits),
            [position[0] + 2.0, position[1], position[2] + 3.0].map(f32::to_bits)
        );
        assert_eq!(particles.sounds.len(), 1);
        assert_eq!(particles.sounds[0].0, "ToppleSound");
        assert_eq!(
            particles.sounds[0].1.map(f32::to_bits),
            position.map(f32::to_bits)
        );
    }

    #[test]
    fn callback_pose_and_shroud_are_snapshots_and_sound_is_dispatched_once() {
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("spark")]);
        let files = [content::scan(Asset {
            path: "fx.ini".into(), origin: "native actor FX".into(),
            bytes: b"FXList Converted\nParticleSystem\nName=spark\nOrientToObject=Yes\nOffset=X:2 Y:0 Z:3\nEnd\nSound\nName=Voice\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        particles.lists.push(
            parse_list(
                &files,
                &files[0].declarations[0].lines,
                &particles.templates,
                &mut Vec::new(),
                true,
            )
            .unwrap(),
        );
        particles.actor_fx.insert("Converted".into(), 0);
        particles.sounds.push(("Earlier".into(), [0.0; 3]));
        let event = generals_sim::skirmish::object_fx::Event {
            frame: 3,
            object: 8,
            type_index: 2,
            owner: 1,
            position: [10.0, 20.0, 30.0],
            basis: generals_sim::skirmish::physical_ocl::transform::yaw(
                std::f32::consts::FRAC_PI_2,
            ),
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: 0.0,
            override_radius: 0.0,
            visible_players: 2,
            clear_cells: 2,
            effect: "Converted".into(),
        };
        let mut draws = 0;
        let mut sounds = Vec::new();
        particles.fire_actor(
            &event,
            0,
            &[],
            &|_| 0.0,
            &mut super::super::effects::Output {
                random: &mut rng::real_only(|low, _| {
                    draws += 1;
                    low
                }),
                sound: &mut |name, position, object| {
                    sounds.push((name.to_owned(), position, object));
                },
                clear: &|_| true,
            },
        );
        assert_eq!(draws, 0);
        assert!(particles.emitters.is_empty());
        particles.fire_actor(
            &event,
            1,
            &[],
            &|_| 0.0,
            &mut super::super::effects::Output {
                random: &mut rng::real_only(|low, _| {
                    draws += 1;
                    low
                }),
                sound: &mut |name, position, object| {
                    sounds.push((name.to_owned(), position, object));
                },
                clear: &|_| true,
            },
        );
        assert!(draws > 0);
        assert_eq!(particles.emitters.len(), 1);
        let pose = &particles.emitters[0].transform;
        assert_eq!(
            pose.position.map(f32::to_bits),
            [10.0_f32, 22.0, 33.0].map(f32::to_bits)
        );
        assert_eq!(particles.sounds.len(), 1);
        assert_eq!(particles.sounds[0].0, "Earlier");
        assert_eq!(sounds.len(), 1);
        assert_eq!(sounds[0].0, "Voice");
        assert!(sounds[0].2);
        assert_eq!(
            sounds[0].1.map(f32::to_bits),
            event.position.map(f32::to_bits)
        );
        assert_eq!(pose.axes[0][1].to_bits(), 1.0_f32.to_bits());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn structure_transition_particles_keep_native_attachment_and_graceful_retirement() {
        let (_, battle) = crate::cleanup_verification::tests::installed();
        let mut particles =
            super::super::tests::system(vec![super::super::tests::template("Trail")]);
        let mut animations = crate::model_animation::Animations::effect_fixture(19);
        let snapshot = generals_sim::skirmish::object_fx::Event {
            frame: 3,
            object: 0,
            type_index: 0,
            owner: 0,
            position: [10.0, 20.0, 30.0],
            basis: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: 0.0,
            override_radius: 0.0,
            visible_players: 0,
            clear_cells: 0,
            effect: String::new(),
        };
        let transition = particles.structure_particle(
            &battle,
            &snapshot,
            "Trail",
            [2.0, 0.0, 3.0],
            &mut animations,
        );
        assert_eq!(particles.emitters.len(), 1);
        assert!(!particles.emitters[0].shrouded);
        assert_eq!(
            particles.emitters[0].transform.position.map(f32::to_bits),
            [10.0_f32, 22.0, 33.0].map(f32::to_bits)
        );
        assert_eq!(
            particles.emitters[0].object_attachment.as_ref().unwrap().0,
            0
        );
        particles.stop_structure_systems(&[transition]);
        assert!(particles.emitters[0].lifecycle.destroyed);
    }
}
