# Skirmish roadmap

Zero Hour Recast aims to reproduce local skirmish with the original rules, controls, artwork and audio. The scope includes three base factions, nine Generals variants, the original skirmish maps and compatible custom maps. Campaign and network multiplayer are outside the project scope.

## Milestones

| Date | Gameplay and development |
| --- | --- |
| October 2026 | Playable matches with USA, China and GLA, with resource collection, construction, unit orders, combat, AI opponents and victory and defeat reporting. |
| November 1, 2026 | Verified bridge damage, radar refresh, hydrant effects and selected infantry and aircraft effects. Improved path queuing and weapon interactions. |
| November 4, 2026 | Fixed ground build buttons that incorrectly required aircraft parking. Verified a USA Dozer buying and completing a power plant through the original HUD. |
| November 5, 2026 | Prepared the source checkout with game-independent tests and a six-target native CI matrix. Verified a second sortie after rearming in the four-MiG installed check. |
| November 13, 2026 — v0.2.0 | Verified base special-unit behavior, fake-building conversion, stock airfield lifecycles, countermeasures, repair and deliveries. Restored original wheel effects, tracks, borrowed vehicle tread animation, C4 feedback and disguise hover names. Verified aircraft crash bodies, hulks and pilot release. Fixed faded-track mesh retirement and opaque disguise materials. |
| November 14, 2026 — v0.2.1 | Fixed the main-menu startup failure introduced in v0.2.0. Gameplay controls load when a match starts, including C4 icons, hover labels and terrain tracks. Added a normal-startup regression that also checks missing match data is still rejected. |
| 2026-11-21 — local v0.3.0 | Added the base factions’ upgrades and general powers, plus the Particle Cannon, Nuclear Missile and SCUD Storm. Added mine and supply behavior and Saboteur use against superweapons, with original controls and effects. |

## Known issues

- A reported runway zigzag has not yet been reproduced.
- Broader animation, rendering and audio work remains. The full installed-game test suite has not yet run against the integrated build.
- Large battles can fall below the 60 FPS target. A November 4 build measured a 22.2 ms median frame time (about 45 FPS) in the 50-units-per-side battle at 3024 × 1890 and a 12.0 ms median in the separate garrisoned-defense test.

## Planned work

Next work covers original and custom maps, base skirmish AI, and fog of war and visibility.

- Finish remaining unit behavior and interactions with aircraft, supply collection, transports and repair.
- Implement USA Air Force, Laser and Superweapon, China Tank, Infantry and Nuke, and GLA Toxin, Demolition and Stealth, including their distinct units, powers and AI.
- Verify all original skirmish maps and expand custom-map compatibility. Finish map scripts, visibility, starting slots, AI teams and difficulty through the end of a match.
- Complete original unit animations, effects, terrain, shadows, UI feedback, music, speech and sound. Finish menus, settings, statistics, local saves and replay playback.
- Adopt applicable GeneralsGamePatch fixes through an external content overlay so players can keep the original archives intact.
- Reduce simulation stalls and improve large-battle frame times. Verify complete matches and the 60 FPS target with installed-game and long-running performance checks.
