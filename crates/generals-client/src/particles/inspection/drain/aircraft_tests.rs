//! Actual paid Raptor impact and the original finite impact dependency lifetime.
use super::*;
use crate::{aircraft_verification::crash, particles::effects::rng};
use generals_assets::w3d::Model;

// Inspect the original finite impact systems after their actual creation.
fn raptor_impact_complete(
    particles: &Particles,
    generation: u32,
    ground_frame: u64,
) -> Result<bool, Box<dyn Error>> {
    if particles.generation != generation
        || particles.frame.is_none_or(|frame| frame <= ground_frame)
    {
        return Ok(false);
    }
    let list = *particles
        .actor_fx
        .get("FX_JetDeathHitGround")
        .ok_or("original Raptor impact FX binding missing")?;
    let mut checked = false;
    let mut retired = true;
    for nugget in &particles.lists[list] {
        match nugget {
            Nugget::Particle { template, .. } => {
                // Reuse the finite closure validation, not its conservative
                // deadline: readiness follows actual source system retirement.
                system_bound(particles, *template, &mut Vec::new())?;
                retired &= system_retired(particles, *template);
                checked = true;
            }
            Nugget::AtBones { .. } => {
                return Err("original Raptor impact unexpectedly uses bone traversal".into());
            }
            _ => {}
        }
    }
    if !checked {
        return Err("original Raptor impact has no particle systems".into());
    }
    Ok(retired)
}

fn system_retired(particles: &Particles, index: usize) -> bool {
    // Empty and delayed emitters must finish too: either can still emit later.
    !particles
        .emitters
        .iter()
        .any(|emitter| emitter.template == index)
        && [
            particles.templates[index].slave,
            particles.templates[index].attached,
        ]
        .into_iter()
        .flatten()
        .all(|child| system_retired(particles, child))
}

#[ignore = "requires original game installation"]
#[test]
fn original_impact_retirement_waits_for_descendants_and_empty_delayed_systems() {
    let root = &generals_assets::game_root().unwrap();
    let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let names = std::iter::once("FX_JetDeathHitGround".to_owned()).collect();
    let mut particles = crate::particles::load_with_position_effects(&assets, &[], &names).unwrap();
    let list = particles.actor_fx["FX_JetDeathHitGround"];
    let direct: Vec<_> = particles.lists[list]
        .iter()
        .filter_map(|nugget| {
            let Nugget::Particle { template, .. } = nugget else {
                return None;
            };
            Some(particles.templates[*template].name.as_str())
        })
        .collect();
    assert_eq!(
        direct,
        [
            "JetExplosionGroundDebris",
            "JetExplosionGroundDust",
            "JetExplosionGround"
        ]
    );
    let ground = particles
        .templates
        .iter()
        .position(|t| t.name == "JetExplosionGround")
        .unwrap();
    let smoke = particles.templates[ground].attached.unwrap();
    assert_eq!(particles.templates[smoke].name, "JetExplosionSmoke");
    let frame = 100;
    particles.begin(1, frame);
    particles.fire(
        list,
        &crate::particles::Pose::at([100.0; 3]),
        &[],
        &|_| 0.0,
        0.0,
        &mut rng::real_only(|_, high| high),
    );
    assert!(!raptor_impact_complete(&particles, 1, frame).unwrap());
    particles.begin(1, frame + 1);
    // Creation left all direct emitters empty; they still have pending emission.
    assert!(
        particles
            .emitters
            .iter()
            .all(|emitter| emitter.particles.is_empty())
    );
    assert!(!raptor_impact_complete(&particles, 1, frame).unwrap());
    let mut flash_before_impact = false;
    let mut smoke_after_flash = false;
    let mut finished = None;
    for now in frame + 1..frame + 240 {
        particles.begin(1, now);
        particles.logic(now, &mut |_, high| high, &|_| 0.0);
        let impact = raptor_impact_complete(&particles, 1, frame).unwrap();
        if complete(&particles, "FX_JetDeathHitGround", 1).unwrap() && !impact {
            flash_before_impact = true;
            smoke_after_flash |= particles
                .emitters
                .iter()
                .any(|emitter| emitter.template == smoke);
            assert!(particles.emitters.iter().any(|emitter| {
                emitter.template == smoke
                    || particles.templates[emitter.template].name == "JetExplosionGroundDust"
                    || particles.templates[emitter.template].name == "JetExplosionGroundDebris"
            }));
        }
        if impact {
            finished = Some(now);
            break;
        }
    }
    assert!(flash_before_impact && smoke_after_flash);
    assert!(finished.is_some());
    assert!(!raptor_impact_complete(&particles, 2, frame).unwrap());
    assert!(!raptor_impact_complete(&particles, 1, finished.unwrap()).unwrap());
    let unrelated = particles
        .templates
        .iter()
        .position(|t| t.name == "SmokeFactionSmall")
        .unwrap();
    particles.create(
        unrelated,
        crate::particles::Pose::at([100.0; 3]),
        None,
        &mut |_, high| high,
    );
    assert!(raptor_impact_complete(&particles, 1, frame).unwrap());
}

