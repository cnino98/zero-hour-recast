//! Original independently scheduled point-defense modules and weapon templates.
use super::{blocks, definition, duration_frames, field, scalar, text, weapons};
use generals_assets::content::{ContentFile, Declaration};
use generals_sim::skirmish::{WeaponRules, point_defense::Rules};
use std::error::Error;

pub(super) fn load(
    files: &[ContentFile],
    object: &Declaration,
) -> Result<Vec<Rules>, Box<dyn Error>> {
    let mut result = Vec::new();
    for module in blocks(&object.lines, 0, "Behavior")? {
        if module[0]
            .tokens
            .get(1)
            .is_none_or(|name| name != "PointDefenseLaserUpdate")
        {
            continue;
        }
        let mut weapon = WeaponRules::unarmed();
        weapon.name = text(module, 1, "WeaponTemplate")?.into();
        let lines = &definition(files, "Weapon", &weapon.name)?.lines;
        weapons::parse(&mut weapon, lines)?;
        let flags =
            |name| field(module, 1, name).map_or_else(Vec::new, |line| line.tokens[1..].to_vec());
        let number = |name| -> Result<f32, Box<dyn Error>> {
            // Native module constructor leaves both source real values zero.
            field(module, 1, name).map_or(Ok(0.0), |line| super::finite_number(scalar(line)?))
        };
        result.push(Rules {
            weapon,
            primary: flags("PrimaryTargetTypes"),
            secondary: flags("SecondaryTargetTypes"),
            scan_frames: field(module, 1, "ScanRate").map_or(Ok(0), duration_frames)?,
            scan_range: number("ScanRange")?,
            predict_velocity: number("PredictTargetVelocityFactor")?,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};

    fn file(source: &str) -> ContentFile {
        content::scan(Asset {
            path: "point-defense.ini".into(),
            origin: "original point-defense field contracts".into(),
            bytes: source.as_bytes().to_vec(),
        })
        .unwrap()
    }

    #[test]
    fn independent_avenger_modules_keep_source_order_and_zero_scan_interval() {
        let source = "Object Avenger
Behavior=PointDefenseLaserUpdate LaserOne
WeaponTemplate=Laser
PrimaryTargetTypes=BALLISTIC_MISSILE SMALL_MISSILE
ScanRate=0
ScanRange=200
PredictTargetVelocityFactor=1
End
Behavior=PointDefenseLaserUpdate LaserTwo
WeaponTemplate=Laser
PrimaryTargetTypes=BALLISTIC_MISSILE SMALL_MISSILE
SecondaryTargetTypes=INFANTRY
ScanRate=100
ScanRange=200
PredictTargetVelocityFactor=1
End
End
Weapon Laser
PrimaryDamage=100
AttackRange=100
DelayBetweenShots=500
ClipSize=0
DamageType=LASER
AntiSmallMissile=Yes
AntiProjectile=Yes
LaserName=AvengerPointDefenseLaserBeam
End
";
        let files = [file(source)];
        let rules = load(&files, &files[0].declarations[0]).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!([rules[0].scan_frames, rules[1].scan_frames], [0, 3]);
        assert!(rules[0].secondary.is_empty());
        assert_eq!(rules[1].secondary, ["INFANTRY"]);
        assert_eq!(rules[0].primary, ["BALLISTIC_MISSILE", "SMALL_MISSILE"]);
        assert!(rules.iter().all(|rules| rules.weapon.laser));
        assert!(matches!(
            rules[0].weapon.damage_type,
            generals_sim::skirmish::DamageType::Laser
        ));
        assert_eq!(rules[0].weapon.delay.to_bits(), 0.5_f32.to_bits());
    }

    #[test]
    fn projectile_interception_uses_authored_body_and_ordered_laser_armor() {
        let source = "Object Missile
KindOf=PROJECTILE SMALL_MISSILE
Body=ActiveBody Body
MaxHealth=100
InitialHealth=100
End
ArmorSet
Conditions=None
Armor=MissileArmor
End
End
Object Shell
Body=InactiveBody Body
End
End
Armor MissileArmor
Armor=LASER 50%
Armor=DEFAULT 0%
Armor=LASER 100%
End
";
        let files = [file(source)];
        let object = &files[0].declarations[0];
        let body = super::super::active_body(&files, object, generals_sim::skirmish::Kind::Effect)
            .unwrap()
            .unwrap();
        assert!(matches!(
            super::super::body_kind(&object.lines).unwrap(),
            generals_sim::skirmish::BodyKind::Active
        ));
        assert_eq!(
            super::super::body_health(body, "MaxHealth")
                .unwrap()
                .to_bits(),
            100.0_f32.to_bits()
        );
        assert_eq!(
            super::super::body_health(body, "InitialHealth")
                .unwrap()
                .to_bits(),
            100.0_f32.to_bits()
        );
        let armor = definition(&files, "Armor", "MissileArmor").unwrap();
        let coefficients = super::super::armor_coefficients(&armor.lines).unwrap();
        assert_eq!(
            coefficients[generals_sim::skirmish::DamageType::Laser as usize].to_bits(),
            1.0_f32.to_bits()
        );
        assert!(matches!(
            super::super::body_kind(&files[0].declarations[1].lines).unwrap(),
            generals_sim::skirmish::BodyKind::Inactive
        ));
        let partial = [file(
            &source.replace("InitialHealth=100", "InitialHealth=50"),
        )];
        let body = super::super::active_body(
            &partial,
            &partial[0].declarations[0],
            generals_sim::skirmish::Kind::Effect,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            super::super::body_health(body, "MaxHealth")
                .unwrap()
                .to_bits(),
            100.0_f32.to_bits()
        );
        assert_eq!(
            super::super::body_health(body, "InitialHealth")
                .unwrap()
                .to_bits(),
            50.0_f32.to_bits()
        );
    }
}
