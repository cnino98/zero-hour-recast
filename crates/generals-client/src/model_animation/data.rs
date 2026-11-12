//! `W3DModelDraw` condition parsing and `SparseMatchFinder` selection, EA GPL-3.0+.
use super::{Animations, State};
use crate::skirmish_data::{blocks, definition, field, scalar, state_model};
use generals_assets::{
    AssetError,
    content::{self, ContentLine},
    virtual_assets::VirtualAssets,
    w3d::{Model, animation::Clip},
};
use generals_sim::skirmish::UnitType;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

#[cfg(test)]
mod bridge_tests;
#[cfg(test)]
mod null_animation_tests;

// Cloning is required by the original DefaultConditionState inheritance.
#[derive(Clone)]
pub(super) struct Motion {
    pub name: String,
    pub distance: f32,
    pub idle: bool,
}

pub(super) struct Draw {
    pub states: Vec<State>,
    pub ignored: BTreeSet<String>,
    pub recoil: super::muzzle::Rules,
    pub requires_power: bool,
    pub receives_dynamic_lights: bool,
    pub attach_bone: Option<String>,
    pub attach_offset: [f32; 3],
    pub upgrade_show: Vec<String>,
    pub supply: BTreeMap<usize, std::sync::Arc<crate::supply_presentation::SupplyDraw>>,
}

impl Draw {
    pub fn select(&self, conditions: &BTreeSet<String>) -> usize {
        let mut best = 0;
        let mut best_yes = 0;
        let mut best_extra = usize::MAX;
        for (index, state) in self.states.iter().enumerate() {
            if state.transition.is_some() {
                continue;
            }
            for flags in state.conditions.iter().rev() {
                let yes = flags
                    .iter()
                    .filter(|flag| conditions.contains(*flag) && !self.ignored.contains(*flag))
                    .count();
                let extra = flags.len() - yes;
                if yes > best_yes || (yes >= best_yes && extra < best_extra) {
                    best = index;
                    best_yes = yes;
                    best_extra = extra;
                }
            }
        }
        best
    }
}

pub fn load(
    assets: &VirtualAssets,
    models: &mut Vec<Model>,
    types: &[UnitType],
) -> Result<Animations, Box<dyn Error>> {
    let selected: Vec<_> = types.iter().map(|unit| unit.name.as_str()).collect();
    let (files, _) = crate::skirmish_data::source_files::effective(assets, &selected)?;
    let mut draws = Vec::new();
    let mut clips = BTreeMap::new();
    let mut checked_pairs = 0;
    let mut changing_pairs = 0;
    for stats in types {
        let object = definition(&files, "Object", &stats.name)?;
        let blocks = blocks(&object.lines, 0, "Draw")?;
        if external_draw(&stats.name, &blocks)? {
            if blocks[0][0].tokens[1] == "W3DLaserDraw" && stats.laser_update.is_none() {
                return Err(
                    format!("{} laser draw has no LaserUpdate consumer", stats.name).into(),
                );
            }
            draws.push(Vec::new());
            continue;
        }
        if blocks.is_empty() {
            return Err(format!("{} has no effective primary draw", stats.name).into());
        }
        let mut modules = Vec::new();
        for (module, lines) in blocks.iter().enumerate() {
            if module > 0
                && lines[0]
                    .tokens
                    .get(1)
                    .is_none_or(|name| !matches!(name.as_str(), "W3DModelDraw" | "W3DSupplyDraw"))
            {
                continue;
            }
            let mut draw =
                parse(lines).map_err(|error| format!("{} draw {module}: {error}", stats.name))?;
            draw.upgrade_show = stats
                .behavior
                .sub_object_upgrades
                .iter()
                .flat_map(|rules| rules.show.iter().cloned())
                .collect();
            let (checked, changing) =
                preload_draw(assets, &mut draw, models, &mut clips).map_err(|error| {
                    format!("{} draw {module} animation state: {error}", stats.name)
                })?;
            if lines[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "W3DSupplyDraw")
            {
                let warehouse = stats
                    .supply
                    .warehouse
                    .as_ref()
                    .ok_or("supply draw without warehouse module")?;
                let prefix = field(lines, 1, "SupplyBonePrefix")
                    .map(scalar)
                    .transpose()?;
                for model in draw.states.iter().filter_map(|state| state.render_model) {
                    draw.supply.insert(
                        model,
                        std::sync::Arc::new(crate::supply_presentation::SupplyDraw::new(
                            &models[model],
                            prefix,
                            warehouse.starting_boxes,
                            warehouse.delete_when_empty,
                        )?),
                    );
                }
            }
            checked_pairs += checked;
            changing_pairs += changing;
            if let Some(bone) = &draw.attach_bone {
                draw.attach_offset = modules
                    .iter()
                    .find_map(|prior: &Draw| {
                        models
                            .get(prior.states[prior.select(&BTreeSet::new())].model?)
                            .and_then(|model| model.bone_position(bone))
                    })
                    .unwrap_or([0.0; 3]);
            }
            modules.push(draw);
        }
        draws.push(modules);
    }
    let thresholds = damage_thresholds(assets)?;
    eprintln!(
        "Original animation preload: {} model draw modules, {} raw/timecoded clips; {checked_pairs} model/clip pairs checked, {changing_pairs} change pose between start and midpoint",
        draws.iter().map(Vec::len).sum::<usize>(),
        clips.len()
    );
    let mut animations = Animations::new(draws, clips, thresholds);
    animations.prepare_logical_bones(&files, models, types)?;
    animations.bind_weapon_templates(types);
    Ok(animations)
}

