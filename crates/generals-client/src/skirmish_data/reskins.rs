use super::{blocks, definition};
use generals_assets::content::{ContentFile, ContentLine, Declaration};
use std::error::Error;

pub(super) fn parent(files: &[ContentFile], name: &str) -> Option<String> {
    files
        .iter()
        .flat_map(|file| &file.declarations)
        .find(|declaration| declaration.kind == "ObjectReskin" && declaration.name == name)
        .and_then(|declaration| declaration.parent.clone())
}

pub(super) fn resolve(files: &mut [ContentFile]) -> Result<(), Box<dyn Error>> {
    // ThingFactory::parseObjectDefinition copies the template and marks modules
    // copied-from-default. The first new Draw removes the copied Draw interface.
    // Original represented reskins replace Draw modules and optional geometry.
    let mut resolved = Vec::new();
    for file in files.iter() {
        for child in &file.declarations {
            if child.kind != "ObjectReskin"
                || !matches!(
                    child.name.as_str(),
                    "GLAHoleCommandCenter"
                        | "GLAHoleBarracks"
                        | "GLAHoleSupplyStash"
                        | "GLAHoleArmsDealer"
                        | "GLAHolePalace"
                        | "GLAHoleBlackMarket"
                        | "GLAHoleTunnelNetwork"
                        | "GLAHoleStingerSite"
                        | "GLAVehicleTechnicalChassisTwo"
                        | "GLAVehicleTechnicalChassisThree"
                        | "SCUDMissilePlusOne"
                        | "SCUDMissilePlusTwo"
                        | "ToxicInfantry_TerroristOnly"
                        | "ToxicInfantryBeta_TerroristOnly"
                        | "ToxicInfantryBeta"
                        | "ToxicInfantryGamma"
                        | "GLAInfantryAngryMobPistol03"
                        | "GLAInfantryAngryMobPistol05"
                        | "GLAInfantryAngryMobRock04"
                )
            {
                continue;
            }
            let expected = expected_parent(&child.name);
            if child.parent.as_deref() != Some(expected) {
                return Err("unexpected original reskin parent".into());
            }
            let base = definition(files, "Object", expected)?;
            let replacements = blocks(&child.lines, 0, "Draw")?;
            if replacements.is_empty()
                || child.lines.iter().any(|line| {
                    line.depth == Some(0)
                        && !matches!(
                            line.tokens.first().map(String::as_str),
                            Some(
                                "Draw"
                                    | "End"
                                    | "Geometry"
                                    | "GeometryMajorRadius"
                                    | "GeometryMinorRadius"
                                    | "GeometryHeight"
                                    | "GeometryIsSmall"
                            )
                        )
                })
            {
                return Err(format!("unsupported original reskin fields: {}", child.name).into());
            }
            let mut lines = Vec::new();
            let mut skip = false;
            for (index, source) in base.lines.iter().enumerate() {
                if source.depth == Some(0) && source.tokens.first().is_some_and(|key| key == "Draw")
                {
                    skip = true;
                }
                let replaced = source.depth == Some(0)
                    && source.tokens.first().is_some_and(|key| {
                        key.starts_with("Geometry")
                            && child.lines.iter().any(|line| {
                                line.depth == Some(0) && line.tokens.first() == Some(key)
                            })
                    });
                if !skip && !replaced && index + 1 < base.lines.len() {
                    lines.push(copy_line(source));
                }
                if skip && source.depth == Some(0) && source.tokens == ["End"] {
                    skip = false;
                }
            }
            lines.extend(child.lines.iter().map(copy_line));
            resolved.push(Declaration {
                kind: "Object".into(),
                name: child.name.clone(),
                parent: None,
                line: child.line,
                header: copy_line(&child.header),
                lines,
            });
        }
    }
    if let Some(file) = files.first_mut() {
        file.declarations.extend(resolved);
    }
    Ok(())
}

