# Source reference checks

The differential tests compare selected arithmetic, pathfinding, geometry, random streams and other engine behavior against the pinned [TheSuperHackers/GeneralsGameCode checkout](https://github.com/TheSuperHackers/GeneralsGameCode/tree/2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed). They extract the upstream methods at runtime. The checked-in hierarchy and zone C++ files provide adapters and synthetic inputs. The numeric fixtures record controlled cases, and `module-metadata-reference-r2.native.tsv` covers six synthetic inherited-factory cases. The effective Zero Hour retail build switches are documented in [check-path-reference.py](check-path-reference.py).

From the project root, create the reference checkout outside the repository.

```sh
git init ../private/references/GeneralsGameCode
git -C ../private/references/GeneralsGameCode remote add origin https://github.com/TheSuperHackers/GeneralsGameCode.git
git -C ../private/references/GeneralsGameCode fetch --depth=1 origin 2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed
git -C ../private/references/GeneralsGameCode checkout --detach FETCH_HEAD
```

Keep it clean at that revision, or set `GENERALS_REFERENCE_SOURCE` to another clean checkout at the same revision. Run with Git, Python 3.11 or later, a C++20 compiler and a Rust compiler supporting edition 2024.

```sh
python3 -B tests/reference/run.py
```

The drivers default to `clang++` and `rustc`. Set `CXX` and `RUSTC` to select different executables. Generated files go under `GENERALS_TEST_OUTPUT/reference`, which defaults to `../private/tests/reference/` when running the reference suite directly. Drivers that accept `--write-fixture` put regenerated results in the `regenerated/` subdirectory for review. Each driver describes the cases it covers. See the [license](../../LICENSE.md) and [third-party notices](../../THIRD_PARTY_NOTICES.md) for source attribution.