fn external_draw(name: &str, draws: &[&[ContentLine]]) -> Result<bool, Box<dyn Error>> {
    if draws.first().is_some_and(|draw| {
        draw[0]
            .tokens
            .get(1)
            .is_some_and(|module| module == "W3DLaserDraw")
    }) {
        // W3DLaserDraw is a DrawModule, not W3DModelDraw. Its LaserUpdate
        // endpoints and beam geometry are consumed by laser_beam.
        if draws.len() != 1 {
            return Err("multiple draws on a native laser template are unsupported".into());
        }
        return Ok(true);
    }
    if draws.first().is_some_and(|draw| {
        draw[0]
            .tokens
            .get(1)
            .is_some_and(|module| module == "W3DDefaultDraw")
    }) {
        // W3DDefaultDraw has no render object outside LOAD_TEST_ASSETS.
        if draws.len() != 1 || draws[0].len() != 2 {
            return Err("W3DDefaultDraw requires its original single empty module".into());
        }
        return Ok(true);
    }
    // CreateDebris supplies this draw's model, outside condition-state RNG.
    Ok(name == "GenericDebris"
        && draws.len() == 1
        && draws[0][0]
            .tokens
            .get(1)
            .is_some_and(|module| module == "W3DDebrisDraw"))
}

#[cfg(test)]
fn source_files(assets: &VirtualAssets) -> Result<Vec<content::ContentFile>, Box<dyn Error>> {
    let mut files = [
        "factionbuilding",
        "civilianbuilding",
        "civilianprop",
        "natureprop",
        "techbuildings",
        "civilianunit",
        "system",
        "hulk",
        "weaponobjects",
        "americaair",
        "americamiscunit",
        "chinaair",
        "chinamiscunit",
        "glaair",
        "glamiscunit",
        "gc_chem_glasystem",
        "americainfantry",
        "americavehicle",
        "chinainfantry",
        "chinavehicle",
        "glainfantry",
        "glavehicle",
    ]
    .into_iter()
    .map(|name| content::scan(assets.read(&format!("data/ini/object/{name}.ini"))?))
    .collect::<Result<Vec<_>, generals_assets::AssetError>>()?;
    for path in [
        "data/ini/crate.ini",
        "data/ini/gamedata.ini",
        "data/ini/default/object.ini",
    ] {
        files.push(content::scan(assets.read(path)?)?);
    }
    crate::skirmish_data::resolve_reskins(&mut files)?;
    Ok(files)
}

fn preload_draw(
    assets: &VirtualAssets,
    draw: &mut Draw,
    models: &mut Vec<Model>,
    clips: &mut BTreeMap<String, Clip>,
) -> Result<(usize, usize), Box<dyn Error>> {
    // W3DModelDraw validates every authored condition state before playback.
    // A client flag whitelist cannot determine the authoritative public-bone cache.
    let mut checked_pairs = 0;
    let mut changing_pairs = 0;
    for state in &mut draw.states {
        let (checked, changing) = load_state(assets, state, models, clips)?;
        checked_pairs += checked;
        changing_pairs += changing;
    }
    super::render_objects::bind(assets, draw, models)?;
    validate_missing_clips(draw, clips)?;
    Ok((checked_pairs, changing_pairs))
}

