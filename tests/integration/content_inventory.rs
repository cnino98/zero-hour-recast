//! A failed inventory refresh must not retain stale selectable factions.
use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove local fixture");
    }
}

#[test]
fn accepts_optional_separators_and_invalidates_factions_on_failed_refresh() {
    let fixture =
        Fixture(std::env::temp_dir().join(format!("generals-census-{}", std::process::id())));
    fs::create_dir(&fixture.0).expect("new fixture");
    let base = fixture.0.join("base");
    let zh = fixture.0.join("zh");
    let output = fixture.0.join("report");
    fs::create_dir(&base).expect("base directory");
    fs::create_dir_all(zh.join("Data/INI")).expect("loose INI directory");
    let player = zh.join("Data/INI/PlayerTemplate.ini");
    fs::write(
        &player,
        "PlayerTemplate Faction\n Side America\n StartingBuilding HQ\n OldFaction yes\nEnd\n",
    )
    .expect("player data");
    fs::write(zh.join("Data/INI/ChallengeMode.ini"), "ChallengeGenerals\n GeneralPersona0\n PlayerTemplate = Faction\n StartsEnabled yes\n End\nEnd\n").expect("persona data");
    fs::write(
        zh.join("Data/INI/Object.ini"),
        "Object HQ\n Behavior AIUpdateInterface Tag\n End\nEnd\n",
    )
    .expect("bare module");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_content-inventory"))
            .args([&base, &zh, &output])
            .output()
            .expect("execute census")
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        fs::read_to_string(output.join("players.tsv"))
            .expect("factions")
            .contains("Selectable")
    );
    assert!(
        fs::read_to_string(output.join("module-usage.tsv"))
            .expect("modules")
            .contains("AIUpdateInterface")
    );
    fs::write(&player, "PlayerTemplate Faction\n Side = America\n StartingBuilding = \"Needs source quoted parser\"\nEnd\n").expect("unsupported metadata");
    let second = run();
    assert!(!second.status.success());
    assert!(
        !output.join("players.tsv").exists(),
        "no stale successful faction report"
    );
}
