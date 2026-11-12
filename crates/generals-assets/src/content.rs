//! Source-backed lexical census of installed INIs, not a runtime INI loader.
//!
//! Reference: `SuperHackers` `2bfaea60`, `Core/.../INI/INI.cpp` (`readLine`,
//! `theTypeTable`, `initFromINIMulti`) and `INI.h::getSeps`. Semicolons end
//! semantic text even inside quotes; equals signs are default separators; `//`
//! is not comment syntax. Tokens retain quotes because string interpretation
//! belongs to individual field parsers. Non-ASCII bytes map one-to-one to Latin-1
//! code points for a reversible census, without claiming a runtime encoding.
//!
//! Declaration candidates are identified by the pinned top-level registry and
//! header arity. Ordered raw lines are preserved. Known nested block openers
//! provide depth, but unsupported structure is diagnosed, never treated as
//! resolved inheritance, includes, overrides, or effective gameplay semantics.
use crate::{AssetError, invalid, virtual_assets::Asset};

/// One source line with the pinned engine's default token separators.
pub struct ContentLine {
    pub line: u32,
    pub tokens: Vec<String>,
    /// Original line excluding its newline; comments and whitespace retained.
    pub raw: String,
    /// Whether an equals sign occurs before the first comment or NUL.
    pub assigned: bool,
    /// Direct declaration fields are zero; nested fields are greater than zero.
    /// Headers and their closing `End` have the same depth. Unknown is explicit.
    pub depth: Option<u32>,
}

/// A lexical declaration candidate, without merging or resolving its parent.
pub struct Declaration {
    pub kind: String,
    /// Empty for singleton declarations such as `GameData`.
    pub name: String,
    /// The source parent for `ObjectReskin` or an unsupported `ChildObject`.
    pub parent: Option<String>,
    pub line: u32,
    pub header: ContentLine,
    /// Ordered body lines, excluding the header and including closing `End`.
    pub lines: Vec<ContentLine>,
}

/// A located limitation or malformed construct; no affected text is discarded.
pub struct Diagnostic {
    pub line: u32,
    pub code: String,
    pub message: String,
}

/// An installed file's lexical declarations and unresolved constructs.
pub struct ContentFile {
    pub path: String,
    pub origin: String,
    pub declarations: Vec<Declaration>,
    pub diagnostics: Vec<Diagnostic>,
    /// Nonempty semantic lines before the first recognized declaration.
    pub unclassified: Vec<ContentLine>,
}

const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 65_536;
const MAX_LINES: usize = 500_000;
const MAX_DEPTH: usize = 64;
const INLINE_DECLARATIONS: &[&str] = &["BenchProfile", "LODPreset", "ReallyLowMHz"];

// INI.cpp::theTypeTable at the pinned revision. ChildObject is intentionally
// absent: it is detected separately as an unsupported extension.
const TOP_LEVEL: &[&str] = &[
    "AIData",
    "Animation",
    "Armor",
    "AudioEvent",
    "AudioSettings",
    "BenchProfile",
    "Bridge",
    "Campaign",
    "ChallengeGenerals",
    "CommandButton",
    "CommandMap",
    "CommandSet",
    "ControlBarResizer",
    "ControlBarScheme",
    "CrateData",
    "Credits",
    "DamageFX",
    "DialogEvent",
    "DrawGroupInfo",
    "DynamicGameLOD",
    "EvaEvent",
    "FXList",
    "GameData",
    "HeaderTemplate",
    "InGameUI",
    "LODPreset",
    "Language",
    "Locomotor",
    "MapCache",
    "MapData",
    "MappedImage",
    "MiscAudio",
    "Mouse",
    "MouseCursor",
    "MultiplayerColor",
    "MultiplayerSettings",
    "MultiplayerStartingMoneyChoice",
    "MusicTrack",
    "Object",
    "ObjectCreationList",
    "ObjectReskin",
    "OnlineChatColors",
    "ParticleSystem",
    "PlayerTemplate",
    "Rank",
    "ReallyLowMHz",
    "Road",
    "Science",
    "ScriptAction",
    "ScriptCondition",
    "ShellMenuScheme",
    "SpecialPower",
    "StaticGameLOD",
    "Terrain",
    "Upgrade",
    "Video",
    "WaterSet",
    "WaterTransparency",
    "Weapon",
    "Weather",
    "WebpageURL",
    "WindowTransition",
];

