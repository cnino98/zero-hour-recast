//! `W3DModelDraw` barrel lookup, recoil geometry and flash lifecycle.
use crate::skirmish_data::{field, finite_number, scalar};
use generals_assets::{
    content::ContentLine,
    w3d::{BoneControl, MeshPose, Model},
};
use std::error::Error;

pub(super) struct Rules {
    initial: f32,
    maximum: f32,
    damping: f32,
    settle: f32,
    pub projectile_slots: u8,
}

impl Rules {
    pub const fn new() -> Self {
        // W3DModelDrawModuleData constructor values are already per frame.
        Self {
            initial: 2.0,
            maximum: 3.0,
            damping: 0.4,
            settle: 0.065,
            projectile_slots: 0,
        }
    }

    pub fn parse(lines: &[ContentLine]) -> Result<Self, Box<dyn Error>> {
        let mut rules = Self::new();
        for (name, value, factor) in [
            ("InitialRecoilSpeed", &mut rules.initial, 1.0 / 30.0),
            ("MaxRecoilDistance", &mut rules.maximum, 1.0),
            ("RecoilDamping", &mut rules.damping, 1.0),
            ("RecoilSettleSpeed", &mut rules.settle, 1.0 / 30.0),
        ] {
            if let Some(line) = field(lines, 1, name) {
                *value = finite_number(scalar(line)?)? * factor;
            }
        }
        if let Some(line) = field(lines, 1, "ProjectileBoneFeedbackEnabledSlots") {
            for name in &line.tokens[1..] {
                rules.projectile_slots |= match name.as_str() {
                    "PRIMARY" => 1,
                    "SECONDARY" => 2,
                    "TERTIARY" => 4,
                    _ => return Err(format!("unknown projectile feedback slot {name}").into()),
                };
            }
        }
        Ok(rules)
    }
}

enum Phase {
    Idle,
    Start,
    Settle,
}

struct Barrel {
    mesh: Option<usize>,
    recoil: bool,
    recoil_bone: Option<String>,
    phase: Phase,
    shift: f32,
    rate: f32,
    visible: bool,
}

impl Barrel {
    const fn advance(&mut self, rules: &Rules, fired: bool) {
        if fired {
            self.phase = Phase::Start;
            self.rate = rules.initial;
        }
        // handleClientRecoil sets visibility before changing shift or state.
        self.visible = matches!(self.phase, Phase::Start);
        if !self.recoil {
            self.phase = Phase::Idle;
            return;
        }
        match self.phase {
            Phase::Idle => {}
            Phase::Start => {
                self.shift += self.rate;
                self.rate *= rules.damping;
                if self.shift >= rules.maximum {
                    self.shift = rules.maximum;
                    self.phase = Phase::Settle;
                } else if self.rate.abs() < 0.01 {
                    self.phase = Phase::Settle;
                }
            }
            Phase::Settle => {
                self.shift -= rules.settle;
                if self.shift <= 0.0 {
                    self.shift = 0.0;
                    self.phase = Phase::Idle;
                }
            }
        }
    }
}

pub(super) struct Player {
    state: usize,
    tick: Option<u64>,
    barrels: [Vec<Barrel>; 3],
}

impl Player {
    pub const fn new() -> Self {
        Self {
            state: usize::MAX,
            tick: None,
            barrels: [Vec::new(), Vec::new(), Vec::new()],
        }
    }

    pub fn update(
        &mut self,
        state: usize,
        fields: &[Vec<String>],
        model: &Model,
        rules: &Rules,
        tick: u64,
        fired: Option<(usize, usize)>,
    ) -> bool {
        let mut changed = self.state != state;
        if changed {
            self.state = state;
            self.barrels =
                ["PRIMARY", "SECONDARY", "TERTIARY"].map(|slot| bindings(fields, model, slot));
        }
        let first = self.tick.map_or(tick, |previous| previous + 1);
        for frame in first..=tick {
            for (slot, barrels) in self.barrels.iter_mut().enumerate() {
                for (index, barrel) in barrels.iter_mut().enumerate() {
                    let previous = (barrel.shift.to_bits(), barrel.visible);
                    barrel.advance(rules, frame == tick && fired == Some((slot, index)));
                    changed |= previous != (barrel.shift.to_bits(), barrel.visible);
                }
            }
        }
        self.tick = Some(tick);
        changed
    }

    pub fn controls(&self, controls: &mut Vec<BoneControl>) {
        for barrel in self.barrels.iter().flatten() {
            if let Some(name) = &barrel.recoil_bone {
                controls.push(BoneControl {
                    name: name.clone(),
                    axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    translation: [-barrel.shift, 0.0, 0.0],
                });
            }
        }
    }

    pub fn apply(&self, pose: &mut [MeshPose]) {
        for barrel in self.barrels.iter().flatten() {
            if let Some(index) = barrel.mesh {
                pose[index].visible &= barrel.visible;
            }
        }
    }
}

