//! Installed geometry with controlled pose inputs, not paid gameplay or GPU timing.
use crate::model_animation::*;
use bevy::{asset::AssetPlugin, ecs::system::RunSystemOnce};

struct Part {
    entity: Entity,
    handle: Handle<Mesh>,
    binding: Arc<model_render::PartBinding>,
    reference: Mesh,
    uploaded: Vec<u8>,
}

struct Scene {
    app: App,
    parent: Entity,
    model: usize,
    parts: Vec<Part>,
    changed: usize,
    unchanged_visible: usize,
    unchanged_hidden: usize,
    deferred_hidden: usize,
}

impl Scene {
    fn new(animations: &Animations, model: usize) -> Self {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>();
        let parent = app.world_mut().spawn_empty().id();
        let parts = model_render::parts(&animations.models[model])
            .unwrap()
            .into_iter()
            .map(|part| {
                let reference = part.mesh.clone();
                let binding = Arc::new(part.binding.unwrap());
                let handle = app
                    .world_mut()
                    .resource_mut::<Assets<Mesh>>()
                    .add(part.mesh);
                let entity = app
                    .world_mut()
                    .spawn((
                        AnimatedPart {
                            model,
                            binding: binding.clone(),
                        },
                        Mesh3d(handle.clone()),
                        ChildOf(parent),
                        Visibility::Inherited,
                        Transform::IDENTITY,
                    ))
                    .id();
                Part {
                    entity,
                    handle,
                    binding,
                    uploaded: reference.create_packed_vertex_buffer_data(),
                    reference,
                }
            })
            .collect();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .clear();
        Self {
            app,
            parent,
            model,
            parts,
            changed: 0,
            unchanged_visible: 0,
            unchanged_hidden: 0,
            deferred_hidden: 0,
        }
    }

    fn apply(&mut self, pose: Option<Vec<MeshPose>>, reference: &[MeshPose]) {
        let mut changed = BTreeSet::new();
        for part in &mut self.parts {
            let previous = part.reference.create_packed_vertex_buffer_data();
            let visible =
                model_render::deform(&mut part.reference, &part.binding, reference).unwrap();
            let next = part.reference.create_packed_vertex_buffer_data();
            if previous == next {
                // Count the important case: an updated whole-model pose still
                // leaves some visible chassis/base or hidden subparts unchanged.
                if pose.is_some() {
                    if visible {
                        self.unchanged_visible += 1;
                    } else {
                        self.unchanged_hidden += 1;
                    }
                }
            } else {
                assert!(
                    pose.is_some(),
                    "whole-model pose cache lost changed geometry"
                );
                if !visible {
                    self.deferred_hidden += 1;
                }
            }
            if visible && part.uploaded != next {
                assert!(pose.is_some(), "a reveal must supply the current pose");
                changed.insert(part.handle.id());
                self.changed += 1;
                part.uploaded = next;
            }
        }
        assert_eq!(self.apply_frame(pose), changed);
        self.check_parts(reference);
    }

    fn apply_frame(&mut self, pose: Option<Vec<MeshPose>>) -> BTreeSet<AssetId<Mesh>> {
        let poses = BTreeMap::from([(
            (self.parent, self.model),
            Frame {
                supply: None,
                meshes: pose,
                height_offset: -10.0,
            },
        )]);
        self.app
            .world_mut()
            .run_system_once(
                move |mut meshes: ResMut<Assets<Mesh>>, mut parts: Query<AnimatedMesh>| {
                    apply_poses(&mut meshes, &mut parts, &poses);
                },
            )
            .unwrap();
        self.app.update();
        self.app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .drain()
            .filter_map(|event| match event {
                AssetEvent::Modified { id } => Some(id),
                _ => None,
            })
            .collect()
    }

