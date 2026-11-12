mod aircraft_capture;
mod aircraft_crash_capture;
mod aircraft_pose;
mod aircraft_verification;
mod angry_mob_capture;
mod angry_mob_verification;
mod artillery_capture;
mod artillery_verification;
mod attack_cursor;
mod aurora_capture;
mod aurora_verification;
mod battle_bus_capture;
mod battle_bus_verification;
mod capture_framing;
mod capture_verification;
mod china_aircraft_capture;
mod china_aircraft_verification;
mod cleanup_verification;
mod container_data;
mod content_overlay;
mod control_bar;
mod crawler_verification;
mod defense_performance;
mod demo_trap_capture;
mod demo_trap_verification;
mod diagnostics;
mod disarm_capture;
mod disarm_verification;
mod drone_capture;
mod drone_verification;
mod dynamic_lod;
mod ecm_capture;
mod ecm_verification;
mod fake_building_capture;
mod fake_building_verification;
mod feedback_verification;
mod fire_capture;
mod fire_verification;
mod firebase_capture;
mod firebase_verification;
mod fonts;
mod ground_integration_verification;
mod hacker_verification;
mod helix_capture;
mod helix_verification;
mod hijacker_capture;
mod hijacker_feedback;
mod hijacker_verification;
mod horde_decal;
mod horde_verification;
mod income;
mod income_verification;
mod infantry_verification;
mod infiltrator_capture;
mod infiltrator_verification;
mod input;
mod jarmen_capture;
mod jarmen_verification;
mod laser_beam;
mod light_environment;
mod light_pulse;
mod lightpulse_capture;
mod lockon_cursor;
mod maintenance_verification;
mod map_scripts;
mod map_selection;
mod microwave_capture;
mod microwave_verification;
mod model_animation;
mod model_render;
mod model_upload;
mod move_hint;
mod neutral_expanded_capture;
mod neutral_expanded_verification;
mod neutral_tech_capture;
mod neutral_tech_verification;
mod notifications;
mod opponent_data;
mod opponent_verification;
mod options_preferences;
mod outpost_capture;
mod outpost_verification;
mod paladin_capture;
mod particles;
mod physical_debris;
mod placement_verification;
mod playtest;
mod playtest_ui;
mod playtest_view;
mod point_defense_capture;
mod point_defense_verification;
mod production_verification;
mod progression_data;
mod progression_verification;
mod projectile_data;
mod projectile_render;
mod projectile_stream;
mod projectile_verification;
mod radar;
mod radius_cursor;
mod reinforcement_pad_capture;
mod render_performance;
mod research_capture;
mod results_verification;
mod road_view;
mod sabotage_feedback;
mod sabotage_verification;
mod scenery_clear;
mod scorch;
mod selection_feedback;
mod sentry_capture;
mod sentry_verification;
mod skirmish_data;
mod skirmish_preferences;
mod skirmish_verification;
mod sorted_view;
mod source_audio;
mod source_material;
mod source_texture;
mod speaker_capture;
mod speaker_verification;
mod spectre_decals;
mod stealth_fighter_capture;
mod stealth_fighter_verification;
mod stealth_unit_capture;
mod stealth_view;
mod stinger_capture;
mod stinger_verification;
mod strategy_capture;
mod strategy_verification;
mod structure_capture;
mod structure_view;
mod supply_animation_verification;
mod supply_data;
mod supply_presentation;
mod supply_verification;
mod sway;
mod technical_verification;
mod terrain;
mod terrorist_capture;
mod terrorist_verification;
mod tractor_capture;
mod tractor_verification;
mod transport_verification;
mod tree_buffer;
mod tree_material;
mod tunnel_verification;
mod usa_research_verification;
mod veterancy_feedback;
mod visibility_data;
mod visibility_view;
mod w3d_emitter;
mod water_view;
mod waypoint_display;
mod weapon_fx;
mod weapon_verification;
mod weather_view;
mod world_capture;
mod world_data;

