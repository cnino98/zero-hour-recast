//! Rendered acceptance through the actual native promotion/badge components.
use super::{INSIGNIA, Insignia, Promotion, VeterancyFeedback};
use crate::{control_bar::UiData, playtest::Match, skirmish_verification::require};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(SystemParam)]
pub struct VeterancyInspection<'w, 's> {
    data: Res<'w, UiData>,
    rules: Res<'w, VeterancyFeedback>,
    badges: Query<
        'w,
        's,
        (
            &'static Insignia,
            &'static Node,
            &'static ImageNode,
            &'static ComputedNode,
        ),
    >,
    animations: Query<
        'w,
        's,
        (
            &'static Promotion,
            &'static Node,
            &'static ImageNode,
            &'static ComputedNode,
        ),
    >,
}

impl VeterancyInspection<'_, '_> {
    pub fn badge(&self, battle: &Match, id: usize) -> Result<(), Box<dyn Error>> {
        let rank = usize::from(battle.simulation.entities[id].veterancy);
        let name = *INSIGNIA
            .get(rank.checked_sub(1).ok_or("unit did not earn a rank")?)
            .ok_or("invalid source veterancy level")?;
        let (_, node, image, computed) = self
            .badges
            .iter()
            .find(|(badge, _, _, _)| badge.object == id && badge.generation == battle.generation)
            .ok_or("ranked unit lacks original insignia entity")?;
        let expected = self.data.image(name);
        require(
            node.display != Display::None
                && computed.size().min_element() > 0.0
                && image.image == expected.image
                && image.rect == expected.rect,
            "ranked unit insignia is hidden, unlaid-out or uses different artwork",
        )
    }

    pub fn promotion(
        &self,
        battle: &Match,
        position: [f32; 2],
    ) -> Result<(usize, Vec3), Box<dyn Error>> {
        let (promotion, node, image, computed) = self
            .animations
            .iter()
            .find(|(promotion, _, _, _)| {
                promotion.generation == battle.generation
                    && promotion.map_position.map(f32::to_bits) == position.map(f32::to_bits)
            })
            .ok_or("real unit promotion lacks world animation entity")?;
        let expected = self.data.image(&self.rules.images[promotion.frame]);
        require(
            node.display != Display::None
                && computed.size().min_element() > 0.0
                && image.image == expected.image
                && image.rect == expected.rect
                && image.color.alpha() > 0.0,
            "original promotion frame is hidden, unlaid-out or uses different artwork",
        )?;
        Ok((promotion.frame, promotion.position))
    }

    pub fn no_badge(&self, battle: &Match, id: usize) -> Result<(), Box<dyn Error>> {
        require(
            !self.badges.iter().any(|(badge, _, _, _)| {
                badge.object == id && badge.generation == battle.generation
            }),
            "dead unit retained its insignia ECS entity",
        )
    }
}
