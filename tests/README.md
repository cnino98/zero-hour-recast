# Running the checks

From the checkout root, use Python 3.11 or later and the pinned [Rust toolchain](../rust-toolchain.toml). The [CI workflow](../.github/workflows/ci.yml) lists the Linux build dependencies and Windows compiler setup.

```sh
python3 -B tests/run.py
```

The default run needs no game files, display or audio device. It runs formatting, Cargo checks, strict Clippy, ordinary Rust tests, a linked build, Python tests and a separate simulation-scaling measurement. Each run writes a `manifest.json` and per-step logs under `../private/test-results/` by default. Use `--output PATH` to choose a different external directory. The runner honors external `CARGO_HOME` and `CARGO_TARGET_DIR` settings.

GitHub Actions runs these checks on every pull request and push to `main`, across macOS, Linux and Windows on x64 and ARM64. To require passing checks before merging, protect `main`, require pull requests, and select the six platform jobs as required checks.

On POSIX hosts, generated executables and captures are kept for the latest three inactive runs under each result parent. Reuse that parent across retries. Manifests and diagnostic logs are retained. Normal play keeps forty completed session logs, plus protected crash evidence.

## Reference checks

Run `python3 -B tests/run.py --references` to include differential checks against the pinned C++ engine source. To run just those comparisons, use `python3 -B tests/reference/run.py`. Both commands require the external checkout and compilers described in the [reference guide](reference/PROVENANCE.md).

## Installed-game checks

Use a POSIX desktop with graphics and audio, both [game installations and UI fonts](../README.md#build-and-play), and the pinned checkout from the [reference guide](reference/PROVENANCE.md). Installed playtesting has used Apple Silicon. Run against a separate writable runtime so your normal game settings stay intact.

1. From the checkout root, run both [fixture preparation commands](installed/client-fixtures.md#prepare-the-fixtures). Run `prepare-metadata.py` to generate the comparison inputs and map chooser entries, then `prepare-repair-pad.py` to generate the Repair Pad map. Use the same external runtime directory for both commands and the full suite below.
2. Copy or link the installed `GameData/` directory into that runtime, with both named game directories inside it. Copy the original user's `Options.ini` into the runtime's `UserData/` alongside the generated `Maps/` directory.
3. Run the full suite from the checkout root, reusing the source-check build and dependency caches.

   ```sh
   CARGO_HOME=../private/cache/cargo CARGO_TARGET_DIR=../private/cache/target \
     python3 -B tests/run.py --full --runtime-root ../private/installed-runtime
   ```

The full run adds ignored installed and GPU tests, reference comparisons, a release build, runtime scenarios, native captures and performance measurements. It tests a verified copy of the freshly built client unless you pass `--executable PATH` to select another external build. Use `--case-timeout SECONDS` to change the per-case timeout. Open the manifests and logs to find failing steps and individual cases. Native captures and performance results apply to the tested build, game data and hardware.