    fn check_parts(&self, reference: &[MeshPose]) {
        let world = self.app.world();
        for part in &self.parts {
            assert_eq!(
                world.get::<Mesh3d>(part.entity).unwrap().0.id(),
                part.handle.id()
            );
            let actual = world.resource::<Assets<Mesh>>().get(&part.handle).unwrap();
            assert_eq!(actual.create_packed_vertex_buffer_data(), part.uploaded);
            if reference[part.binding.mesh].visible {
                assert_eq!(
                    part.uploaded,
                    part.reference.create_packed_vertex_buffer_data()
                );
            }
            assert_eq!(
                *world.get::<Visibility>(part.entity).unwrap(),
                if reference[part.binding.mesh].visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            );
            assert_eq!(
                world
                    .get::<Transform>(part.entity)
                    .unwrap()
                    .translation
                    .y
                    .to_bits(),
                (-1.0_f32).to_bits()
            );
        }
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_hidden_turret_defers_motion_uploads_until_its_first_visible_pose() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let mut animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    animations.retain_models(models, 123);
    let kind =
        crate::strategy_verification::template(&battle.simulation, "GLATankScorpion").unwrap();
    let unit = &mut battle.simulation.entities[0];
    unit.type_index = kind;
    unit.ai.turret.angle = 0.0;
    unit.ai.turret.pitch = 0.0;
    let draw = &animations.draws[kind][0];
    let ordinary = draw.select(&BTreeSet::new());
    let upgraded = draw.select(&BTreeSet::from(["WEAPONSET_CRATEUPGRADE_ONE".into()]));
    assert_ne!(ordinary, upgraded);
    let model = draw.states[ordinary].render_model.unwrap();
    assert_eq!(draw.states[upgraded].render_model, Some(model));
    let mut scene = Scene::new(&animations, model);
    let mut player = Player::new(unit.position, 0);
    player.state = ordinary;
    let pose = animations.pose(&mut player, unit, 0, 0, None);
    let reference = animations.uncached_pose(&player, unit, 0).unwrap();
    scene.apply(pose, &reference);
    player.model = Some(model);
    player.owner = Some(unit.owner);
    let hidden = scene
        .parts
        .iter()
        .position(|part| {
            animations.models[model].meshes[part.binding.mesh]
                .name
                .rsplit('.')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("TurretUP01"))
        })
        .expect("original hidden upgraded Scorpion turret");
    assert!(!reference[scene.parts[hidden].binding.mesh].visible);
    let retained = scene.parts[hidden].uploaded.clone();
    for (tick, angle) in [(1, 0.35), (2, -0.6), (3, 1.2)] {
        unit.ai.turret.angle = angle;
        let pose = animations.pose(&mut player, unit, 0, tick, None);
        assert!(pose.is_some());
        scene.apply(pose, &animations.uncached_pose(&player, unit, 0).unwrap());
        assert_eq!(scene.parts[hidden].uploaded, retained);
    }
    assert!(scene.deferred_hidden > 0);
    assert_ne!(
        scene.parts[hidden]
            .reference
            .create_packed_vertex_buffer_data(),
        retained
    );
    // A cached frame retains the hidden state; it cannot reveal stale geometry.
    let pose = animations.pose(&mut player, unit, 0, 3, None);
    assert!(pose.is_none());
    scene.apply(pose, &animations.uncached_pose(&player, unit, 0).unwrap());
    player.state = upgraded;
    let pose = animations.pose(&mut player, unit, 0, 3, None);
    assert!(
        pose.is_some(),
        "native state change invalidates the pose cache"
    );
    let reference = animations.uncached_pose(&player, unit, 0).unwrap();
    assert!(reference[scene.parts[hidden].binding.mesh].visible);
    scene.apply(pose, &reference);
    assert_ne!(scene.parts[hidden].uploaded, retained);
    // Repeated first-visible frame stays exact without another Modified event.
    let pose = animations.pose(&mut player, unit, 0, 3, None);
    assert!(pose.is_none());
    scene.apply(pose, &reference);
}

