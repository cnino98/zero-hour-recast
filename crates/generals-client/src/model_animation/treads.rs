//! SH `W3DTankDraw` / `W3DTankTruckDraw`: per-render-object material overrides.
use super::{Animations, Clock, Players};
use crate::{
    playtest::Match,
    playtest_view::{UnitVisual, WorldCamera},
    skirmish_data::{field, scalar},
};
use bevy::prelude::*;
use generals_assets::{
    content::ContentLine,
    w3d::{Model, material::LinearMapper},
};
use std::error::Error;

pub(super) struct Rules {
    truck: bool,
    rate: f32,
    pivot: f32,
    drive: f32,
}
impl Rules {
    pub(super) fn load(lines: &[ContentLine]) -> Result<Option<Self>, Box<dyn Error>> {
        let kind = lines[0].tokens.get(1).map(String::as_str);
        if !matches!(
            kind,
            Some("W3DTankDraw" | "W3DTankTruckDraw" | "W3DOverlordTankDraw")
        ) {
            return Ok(None);
        }
        let real = |key, default| -> Result<f32, Box<dyn Error>> {
            field(lines, 1, key)
                .map(scalar)
                .transpose()?
                .map_or(Ok(default), |text| Ok(text.parse()?))
        };
        Ok(Some(Self {
            truck: kind == Some("W3DTankTruckDraw"),
            rate: real("TreadAnimationRate", 0.0)? / 30.0,
            pivot: real("TreadPivotSpeedFraction", 0.6)?,
            drive: real("TreadDriveSpeedFraction", 0.3)?,
        }))
    }
}

struct Tread {
    mesh: usize,
    side: i8,
    offset: f32,
}
pub(super) struct State {
    model: Option<usize>,
    direction: [f32; 2],
    treads: Vec<Tread>,
}
impl State {
    pub(super) const fn new() -> Self {
        Self {
            model: None,
            direction: [1.0, 0.0],
            treads: Vec::new(),
        }
    }
    fn bind(&mut self, model_index: usize, model: &Model, rules: &Rules) {
        if self.model == Some(model_index) {
            return;
        }
        self.model = Some(model_index);
        self.treads.clear();
        if rules.rate == 0.0 {
            return;
        }
        for (index, mesh) in model.meshes.iter().enumerate() {
            if self.treads.len() >= 4 {
                break;
            }
            let Some((_, name)) = mesh.name.split_once('.') else {
                continue;
            };
            let name = name.to_ascii_uppercase();
            if !name.starts_with("TREADS") {
                continue;
            }
            let Some(materials) = &mesh.materials else {
                continue;
            };
            for _ in materials
                .vertex_materials
                .iter()
                .filter(|material| material.linear_mapper.is_some())
            {
                self.treads.push(Tread {
                    mesh: index,
                    side: if rules.truck {
                        0
                    } else {
                        match name.as_bytes().get(6) {
                            Some(b'L') => 1,
                            Some(b'R') => -1,
                            _ => 0,
                        }
                    },
                    offset: 0.0,
                });
            }
        }
    }
    fn advance(&mut self, rules: &Rules, motion: (f32, f32, bool, i8), facing: f32) {
        let (speed, maximum, motive, turning) = motion;
        let fraction = speed / maximum;
        if !rules.truck && turning != 0 && fraction < rules.pivot {
            let direction = [facing.cos(), facing.sin()];
            let products = [
                direction[0] * self.direction[0],
                direction[1] * self.direction[1],
            ];
            let cosine = products[0] + products[1];
            if (1.0 - cosine).abs() > 0.00001 {
                let delta = if turning < 0 { -rules.rate } else { rules.rate };
                for tread in &mut self.treads {
                    assert_ne!(
                        tread.side, 0,
                        "native tank pivot requires left/right tread names"
                    );
                    tread.offset = wrap(tread.offset + delta * f32::from(tread.side));
                }
            }
            self.direction = direction;
        } else if motive && fraction >= rules.drive {
            for tread in &mut self.treads {
                tread.offset = wrap(tread.offset - rules.rate);
            }
        }
    }
}
fn wrap(value: f32) -> f32 {
    value - value.floor()
}