// A null Get_HAnim does not clear animation on a retained render object.
// Reject mixed handles until playback retains that independent animation identity.
fn validate_missing_clips(
    draw: &Draw,
    clips: &BTreeMap<String, Clip>,
) -> Result<(), Box<dyn Error>> {
    for state in &draw.states {
        let Some(model) = state.render_model else {
            continue;
        };
        if !state
            .motions
            .iter()
            .any(|motion| !clips.contains_key(&motion.name))
        {
            continue;
        }
        if draw
            .states
            .iter()
            .filter(|other| other.render_model == Some(model))
            .flat_map(|other| &other.motions)
            .any(|motion| clips.contains_key(&motion.name))
        {
            return Err(format!(
                "model {} mixes valid and unavailable animations on a retained render object",
                state.name
            )
            .into());
        }
    }
    Ok(())
}

impl Animations {
    fn bind_weapon_templates(&mut self, types: &[UnitType]) {
        self.animate_when_disabled = types
            .iter()
            .map(|unit| {
                unit.kind_of
                    .iter()
                    .any(|kind| kind == "PRODUCED_AT_HELIPAD")
            })
            .collect();
        self.weapon_contexts = types
            .iter()
            .map(|unit| {
                (0..unit.weapon_template_count())
                    .map(|index| {
                        let (slot, flags) = unit.weapon_context(index);
                        (slot, flags | if unit.battle_plan.is_some() { 8 } else { 0 })
                    })
                    .collect()
            })
            .collect();
        self.weapon_clips = types
            .iter()
            .map(|unit| {
                (0..unit.weapon_template_count())
                    .map(|index| {
                        unit.weapon_template(index)
                            .map_or(0, |weapon| weapon.clip_size)
                    })
                    .collect()
            })
            .collect();
    }

    pub fn load_world(
        &mut self,
        assets: &VirtualAssets,
        models: &mut Vec<Model>,
        objects: &crate::playtest_view::WorldObjects,
    ) -> Result<(), Box<dyn Error>> {
        let definitions = crate::world_data::object_definitions(assets)?;
        for object in &objects.0 {
            let Some(source) = &object.animation else {
                continue;
            };
            if self.world_draws.contains_key(&object.model_index) {
                continue;
            }
            let definition = definitions
                .get(&source.template)
                .ok_or("world animation template missing")?;
            let modules = blocks(&definition.lines, 0, "Draw")?;
            let lines = modules
                .into_iter()
                .find(|lines| lines[0].tokens.get(2) == Some(&source.module))
                .ok_or("world animation module missing")?;
            let mut draw = parse(lines)?;
            // Selected supply machinery declares one unconditional native loop.
            // Do not pretend that dynamic neutral object state has been ported.
            if draw.states.len() != 1
                || draw.states[0]
                    .conditions
                    .iter()
                    .any(|flags| !flags.is_empty())
            {
                return Err(format!(
                    "{} {} needs unported world model conditions",
                    source.template, source.module
                )
                .into());
            }
            load_state(assets, &mut draw.states[0], models, &mut self.clips)?;
            validate_missing_clips(&draw, &self.clips)?;
            self.world_draws.insert(object.model_index, draw);
        }
        Ok(())
    }
}

fn damage_thresholds(assets: &VirtualAssets) -> Result<[f32; 2], Box<dyn Error>> {
    let global = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let lines = &global
        .declarations
        .iter()
        .find(|item| item.kind == "GameData")
        .ok_or("missing GameData")?
        .lines;
    let thresholds = ["UnitDamagedThreshold", "UnitReallyDamagedThreshold"].map(|name| {
        field(lines, 0, name)
            .ok_or_else(|| format!("missing {name}"))
            .and_then(|line| scalar(line).map_err(|error| error.to_string()))
            .and_then(|value| value.parse::<f32>().map_err(|error| error.to_string()))
    });
    let [damaged, really_damaged] = thresholds;
    let thresholds = [damaged?, really_damaged?];
    if thresholds
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        || thresholds[0] < thresholds[1]
    {
        return Err("invalid source damage thresholds".into());
    }
    Ok(thresholds)
}