#[ignore = "requires original game installation"]
#[test]
fn hidden_invalid_binding_is_rejected_before_visibility_or_mesh_changes() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let mut animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    animations.retain_models(models, 123);
    let kind =
        crate::strategy_verification::template(&battle.simulation, "GLATankScorpion").unwrap();
    let draw = &animations.draws[kind][0];
    let model = draw.states[draw.select(&BTreeSet::new())]
        .render_model
        .unwrap();
    let mut scene = Scene::new(&animations, model);
    let invalid = animations.models[model]
        .meshes
        .iter()
        .map(|_| MeshPose {
            bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
            positions: Vec::new(),
            normals: None,
            visible: false,
        })
        .collect();
    assert!(scene.apply_frame(Some(invalid)).is_empty());
    for part in &scene.parts {
        assert_eq!(
            *scene.app.world().get::<Visibility>(part.entity).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            scene
                .app
                .world()
                .resource::<Assets<Mesh>>()
                .get(&part.handle)
                .unwrap()
                .create_packed_vertex_buffer_data(),
            part.uploaded
        );
    }
}

#[ignore = "requires original game installation"]
#[test]
fn installed_tank_and_defenses_upload_only_changed_parts_with_exact_uncached_geometry() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let mut models = Vec::new();
    let mut animations = load(&assets, &mut models, &battle.simulation.catalog).unwrap();
    animations.retain_models(models, 123);
    for name in [
        "AmericaTankCrusader",
        "AmericaFireBase",
        "AmericaPatriotBattery",
        "GLATankScorpion",
    ] {
        let kind = crate::strategy_verification::template(&battle.simulation, name).unwrap();
        let unit = &mut battle.simulation.entities[0];
        unit.type_index = kind;
        unit.ai.turret.angle = 0.0;
        unit.ai.turret.pitch = 0.0;
        let draw = &animations.draws[kind][0];
        let mut player = Player::new(unit.position, 0);
        player.state = draw.select(&BTreeSet::new());
        let model = draw.states[player.state].render_model.unwrap();
        let mut scene = Scene::new(&animations, model);
        // Original turret, barrel-recoil and flash consumers run on every input.
        // Alternating the clip sample also exercises unchanged subparts of a
        // changed overall sample instead of relying on the whole-pose cache.
        for tick in 0..80 {
            match tick {
                1 => unit.ai.turret.angle = 0.35,
                15 => unit.ai.turret.pitch = 0.15,
                31 => unit.ai.turret.angle = -0.2,
                _ => {}
            }
            player.frame = if tick % 2 == 0 { 0.0 } else { 0.5 };
            let fired = matches!(tick, 2 | 32).then_some((0, 0));
            let pose = animations.pose(&mut player, unit, 0, tick, fired);
            let reference = animations.uncached_pose(&player, unit, 0).unwrap();
            if name == "GLATankScorpion" && tick == 0 {
                // UVLiteTank retains the same model and Turret keys across
                // native rack/turret upgrades. Its prepared union must retain
                // geometry hidden by this original unupgraded state.
                let source = &animations.models[draw.states[player.state].model.unwrap()];
                assert!(scene.parts.iter().any(|part| {
                    let index = part.binding.mesh;
                    source.meshes[index].attributes & 0x1000 != 0 && !reference[index].visible
                }));
            }
            scene.apply(pose, &reference);
            player.model = Some(model);
            player.owner = Some(unit.owner);
        }
        assert!(
            scene.changed > 0,
            "{name} must really deform turret/barrel geometry"
        );
        assert!(
            scene.unchanged_visible > 20,
            "{name} must retain unchanged visible base/chassis parts"
        );
        if name == "GLATankScorpion" {
            assert!(
                scene.unchanged_hidden > 20,
                "original hidden rack/turret parts must avoid unchanged geometry uploads"
            );
        }
        println!(
            "{name}: changed={}, unchanged visible={}, unchanged hidden={}",
            scene.changed, scene.unchanged_visible, scene.unchanged_hidden
        );
        // Same tick and pose keep all buffers and visibility unchanged.
        let pose = animations.pose(&mut player, unit, 0, 79, None);
        assert!(pose.is_none());
        scene.apply(pose, &animations.uncached_pose(&player, unit, 0).unwrap());
    }
}
