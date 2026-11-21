//! Native `GrantStealthBehavior` module data.
use super::{blocks, field, finite_number, scalar};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::gps_scrambler::Rules;
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    blocks(&object.lines, 0, "Behavior")?
        .into_iter()
        .filter(|module| module[0].tokens[1] == "GrantStealthBehavior")
        .map(|module| {
            let number = |name, default| {
                field(module, 1, name).map_or(Ok(default), |line| finite_number(scalar(line)?))
            };
            let radius_particle = field(module, 1, "RadiusParticleSystemName")
                .map(scalar)
                .transpose()?
                .map(|name| super::instant_death::nullable(files, "ParticleSystem", name))
                .transpose()?
                .flatten()
                .map(|template| template.name.clone());
            let kind_of = field(module, 1, "KindOf").map(|line| {
                line.tokens[1..]
                    .iter()
                    .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                    .map(|name| name.to_ascii_uppercase())
                    .collect()
            });
            Ok(Rules {
                start_radius: number("StartRadius", 0.0)?,
                final_radius: number("FinalRadius", 200.0)?,
                radius_per_frame: number("RadiusGrowRate", 10.0)?,
                kind_of,
                radius_particle,
            })
        })
        .collect()
}