fn exposed_hulk_vertices(
    game: &generals_sim::skirmish::Skirmish,
    id: usize,
    model: &Model,
) -> (usize, usize, f32) {
    let (origin, basis) = game.object_transform(id);
    let mut above = 0;
    let mut total = 0;
    let mut highest = f32::NEG_INFINITY;
    for mesh in &model.meshes {
        if mesh.attributes & 0x1000 != 0 {
            continue;
        }
        for &index in &mesh.indices {
            let point = mesh.positions[usize::try_from(index).unwrap()];
            let world: [f32; 3] = std::array::from_fn(|axis| {
                (0..3).fold(origin[axis], |value, column| {
                    basis[axis][column].mul_add(point[column], value)
                })
            });
            let height = world[2] - game.ground_height([world[0], world[1]]);
            above += usize::from(height > 0.0);
            total += 1;
            highest = highest.max(height);
        }
    }
    (above, total, highest)
}

struct Impact {
    frame: u64,
    position: [f32; 3],
    origins: [[f32; 3]; 3],
}

fn record_impact(
    particles: &mut Particles,
    battle: &crate::playtest::Match,
    source: usize,
    audio: &mut crate::source_audio::SourceAudio,
    animations: &mut crate::model_animation::Animations,
) -> Option<Impact> {
    let mut impact = None;
    for index in 0..battle.simulation.presentation.object_fx_events.len() {
        let event = &battle.simulation.presentation.object_fx_events[index];
        if event.object == source && event.effect == "FX_JetDeathHitGround" {
            // Retain real emitter origins immediately after the actual callback,
            // before source updates can move or retire these direct systems.
            particles.actor_event(battle, index, None, audio, animations);
            let origins = [
                "JetExplosionGroundDebris",
                "JetExplosionGroundDust",
                "JetExplosionGround",
            ]
            .map(|name| {
                particles
                    .emitters
                    .iter()
                    .find(|emitter| particles.templates[emitter.template].name == name)
                    .expect("actual impact must create each original direct system")
                    .transform
                    .position
            });
            assert!(
                impact
                    .replace(Impact {
                        frame: event.frame,
                        position: event.position,
                        origins,
                    })
                    .is_none()
            );
        }
    }
    impact
}