pub(super) fn load_state(
    assets: &VirtualAssets,
    state: &mut State,
    models: &mut Vec<Model>,
    clips: &mut BTreeMap<String, Clip>,
) -> Result<(usize, usize), Box<dyn Error>> {
    if state.name.is_empty() || state.name.eq_ignore_ascii_case("NONE") {
        return Ok((0, 0));
    }
    let lines = state
        .fields
        .iter()
        .enumerate()
        .map(|(index, tokens)| ContentLine {
            line: u32::try_from(index).expect("bounded state lines"),
            tokens: tokens.clone(),
            raw: String::new(),
            assigned: true,
            depth: Some(2),
        })
        .collect::<Vec<_>>();
    let path = format!("art/w3d/{}.w3d", state.name.to_ascii_lowercase());
    let model = match state_model(assets, &state.name, &lines) {
        Ok(model) => Some(model),
        Err(error) if matches!(error.downcast_ref::<AssetError>(), Some(AssetError::Missing(missing)) if missing == &path) =>
        {
            // W3DModelDraw::setModelState retains a null render object when
            // Create_Render_Obj cannot find the authored model prototype.
            eprintln!("Original draw model unavailable: {}", state.name);
            None
        }
        Err(error) => return Err(format!("{} model: {error}", state.name).into()),
    };
    let mut checked = BTreeSet::new();
    let mut checked_pairs = 0;
    let mut changing = 0;
    for motion in &state.motions {
        if !checked.insert(motion.name.as_str()) {
            continue;
        }
        if !clips.contains_key(&motion.name) {
            let Some(clip) = load_motion(assets, &motion.name)? else {
                continue;
            };
            clips.insert(motion.name.clone(), clip);
        }
        let clip = &clips[&motion.name];
        let Some(model) = &model else {
            continue;
        };
        checked_pairs += 1;
        let context = |error| {
            format!(
                "model {} with clip {} (declared hierarchy {}): {error}",
                state.name, clip.name, clip.hierarchy
            )
        };
        let first = model.pose(clip, 0.0).map_err(context)?;
        let middle = model
            .pose(clip, f32::from(clip.frame_count - 1) * 0.5)
            .map_err(context)?;
        if first.iter().zip(&middle).any(|(left, right)| {
            left.visible != right.visible
                || left
                    .positions
                    .iter()
                    .flatten()
                    .zip(right.positions.iter().flatten())
                    .any(|(left, right)| left.to_bits() != right.to_bits())
        }) {
            changing += 1;
        }
    }
    state.hlod = model.as_ref().is_some_and(|model| model.hlod);
    state.model = model.as_ref().map(|_| models.len());
    state.render_model = state.model;
    let Some(model) = model else {
        return Ok((0, 0));
    };
    models.push(model);
    Ok((checked_pairs, changing))
}