const SINGLETONS: &[&str] = &[
    "AIData",
    "AudioSettings",
    "ChallengeGenerals",
    "Credits",
    "DrawGroupInfo",
    "GameData",
    "InGameUI",
    "Language",
    "MiscAudio",
    "Mouse",
    "MultiplayerSettings",
    "MultiplayerStartingMoneyChoice",
    "OnlineChatColors",
    "WaterTransparency",
    "Weather",
];

/// Inspect one file without executing directives or resolving gameplay meaning.
///
/// Header recognition uses the pinned registry and observed header forms. All
/// body lines, duplicate fields and declaration order survive. Diagnostics mark
/// unsupported extensions, uncertain boundaries, and unresolved nesting. This
/// census is deliberately separate from the selected-unit runtime INI reader.
///
/// # Errors
/// Rejects files over 16 MiB, physical lines over 64 KiB, more than 500000 lines,
/// or recognized nesting deeper than 64. Syntax limitations produce located
/// diagnostics instead of silently losing the affected source text.
pub fn scan(asset: Asset) -> Result<ContentFile, AssetError> {
    if asset.bytes.len() > MAX_FILE_BYTES {
        return Err(invalid("content census INI exceeds 16 MiB"));
    }
    let mut file = ContentFile {
        path: asset.path,
        origin: asset.origin,
        declarations: Vec::new(),
        diagnostics: Vec::new(),
        unclassified: Vec::new(),
    };
    for (index, bytes) in asset.bytes.split(|byte| *byte == b'\n').enumerate() {
        if index >= MAX_LINES || bytes.len() > MAX_LINE_BYTES {
            return Err(invalid("content census line count or length exceeds limit"));
        }
        let number = u32::try_from(index + 1).map_err(|_| invalid("too many INI lines"))?;
        let line = lex_line(bytes, number, &mut file.diagnostics);
        if line.tokens.is_empty() {
            continue;
        }
        if is_header(&line) {
            let declaration = declaration(line, &mut file.diagnostics);
            file.declarations.push(declaration);
        } else if let Some(current) = file.declarations.last_mut() {
            current.lines.push(line);
        } else {
            diagnostic(
                &mut file.diagnostics,
                number,
                "unclassified-line",
                "semantic text outside a recognized declaration",
            );
            file.unclassified.push(line);
        }
    }
    for declaration in &mut file.declarations {
        classify_depth(declaration, &mut file.diagnostics)?;
    }
    Ok(file)
}

fn diagnostic(diagnostics: &mut Vec<Diagnostic>, line: u32, code: &str, message: &str) {
    diagnostics.push(Diagnostic {
        line,
        code: code.to_owned(),
        message: message.to_owned(),
    });
}

fn lex_line(bytes: &[u8], line: u32, diagnostics: &mut Vec<Diagnostic>) -> ContentLine {
    let raw: String = bytes.iter().copied().map(char::from).collect();
    let end = bytes
        .iter()
        .position(|byte| matches!(byte, b';' | 0))
        .unwrap_or(bytes.len());
    let semantic: String = bytes[..end]
        .iter()
        .map(|byte| if *byte < 32 { ' ' } else { char::from(*byte) })
        .collect();
    let tokens: Vec<String> = semantic
        .split([' ', '='])
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect();
    if bytes.len() >= 1028 {
        diagnostic(
            diagnostics,
            line,
            "source-line-limit",
            "physical line reaches the pinned engine's 1028-byte line buffer; census does not emulate truncation",
        );
    }
    if bytes.contains(&0) {
        diagnostic(
            diagnostics,
            line,
            "embedded-nul",
            "semantic text ends at embedded NUL; remaining original bytes retained in raw",
        );
    }
    if tokens
        .first()
        .is_some_and(|token| token.starts_with(['#', '$']))
    {
        diagnostic(
            diagnostics,
            line,
            "unsupported-directive",
            "directive or macro retained but not executed; absent from the pinned top-level INI registry",
        );
    }
    if tokens.first().is_some_and(|token| token.starts_with("//")) {
        diagnostic(
            diagnostics,
            line,
            "unsupported-slash-comment",
            "the pinned INI reader does not treat // as a comment",
        );
    }
    if semantic.bytes().filter(|byte| *byte == b'"').count() % 2 != 0 {
        diagnostic(
            diagnostics,
            line,
            "unbalanced-quote",
            "quotes are parser-specific; default tokens and original text retained without string interpretation",
        );
    }
    ContentLine {
        line,
        tokens,
        raw,
        assigned: semantic.contains('='),
        depth: None,
    }
}

