//! Verify the installed result rules, including their authored timer omissions.
use super::{Change, ResultState, load};
use crate::playtest_ui::ResultAction;
use generals_assets::virtual_assets::VirtualAssets;
use std::error::Error;

pub fn verify(assets: &VirtualAssets) -> Result<(), Box<dyn Error>> {
    let mut program = load(assets)?;
    for (faction, suffix, voice) in [
        ("America", "USA", "USA"),
        ("China", "Chi", "China"),
        ("GLA", "GLA", "GLA"),
    ] {
        for action in [
            ResultAction::LocalDefeat,
            ResultAction::Defeat,
            ResultAction::Victory,
        ] {
            program.reset();
            let mut state = ResultState {
                defeated: false,
                ended: false,
                victorious: false,
            };
            // Native initial scripts establish _FACTION before any defeat occurs.
            program.evaluate(faction, None, |_| 0, Some(&state));
            state.defeated = !matches!(action, ResultAction::Victory);
            state.ended = !matches!(action, ResultAction::LocalDefeat);
            state.victorious = matches!(action, ResultAction::Victory);
            let mut events = Vec::new();
            for frame in 0..=180 {
                for change in program.evaluate(faction, None, |_| 0, Some(&state)) {
                    events.push((frame, change));
                }
            }
            let track = format!(
                "End_{suffix}{}",
                if state.victorious { "" } else { "_Failure" }
            );
            let mut expected = vec![(0, format!("music:{track}"))];
            if state.victorious {
                expected.push((120, format!("speech:Eva{voice}_Victory:true")));
                expected.push((150, "speech:UI_AllCheerSound:true".into()));
            } else if state.ended || faction == "America" {
                expected.push((150, format!("speech:Eva{voice}_Defeat:false")));
            }
            expected.push((180, event_name(Change::Result(action))));
            let actual: Vec<_> = events
                .into_iter()
                .map(|(frame, event)| (frame, event_name(event)))
                .collect();
            if actual != expected {
                return Err(format!(
                    "installed {faction} result sequence differs: {actual:?}, expected {expected:?}"
                )
                .into());
            }
        }
    }
    println!(
        "Original result scripts: all three factions' victory, defeat and local-defeat music/speech/declaration ordering passed; 180-frame declaration and authored missing China/GLA personal speech timer retained"
    );
    Ok(())
}

fn event_name(event: Change) -> String {
    match event {
        Change::Track { name, fade } => {
            format!("music:{name}{}", if fade { "" } else { ":no-fade" })
        }
        Change::Speech {
            name,
            allow_overlap,
        } => format!("speech:{name}:{allow_overlap}"),
        Change::Result(action) => match action {
            ResultAction::Victory => "window:victory".into(),
            ResultAction::Defeat => "window:defeat".into(),
            ResultAction::LocalDefeat => "window:local-defeat".into(),
        },
    }
}
