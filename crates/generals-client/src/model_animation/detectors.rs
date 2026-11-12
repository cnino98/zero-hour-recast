use super::Animations;

impl Animations {
    pub fn detector_bone_position(&self, unit: usize, health: f32, bone: &str) -> [f32; 3] {
        let [damaged, really_damaged] = self.damage_thresholds;
        let damage = if health <= really_damaged && health > 0.0 {
            Some("REALLYDAMAGED")
        } else if health <= damaged && health > 0.0 {
            Some("DAMAGED")
        } else {
            None
        };
        // The represented Outpost's literal IRFX belongs to its body draw,
        // whose model varies only with damage. Its moving tower has no IRFX.
        self.logical_bone_position(&self.models, unit, damage, bone)
            .unwrap_or([-1.66, 5.5, 15.0])
    }
}