fn is_header(line: &ContentLine) -> bool {
    let Some(kind) = line.tokens.first() else {
        return false;
    };
    // These non-colliding top-level forms use '=' in installed files. Other
    // registry names also occur as assigned fields, so '=' needs context.
    if line.assigned
        && ![
            "StaticGameLOD",
            "DynamicGameLOD",
            "BenchProfile",
            "LODPreset",
            "ReallyLowMHz",
        ]
        .contains(&kind.as_str())
    {
        return false;
    }
    if kind.eq_ignore_ascii_case("ChildObject") {
        return true;
    }
    if !TOP_LEVEL
        .iter()
        .any(|candidate| kind.eq_ignore_ascii_case(candidate))
    {
        return false;
    }
    if SINGLETONS
        .iter()
        .any(|candidate| kind.eq_ignore_ascii_case(candidate))
    {
        return line.tokens.len() == 1;
    }
    // The same words appear as fields or nested blocks. Their source header
    // arity distinguishes Armor DEFAULT 100%, Weapon PRIMARY Name, and the
    // anonymous ParticleSystem block inside an FXList from top-level records.
    if ["Armor", "Weapon", "Locomotor", "ParticleSystem"]
        .iter()
        .any(|candidate| kind.eq_ignore_ascii_case(candidate))
    {
        return line.tokens.len() == 2;
    }
    line.tokens.len() >= 2
}

fn declaration(header: ContentLine, diagnostics: &mut Vec<Diagnostic>) -> Declaration {
    let kind = header.tokens[0].clone();
    let inherited =
        kind.eq_ignore_ascii_case("ObjectReskin") || kind.eq_ignore_ascii_case("ChildObject");
    let name = header.tokens.get(1).cloned().unwrap_or_default();
    let parent = inherited.then(|| header.tokens.get(2).cloned()).flatten();
    if kind.eq_ignore_ascii_case("ChildObject") {
        diagnostic(
            diagnostics,
            header.line,
            "unsupported-child-object",
            "ChildObject is not in the pinned source registry; inheritance is not interpreted",
        );
    } else if !TOP_LEVEL.contains(&kind.as_str()) {
        diagnostic(
            diagnostics,
            header.line,
            "header-case-mismatch",
            "pinned top-level registry uses case-sensitive matching",
        );
    }
    if inherited {
        diagnostic(
            diagnostics,
            header.line,
            "unresolved-parent",
            "parent recorded only; ObjectReskin is restricted reskinning, not general inheritance",
        );
    }
    let expected = if ["BenchProfile", "LODPreset"].contains(&kind.as_str()) {
        6
    } else if inherited {
        3
    } else if SINGLETONS.contains(&kind.as_str()) {
        1
    } else {
        2
    };
    if header.tokens.len() != expected {
        diagnostic(
            diagnostics,
            header.line,
            "header-arity",
            "unexpected header arguments retained; boundary is a census candidate, not a validated runtime declaration",
        );
    }
    Declaration {
        kind,
        name,
        parent,
        line: header.line,
        header,
        lines: Vec::new(),
    }
}

fn classify_depth(
    declaration: &mut Declaration,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), AssetError> {
    let mut stack: Vec<String> = Vec::new();
    // GameLOD.cpp parses these values directly from the header, without End.
    let mut closed = INLINE_DECLARATIONS.contains(&declaration.kind.as_str());
    let mut uncertain = false;
    for line in &mut declaration.lines {
        let Some(key) = line.tokens.first() else {
            continue;
        };
        if closed {
            if !uncertain {
                diagnostic(
                    diagnostics,
                    line.line,
                    "uncertain-boundary",
                    "semantic text follows the apparent closing End; unknown nesting or declaration syntax needs review",
                );
            }
            uncertain = true;
            continue;
        }
        if key.eq_ignore_ascii_case("End") {
            if stack.is_empty() {
                closed = true;
            } else {
                stack.pop();
            }
            line.depth =
                Some(u32::try_from(stack.len()).map_err(|_| invalid("content nesting overflow"))?);
            continue;
        }
        line.depth =
            Some(u32::try_from(stack.len()).map_err(|_| invalid("content nesting overflow"))?);
        if opens_block(&declaration.kind, stack.last().map(String::as_str), line) {
            stack.push(key.clone());
            if stack.len() > MAX_DEPTH {
                return Err(invalid("content census nesting exceeds 64"));
            }
        }
    }
    if !closed {
        diagnostic(
            diagnostics,
            declaration.line,
            "unclosed-declaration",
            "missing closing End or unclassified nesting; body retained for inspection",
        );
        uncertain = true;
    }
    if uncertain {
        for line in &mut declaration.lines {
            line.depth = None;
        }
    }
    Ok(())
}