fn expected_parent(name: &str) -> &'static str {
    if matches!(
        name,
        "GLAInfantryAngryMobPistol03" | "GLAInfantryAngryMobPistol05"
    ) {
        "GLAInfantryAngryMobPistol01"
    } else if name == "GLAInfantryAngryMobRock04" {
        "GLAInfantryAngryMobRock02"
    } else if name.starts_with("GLAVehicleTechnical") {
        "GLAVehicleTechnicalChassisOne"
    } else if name.starts_with("SCUDMissile") {
        "SCUDMissile"
    } else if name.starts_with("ToxicInfantry") {
        "ToxicInfantry"
    } else {
        "GLAHole"
    }
}

fn copy_line(line: &ContentLine) -> ContentLine {
    ContentLine {
        line: line.line,
        tokens: line.tokens.clone(),
        raw: line.raw.clone(),
        assigned: line.assigned,
        depth: line.depth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_assets::{content, virtual_assets::Asset};
    use std::fmt::Write;

    #[test]
    fn toxin_reskins_replace_draw_and_preserve_original_lifetime_body_and_deaths() {
        let mut source = String::from(
            "Object ToxicInfantry\nDraw=W3DModelDraw DrawTag\nDefaultConditionState\nModel=FixtureBaseMesh\nEnd\nEnd\nBody=ActiveBody BodyTag\nMaxHealth=47\nEnd\nBehavior=PhysicsBehavior PhysicsTag\nMass=5\nEnd\nBehavior=LifetimeUpdate LifetimeTag\nMinLifetime=3100\nMaxLifetime=3100\nEnd\nBehavior=SlowDeathBehavior DeathTag\nSinkDelay=4700\nDestructionDelay=9700\nEnd\nBehavior=FXListDie EffectTag\nDeathTypes=ALL -CRUSHED -SPLATTED\nDeathFX=FixtureDeathFx\nEnd\nGeometry=CYLINDER\nGeometryMajorRadius=3\nEnd\n",
        );
        for (name, model) in super::super::death_lifecycle::TOXIC
            .iter()
            .zip(["FixtureVariantA", "FixtureVariantB"])
        {
            writeln!(source, "ObjectReskin {name} ToxicInfantry\nDraw=W3DModelDraw DrawTag\nDefaultConditionState\nModel={model}\nAnimationSpeedFactorRange=0.9 1.3\nAnimationMode=ONCE\nEnd\nEnd\nEnd").unwrap();
        }
        let mut files = vec![
            content::scan(Asset {
                path: "object/system.ini".into(),
                origin: "authored represented toxin inheritance".into(),
                bytes: source.into_bytes(),
            })
            .unwrap(),
        ];
        resolve(&mut files).unwrap();
        for (name, model) in super::super::death_lifecycle::TOXIC
            .iter()
            .zip(["FixtureVariantA", "FixtureVariantB"])
        {
            let child = definition(&files, "Object", name).unwrap();
            let draws = blocks(&child.lines, 0, "Draw").unwrap();
            assert_eq!(draws.len(), 1);
            assert_eq!(super::super::text(draws[0], 2, "Model").unwrap(), model);
            let body = blocks(&child.lines, 0, "Body").unwrap();
            assert_eq!(super::super::text(body[0], 1, "MaxHealth").unwrap(), "47");
            let modules = blocks(&child.lines, 0, "Behavior").unwrap();
            assert_eq!(modules.len(), 4);
            assert_eq!(
                super::super::text(modules[1], 1, "MinLifetime").unwrap(),
                "3100"
            );
            assert_eq!(
                super::super::text(modules[2], 1, "DestructionDelay").unwrap(),
                "9700"
            );
            assert_eq!(
                super::super::text(modules[3], 1, "DeathFX").unwrap(),
                "FixtureDeathFx"
            );
            assert_eq!(
                super::super::text(&child.lines, 0, "GeometryMajorRadius").unwrap(),
                "3"
            );
        }
    }

    #[test]
    fn scud_reskins_replace_only_draws_and_preserve_original_missile_modules() {
        let mut files = vec![content::scan(Asset {
            path: "weaponobjects.ini".into(), origin: "authored represented missile inheritance".into(),
            bytes: b"Object SCUDMissile\nDraw=W3DModelDraw DrawTag\nDefaultConditionState\nModel=FixtureMissileBase\nEnd\nEnd\nBehavior=MissileAIUpdate BodyTag\nFuelLifetime=10000\nEnd\nBehavior=WeaponBonusUpgrade EffectTag\nTriggeredBy=Upgrade_GLAAPRockets\nEnd\nGeometry=Cylinder\nGeometryMajorRadius=4\nGeometryHeight=4\nEnd\nObjectReskin SCUDMissilePlusOne SCUDMissile\nDraw=W3DModelDraw DrawTag\nDefaultConditionState\nModel=FixtureMissileA\nEnd\nEnd\nEnd\nObjectReskin SCUDMissilePlusTwo SCUDMissile\nDraw=W3DModelDraw DrawTag\nDefaultConditionState\nModel=FixtureMissileB\nEnd\nEnd\nEnd\n".to_vec(),
        }).unwrap()];
        resolve(&mut files).unwrap();
        for (name, model) in [
            ("SCUDMissilePlusOne", "FixtureMissileA"),
            ("SCUDMissilePlusTwo", "FixtureMissileB"),
        ] {
            let child = definition(&files, "Object", name).unwrap();
            let draws = blocks(&child.lines, 0, "Draw").unwrap();
            assert_eq!(draws.len(), 1);
            assert_eq!(super::super::text(draws[0], 2, "Model").unwrap(), model);
            let modules = blocks(&child.lines, 0, "Behavior").unwrap();
            assert_eq!(modules.len(), 2);
            assert_eq!(
                super::super::text(modules[0], 1, "FuelLifetime").unwrap(),
                "10000"
            );
            assert_eq!(
                super::super::text(modules[1], 1, "TriggeredBy").unwrap(),
                "Upgrade_GLAAPRockets"
            );
            assert_eq!(
                super::super::text(&child.lines, 0, "GeometryMajorRadius").unwrap(),
                "4"
            );
        }
    }

    #[test]
    fn technical_reskin_replaces_both_draws_and_geometry_without_losing_transport() {
        let mut files = vec![content::scan(Asset {
            path: "glavehicle.ini".into(), origin: "Technical source inheritance".into(),
            bytes: b"Object GLAVehicleTechnicalChassisOne\nDraw=W3DTruckDraw Body\nDefaultConditionState\nModel=Truck\nEnd\nEnd\nDraw=W3DModelDraw Gunner\nDefaultConditionState\nModel=Soldier\nEnd\nEnd\nGeometry=BOX\nGeometryMajorRadius=13\nGeometryMinorRadius=6\nBehavior=TransportContain Cargo\nSlots=5\nEnd\nEnd\nObjectReskin GLAVehicleTechnicalChassisTwo GLAVehicleTechnicalChassisOne\nDraw=W3DTruckDraw Body\nDefaultConditionState\nModel=Van\nEnd\nEnd\nDraw=W3DModelDraw Gunner\nDefaultConditionState\nModel=Soldier2\nEnd\nEnd\nGeometryMajorRadius=14\nEnd\n".to_vec(),
        }).unwrap()];
        resolve(&mut files).unwrap();
        let child = definition(&files, "Object", "GLAVehicleTechnicalChassisTwo").unwrap();
        assert_eq!(blocks(&child.lines, 0, "Draw").unwrap().len(), 2);
        assert_eq!(
            super::super::number(&child.lines, 0, "GeometryMajorRadius")
                .unwrap()
                .to_bits(),
            14.0_f32.to_bits()
        );
        assert_eq!(
            super::super::number(&child.lines, 0, "GeometryMinorRadius")
                .unwrap()
                .to_bits(),
            6.0_f32.to_bits()
        );
        let behavior = blocks(&child.lines, 0, "Behavior").unwrap();
        assert_eq!(super::super::text(behavior[0], 1, "Slots").unwrap(), "5");
        assert_eq!(
            parent(&files, &child.name).as_deref(),
            Some("GLAVehicleTechnicalChassisOne")
        );
    }

    #[test]
    fn reskin_replaces_draws_and_retains_nested_module_boundaries() {
        let mut files = vec![content::scan(Asset {
            path: "fixture.ini".into(), origin: "fixture".into(), bytes: b"Object GLAHole\nDraw = W3DModelDraw Original\nDefaultConditionState\nModel = Old\nEnd\nEnd\nPrerequisites\nObject = GLACommandCenter\nEnd\nBehavior = RebuildHoleBehavior Rebuild\nWorkerObjectName = GLAInfantryWorker\nEnd\nEnd\nObjectReskin GLAHoleBarracks GLAHole\nDraw = W3DModelDraw Replacement\nDefaultConditionState\nModel = New\nEnd\nEnd\nEnd\nObjectReskin GLAHoleScudStorm GLAHole\nGeometryMajorRadius = 100\nEnd\n".to_vec(),
        }).unwrap()];
        resolve(&mut files).unwrap();
        let child = definition(&files, "Object", "GLAHoleBarracks").unwrap();
        assert_eq!(blocks(&child.lines, 0, "Draw").unwrap().len(), 1);
        let prerequisites = super::super::support::prerequisites(&child.lines).unwrap();
        assert_eq!(prerequisites.objects, vec![vec!["GLACommandCenter"]]);
        let behavior = blocks(&child.lines, 0, "Behavior").unwrap();
        assert_eq!(behavior.len(), 1);
        assert_eq!(
            super::super::text(behavior[0], 1, "WorkerObjectName").unwrap(),
            "GLAInfantryWorker"
        );
        assert!(
            !child
                .lines
                .iter()
                .any(|line| line.tokens.iter().any(|token| token == "Old"))
        );
    }
    #[test]
    fn every_represented_hole_resolves_its_original_parent_and_replacement_draws() {
        let mut source = String::from(
            "Object GLAHole\nBehavior=RebuildHoleBehavior Rebuild\nWorkerObjectName=GLAInfantryWorker\nEnd\nEnd\n",
        );
        let holes: Vec<_> = super::super::ROSTER
            .iter()
            .copied()
            .filter(|(name, _, _)| name.starts_with("GLAHole"))
            .map(|(name, _, _)| name)
            .collect();
        for name in &holes {
            writeln!(source, "ObjectReskin {name} GLAHole\nDraw=W3DModelDraw Replacement\nConditionState=NONE\nModel=UBHole\nEnd\nEnd\nEnd").unwrap();
        }
        let mut files = vec![
            content::scan(Asset {
                path: "factionbuilding.ini".into(),
                origin: "original hole inheritance fixture".into(),
                bytes: source.into_bytes(),
            })
            .unwrap(),
        ];
        resolve(&mut files).unwrap();
        assert!(holes.contains(&"GLAHoleStingerSite"));
        for name in holes {
            let child = definition(&files, "Object", name).unwrap();
            assert!(child.parent.is_none());
            assert_eq!(blocks(&child.lines, 0, "Draw").unwrap().len(), 1);
            assert_eq!(blocks(&child.lines, 0, "Behavior").unwrap().len(), 1);
        }
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_mob_reskins_keep_parent_weapons_and_slaved_ai_with_original_models() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let files = super::super::source_files(&assets).unwrap();
        for (name, base, model) in [
            (
                "GLAInfantryAngryMobPistol03",
                "GLAInfantryAngryMobPistol01",
                "UIMOB03_SKN",
            ),
            (
                "GLAInfantryAngryMobPistol05",
                "GLAInfantryAngryMobPistol01",
                "UIMOB05_SKN",
            ),
            (
                "GLAInfantryAngryMobRock04",
                "GLAInfantryAngryMobRock02",
                "UIMOB04_SKN",
            ),
        ] {
            let child = definition(&files, "Object", name).unwrap();
            let base_object = definition(&files, "Object", base).unwrap();
            assert_eq!(parent(&files, name).as_deref(), Some(base));
            let draws = blocks(&child.lines, 0, "Draw").unwrap();
            assert_eq!(draws.len(), 1);
            let normal = super::super::normal_state(draws[0]).unwrap();
            assert_eq!(super::super::text(normal, 2, "Model").unwrap(), model);
            for key in ["Body", "Behavior", "WeaponSet"] {
                let inherited = blocks(&child.lines, 0, key).unwrap();
                let original = blocks(&base_object.lines, 0, key).unwrap();
                let fields = |blocks: Vec<&[ContentLine]>| {
                    blocks
                        .into_iter()
                        .map(|block| {
                            block
                                .iter()
                                .map(|line| (line.tokens.clone(), line.depth))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(fields(inherited), fields(original));
            }
        }
    }
}