#[ignore = "requires original game installation"]
#[test]
fn paid_raptor_hulk_is_exposed_after_flash_before_impact_retirement() {
    let (assets, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let setup = crash::prepare(&mut battle.simulation).unwrap();
    let mut particles = crate::particles::load(&assets, &battle.simulation.catalog).unwrap();
    let root = &generals_assets::game_root().unwrap();
    let mut audio = crate::source_audio::load(
        &assets,
        &battle.simulation.catalog,
        &root.join("UserData/Options.ini"),
    )
    .unwrap();
    let model = Model::parse(&assets.read("art/w3d/avraptor_d1.w3d").unwrap().bytes).unwrap();
    crash::begin(&mut battle.simulation, &setup).unwrap();
    let mut impact = None;
    let mut flash_frame = None;
    let mut observed = None;
    let mut exposed = None;
    for _ in 0..7200 {
        if impact.is_none() {
            crash::attack_when_visible(&mut battle.simulation, &setup).unwrap();
        }
        battle.simulation.step();
        let now = battle.simulation.tick;
        particles.begin(battle.generation, now);
        if let Some(event) = record_impact(
            &mut particles,
            &battle,
            setup.aircraft.raptor,
            &mut audio,
            &mut animations,
        ) {
            assert!(impact.replace(event).is_none());
        }
        particles.logic(
            now,
            &mut crate::particles::random(&mut animations, battle.generation),
            &|point| battle.simulation.ground_height(point),
        );
        let Some(impact) = &impact else {
            continue;
        };
        let frame = impact.frame;
        let Some(hulk) = battle.simulation.entities.iter().position(|actor| {
            actor.producer() == Some(setup.aircraft.raptor)
                && actor.physical_present()
                && battle.simulation.catalog[actor.type_index].name == "AmericaJetRaptorHulk"
        }) else {
            assert!(
                observed.is_none(),
                "original hulk deleted before inspection"
            );
            continue;
        };
        let (above, total, highest) = exposed_hulk_vertices(&battle.simulation, hulk, &model);
        if above > 0 {
            let interval = exposed.get_or_insert((now, now));
            interval.1 = now;
        }
        if flash_frame.is_none()
            && complete(&particles, "FX_JetDeathHitGround", battle.generation).unwrap()
        {
            assert!(
                above > 0 && above < total,
                "after-flash wreck must intersect terrain"
            );
            assert!(highest > 0.0);
            verify_impact_view(&battle.simulation, hulk, &particles, impact);
            flash_frame = Some(now);
        }
        if !raptor_impact_complete(&particles, battle.generation, frame).unwrap() {
            continue;
        }
        let (first, last) = exposed.expect("actual source wreck must emerge above terrain");
        let flash = flash_frame.expect("direct additive flash must finish first");
        assert!(first <= flash && flash <= last && last < now);
        assert_eq!(
            above, 0,
            "full impact retirement is too late for this actual wreck"
        );
        println!(
            "Paid hulk source window: exposed={}..={} flash={} impact_retired={} vertices={above}/{total} highest={highest} position={:?}",
            first - frame,
            last - frame,
            flash - frame,
            now - frame,
            battle.simulation.object_position(hulk)
        );
        observed = Some(hulk);
        break;
    }
    let hulk = observed.expect("paid original crash must reach its natural impact retirement");
    verify_hulk_deletion(&mut battle.simulation, hulk);
}

fn verify_hulk_deletion(game: &mut generals_sim::skirmish::Skirmish, hulk: usize) {
    let position = game.object_position(hulk);
    let mut sank = false;
    for _ in 0..300 {
        game.step();
        if !game.entities[hulk].physical_present() {
            break;
        }
        sank |= game.object_position(hulk)[2] < position[2];
    }
    assert!(
        sank,
        "native sinking must continue without capture mutation"
    );
    assert!(!game.entities[hulk].physical_present());
}

fn verify_impact_view(
    game: &generals_sim::skirmish::Skirmish,
    hulk: usize,
    particles: &Particles,
    impact: &Impact,
) {
    let position = game.object_transform(hulk).0;
    let yaw = crate::aircraft_crash_capture::test_impact_view_yaw(position, impact.position);
    let view = [yaw.sin(), -yaw.cos()];
    let toward_impact = [
        impact.position[0] - position[0],
        impact.position[1] - position[1],
    ];
    let length = toward_impact[0].hypot(toward_impact[1]);
    assert!(
        length > 0.0,
        "actual hulk must move beyond the ground callback"
    );
    let alignment = view[0].mul_add(toward_impact[0], view[1] * toward_impact[1]) / length;
    assert!(
        alignment < -0.99,
        "capture must look away from the actual impact: {alignment}"
    );
    for origin in impact.origins {
        let delta = [origin[0] - position[0], origin[1] - position[1]];
        let separation = view[0].mul_add(delta[0], view[1] * delta[1]);
        assert!(
            separation < 0.0,
            "actual emitted origin must lie behind the exposed hulk: {origin:?}, {position:?}, {separation}"
        );
        println!(
            "Actual impact origin camera depth: separation={separation} alignment={alignment} origin={origin:?} hulk={position:?}"
        );
    }
    let list = particles.actor_fx["FX_JetDeathHitGround"];
    let mut count = 0;
    for nugget in &particles.lists[list] {
        let Nugget::Particle {
            oriented, offset, ..
        } = nugget
        else {
            continue;
        };
        assert!(*oriented);
        assert_eq!(offset[0].to_bits(), 6.0_f32.to_bits());
        assert_eq!(offset[1].to_bits(), 0.0_f32.to_bits());
        count += 1;
    }
    assert_eq!(count, 3);
}