fn opens_block(kind: &str, parent: Option<&str>, line: &ContentLine) -> bool {
    let key = line.tokens[0].as_str();
    if matches!(kind, "Object" | "ObjectReskin" | "ChildObject") {
        // ThingTemplate's FieldParse plus W3D model/tank drawing, AIUpdate's
        // turrets, and source object module wrappers. RemoveModule is scalar.
        if ["Turret", "AltTurret"].contains(&key) {
            return line.tokens.len() == 1;
        }
        return [
            "Draw",
            "Body",
            "Behavior",
            "ClientUpdate",
            "WeaponSet",
            "ArmorSet",
            "DefaultConditionState",
            "ConditionState",
            "TransitionState",
            "UnitSpecificSounds",
            "UnitSpecificFX",
            "Prerequisites",
            "AddModule",
            "ReplaceModule",
            "InheritableModule",
            "OverrideableByLikeKind",
            "AttackAreaDecal",
            "TargetingReticleDecal",
            "GridDecalTemplate",
            "DeliveryDecal",
        ]
        .contains(&key);
    }
    if kind == "ChallengeGenerals" {
        return key.strip_prefix("GeneralPersona").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        });
    }
    if kind == "ObjectCreationList" {
        if parent.is_some() {
            // DeliverPayload's FireWeapon is a Bool field, not an OCL nugget.
            return key == "DeliveryDecal";
        }
        return [
            "CreateObject",
            "CreateDebris",
            "ApplyRandomForce",
            "DeliverPayload",
            "FireWeapon",
            "Attack",
            "DeliveryDecal",
        ]
        .contains(&key);
    }
    if kind == "FXList" && parent.is_none() {
        // FXList.cpp::TheFXParseTable: each nugget consumes an End block.
        return [
            "Sound",
            "ViewShake",
            "TerrainScorch",
            "ParticleSystem",
            "FXListAtBonePos",
            "LightPulse",
            "Tracer",
        ]
        .contains(&key);
    }
    auxiliary_block(kind, key)
}

