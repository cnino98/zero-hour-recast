//! Drawable-owned shell trails and missile JAMMED state particles.
use super::{Attachment, Particles};
use crate::skirmish_data::{blocks, definition, field, scalar};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Skirmish, UnitType};
use std::{collections::BTreeMap, error::Error};

pub(super) type Bindings = BTreeMap<(usize, usize, bool), Vec<String>>;

pub(super) fn load(assets: &VirtualAssets, types: &[UnitType]) -> Result<Bindings, Box<dyn Error>> {
    let mut files = [
        content::scan(assets.read("data/ini/weapon.ini")?)?,
        content::scan(assets.read("data/ini/object/weaponobjects.ini")?)?,
    ];
    crate::skirmish_data::resolve_reskins(&mut files)?;
    let mut result = BTreeMap::new();
    for (kind, unit) in types.iter().enumerate() {
        for slot in 0..unit.weapon_template_count() {
            let Some(weapon) = unit
                .weapon_template(slot)
                .filter(|weapon| !weapon.name.is_empty())
            else {
                continue;
            };
            let weapon = definition(&files, "Weapon", &weapon.name)?;
            let Some(name) = field(&weapon.lines, 0, "ProjectileObject")
                .map(scalar)
                .transpose()?
            else {
                continue;
            };
            if name.eq_ignore_ascii_case("NONE") {
                continue;
            }
            let object = definition(&files, "Object", name)?;
            let draws = blocks(&object.lines, 0, "Draw")?;
            let inferno = matches!(name, "InfernoTankShell" | "InfernoTankShellUpgraded");
            let jammed = draws.iter().any(|draw| {
                draw.iter()
                    .any(|line| line.tokens.iter().any(|token| token == "JAMMED"))
            });
            if !inferno && !jammed {
                continue;
            }
            for condition in [false, true] {
                let conditions = if condition {
                    ["JAMMED".into()].into()
                } else {
                    std::collections::BTreeSet::new()
                };
                let mut names = Vec::new();
                for lines in &draws {
                    let draw = crate::model_animation::state_particles::particle_draw(lines)?;
                    let state = crate::model_animation::state_particles::selected_particle_state(
                        lines,
                        &conditions,
                    )?;
                    if draw.bones[state]
                        .iter()
                        .any(|bone| !bone.eq_ignore_ascii_case("none"))
                    {
                        return Err(format!("{name}: unported projectile particle bone").into());
                    }
                    names.extend(draw.states[state].iter().cloned());
                }
                result.insert((kind, slot, condition), names);
            }
        }
    }
    Ok(result)
}

impl Particles {
    pub(super) fn fire_projectile_draws(
        &mut self,
        game: &Skirmish,
        region: [[f32; 2]; 2],
        random: &mut impl FnMut(f32, f32) -> f32,
    ) {
        if self.fires.projectile_bindings.is_empty() {
            return;
        }
        let live: BTreeMap<_, _> = game
            .projectiles
            .iter()
            .filter(|projectile| {
                self.fires.projectile_bindings.contains_key(&(
                    projectile.weapon_type,
                    projectile.weapon_index,
                    projectile.jammed(&game.entities),
                ))
            })
            .map(|projectile| (projectile.id, projectile))
            .collect();
        self.fires.projectiles.retain(|id, _| live.contains_key(id));
        for emitter in &mut self.emitters {
            if let Some(Attachment::Projectile(id)) = emitter.fire_attachment
                && live.get(&id).is_none_or(|projectile| {
                    self.fires
                        .projectiles
                        .get(&id)
                        .is_some_and(|state| *state != projectile.jammed(&game.entities))
                })
            {
                emitter.lifecycle.destroyed = true;
                emitter.wind_origin = Some([0.0; 3]);
            }
        }
        for projectile in game.projectiles.iter().rev() {
            let kind = projectile.weapon_type;
            let Some(templates) = self.fires.projectile_bindings.get(&(
                kind,
                projectile.weapon_index,
                projectile.jammed(&game.entities),
            )) else {
                continue;
            };
            let Some((pose, hidden)) = super::super::effects::attached_pose(game, projectile)
            else {
                continue;
            };
            if self.fires.projectiles.get(&projectile.id)
                == Some(&projectile.jammed(&game.entities))
                || hidden
                || !super::super::model_states::view::contains(
                    region,
                    [pose.position[0], pose.position[1]],
                )
            {
                continue;
            }
            let templates = templates.clone();
            for template in templates {
                self.create(template, pose.clone(), Some(projectile.id), random);
                self.emitters
                    .last_mut()
                    .expect("created projectile drawable particles")
                    .fire_attachment = Some(Attachment::Projectile(projectile.id));
            }
            self.fires
                .projectiles
                .insert(projectile.id, projectile.jammed(&game.entities));
        }
        // Existing systems follow the object even outside the current view region.
        for emitter in &mut self.emitters {
            if let Some(Attachment::Projectile(id)) = emitter.fire_attachment
                && !emitter.lifecycle.destroyed
                && let Some((pose, hidden)) = live
                    .get(&id)
                    .and_then(|projectile| super::super::effects::attached_pose(game, projectile))
            {
                emitter.transform = pose;
                emitter.shrouded = hidden;
            }
        }
    }
}
