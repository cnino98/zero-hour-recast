# Zero Hour Recast

Zero Hour Recast is a work-in-progress Rust and Bevy translation of *Command & Conquer: Generals – Zero Hour* for local skirmish. Play as USA, China or GLA against computer opponents using the maps, models, interface and sounds from your own installation. Faction rosters and map support are still incomplete. The [roadmap](ROADMAP.md) tracks known issues and planned work.

## Build and play

Install Rust through rustup. The checkout selects the toolchain and components in [rust-toolchain.toml](rust-toolchain.toml). Use the Xcode Command Line Tools on macOS, the [native libraries used by CI](.github/workflows/ci.yml) on Linux, or the Visual Studio C++ build tools and Windows SDK with the MSVC Rust toolchain on Windows. To play, you also need a GPU supported by Bevy, an audio output device, and the original UI fonts installed locally, including Arial (regular and bold) and Times New Roman (regular).

Use your own installed copies of both *Generals* and *Zero Hour*. The tested setup uses English game files. Copy or link the two installed directories into this layout, retaining their original archives and loose files. The base directory contains `INI.big` and the expansion directory contains `INIZH.big` and `MapsZH.big`.

```text
Generals/
├── zero-hour-recast/                  # source checkout
└── private/
    └── GameData/
        ├── Command and Conquer Generals/
        └── Command and Conquer Generals Zero Hour/
```

Build the optimized client from the checkout root.

```sh
cargo build --release --locked -p generals-client --bin generals-client --target-dir ../private/cache/target
```

From that same directory, run `../private/cache/target/release/generals-client` on macOS or Linux, or `..\private\cache\target\release\generals-client.exe` in PowerShell on Windows. Choose **Solo Play**, then **Skirmish**, to select a map and opponents. Native playtesting to date has used Apple Silicon. The [CI workflow](.github/workflows/ci.yml) checks x64 and ARM64 builds on macOS, Linux and Windows without an installed game.

For a different layout, pass `--game-root "/path/to/runtime"` to the executable or set `GENERALS_GAME_ROOT`. Both take the directory containing `GameData`. The command-line flag takes precedence. Keep this directory writable and outside the checkout. The client reads the game archives in place and saves settings and logs under `output/`. To import existing preferences, copy `Options.ini` and, if available, `Skirmish.ini` into `UserData/` beside `GameData/`.

## Run the checks

From the checkout root, run the following with Python 3.11 or later. This checks formatting, Rust and Python tests, strict Clippy, a linked build and simulation scaling. It runs without an installed game, display or audio device.

```sh
python3 -B tests/run.py
```

Use `python` in place of `python3` if that is how your Python installation is named. The [test guide](tests/README.md) covers C++ source comparisons, installed-game checks and test output.

## Source and licensing

The workspace contains the [asset readers](crates/generals-assets/), [skirmish simulation](crates/generals-sim/) and [Bevy client](crates/generals-client/). Its primary engine reference is [TheSuperHackers/GeneralsGameCode](https://github.com/TheSuperHackers/GeneralsGameCode/tree/2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed), with original engine context from the [EA source release](https://github.com/electronicarts/CnC_Generals_Zero_Hour/tree/0a05454d8574207440a5fb15241b98ad0b435590). The pinned [GeneralsGamePatch](https://github.com/TheSuperHackers/GeneralsGamePatch/tree/6c7f40d6af6a4babb77a33a4502ec7656446c13e) is the reference for planned data fixes.

Zero Hour Recast is a modified program. You supply all original game assets from your own installation. See the [GPL-3.0-or-later license and EA additional terms](LICENSE.md) and [third-party notices](THIRD_PARTY_NOTICES.md).