impl Players {
    pub(crate) fn tread_uv(
        &self,
        model: usize,
        mesh: usize,
        mapper: &LinearMapper,
    ) -> Option<Vec4> {
        let tread = self
            .0
            .iter()
            .filter(|player| player.treads.model == Some(model))
            .find_map(|player| {
                player
                    .treads
                    .treads
                    .iter()
                    .rev()
                    .find(|tread| tread.mesh == mesh)
            })?;
        // DX8MeshRenderer temporarily replaces the mapper offset and sync time;
        // UV scale remains authored and no automatic delta is accumulated.
        let offset = if mapper.clamp {
            // WWMath tests the minimum first, including authored negative scales.
            if tread.offset < -mapper.scale[0] {
                -mapper.scale[0]
            } else {
                tread.offset.min(mapper.scale[0])
            }
        } else {
            wrap(tread.offset)
        };
        Some(Vec4::new(mapper.scale[0], mapper.scale[1], offset, 0.0))
    }
}

type CameraView = (&'static Transform, &'static Projection, &'static Camera);
pub fn update(
    battle: Res<Match>,
    clock: Res<Clock>,
    animations: Res<Animations>,
    terrain: Res<crate::terrain::Terrain>,
    camera: Single<CameraView, With<WorldCamera>>,
    mut visuals: Query<(&UnitVisual, &mut Players, &Visibility)>,
    visibility_cache: Option<Res<super::draw_visibility::DrawVisibility>>,
) {
    let battle = battle.into_inner();
    let clock = clock.into_inner();
    let animations = animations.into_inner();
    let terrain = terrain.into_inner();
    let visibility_cache = visibility_cache.map(Res::into_inner);
    if !battle.started {
        return;
    }
    let game = &battle.simulation;
    let (transform, projection, camera) = camera.into_inner();
    let region =
        crate::particles::drawable_region(transform, projection, camera, terrain.draw_region());
    for (visual, mut players, visibility) in &mut visuals {
        if visual.generation != battle.generation
            || *visibility == Visibility::Hidden
            || !crate::particles::drawable_in_region(region, game.entities[visual.index].position)
            || super::draw_visibility::hidden(visibility_cache, battle, visual.index)
        {
            continue;
        }
        let unit = &game.entities[visual.index];
        let draw_type = players.draw_type(unit.type_index);
        for (module, player) in players.0.iter_mut().enumerate() {
            let Some(rules) = &animations.draws[draw_type][module].treads else {
                continue;
            };
            let Some(model) = player.model else {
                player.treads.model = None;
                player.treads.treads.clear();
                continue;
            };
            player.treads.bind(model, &animations.models[model], rules);
            if !battle.paused
                && clock.frame_milliseconds() != 0
                && let Some(motion) = game.tread_draw_motion(visual.index)
            {
                player.treads.advance(rules, motion, unit.facing);
            }
        }
    }
}

#[cfg(test)]
pub fn fixture_players(offset: f32) -> Players {
    let mut player = super::Player::new([0.0; 2], 0);
    player.treads.model = Some(0);
    player.treads.treads.push(Tread {
        mesh: 0,
        side: 1,
        offset,
    });
    Players(vec![player])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> State {
        State {
            model: Some(0),
            direction: [1.0, 0.0],
            treads: vec![
                Tread {
                    mesh: 0,
                    side: 1,
                    offset: 0.0,
                },
                Tread {
                    mesh: 1,
                    side: -1,
                    offset: 0.0,
                },
            ],
        }
    }
    fn rules(truck: bool) -> Rules {
        Rules {
            truck,
            rate: 0.25,
            pivot: 0.6,
            drive: 0.3,
        }
    }
    fn offsets(state: &State) -> Vec<u32> {
        state
            .treads
            .iter()
            .map(|tread| tread.offset.to_bits())
            .collect()
    }
    #[test]
    #[ignore = "requires original game installation"]
    fn installed_borrowable_tread_materials_keep_instance_identity_and_native_scale() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let mut models = Vec::new();
        let animations =
            super::super::load(&assets, &mut models, &battle.simulation.catalog).unwrap();
        let mut found = [false; 2];
        for draws in &animations.draws {
            for draw in draws {
                let Some(rules) = &draw.treads else { continue };
                for source in &draw.states {
                    let Some(index) = source.render_model else {
                        continue;
                    };
                    let model = &models[index];
                    let mut state = State::new();
                    state.bind(index, model, rules);
                    if state.treads.is_empty() {
                        continue;
                    }
                    found[usize::from(rules.truck)] = true;
                    let parts = crate::model_render::parts(model).unwrap();
                    for tread in &state.treads {
                        let part = parts
                            .iter()
                            .find(|part| {
                                part.binding.as_ref().is_some_and(|binding| {
                                    binding.mesh == tread.mesh && binding.linear_mapper.is_some()
                                })
                            })
                            .unwrap();
                        assert!(
                            !part.order.sorted(),
                            "stock tread {} unexpectedly uses sorting renderer",
                            model.meshes[tread.mesh].name
                        );
                    }
                    state.advance(rules, (1.0, 1.0, true, 0), 0.0);
                    let before: Vec<_> = state
                        .treads
                        .iter()
                        .map(|tread| tread.offset.to_bits())
                        .collect();
                    state.bind(index, model, rules);
                    assert_eq!(
                        state
                            .treads
                            .iter()
                            .map(|tread| tread.offset.to_bits())
                            .collect::<Vec<_>>(),
                        before
                    );
                    state.bind(models.len(), model, rules);
                    assert!(state.treads.iter().all(|tread| tread.offset == 0.0));
                }
            }
        }
        assert_eq!(
            found,
            [true, true],
            "both tank and halftrack borrowed draw families require original linear-mapped art"
        );
    }

    #[test]
    fn native_drive_reverse_still_and_pivot_keep_independent_offsets() {
        let rules = rules(false);
        let mut moving = state();
        let still = state();
        moving.advance(&rules, (1.0, 1.0, true, 0), 0.0);
        assert_eq!(offsets(&moving), [0.75_f32.to_bits(); 2]);
        // Source drive uses magnitude; reverse never reverses the texture rate.
        moving.advance(&rules, (1.0, 1.0, true, 0), std::f32::consts::PI);
        assert_eq!(offsets(&moving), [0.5_f32.to_bits(); 2]);
        moving.advance(&rules, (0.29, 1.0, true, 0), 0.0);
        moving.advance(&rules, (1.0, 1.0, false, 0), 0.0);
        assert_eq!(offsets(&moving), [0.5_f32.to_bits(); 2]);
        moving.advance(&rules, (0.0, 1.0, true, 1), 0.25);
        assert_eq!(offsets(&moving), [0.75_f32.to_bits(), 0.25_f32.to_bits()]);
        moving.advance(&rules, (0.0, 1.0, true, 1), 0.25);
        assert_eq!(offsets(&moving), [0.75_f32.to_bits(), 0.25_f32.to_bits()]);
        moving.advance(&rules, (0.0, 1.0, true, -1), 0.0);
        assert_eq!(offsets(&moving), [0.5_f32.to_bits(); 2]);
        assert_eq!(offsets(&still), [0.0_f32.to_bits(); 2]);
    }
    #[test]
    fn halftracks_never_pivot_and_material_keeps_authored_scale() {
        let mut state = state();
        state.advance(&rules(true), (0.0, 1.0, true, 1), 0.5);
        assert_eq!(offsets(&state), [0.0_f32.to_bits(); 2]);
        state.advance(&rules(true), (0.3, 1.0, true, -1), 0.0);
        let mut player = super::super::Player::new([0.0; 2], 0);
        player.treads = state;
        let players = Players(vec![player]);
        let mapper = LinearMapper {
            scale: [2.0, 3.0],
            offset: [0.2, 0.4],
            per_second: [50.0; 2],
            clamp: false,
        };
        assert_eq!(
            players.tread_uv(0, 0, &mapper),
            Some(Vec4::new(2.0, 3.0, 0.75, 0.0))
        );
        assert_eq!(players.tread_uv(1, 0, &mapper), None);
    }
}