/// `W3DModelDraw::doHideShowProjectileObjects` hides from the front of the clip.
pub(super) fn projectile_visibility(
    fields: &[Vec<String>],
    clips: [(u32, u32); 3],
    slots: u8,
) -> Vec<(String, bool)> {
    let mut result = Vec::new();
    for (slot, name) in ["PRIMARY", "SECONDARY", "TERTIARY"].into_iter().enumerate() {
        let (remaining, maximum) = clips[slot];
        if slots & (1 << slot) == 0 || maximum < remaining {
            continue;
        }
        let value = |key| {
            fields.iter().rev().find_map(|tokens| {
                (tokens.first().is_some_and(|name| name == key)
                    && tokens.get(1).is_some_and(|value| value == name))
                .then(|| tokens.get(2))
                .flatten()
            })
        };
        let hidden = maximum - remaining;
        if let Some(name) = value("WeaponHideShowBone") {
            result.push((name.clone(), hidden == 0));
        } else if let Some(name) = value("WeaponLaunchBone") {
            for index in 0..maximum {
                result.push((format!("{name}{:02}", index + 1), index >= hidden));
            }
        }
    }
    result
}

fn names<'a>(fields: &'a [Vec<String>], slot: &str) -> [Option<&'a str>; 4] {
    [
        "WeaponFireFXBone",
        "WeaponRecoilBone",
        "WeaponMuzzleFlash",
        "WeaponLaunchBone",
    ]
    .map(|key| {
        fields.iter().rev().find_map(|tokens| {
            (tokens.first().is_some_and(|token| token == key)
                && tokens.get(1).is_some_and(|token| token == slot))
            .then(|| tokens.get(2).map(String::as_str))
            .flatten()
        })
    })
}

fn bindings(fields: &[Vec<String>], model: &Model, slot: &str) -> Vec<Barrel> {
    barrels(names(fields, slot), model)
        .into_iter()
        .map(|bones| Barrel {
            // Native Get_Sub_Object_On_Bone(0, bone): first attached subobject.
            mesh: bones.flash.and_then(|bone| {
                model
                    .meshes
                    .iter()
                    .enumerate()
                    .filter(|(_, mesh)| {
                        mesh.bone_name
                            .as_ref()
                            .is_some_and(|name| name.eq_ignore_ascii_case(&bone))
                    })
                    .min_by_key(|(_, mesh)| mesh.subobject_order)
                    .map(|(index, _)| index)
            }),
            recoil: bones.recoil,
            recoil_bone: bones.recoil_bone,
            phase: Phase::Idle,
            shift: 0.0,
            rate: 0.0,
            visible: false,
        })
        .collect()
}

struct Bones {
    recoil: bool,
    recoil_bone: Option<String>,
    flash: Option<String>,
    fire: Option<String>,
}

pub(super) fn fire_bone(
    fields: &[Vec<String>],
    model: &Model,
    slot: usize,
    barrel: usize,
) -> Option<String> {
    barrels(
        names(fields, ["PRIMARY", "SECONDARY", "TERTIARY"][slot]),
        model,
    )
    .into_iter()
    .nth(barrel)?
    .fire
}