fn load_motion(assets: &VirtualAssets, name: &str) -> Result<Option<Clip>, Box<dyn Error>> {
    let suffix = name.rsplit('.').next().ok_or("animation name missing")?;
    let path = format!("art/w3d/{suffix}.w3d");
    let asset = match assets.read(&path) {
        Ok(asset) => asset,
        // Get_HAnim returns null after an unsuccessful file load and exact-name
        // retry. Decoder and I/O failures remain explicit admission errors.
        Err(AssetError::Missing(missing)) if missing == path => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some(mut clip) =
        Clip::load(&asset.bytes).map_err(|error| format!("{name} ({path}): {error}"))?
    else {
        return Ok(None);
    };
    if !clip.name.eq_ignore_ascii_case(name) {
        return Ok(None);
    }
    let hierarchy_path = format!("art/w3d/{}.w3d", clip.hierarchy);
    clip.bind_hierarchy(&assets.read(&hierarchy_path)?.bytes)
        .map_err(|error| format!("{name} hierarchy {hierarchy_path}: {error}"))?;
    Ok(Some(clip))
}

pub(super) fn parse(lines: &[ContentLine]) -> Result<Draw, Box<dyn Error>> {
    let mut states: Vec<State> = Vec::new();
    let mut default: Option<State> = None;
    for (index, line) in lines.iter().enumerate() {
        if line.depth != Some(1) {
            continue;
        }
        let Some(kind) = line.tokens.first() else {
            continue;
        };
        if kind == "AliasConditionState" {
            let previous = states
                .iter_mut()
                .rev()
                .find(|state| state.transition.is_none())
                .ok_or("alias without previous state")?;
            previous.conditions.push(flags(&line.tokens[1..]));
            continue;
        }
        if !matches!(
            kind.as_str(),
            "DefaultConditionState" | "ConditionState" | "TransitionState"
        ) {
            continue;
        }
        let end = lines[index + 1..]
            .iter()
            .position(|line| {
                line.depth == Some(1)
                    && line
                        .tokens
                        .first()
                        .is_some_and(|key| key.eq_ignore_ascii_case("End"))
            })
            .ok_or("unclosed draw state")?;
        let body = &lines[index + 1..index + 1 + end];
        let mut state = default.clone().unwrap_or_else(State::empty);
        state.conditions = vec![flags(&line.tokens[1..])];
        state.transition = None;
        if kind == "TransitionState" {
            // Native PARSE_TRANSITION consumes two keys without checking trailing tokens.
            let [_, from, to, ..] = line.tokens.as_slice() else {
                return Err(format!("TransitionState needs two keys at line {}", line.line).into());
            };
            if from.eq_ignore_ascii_case(to) {
                return Err(
                    format!("TransitionState repeats its key at line {}", line.line).into(),
                );
            }
            state.transition = Some((from.to_ascii_lowercase(), to.to_ascii_lowercase()));
            state.key.clear();
            state.finish_key.clear();
        }
        parse_fields(&mut state, body)?;
        if kind == "DefaultConditionState" {
            if !states.is_empty() || default.is_some() {
                return Err("invalid default state order".into());
            }
            default = Some(state.clone());
        }
        states.push(state);
    }
    if states.is_empty() {
        return Err("draw has no states".into());
    }
    let ignored = field(lines, 1, "IgnoreConditionStates")
        .map_or_else(BTreeSet::new, |line| flags(&line.tokens[1..]));
    Ok(Draw {
        supply: BTreeMap::new(),
        upgrade_show: Vec::new(),
        attach_bone: field(lines, 1, "AttachToBoneInAnotherModule")
            .map(scalar)
            .transpose()?
            .map(str::to_owned),
        attach_offset: [0.0; 3],
        receives_dynamic_lights: field(lines, 1, "ReceivesDynamicLights")
            .map(scalar)
            .transpose()?
            .map_or(Ok(true), |value| {
                match value.to_ascii_uppercase().as_str() {
                    "YES" => Ok(true),
                    "NO" => Ok(false),
                    _ => Err("invalid ReceivesDynamicLights"),
                }
            })?,
        requires_power: field(lines, 1, "AnimationsRequirePower")
            .map(scalar)
            .transpose()?
            .is_none_or(|value| !value.eq_ignore_ascii_case("No")),
        states,
        ignored,
        recoil: super::muzzle::Rules::parse(lines)?,
    })
}

fn flags(tokens: &[String]) -> BTreeSet<String> {
    tokens
        .iter()
        .filter(|token| !token.eq_ignore_ascii_case("NONE"))
        .map(|token| token.to_ascii_uppercase())
        .collect()
}

fn parse_fields(state: &mut State, lines: &[ContentLine]) -> Result<(), Box<dyn Error>> {
    let mut replaced_animations = false;
    for line in lines {
        let Some(key) = line.tokens.first() else {
            continue;
        };
        match key.as_str() {
            "Model" => state.name = scalar(line)?.to_ascii_lowercase(),
            "Animation" | "IdleAnimation" => {
                if !replaced_animations {
                    state.motions.clear();
                    replaced_animations = true;
                }
                read_motion(state, line)?;
            }
            "AnimationMode" => state.mode = scalar(line)?.to_ascii_uppercase(),
            "TransitionKey" => state.key = scalar(line)?.to_ascii_lowercase(),
            "WaitForStateToFinishIfPossible" => {
                state.finish_key = scalar(line)?.to_ascii_lowercase();
            }
            "Flags" => state.flags = flags(&line.tokens[1..]),
            "AnimationSpeedFactorRange" => {
                let [_, low, high] = line.tokens.as_slice() else {
                    return Err("invalid animation speed range".into());
                };
                state.speed = [low.parse()?, high.parse()?];
                if state
                    .speed
                    .iter()
                    .any(|speed| !speed.is_finite() || *speed <= 0.0)
                    || state.speed[0] > state.speed[1]
                {
                    return Err("invalid animation speed factors".into());
                }
            }
            _ => {}
        }
        if matches!(key.as_str(), "HideSubObject" | "ShowSubObject")
            && line
                .tokens
                .get(1)
                .is_some_and(|name| name.eq_ignore_ascii_case("NONE"))
        {
            state
                .fields
                .retain(|tokens| !matches!(tokens[0].as_str(), "HideSubObject" | "ShowSubObject"));
        } else {
            state.fields.push(line.tokens.clone());
        }
    }
    if !matches!(
        state.mode.as_str(),
        "ONCE" | "LOOP" | "ONCE_BACKWARDS" | "LOOP_BACKWARDS" | "LOOP_PINGPONG" | "MANUAL"
    ) {
        return Err(format!("unsupported animation mode {}", state.mode).into());
    }
    Ok(())
}

fn read_motion(state: &mut State, line: &ContentLine) -> Result<(), Box<dyn Error>> {
    let name = line
        .tokens
        .get(1)
        .ok_or("animation name missing")?
        .to_ascii_lowercase();
    if name == "none" {
        return Ok(());
    }
    let distance = line
        .tokens
        .get(2)
        .map_or(Ok(0.0), |value| value.parse::<f32>())?;
    let repeats = line
        .tokens
        .get(3)
        .map_or(Ok(1), |value| value.parse::<i32>())?
        .max(1);
    if repeats > 1024 || !distance.is_finite() || distance < 0.0 {
        return Err("invalid animation weight/distance".into());
    }
    for _ in 0..repeats {
        state.motions.push(Motion {
            name: name.clone(),
            distance,
            idle: line.tokens[0] == "IdleAnimation",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Animations, BTreeMap, VirtualAssets, blocks, definition, flags, parse, preload_draw,
        source_files,
    };
    use generals_assets::{content, virtual_assets::Asset};

    #[test]
    fn default_draw_is_a_real_empty_draw_for_any_authored_template() {
        let file = content::scan(Asset {
            path: "fixture.ini".into(),
            origin: "test".into(),
            bytes: b"Object Invisible\nDraw=W3DDefaultDraw Tag\nEnd\nEnd\n".to_vec(),
        })
        .unwrap();
        let draws = blocks(&file.declarations[0].lines, 0, "Draw").unwrap();
        assert!(super::external_draw("Invisible", &draws).unwrap());
        let file = content::scan(Asset {
            path: "fixture.ini".into(),
            origin: "test".into(),
            bytes: b"Object Invalid\nDraw=W3DDefaultDraw Tag\nModel=Unexpected\nEnd\nEnd\n"
                .to_vec(),
        })
        .unwrap();
        let draws = blocks(&file.declarations[0].lines, 0, "Draw").unwrap();
        assert!(super::external_draw("Invalid", &draws).is_err());
    }

    #[test]
    fn transition_uses_first_two_keys_in_the_installed_command_center_declaration() {
        let file = content::scan(Asset {
            path: "fixture.ini".into(), origin: "test".into(),
            bytes: b"Object AmericaCommandCenter\nDraw=W3DModelDraw Tag\nDefaultConditionState\nModel=BASE\nEnd\nTransitionState=UP_SNOW NIGHT DOWN_DEFAULT\nAnimation=BASE.DOWN\nEnd\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let draw = parse(&file.declarations[0].lines).unwrap();
        assert_eq!(
            draw.states[1].transition.as_ref(),
            Some(&("up_snow".into(), "night".into()))
        );
        assert_eq!(draw.states[1].name, "base");
        assert_eq!(draw.states[1].motions[0].name, "base.down");
    }

    #[test]
    fn transition_rejects_missing_or_identical_keys() {
        for header in ["ONLY_ONE", "SAME same"] {
            let file = content::scan(Asset {
                path: "fixture.ini".into(), origin: "test".into(),
                bytes: format!("Object Item\nDraw=W3DModelDraw Tag\nDefaultConditionState\nModel=BASE\nEnd\nTransitionState={header}\nEnd\nEnd\nEnd\n").into_bytes(),
            }).unwrap();
            let Err(error) = parse(&file.declarations[0].lines) else {
                panic!("invalid transition accepted");
            };
            assert!(error.to_string().contains("TransitionState"));
        }
    }

    #[test]
    fn default_animation_is_replaced_while_model_and_alias_are_inherited() {
        let file = content::scan(Asset { path: "fixture.ini".into(), origin: "test".into(), bytes: b"Object Infantry\nDraw=W3DModelDraw Tag\nDefaultConditionState\nModel=BODY\nIdleAnimation=RIG.IDLE 0 9\nAnimationMode=ONCE\nEnd\nConditionState=MOVING\nAnimation=RIG.RUN 30\nAnimationMode=LOOP\nEnd\nAliasConditionState=MOVING CARRYING\nEnd\nEnd\n".to_vec() }).unwrap();
        let draw = parse(&file.declarations[0].lines).unwrap();
        assert_eq!(draw.states[0].motions.len(), 9);
        let moving = &draw.states[1];
        assert_eq!(moving.name, "body");
        assert_eq!(moving.motions.len(), 1);
        assert_eq!(moving.motions[0].name, "rig.run");
        assert_eq!(moving.conditions.len(), 2);
        assert_eq!(
            draw.select(&flags(&["MOVING".into(), "CARRYING".into()])),
            1
        );
        assert_eq!(draw.select(&flags(&["CARRYING".into()])), 1);
    }

    #[test]
    fn condition_none_does_not_create_default_inheritance() {
        let file = content::scan(Asset { path: "fixture.ini".into(), origin: "test".into(), bytes: b"Object Building\nDraw=W3DModelDraw Tag\nConditionState=NONE\nModel=BASE\nAnimation=BASE.LOOP\nEnd\nConditionState=DAMAGED\nModel=BROKEN\nEnd\nEnd\nEnd\n".to_vec() }).unwrap();
        let draw = parse(&file.declarations[0].lines).unwrap();
        assert!(draw.states[1].motions.is_empty());
        assert_eq!(draw.states[1].name, "broken");
        assert_eq!(draw.select(&flags(&["UNRELATED".into()])), 0);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn authored_model_states_load_public_bones_without_a_client_flag_whitelist() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let source = source_files(&assets).unwrap();
        let object = definition(&source, "Object", "AmericaParachute").unwrap();
        let mut unit = crate::skirmish_data::parachute::tests::installed_unit(&assets);
        let modules = blocks(&object.lines, 0, "Draw").unwrap();
        let original = parse(modules[0]).unwrap();
        let name = &original.states[original.select(&std::collections::BTreeSet::new())].name;
        let conditions = [
            "PRONE",
            "FREEFALL",
            "SPECIAL_CHEERING",
            "MOVING FIRING_A",
            "POWER_PLANT_UPGRADED",
            "USER_1",
            "GARRISONED",
            "DOOR_1_WAITING_TO_CLOSE",
        ];
        let mut text = String::from(
            "Object AmericaParachute\nDraw = W3DModelDraw Tag\nDefaultConditionState\nModel = NONE\nEnd\n",
        );
        for condition in conditions {
            use std::fmt::Write as _;
            writeln!(text, "ConditionState = {condition}\nModel = {name}\nEnd").unwrap();
        }
        text.push_str("End\nEnd\n");
        let fixture = content::scan(Asset {
            path: "pristine.ini".into(),
            origin: "condition-state public-bone regression".into(),
            bytes: text.into_bytes(),
        })
        .unwrap();
        let modules = blocks(&fixture.declarations[0].lines, 0, "Draw").unwrap();
        let mut draw = parse(modules[0]).unwrap();
        let mut models = Vec::new();
        let mut clips = BTreeMap::new();
        preload_draw(&assets, &mut draw, &mut models, &mut clips).unwrap();
        for condition in conditions {
            let flags = condition.split_whitespace().map(str::to_owned).collect();
            let index = draw.select(&flags);
            assert_ne!(index, 0, "{condition}");
            assert!(draw.states[index].model.is_some(), "{condition}");
        }
        let mut files = Vec::new();
        for path in ["data/ini/gamedata.ini", "data/ini/default/object.ini"] {
            files.push(content::scan(assets.read(path).unwrap()).unwrap());
        }
        files.push(fixture);
        let mut animations = Animations::new(vec![vec![draw]], clips, [0.5, 0.25]);
        animations
            .prepare_logical_bones(&files, &models, std::slice::from_ref(&unit))
            .unwrap();
        animations.bind_logical_bones(std::slice::from_mut(&mut unit));
        for (state, condition) in unit.pristine_draws[0].states[1..].iter().zip(conditions) {
            assert!(state.transforms.contains_key("para_attch"), "{condition}");
        }
    }
}