use std::{error::Error, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Generals Rust: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.iter().any(|arg| arg == "--help") {
        print_help();
        return Ok(());
    }
    validate_arguments(&arguments)?;
    let root = generals_assets::game_root()?;
    std::fs::create_dir_all(root.join("output"))?;
    if arguments.iter().any(|arg| arg == "--benchmark-defenses") {
        defense_performance::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--benchmark-render") {
        render_performance::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--benchmark-skirmish") {
        opponent_verification::benchmark(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-multi-ai") {
        opponent_verification::multiple(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-opponent") {
        opponent_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-skirmish") {
        skirmish_verification::run(&root)?;
    } else if arguments
        .iter()
        .any(|arg| arg == "--verify-neutral-expanded")
    {
        neutral_expanded_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-neutral-tech") {
        neutral_tech_verification::run(&root)?;
    } else if arguments
        .iter()
        .any(|arg| arg == "--verify-ground-integration")
    {
        ground_integration_verification::run(&root, false)?;
    } else if arguments.iter().any(|arg| arg == "--verify-speaker") {
        speaker_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-outpost") {
        outpost_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-aircraft") {
        aircraft_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-maps") {
        playtest::verify_maps(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-angry-mob") {
        angry_mob_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-microwave") {
        angry_mob_verification::run_with(&root, microwave_verification::verify)?;
    } else if arguments.iter().any(|arg| arg == "--verify-ecm") {
        angry_mob_verification::run_with(&root, ecm_verification::verify)?;
    } else if arguments.iter().any(|arg| arg == "--verify-battle-bus") {
        angry_mob_verification::run_with(&root, battle_bus_verification::verify)?;
    } else if arguments
        .iter()
        .any(|arg| arg == "--verify-stealth-fighter")
    {
        angry_mob_verification::run_with(&root, stealth_fighter_verification::verify)?;
    } else if arguments.iter().any(|arg| arg == "--verify-aurora") {
        aurora_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-helix") {
        helix_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-hijacker") {
        hijacker_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-china-aircraft") {
        china_aircraft_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-sabotage") {
        sabotage_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-terrorist") {
        terrorist_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-firebase") {
        firebase_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-stinger") {
        stinger_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-infiltrators") {
        infiltrator_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-drones") {
        drone_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-fire") {
        fire_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-tractor") {
        tractor_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-artillery") {
        artillery_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-point-defense") {
        point_defense_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-fake-buildings") {
        fake_building_verification::run(&root)?;
    } else if arguments.iter().any(|arg| arg == "--verify-strategy") {
        ground_integration_verification::run(&root, true)?;
    } else {
        run_window(&root, &arguments)?;
    }
    Ok(())
}

fn run_window(root: &std::path::Path, arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let profile_skirmish = arguments.iter().any(|arg| arg == "--profile-skirmish");
    let profile_defenses = arguments.iter().any(|arg| arg == "--profile-defenses");
    let capture = if profile_skirmish || profile_defenses {
        playtest::ScreenCapture::None
    } else {
        screen_capture(arguments)
    };
    let profile = if profile_defenses {
        playtest::Profile::Defenses
    } else if profile_skirmish {
        playtest::Profile::Skirmish
    } else if arguments.iter().any(|arg| arg == "--profile-performance") {
        playtest::Profile::Startup
    } else {
        playtest::Profile::None
    };
    let overlay = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--content-overlay="));
    playtest::run(root, &capture, &profile, overlay.map(std::path::Path::new))?;
    Ok(())
}

fn validate_arguments(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let overlays: Vec<_> = arguments
        .iter()
        .filter_map(|argument| argument.strip_prefix("--content-overlay="))
        .collect();
    if overlays.len() > 1 || overlays.iter().any(|path| path.is_empty()) {
        return Err("Select exactly one nonempty content overlay path".into());
    }
    if !overlays.is_empty()
        && arguments.iter().any(|argument| {
            argument.starts_with("--verify-") || argument.starts_with("--benchmark-")
        })
    {
        return Err(
            "Content overlays currently support windowed playtests, captures and profiles only"
                .into(),
        );
    }
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--game-root" {
            let path = arguments.next().ok_or("--game-root requires a path")?;
            if path.is_empty() || path.starts_with("--") {
                return Err("--game-root requires a nonempty path".into());
            }
            continue;
        }
        if argument.starts_with("--game-root=") {
            continue;
        }
        if argument.starts_with("--content-overlay=") {
            continue;
        }
        if !matches!(
            argument.as_str(),
            "--verify-skirmish"
                | "--verify-neutral-tech"
                | "--verify-neutral-expanded"
                | "--benchmark-defenses"
                | "--profile-defenses"
                | "--verify-ground-integration"
                | "--verify-strategy"
                | "--verify-point-defense"
                | "--verify-speaker"
                | "--verify-outpost"
                | "--verify-aircraft"
                | "--verify-china-aircraft"
                | "--verify-helix"
                | "--verify-aurora"
                | "--verify-angry-mob"
                | "--verify-battle-bus"
                | "--verify-microwave"
                | "--verify-ecm"
                | "--verify-stealth-fighter"
                | "--verify-fake-buildings"
                | "--verify-hijacker"
                | "--verify-maps"
                | "--verify-terrorist"
                | "--verify-sabotage"
                | "--verify-firebase"
                | "--verify-stinger"
                | "--verify-infiltrators"
                | "--verify-drones"
                | "--verify-fire"
                | "--verify-artillery"
                | "--verify-tractor"
                | "--benchmark-render"
                | "--benchmark-skirmish"
                | "--profile-performance"
                | "--profile-skirmish"
                | "--verify-multi-ai"
                | "--verify-opponent"
        ) && !SCREEN_CAPTURES
            .iter()
            .any(|(flag, _)| *flag == argument.as_str())
        {
            return Err(format!("Unknown argument: {argument}").into());
        }
    }
    Ok(())
}

const SCREEN_CAPTURES: [(&str, playtest::ScreenCapture); 110] = {
    use generals_sim::skirmish::Faction;
    use playtest::ScreenCapture;
    [
        ("--smoke-infantry-capture", ScreenCapture::InfantryCapture),
        ("--microwave-capture", ScreenCapture::Microwave),
        ("--ecm-capture", ScreenCapture::Ecm),
        ("--lightpulse-capture", ScreenCapture::LightPulse),
        ("--world-rain-capture", ScreenCapture::World(false)),
        ("--world-bridge-capture", ScreenCapture::World(true)),
        (
            "--bridge-destruction-capture",
            ScreenCapture::BridgeDestruction,
        ),
        (
            "--reinforcement-pad-usa-capture",
            ScreenCapture::ReinforcementPad(Faction::Usa),
        ),
        (
            "--reinforcement-pad-china-capture",
            ScreenCapture::ReinforcementPad(Faction::China),
        ),
        (
            "--reinforcement-pad-gla-capture",
            ScreenCapture::ReinforcementPad(Faction::Gla),
        ),
        (
            "--structure-collapse-capture",
            ScreenCapture::Structure(false),
        ),
        ("--structure-topple-capture", ScreenCapture::Structure(true)),
        ("--smoke-animation-clock", ScreenCapture::AnimationClock),
        ("--smoke-usa-research", ScreenCapture::UsaResearch),
        ("--smoke-hospital", ScreenCapture::NeutralTech(false)),
        ("--smoke-refinery", ScreenCapture::NeutralTech(true)),
        ("--smoke-tunnel", ScreenCapture::Tunnel),
        ("--smoke-crawler", ScreenCapture::Crawler),
        ("--smoke-strategy", ScreenCapture::Strategy),
        ("--smoke-point-defense", ScreenCapture::PointDefense(false)),
        ("--smoke-avenger", ScreenCapture::PointDefense(true)),
        ("--smoke-paladin", ScreenCapture::Paladin),
        ("--smoke-speaker", ScreenCapture::Speaker),
        ("--smoke-firebase", ScreenCapture::FireBase),
        ("--smoke-stinger", ScreenCapture::Stinger),
        ("--smoke-infiltrators", ScreenCapture::Infiltrators),
        ("--smoke-jarmen", ScreenCapture::Jarmen),
        ("--smoke-outpost", ScreenCapture::Outpost),
        ("--smoke-sentry", ScreenCapture::Sentry),
        ("--smoke-disarm", ScreenCapture::Disarm),
        ("--smoke-demo-trap", ScreenCapture::DemoTrap(false)),
        ("--smoke-demo-trap-proximity", ScreenCapture::DemoTrap(true)),
        ("--smoke-scout", ScreenCapture::Scout),
        ("--smoke-pathfinder", ScreenCapture::Pathfinder),
        ("--smoke-aircraft", ScreenCapture::Aircraft),
        ("--smoke-aircraft-crash", ScreenCapture::AircraftCrash),
        ("--smoke-terrorist", ScreenCapture::Terrorist),
        ("--sabotage-capture", ScreenCapture::Sabotage),
        ("--hijacker-capture", ScreenCapture::Hijacker),
        ("--helix-capture", ScreenCapture::Helix),
        ("--aurora-capture", ScreenCapture::Aurora),
        ("--angry-mob-capture", ScreenCapture::AngryMob),
        ("--battle-bus-capture", ScreenCapture::BattleBus),
        ("--stealth-fighter-capture", ScreenCapture::StealthFighter),
        ("--neutral-expanded-capture", ScreenCapture::NeutralExpanded),
        ("--repair-bay-capture", ScreenCapture::RepairBay),
        ("--capture-fake-buildings", ScreenCapture::FakeBuildings),
        ("--options-capture", ScreenCapture::Options),
        ("--china-aircraft-capture", ScreenCapture::ChinaAircraft),
        ("--custom-map-capture", ScreenCapture::CustomMap),
        (
            "--map-selection-two-capture",
            ScreenCapture::MapSelection(2),
        ),
        (
            "--map-selection-four-capture",
            ScreenCapture::MapSelection(4),
        ),
        (
            "--map-selection-six-capture",
            ScreenCapture::MapSelection(6),
        ),
        (
            "--map-selection-eight-capture",
            ScreenCapture::MapSelection(8),
        ),
        ("--smoke-tractor", ScreenCapture::Tractor),
        ("--smoke-fire-wall", ScreenCapture::Fire(false)),
        ("--smoke-firestorm", ScreenCapture::Fire(true)),
        ("--smoke-drones", ScreenCapture::Drones),
        ("--smoke-artillery", ScreenCapture::Artillery(0)),
        ("--smoke-buggy", ScreenCapture::Artillery(1)),
        ("--smoke-scud", ScreenCapture::Artillery(2)),
        ("--smoke-cleanup", ScreenCapture::Cleanup),
        ("--smoke-horde", ScreenCapture::Horde),
        ("--technical-capture", ScreenCapture::Technical),
        ("--research-capture", ScreenCapture::Research),
        ("--smoke-flashbang", ScreenCapture::Flashbang),
        ("--smoke-placement", ScreenCapture::Placement),
        ("--smoke-missiles", ScreenCapture::Missiles(Faction::Usa)),
        (
            "--smoke-missiles-china",
            ScreenCapture::Missiles(Faction::China),
        ),
        (
            "--smoke-missiles-gla",
            ScreenCapture::Missiles(Faction::Gla),
        ),
        ("--smoke-targeting", ScreenCapture::Targeting),
        ("--smoke-guard", ScreenCapture::Guard),
        ("--smoke-hacker-disable", ScreenCapture::HackerDisable),
        (
            "--smoke-infantry-moving",
            ScreenCapture::InfantryMoving(Faction::Usa),
        ),
        (
            "--smoke-infantry-moving-china",
            ScreenCapture::InfantryMoving(Faction::China),
        ),
        (
            "--smoke-infantry-moving-gla",
            ScreenCapture::InfantryMoving(Faction::Gla),
        ),
        ("--smoke-infantry", ScreenCapture::Infantry(Faction::Usa)),
        (
            "--smoke-infantry-china",
            ScreenCapture::Infantry(Faction::China),
        ),
        (
            "--smoke-infantry-gla",
            ScreenCapture::Infantry(Faction::Gla),
        ),
        ("--smoke-local-defeat", ScreenCapture::LocalDefeat),
        ("--smoke-observer-info", ScreenCapture::ObserverInfo),
        ("--smoke-observer-quit", ScreenCapture::ObserverQuit),
        ("--smoke-results-four", ScreenCapture::ResultsFour),
        ("--smoke-results-victory", ScreenCapture::Results(true)),
        ("--smoke-results-defeat", ScreenCapture::Results(false)),
        ("--smoke-projectiles", ScreenCapture::Projectiles),
        ("--smoke-supply-lift", ScreenCapture::SupplyLift),
        ("--verify-audio", ScreenCapture::Audio),
        ("--smoke-supplies", ScreenCapture::Supplies(Faction::Usa)),
        (
            "--smoke-supplies-china",
            ScreenCapture::Supplies(Faction::China),
        ),
        (
            "--smoke-supplies-gla",
            ScreenCapture::Supplies(Faction::Gla),
        ),
        ("--smoke-movement", ScreenCapture::Movement),
        ("--smoke-waypoints", ScreenCapture::Waypoints),
        ("--smoke-promotions", ScreenCapture::Promotions),
        ("--smoke-upgrades", ScreenCapture::Upgrades),
        ("--smoke-defenses", ScreenCapture::Defenses),
        ("--smoke-weapons", ScreenCapture::Weapons),
        ("--smoke-internet-center", ScreenCapture::InternetCenter),
        ("--smoke-hacker", ScreenCapture::Hacker),
        ("--smoke-neutral-garrison", ScreenCapture::NeutralGarrison),
        ("--smoke-garrisons", ScreenCapture::Garrisons),
        ("--smoke-overlord", ScreenCapture::Overlord),
        ("--smoke-chinook", ScreenCapture::Chinook),
        ("--smoke-transports", ScreenCapture::Transports),
        ("--smoke-barracks-healing", ScreenCapture::BarracksHealing),
        ("--smoke-controls", ScreenCapture::Controls),
        ("--smoke", ScreenCapture::Match),
        ("--smoke-skirmish", ScreenCapture::Skirmish),
        ("--smoke-menu", ScreenCapture::Menu),
        ("--smoke-shell-flow", ScreenCapture::ShellFlow),
    ]
};

fn screen_capture(arguments: &[String]) -> playtest::ScreenCapture {
    SCREEN_CAPTURES
        .into_iter()
        .find_map(|(flag, capture)| arguments.iter().any(|arg| arg == flag).then_some(capture))
        .unwrap_or(playtest::ScreenCapture::None)
}

fn print_help() {
    println!(
        "Generals Rust translation\nNo flags: open the game\nGame files: --game-root PATH (or GENERALS_GAME_ROOT; default ../private)\nContent selection: --content-overlay=PATH (windowed playtest/captures/profiles)\n\nInstalled gameplay checks:"
    );
    for flag in [
        "skirmish",
        "neutral-tech",
        "neutral-expanded",
        "ground-integration",
        "speaker",
        "aircraft",
        "china-aircraft",
        "maps",
        "outpost",
        "terrorist",
        "sabotage",
        "hijacker",
        "helix",
        "aurora",
        "angry-mob",
        "battle-bus",
        "microwave",
        "ecm",
        "stealth-fighter",
        "fake-buildings",
        "firebase",
        "stinger",
        "infiltrators",
        "drones",
        "fire",
        "tractor",
        "artillery",
        "point-defense",
        "strategy",
        "opponent",
        "multi-ai",
    ] {
        println!("  --verify-{flag}");
    }
    println!(
        "\nPerformance checks:\n  --benchmark-render\n  --benchmark-skirmish\n  --benchmark-defenses\n  --profile-performance\n  --profile-skirmish\n  --profile-defenses"
    );
    println!("\nNative controls, art and audio checks (require a visible game window):");
    for (flag, _) in &SCREEN_CAPTURES {
        println!("  {flag}");
    }
}

#[cfg(test)]
mod argument_tests {
    use super::validate_arguments;

    #[test]
    fn game_root_arguments_preserve_paths_and_reject_missing_values() {
        for arguments in [
            vec!["--game-root", "/private/My Game"],
            vec!["--game-root=/private/My Game"],
            vec!["--game-root", "/private/My Game", "--verify-skirmish"],
        ] {
            assert!(
                validate_arguments(&arguments.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .is_ok()
            );
        }
        for arguments in [
            vec!["--game-root"],
            vec!["--game-root", ""],
            vec!["--game-root", "--verify-skirmish"],
            vec!["--game-root=/private/game", "--unknown-option"],
        ] {
            assert!(
                validate_arguments(&arguments.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }

    #[test]
    fn content_overlay_selection_cannot_be_duplicated_empty_or_silently_ignored() {
        for arguments in [
            vec!["--content-overlay=output/content-overlays/chosen"],
            vec![
                "--content-overlay=output/content-overlays/chosen",
                "--options-capture",
            ],
        ] {
            assert!(
                validate_arguments(&arguments.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .is_ok()
            );
        }
        for arguments in [
            vec!["--content-overlay="],
            vec!["--content-overlay=one", "--content-overlay=two"],
            vec!["--content-overlay=chosen", "--verify-skirmish"],
            vec!["--content-overlay=chosen", "--benchmark-skirmish"],
        ] {
            assert!(
                validate_arguments(&arguments.into_iter().map(str::to_owned).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }
}