fn auxiliary_block(kind: &str, key: &str) -> bool {
    // Each of these pinned FieldParse handlers calls initFromINI: Eva.cpp's
    // parseSideSoundsList, CampaignManager.cpp's parseMission, GameWindowTransitions.cpp's
    // parseWindow, ControlBarScheme.cpp's parseImagePart/parseAnimatingPart,
    // AI.cpp's parseSideInfo/parseSkillSet/parseSkirmishBuildList and BuildListInfo::parseStructure.
    match kind {
        "EvaEvent" => key == "SideSounds",
        "Campaign" => key == "Mission",
        "WindowTransition" => key == "Window",
        "ControlBarScheme" => ["ImagePart", "AnimatingPart"].contains(&key),
        "AIData" => [
            "SideInfo",
            "SkillSet1",
            "SkillSet2",
            "SkillSet3",
            "SkillSet4",
            "SkillSet5",
            "SkirmishBuildList",
            "Structure",
        ]
        .contains(&key),
        // InGameUI.cpp's radius-cursor fields all use parseRadiusDecalTemplate.
        "InGameUI" => key.ends_with("RadiusCursor"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::scan;
    use crate::virtual_assets::Asset;

    fn file(text: &[u8]) -> super::ContentFile {
        scan(Asset {
            path: "data/ini/object/test.ini".to_owned(),
            origin: "fixture.big".to_owned(),
            bytes: text.to_vec(),
        })
        .expect("bounded census")
    }

    #[test]
    fn source_lexer_retains_duplicates_and_does_not_invent_comments() {
        let result = file(b"Object Tank\n KindOf = VEHICLE SELECTABLE\n KindOf=CAN_ATTACK\n Name = \"a;b\"\n // visible\nEnd\n");
        let object = &result.declarations[0];
        assert_eq!(object.lines[0].tokens, ["KindOf", "VEHICLE", "SELECTABLE"]);
        assert_eq!(object.lines[1].tokens, ["KindOf", "CAN_ATTACK"]);
        assert_eq!(object.lines[2].tokens, ["Name", "\"a"]);
        assert_eq!(object.lines[2].raw, " Name = \"a;b\"");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.code == "unbalanced-quote")
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.code == "unsupported-slash-comment")
        );
    }

    #[test]
    fn retains_reskins_module_wrappers_and_nested_weapon_fields() {
        let result = file(b"Object Tank\n AddModule\n Behavior = AIUpdateInterface Tag\n Turret\n TurretTurnRate = 90\n End\n End\n End\n WeaponSet\n Weapon PRIMARY Cannon\n End\n RemoveModule = Gone\nEnd\nObjectReskin Skin Tank\n Draw = W3DModelDraw DrawTag\n DefaultConditionState\n Model = AVTank\n End\n End\nEnd\n");
        assert_eq!(result.declarations.len(), 2);
        let tank = &result.declarations[0];
        assert_eq!(tank.lines[3].depth, Some(3));
        assert_eq!(tank.lines[8].tokens, ["Weapon", "PRIMARY", "Cannon"]);
        assert_eq!(tank.lines[8].depth, Some(1));
        assert_eq!(result.declarations[1].parent.as_deref(), Some("Tank"));
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "unresolved-parent");
    }

    #[test]
    fn directives_and_unknown_nesting_remain_explicit() {
        let result = file(
            b"#include other.ini\nChildObject Child Parent\n MysteryBlock\n Value=1\n End\nEnd\n",
        );
        assert_eq!(result.unclassified[0].line, 1);
        assert_eq!(result.declarations[0].parent.as_deref(), Some("Parent"));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.code == "unsupported-directive")
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.code == "unsupported-child-object")
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.code == "uncertain-boundary")
        );
        assert!(
            result.declarations[0]
                .lines
                .iter()
                .all(|line| line.depth.is_none())
        );
    }

    #[test]
    fn anonymous_fx_particle_and_persona_blocks_keep_boundaries() {
        let result = file(b"FXList Hit\n ParticleSystem\n Name=Smoke\n End\nEnd\nParticleSystem Smoke\n Lifetime=2 2\nEnd\nChallengeGenerals\n GeneralPersona9\n PlayerTemplate=FactionBossGeneral\n StartsEnabled=no\n End\nEnd\n");
        assert_eq!(result.declarations.len(), 3);
        assert_eq!(result.declarations[2].lines[1].depth, Some(1));
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn assigned_turret_bones_are_fields_but_ai_turrets_are_blocks() {
        let result = file(b"Object Tank\nDraw=W3DTankDraw DrawTag\nDefaultConditionState\nTurret=TurretBone\nEnd\nEnd\nBehavior=AIUpdateInterface AI\nTurret\nTurretTurnRate=180\nEnd\nEnd\nEnd\n");
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.declarations[0].lines[2].depth, Some(2));
        assert_eq!(result.declarations[0].lines[7].depth, Some(2));
    }

    #[test]
    fn lod_headers_use_equals_and_inline_records_do_not_require_end() {
        let result = file(b"StaticGameLOD = High\nMaxParticleCount=100\nEnd\nBenchProfile=K7 1009 4.6 10.5 6.0\nReallyLowMHz=600\nLODPreset=HIGH K7 2000 GF4 512\n");
        assert_eq!(result.declarations.len(), 4);
        assert!(result.diagnostics.is_empty());
        assert!(result.declarations[1].lines.is_empty());
    }

    #[test]
    fn payload_weapon_flag_does_not_open_a_fire_weapon_nugget() {
        let result = file(b"ObjectCreationList Strike\nDeliverPayload\nFireWeapon=Yes\nDeliveryDecal\nTexture=Target\nEnd\nEnd\nFireWeapon\nWeapon=ArtilleryGun\nEnd\nEnd\n");
        assert!(result.diagnostics.is_empty());
        let lines = &result.declarations[0].lines;
        assert_eq!(lines[1].depth, Some(1));
        assert_eq!(lines[3].depth, Some(2));
        assert_eq!(lines[7].depth, Some(1));
    }
}
