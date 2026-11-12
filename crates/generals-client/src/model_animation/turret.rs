//! `W3DModelDraw::handleClientTurretPositioning` captured-bone transforms.
use generals_assets::w3d::BoneControl;

pub(super) fn controls(
    fields: &[Vec<String>],
    angle: f32,
    pitch: f32,
    alternate: Option<&generals_sim::skirmish::turret::State>,
) -> Vec<BoneControl> {
    let mut result = Vec::new();
    for (name_key, art_key, value, pitch) in [
        ("Turret", "TurretArtAngle", angle, false),
        ("TurretPitch", "TurretArtPitch", pitch, true),
        (
            "AltTurret",
            "AltTurretArtAngle",
            alternate.map_or(0.0, |turret| turret.angle),
            false,
        ),
        (
            "AltTurretPitch",
            "AltTurretArtPitch",
            alternate.map_or(0.0, |turret| turret.pitch),
            true,
        ),
    ] {
        let lookup = |name: &str| {
            fields.iter().rev().find_map(|tokens| {
                (tokens.first().is_some_and(|key| key == name))
                    .then(|| tokens.get(1))
                    .flatten()
            })
        };
        let Some(name) = lookup(name_key) else {
            continue;
        };
        let art = lookup(art_key)
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0)
            * (std::f32::consts::PI / 180.0);
        let angle = if pitch { -(value + art) } else { value + art };
        let (sine, cosine) = angle.sin_cos();
        let axes = if pitch {
            [[cosine, 0.0, -sine], [0.0, 1.0, 0.0], [sine, 0.0, cosine]]
        } else {
            [[cosine, sine, 0.0], [-sine, cosine, 0.0], [0.0, 0.0, 1.0]]
        };
        result.push(BoneControl {
            name: name.clone(),
            axes,
            translation: [0.0; 3],
        });
    }
    result
}