fn barrels(names: [Option<&str>; 4], model: &Model) -> Vec<Bones> {
    let public = names.map(|name| name.map(|name| model.public_bone_indices(name)));
    let find = |role: usize, name: &str, numbered: bool| {
        let index = *public[role].as_ref()?.get(&name.to_ascii_lowercase())?;
        (index != 0 || (!numbered && role == 3)).then(|| model.bone_names[index].clone())
    };
    let mut result = Vec::new();
    let mut previous_fire = None;
    for index in 1..=99 {
        let mut found: [Option<String>; 4] = std::array::from_fn(|role| {
            names[role].and_then(|name| find(role, &format!("{name}{index:02}"), true))
        });
        // validateWeaponBarrelInfo reuses the preceding FX bone only when
        // this barrel has a muzzle flash and the FX prefix is authored.
        if names[0].is_some() && found[0].is_none() && found[2].is_some() {
            found[0].clone_from(&previous_fire);
        }
        if found.iter().all(Option::is_none) {
            break;
        }
        previous_fire.clone_from(&found[0]);
        result.push(Bones {
            recoil: found[1].is_some(),
            recoil_bone: found[1].clone(),
            flash: found[2].clone(),
            fire: found[0].clone(),
        });
    }
    if result.is_empty() {
        let found: [Option<String>; 4] =
            std::array::from_fn(|role| names[role].and_then(|name| find(role, name, false)));
        if found.iter().any(Option::is_some) {
            result.push(Bones {
                recoil: found[1].is_some(),
                recoil_bone: found[1].clone(),
                flash: found[2].clone(),
                fire: found[0].clone(),
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{Barrel, Phase, Rules, barrels};
    use generals_assets::{
        content,
        virtual_assets::Asset,
        w3d::{Bounds, Mesh, MeshPose, Model},
    };

    #[test]
    fn velocity_overrides_convert_to_frames_but_constructor_defaults_do_not() {
        let file = content::scan(Asset {
            path: "test.ini".into(), origin: "fixture".into(),
            bytes: b"Object Test\nDraw=W3DModelDraw Module\nInitialRecoilSpeed=60\nMaxRecoilDistance=4\nRecoilDamping=0.5\nRecoilSettleSpeed=3\nEnd\nEnd".to_vec(),
        }).unwrap();
        let rules = Rules::parse(&file.declarations[0].lines).unwrap();
        assert_eq!(rules.initial.to_bits(), 2.0_f32.to_bits());
        assert_eq!(rules.settle.to_bits(), (3.0_f32 * (1.0 / 30.0)).to_bits());
        assert_eq!(
            Rules::parse(&[]).unwrap().settle.to_bits(),
            0.065_f32.to_bits()
        );
    }

    #[test]
    fn flash_clock_preserves_body_visibility_and_does_not_repeat_within_a_tick() {
        let names = ["ROOT", "Flash01", "Flash02"];
        let mut model = Model {
            hlod: true,
            emitter: None,
            scene_sphere: None,
            scene_bounds: generals_assets::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            meshes: names
                .iter()
                .map(|bone| Mesh {
                    name: format!("RIFLE.{bone}"),
                    bone_name: Some((*bone).into()),
                    attributes: 0,
                    sort_level: 0,
                    subobject_order: 0,
                    bounds: generals_assets::w3d::Bounds {
                        min: [0.0; 3],
                        max: [0.0; 3],
                    },
                    sphere_center: [0.0; 3],
                    sphere_radius: 1.0,
                    bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
                    positions: Vec::new(),
                    normals: None,
                    indices: Vec::new(),
                    materials: None,
                    deformation: None,
                })
                .collect(),
            bone_names: names.map(String::from).to_vec(),
            bounds: Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            hierarchy: None,
        };
        let fields = vec![vec![
            "WeaponMuzzleFlash".into(),
            "PRIMARY".into(),
            "Flash".into(),
        ]];
        let mut player = super::Player::new();
        for (tick, shot, expected) in [
            (0, None, [true, false, false]),
            (1, Some((0, 1)), [true, false, true]),
            (1, Some((0, 1)), [true, false, true]),
            (2, None, [true, false, false]),
        ] {
            player.update(0, &fields, &model, &Rules::new(), tick, shot);
            let mut pose = (0..3)
                .map(|_| MeshPose {
                    bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
                    positions: Vec::new(),
                    normals: None,
                    visible: true,
                })
                .collect::<Vec<_>>();
            player.apply(&mut pose);
            assert_eq!(
                pose.iter().map(|mesh| mesh.visible).collect::<Vec<_>>(),
                expected
            );
        }
        // A chunk appearing first in the W3D file need not be the first HLOD
        // child on a bone. Only that native child receives muzzle visibility.
        model.meshes[0].bone_name = Some("Flash01".into());
        model.meshes[0].subobject_order = 2;
        model.meshes[1].subobject_order = 0;
        let barrels = super::bindings(&fields, &model, "PRIMARY");
        assert_eq!(barrels[0].mesh, Some(1));
    }

    #[test]
    fn numbered_roles_preserve_barrel_indices_before_unnumbered_fallback() {
        let bones = ["ROOT", "FIRE01", "FIRE02", "FLASH01", "FLASH02", "FLASH"].map(String::from);
        let mut model = Model {
            hlod: true,
            emitter: None,
            scene_sphere: None,
            scene_bounds: generals_assets::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            meshes: Vec::new(),
            bone_names: bones.to_vec(),
            bounds: Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            hierarchy: None,
        };
        let numbered = barrels([Some("FIRE"), None, Some("FLASH"), None], &model);
        assert_eq!(
            numbered
                .iter()
                .map(|bones| bones.flash.as_deref())
                .collect::<Vec<_>>(),
            [Some("FLASH01"), Some("FLASH02")]
        );
        assert_eq!(
            numbered
                .iter()
                .map(|bones| bones.fire.as_deref())
                .collect::<Vec<_>>(),
            [Some("FIRE01"), Some("FIRE02")]
        );
        assert!(numbered.iter().all(|bones| !bones.recoil));
        model.bone_names = [
            "ROOT", "FIRE01", "FLASH01", "FLASH02", "RECOIL01", "RECOIL02", "RECOIL03", "RECOIL04",
        ]
        .map(String::from)
        .to_vec();
        let shared_fx = barrels([Some("FIRE"), Some("RECOIL"), Some("FLASH"), None], &model);
        assert_eq!(
            shared_fx
                .iter()
                .map(|bones| bones.fire.as_deref())
                .collect::<Vec<_>>(),
            [Some("FIRE01"), Some("FIRE01"), None, None]
        );
        model.bone_names = ["ROOT", "FLASH"].map(String::from).to_vec();
        let plain = barrels([None, None, Some("FLASH"), None], &model);
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].flash.as_deref(), Some("FLASH"));
        assert!(plain[0].fire.is_none() && !plain[0].recoil);
    }

    #[test]
    fn root_attached_launch_only_counts_in_the_unnumbered_fallback() {
        let mut model = Model {
            hlod: true,
            emitter: None,
            scene_sphere: None,
            scene_bounds: generals_assets::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            meshes: vec![Mesh {
                name: "TEST.LAUNCH01".into(),
                bone_name: Some("ROOT".into()),
                attributes: 0,
                sort_level: 0,
                subobject_order: 0,
                bounds: generals_assets::w3d::Bounds {
                    min: [0.0; 3],
                    max: [0.0; 3],
                },
                sphere_center: [0.0; 3],
                sphere_radius: 1.0,
                bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
                positions: Vec::new(),
                normals: None,
                indices: Vec::new(),
                materials: None,
                deformation: None,
            }],
            bone_names: vec!["ROOT".into()],
            bounds: Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            hierarchy: None,
        };
        assert!(barrels([None, None, None, Some("Launch")], &model).is_empty());
        model.meshes[0].name = "TEST.LAUNCH".into();
        let result = barrels([None, None, None, Some("Launch")], &model);
        assert_eq!(result.len(), 1);
        assert!(result[0].fire.is_none() && result[0].flash.is_none() && !result[0].recoil);
    }

    const fn barrel(recoil: bool) -> Barrel {
        Barrel {
            mesh: Some(0),
            recoil,
            recoil_bone: None,
            phase: Phase::Idle,
            shift: 0.0,
            rate: 0.0,
            visible: false,
        }
    }

    #[test]
    fn recoil_flash_remains_in_start_until_source_shift_or_rate_limit() {
        let rules = Rules::new();
        let mut gun = barrel(true);
        for fired in [true, false, false] {
            gun.advance(&rules, fired);
            assert!(gun.visible);
        }
        assert!(matches!(gun.phase, Phase::Settle));
        assert_eq!(gun.shift.to_bits(), 3.0_f32.to_bits());
        gun.advance(&rules, false);
        assert!(!gun.visible);
        // Firing during settling resets velocity but retains the current shift.
        gun.advance(&rules, true);
        assert!(gun.visible);
        assert!(matches!(gun.phase, Phase::Settle));
        let mut slow = barrel(true);
        let rules = Rules {
            initial: 0.01,
            maximum: 3.0,
            damping: 0.4,
            settle: 0.065,
            projectile_slots: 0,
        };
        slow.advance(&rules, true);
        assert!(slow.visible && matches!(slow.phase, Phase::Settle));
        slow.advance(&rules, false);
        assert!(!slow.visible && matches!(slow.phase, Phase::Idle));
    }

    #[test]
    fn rifle_without_recoil_bone_hides_on_the_next_client_tick() {
        let mut rifle = barrel(false);
        rifle.advance(&Rules::new(), true);
        assert!(rifle.visible && matches!(rifle.phase, Phase::Idle));
        rifle.advance(&Rules::new(), false);
        assert!(!rifle.visible);
    }
}

#[cfg(test)]
mod projectile_tests {
    #[test]
    fn source_rack_hides_fired_front_rounds_and_restores_reloaded_clip() {
        let fields = vec![vec![
            "WeaponLaunchBone".into(),
            "SECONDARY".into(),
            "WeaponA".into(),
        ]];
        let expected =
            |first, second| vec![("WeaponA01".into(), first), ("WeaponA02".into(), second)];
        assert_eq!(
            super::projectile_visibility(&fields, [(0, 0), (2, 2), (0, 0)], 2),
            expected(true, true)
        );
        assert_eq!(
            super::projectile_visibility(&fields, [(0, 0), (1, 2), (0, 0)], 2),
            expected(false, true)
        );
        assert_eq!(
            super::projectile_visibility(&fields, [(0, 0), (0, 2), (0, 0)], 2),
            expected(false, false)
        );
        assert!(super::projectile_visibility(&fields, [(0, 0), (0, 2), (0, 0)], 0).is_empty());
        let fields = vec![vec![
            "WeaponHideShowBone".into(),
            "SECONDARY".into(),
            "Rack".into(),
        ]];
        assert_eq!(
            super::projectile_visibility(&fields, [(0, 0), (1, 2), (0, 0)], 2),
            vec![("Rack".into(), false)]
        );
    }
}
