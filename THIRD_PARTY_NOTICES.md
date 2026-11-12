# Third-party notices

## Released game source

Zero Hour Recast contains a modified Rust translation of the released game
engine source, together with Rust/Bevy integration and source-comparison tests.
It is a modified program and is not an Electronic Arts product or endorsement.

The primary behavior reference is TheSuperHackers/GeneralsGameCode at commit
`2bfaea60b3b9d5a798a0d0a1aff7ea61b6cee3ed`, particularly `Core/` and
`GeneralsMD/Code/`. The secondary reference is
ElectronicArts/CnC_Generals_Zero_Hour at commit
`0a05454d8574207440a5fb15241b98ad0b435590`.

Copyright notices in that released source include:

- Copyright 2025 Electronic Arts Inc.
- Copyright 2025 TheSuperHackers.
- (c) 2001-2003 Electronic Arts Inc.

[LICENSE.md](LICENSE.md) reproduces the primary pinned checkout's complete
`GPL-3.0-or-later` license document, including Electronic Arts' additional terms
under GPL Section 7. Those terms include the trademark, attribution, origin,
contractual-liability and warranty provisions; they are part of the reproduced
license. The translation, comparison drivers and environmental adapters preserve
the source provenance; the external checkout itself is not vendored here.

Original retail game data, artwork, audio, fonts, installers and executables are
not included in this repository. The engine reads the user's separate installation
at runtime. Source identifiers used by parsers and gameplay do not grant rights
to the corresponding game assets or trademarks.

## Locked Rust dependencies

The following inventory records every registry package in the checked-in
`Cargo.lock`, including target-specific and build-time packages. Versions and
license expressions are copied from the packaged `Cargo.toml` for those exact
locked crate versions. Expressions are preserved as declared, including older
slash-separated alternatives. This table does not imply that every listed package
is compiled into every target. Dependency source and its license files are obtained
through Cargo; these crate sources are not vendored in this repository.

| Package | Locked version | Package-declared license |
| --- | --- | --- |
| [accesskit](https://docs.rs/crate/accesskit/0.24.1/source/Cargo.toml) | `0.24.1` | `MIT OR Apache-2.0` |
| [accesskit_consumer](https://docs.rs/crate/accesskit_consumer/0.35.0/source/Cargo.toml) | `0.35.0` | `MIT OR Apache-2.0` |
| [accesskit_consumer](https://docs.rs/crate/accesskit_consumer/0.38.0/source/Cargo.toml) | `0.38.0` | `MIT OR Apache-2.0` |
| [accesskit_macos](https://docs.rs/crate/accesskit_macos/0.26.3/source/Cargo.toml) | `0.26.3` | `MIT OR Apache-2.0` |
| [accesskit_windows](https://docs.rs/crate/accesskit_windows/0.32.1/source/Cargo.toml) | `0.32.1` | `MIT OR Apache-2.0` |
| [accesskit_winit](https://docs.rs/crate/accesskit_winit/0.32.2/source/Cargo.toml) | `0.32.2` | `Apache-2.0` |
| [adler2](https://docs.rs/crate/adler2/2.0.1/source/Cargo.toml) | `2.0.1` | `0BSD OR MIT OR Apache-2.0` |
| [aho-corasick](https://docs.rs/crate/aho-corasick/1.1.5/source/Cargo.toml) | `1.1.5` | `Unlicense OR MIT` |
| [allocator-api2](https://docs.rs/crate/allocator-api2/0.2.21/source/Cargo.toml) | `0.2.21` | `MIT OR Apache-2.0` |
| [alsa](https://docs.rs/crate/alsa/0.11.0/source/Cargo.toml) | `0.11.0` | `Apache-2.0/MIT` |
| [alsa-sys](https://docs.rs/crate/alsa-sys/0.4.0/source/Cargo.toml) | `0.4.0` | `MIT` |
| [android-activity](https://docs.rs/crate/android-activity/0.6.1/source/Cargo.toml) | `0.6.1` | `MIT OR Apache-2.0` |
| [android-properties](https://docs.rs/crate/android-properties/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [android_log-sys](https://docs.rs/crate/android_log-sys/0.3.2/source/Cargo.toml) | `0.3.2` | `MIT OR Apache-2.0` |
| [android_system_properties](https://docs.rs/crate/android_system_properties/0.1.6/source/Cargo.toml) | `0.1.6` | `MIT OR Apache-2.0` |
| [approx](https://docs.rs/crate/approx/0.5.1/source/Cargo.toml) | `0.5.1` | `Apache-2.0` |
| [arrayvec](https://docs.rs/crate/arrayvec/0.7.8/source/Cargo.toml) | `0.7.8` | `MIT OR Apache-2.0` |
| [as-raw-xcb-connection](https://docs.rs/crate/as-raw-xcb-connection/1.0.1/source/Cargo.toml) | `1.0.1` | `MIT OR Apache-2.0` |
| [ash](https://docs.rs/crate/ash/0.38.0+1.3.281/source/Cargo.toml) | `0.38.0+1.3.281` | `MIT OR Apache-2.0` |
| [assert_type_match](https://docs.rs/crate/assert_type_match/0.1.1/source/Cargo.toml) | `0.1.1` | `MIT OR Apache-2.0` |
| [async-broadcast](https://docs.rs/crate/async-broadcast/0.7.2/source/Cargo.toml) | `0.7.2` | `MIT OR Apache-2.0` |
| [async-channel](https://docs.rs/crate/async-channel/2.5.0/source/Cargo.toml) | `2.5.0` | `Apache-2.0 OR MIT` |
| [async-executor](https://docs.rs/crate/async-executor/1.14.0/source/Cargo.toml) | `1.14.0` | `Apache-2.0 OR MIT` |
| [async-fs](https://docs.rs/crate/async-fs/2.2.0/source/Cargo.toml) | `2.2.0` | `Apache-2.0 OR MIT` |
| [async-io](https://docs.rs/crate/async-io/2.6.0/source/Cargo.toml) | `2.6.0` | `Apache-2.0 OR MIT` |
| [async-lock](https://docs.rs/crate/async-lock/3.4.2/source/Cargo.toml) | `3.4.2` | `Apache-2.0 OR MIT` |
| [async-task](https://docs.rs/crate/async-task/4.7.1/source/Cargo.toml) | `4.7.1` | `Apache-2.0 OR MIT` |
| [atomic-waker](https://docs.rs/crate/atomic-waker/1.1.2/source/Cargo.toml) | `1.1.2` | `Apache-2.0 OR MIT` |
| [atomicow](https://docs.rs/crate/atomicow/1.2.0/source/Cargo.toml) | `1.2.0` | `MIT OR Apache-2.0` |
| [autocfg](https://docs.rs/crate/autocfg/1.5.1/source/Cargo.toml) | `1.5.1` | `Apache-2.0 OR MIT` |
| [bevy](https://docs.rs/crate/bevy/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_a11y](https://docs.rs/crate/bevy_a11y/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_android](https://docs.rs/crate/bevy_android/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_app](https://docs.rs/crate/bevy_app/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_asset](https://docs.rs/crate/bevy_asset/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_asset_macros](https://docs.rs/crate/bevy_asset_macros/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_audio](https://docs.rs/crate/bevy_audio/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_camera](https://docs.rs/crate/bevy_camera/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_clipboard](https://docs.rs/crate/bevy_clipboard/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_color](https://docs.rs/crate/bevy_color/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_core_pipeline](https://docs.rs/crate/bevy_core_pipeline/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_derive](https://docs.rs/crate/bevy_derive/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_diagnostic](https://docs.rs/crate/bevy_diagnostic/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_ecs](https://docs.rs/crate/bevy_ecs/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_ecs_macro_logic](https://docs.rs/crate/bevy_ecs_macro_logic/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_ecs_macros](https://docs.rs/crate/bevy_ecs_macros/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_encase_derive](https://docs.rs/crate/bevy_encase_derive/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_gizmos](https://docs.rs/crate/bevy_gizmos/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_gizmos_macros](https://docs.rs/crate/bevy_gizmos_macros/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_gizmos_render](https://docs.rs/crate/bevy_gizmos_render/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_image](https://docs.rs/crate/bevy_image/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_input](https://docs.rs/crate/bevy_input/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_input_focus](https://docs.rs/crate/bevy_input_focus/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_internal](https://docs.rs/crate/bevy_internal/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_light](https://docs.rs/crate/bevy_light/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_log](https://docs.rs/crate/bevy_log/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_macro_utils](https://docs.rs/crate/bevy_macro_utils/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_material](https://docs.rs/crate/bevy_material/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_material_macros](https://docs.rs/crate/bevy_material_macros/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_math](https://docs.rs/crate/bevy_math/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_mesh](https://docs.rs/crate/bevy_mesh/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_mikktspace](https://docs.rs/crate/bevy_mikktspace/1.0.0/source/Cargo.toml) | `1.0.0` | `Zlib AND (MIT OR Apache-2.0)` |
| [bevy_pbr](https://docs.rs/crate/bevy_pbr/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_platform](https://docs.rs/crate/bevy_platform/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_ptr](https://docs.rs/crate/bevy_ptr/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_reflect](https://docs.rs/crate/bevy_reflect/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_reflect_derive](https://docs.rs/crate/bevy_reflect_derive/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_render](https://docs.rs/crate/bevy_render/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_render_macros](https://docs.rs/crate/bevy_render_macros/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_shader](https://docs.rs/crate/bevy_shader/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_sprite](https://docs.rs/crate/bevy_sprite/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_sprite_render](https://docs.rs/crate/bevy_sprite_render/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_state](https://docs.rs/crate/bevy_state/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_state_macros](https://docs.rs/crate/bevy_state_macros/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_tasks](https://docs.rs/crate/bevy_tasks/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_text](https://docs.rs/crate/bevy_text/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_time](https://docs.rs/crate/bevy_time/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_transform](https://docs.rs/crate/bevy_transform/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_ui](https://docs.rs/crate/bevy_ui/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_ui_render](https://docs.rs/crate/bevy_ui_render/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_utils](https://docs.rs/crate/bevy_utils/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_window](https://docs.rs/crate/bevy_window/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bevy_winit](https://docs.rs/crate/bevy_winit/0.19.1/source/Cargo.toml) | `0.19.1` | `MIT OR Apache-2.0` |
| [bit-set](https://docs.rs/crate/bit-set/0.9.1/source/Cargo.toml) | `0.9.1` | `Apache-2.0 OR MIT` |
| [bit-vec](https://docs.rs/crate/bit-vec/0.9.1/source/Cargo.toml) | `0.9.1` | `Apache-2.0 OR MIT` |
| [bitflags](https://docs.rs/crate/bitflags/1.3.2/source/Cargo.toml) | `1.3.2` | `MIT/Apache-2.0` |
| [bitflags](https://docs.rs/crate/bitflags/2.13.2/source/Cargo.toml) | `2.13.2` | `MIT OR Apache-2.0` |
| [blake3](https://docs.rs/crate/blake3/1.8.7/source/Cargo.toml) | `1.8.7` | `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` |
| [block-buffer](https://docs.rs/crate/block-buffer/0.10.4/source/Cargo.toml) | `0.10.4` | `MIT OR Apache-2.0` |
| [block2](https://docs.rs/crate/block2/0.5.1/source/Cargo.toml) | `0.5.1` | `MIT` |
| [block2](https://docs.rs/crate/block2/0.6.2/source/Cargo.toml) | `0.6.2` | `MIT` |
| [blocking](https://docs.rs/crate/blocking/1.7.0/source/Cargo.toml) | `1.7.0` | `Apache-2.0 OR MIT` |
| [bumpalo](https://docs.rs/crate/bumpalo/3.20.3/source/Cargo.toml) | `3.20.3` | `MIT OR Apache-2.0` |
| [bytemuck](https://docs.rs/crate/bytemuck/1.25.2/source/Cargo.toml) | `1.25.2` | `Zlib OR Apache-2.0 OR MIT` |
| [bytemuck_derive](https://docs.rs/crate/bytemuck_derive/1.12.1/source/Cargo.toml) | `1.12.1` | `Zlib OR Apache-2.0 OR MIT` |
| [byteorder](https://docs.rs/crate/byteorder/1.5.0/source/Cargo.toml) | `1.5.0` | `Unlicense OR MIT` |
| [byteorder-lite](https://docs.rs/crate/byteorder-lite/0.1.0/source/Cargo.toml) | `0.1.0` | `Unlicense OR MIT` |
| [bytes](https://docs.rs/crate/bytes/1.12.1/source/Cargo.toml) | `1.12.1` | `MIT` |
| [calloop](https://docs.rs/crate/calloop/0.13.0/source/Cargo.toml) | `0.13.0` | `MIT` |
| [cc](https://docs.rs/crate/cc/1.6.0/source/Cargo.toml) | `1.6.0` | `MIT OR Apache-2.0` |
| [cesu8](https://docs.rs/crate/cesu8/1.1.0/source/Cargo.toml) | `1.1.0` | `Apache-2.0/MIT` |
| [cfg-if](https://docs.rs/crate/cfg-if/1.0.5/source/Cargo.toml) | `1.0.5` | `MIT OR Apache-2.0` |
| [cfg_aliases](https://docs.rs/crate/cfg_aliases/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [codespan-reporting](https://docs.rs/crate/codespan-reporting/0.12.0/source/Cargo.toml) | `0.12.0` | `Apache-2.0` |
| [codespan-reporting](https://docs.rs/crate/codespan-reporting/0.13.1/source/Cargo.toml) | `0.13.1` | `Apache-2.0` |
| [combine](https://docs.rs/crate/combine/4.6.8/source/Cargo.toml) | `4.6.8` | `MIT` |
| [concurrent-queue](https://docs.rs/crate/concurrent-queue/2.5.0/source/Cargo.toml) | `2.5.0` | `Apache-2.0 OR MIT` |
| [console_error_panic_hook](https://docs.rs/crate/console_error_panic_hook/0.1.7/source/Cargo.toml) | `0.1.7` | `Apache-2.0/MIT` |
| [const-fnv1a-hash](https://docs.rs/crate/const-fnv1a-hash/1.1.0/source/Cargo.toml) | `1.1.0` | `MIT` |
| [const_panic](https://docs.rs/crate/const_panic/0.2.17/source/Cargo.toml) | `0.2.17` | `Zlib` |
| [const_soft_float](https://docs.rs/crate/const_soft_float/0.1.4/source/Cargo.toml) | `0.1.4` | `MIT OR Apache-2.0` |
| [constant_time_eq](https://docs.rs/crate/constant_time_eq/0.4.2/source/Cargo.toml) | `0.4.2` | `CC0-1.0 OR MIT-0 OR Apache-2.0` |
| [constgebra](https://docs.rs/crate/constgebra/0.1.4/source/Cargo.toml) | `0.1.4` | `MIT OR Apache-2.0` |
| [convert_case](https://docs.rs/crate/convert_case/0.10.0/source/Cargo.toml) | `0.10.0` | `MIT` |
| [core-foundation](https://docs.rs/crate/core-foundation/0.9.4/source/Cargo.toml) | `0.9.4` | `MIT OR Apache-2.0` |
| [core-foundation-sys](https://docs.rs/crate/core-foundation-sys/0.8.7/source/Cargo.toml) | `0.8.7` | `MIT OR Apache-2.0` |
| [core-graphics](https://docs.rs/crate/core-graphics/0.23.2/source/Cargo.toml) | `0.23.2` | `MIT OR Apache-2.0` |
| [core-graphics-types](https://docs.rs/crate/core-graphics-types/0.1.3/source/Cargo.toml) | `0.1.3` | `MIT OR Apache-2.0` |
| [core_detect](https://docs.rs/crate/core_detect/1.0.0/source/Cargo.toml) | `1.0.0` | `MIT/Apache-2.0` |
| [core_maths](https://docs.rs/crate/core_maths/0.1.1/source/Cargo.toml) | `0.1.1` | `MIT` |
| [coreaudio-rs](https://docs.rs/crate/coreaudio-rs/0.14.2/source/Cargo.toml) | `0.14.2` | `MIT/Apache-2.0` |
| [cpal](https://docs.rs/crate/cpal/0.17.3/source/Cargo.toml) | `0.17.3` | `Apache-2.0` |
| [cpufeatures](https://docs.rs/crate/cpufeatures/0.2.17/source/Cargo.toml) | `0.2.17` | `MIT OR Apache-2.0` |
| [cpufeatures](https://docs.rs/crate/cpufeatures/0.3.1/source/Cargo.toml) | `0.3.1` | `MIT OR Apache-2.0` |
| [crc32fast](https://docs.rs/crate/crc32fast/1.5.2/source/Cargo.toml) | `1.5.2` | `MIT OR Apache-2.0` |
| [critical-section](https://docs.rs/crate/critical-section/1.2.0/source/Cargo.toml) | `1.2.0` | `MIT OR Apache-2.0` |
| [crossbeam-channel](https://docs.rs/crate/crossbeam-channel/0.5.17/source/Cargo.toml) | `0.5.17` | `MIT OR Apache-2.0` |
| [crossbeam-queue](https://docs.rs/crate/crossbeam-queue/0.3.14/source/Cargo.toml) | `0.3.14` | `MIT OR Apache-2.0` |
| [crossbeam-utils](https://docs.rs/crate/crossbeam-utils/0.8.23/source/Cargo.toml) | `0.8.23` | `MIT OR Apache-2.0` |
| [crunchy](https://docs.rs/crate/crunchy/0.2.4/source/Cargo.toml) | `0.2.4` | `MIT` |
| [crypto-common](https://docs.rs/crate/crypto-common/0.1.7/source/Cargo.toml) | `0.1.7` | `MIT OR Apache-2.0` |
| [ctrlc](https://docs.rs/crate/ctrlc/3.5.2/source/Cargo.toml) | `3.5.2` | `MIT/Apache-2.0` |
| [cursor-icon](https://docs.rs/crate/cursor-icon/1.2.0/source/Cargo.toml) | `1.2.0` | `MIT OR Apache-2.0 OR Zlib` |
| [dasp_sample](https://docs.rs/crate/dasp_sample/0.11.0/source/Cargo.toml) | `0.11.0` | `MIT OR Apache-2.0` |
| [data-encoding](https://docs.rs/crate/data-encoding/2.11.1/source/Cargo.toml) | `2.11.1` | `MIT` |
| [ddsfile](https://docs.rs/crate/ddsfile/0.5.2/source/Cargo.toml) | `0.5.2` | `MIT` |
| [derive_more](https://docs.rs/crate/derive_more/2.1.1/source/Cargo.toml) | `2.1.1` | `MIT` |
| [derive_more-impl](https://docs.rs/crate/derive_more-impl/2.1.1/source/Cargo.toml) | `2.1.1` | `MIT` |
| [digest](https://docs.rs/crate/digest/0.10.7/source/Cargo.toml) | `0.10.7` | `MIT OR Apache-2.0` |
| [dispatch](https://docs.rs/crate/dispatch/0.2.0/source/Cargo.toml) | `0.2.0` | `MIT` |
| [dispatch2](https://docs.rs/crate/dispatch2/0.3.1/source/Cargo.toml) | `0.3.1` | `Zlib OR Apache-2.0 OR MIT` |
| [displaydoc](https://docs.rs/crate/displaydoc/0.2.7/source/Cargo.toml) | `0.2.7` | `MIT OR Apache-2.0` |
| [disqualified](https://docs.rs/crate/disqualified/1.1.0/source/Cargo.toml) | `1.1.0` | `MIT OR Apache-2.0` |
| [dlib](https://docs.rs/crate/dlib/0.5.3/source/Cargo.toml) | `0.5.3` | `MIT` |
| [document-features](https://docs.rs/crate/document-features/0.2.12/source/Cargo.toml) | `0.2.12` | `MIT OR Apache-2.0` |
| [downcast-rs](https://docs.rs/crate/downcast-rs/2.0.2/source/Cargo.toml) | `2.0.2` | `MIT OR Apache-2.0` |
| [dpi](https://docs.rs/crate/dpi/0.1.2/source/Cargo.toml) | `0.1.2` | `Apache-2.0 AND MIT` |
| [either](https://docs.rs/crate/either/1.19.0/source/Cargo.toml) | `1.19.0` | `MIT OR Apache-2.0` |
| [encase](https://docs.rs/crate/encase/0.12.1/source/Cargo.toml) | `0.12.1` | `MIT-0` |
| [encase_derive](https://docs.rs/crate/encase_derive/0.12.1/source/Cargo.toml) | `0.12.1` | `MIT-0` |
| [encase_derive_impl](https://docs.rs/crate/encase_derive_impl/0.12.1/source/Cargo.toml) | `0.12.1` | `MIT-0` |
| [encoding_rs](https://docs.rs/crate/encoding_rs/0.8.42/source/Cargo.toml) | `0.8.42` | `(Apache-2.0 OR MIT) AND BSD-3-Clause` |
| [enum-primitive-derive](https://docs.rs/crate/enum-primitive-derive/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [equivalent](https://docs.rs/crate/equivalent/1.0.2/source/Cargo.toml) | `1.0.2` | `Apache-2.0 OR MIT` |
| [erased-serde](https://docs.rs/crate/erased-serde/0.4.10/source/Cargo.toml) | `0.4.10` | `MIT OR Apache-2.0` |
| [errno](https://docs.rs/crate/errno/0.3.14/source/Cargo.toml) | `0.3.14` | `MIT OR Apache-2.0` |
| [euclid](https://docs.rs/crate/euclid/0.22.14/source/Cargo.toml) | `0.22.14` | `MIT OR Apache-2.0` |
| [event-listener](https://docs.rs/crate/event-listener/5.4.2/source/Cargo.toml) | `5.4.2` | `Apache-2.0 OR MIT` |
| [event-listener-strategy](https://docs.rs/crate/event-listener-strategy/0.5.4/source/Cargo.toml) | `0.5.4` | `Apache-2.0 OR MIT` |
| [extended](https://docs.rs/crate/extended/0.1.0/source/Cargo.toml) | `0.1.0` | `MIT` |
| [fastrand](https://docs.rs/crate/fastrand/2.5.0/source/Cargo.toml) | `2.5.0` | `Apache-2.0 OR MIT` |
| [fdeflate](https://docs.rs/crate/fdeflate/0.3.7/source/Cargo.toml) | `0.3.7` | `MIT OR Apache-2.0` |
| [find-msvc-tools](https://docs.rs/crate/find-msvc-tools/0.1.14/source/Cargo.toml) | `0.1.14` | `MIT OR Apache-2.0` |
| [fixedbitset](https://docs.rs/crate/fixedbitset/0.5.7/source/Cargo.toml) | `0.5.7` | `MIT OR Apache-2.0` |
| [flate2](https://docs.rs/crate/flate2/1.1.10/source/Cargo.toml) | `1.1.10` | `MIT OR Apache-2.0` |
| [foldhash](https://docs.rs/crate/foldhash/0.1.5/source/Cargo.toml) | `0.1.5` | `Zlib` |
| [foldhash](https://docs.rs/crate/foldhash/0.2.0/source/Cargo.toml) | `0.2.0` | `Zlib` |
| [font-types](https://docs.rs/crate/font-types/0.11.3/source/Cargo.toml) | `0.11.3` | `MIT OR Apache-2.0` |
| [font-types](https://docs.rs/crate/font-types/0.12.6/source/Cargo.toml) | `0.12.6` | `MIT OR Apache-2.0` |
| [fontconfig-parser](https://docs.rs/crate/fontconfig-parser/0.5.8/source/Cargo.toml) | `0.5.8` | `MIT` |
| [fontdb](https://docs.rs/crate/fontdb/0.23.0/source/Cargo.toml) | `0.23.0` | `MIT` |
| [fontique](https://docs.rs/crate/fontique/0.9.0/source/Cargo.toml) | `0.9.0` | `Apache-2.0 OR MIT` |
| [foreign-types](https://docs.rs/crate/foreign-types/0.5.0/source/Cargo.toml) | `0.5.0` | `MIT/Apache-2.0` |
| [foreign-types-macros](https://docs.rs/crate/foreign-types-macros/0.2.4/source/Cargo.toml) | `0.2.4` | `MIT/Apache-2.0` |
| [foreign-types-shared](https://docs.rs/crate/foreign-types-shared/0.3.1/source/Cargo.toml) | `0.3.1` | `MIT/Apache-2.0` |
| [futures-channel](https://docs.rs/crate/futures-channel/0.3.34/source/Cargo.toml) | `0.3.34` | `MIT OR Apache-2.0` |
| [futures-core](https://docs.rs/crate/futures-core/0.3.34/source/Cargo.toml) | `0.3.34` | `MIT OR Apache-2.0` |
| [futures-io](https://docs.rs/crate/futures-io/0.3.34/source/Cargo.toml) | `0.3.34` | `MIT OR Apache-2.0` |
| [futures-lite](https://docs.rs/crate/futures-lite/2.6.1/source/Cargo.toml) | `2.6.1` | `Apache-2.0 OR MIT` |
| [futures-macro](https://docs.rs/crate/futures-macro/0.3.34/source/Cargo.toml) | `0.3.34` | `MIT OR Apache-2.0` |
| [futures-task](https://docs.rs/crate/futures-task/0.3.34/source/Cargo.toml) | `0.3.34` | `MIT OR Apache-2.0` |
| [futures-util](https://docs.rs/crate/futures-util/0.3.34/source/Cargo.toml) | `0.3.34` | `MIT OR Apache-2.0` |
| [generic-array](https://docs.rs/crate/generic-array/0.14.7/source/Cargo.toml) | `0.14.7` | `MIT` |
| [gethostname](https://docs.rs/crate/gethostname/1.1.0/source/Cargo.toml) | `1.1.0` | `Apache-2.0` |
| [getrandom](https://docs.rs/crate/getrandom/0.4.3/source/Cargo.toml) | `0.4.3` | `MIT OR Apache-2.0` |
| [glam](https://docs.rs/crate/glam/0.32.1/source/Cargo.toml) | `0.32.1` | `MIT OR Apache-2.0` |
| [gpu-allocator](https://docs.rs/crate/gpu-allocator/0.28.0/source/Cargo.toml) | `0.28.0` | `MIT OR Apache-2.0` |
| [gpu-descriptor](https://docs.rs/crate/gpu-descriptor/0.3.2/source/Cargo.toml) | `0.3.2` | `MIT OR Apache-2.0` |
| [gpu-descriptor-types](https://docs.rs/crate/gpu-descriptor-types/0.2.0/source/Cargo.toml) | `0.2.0` | `MIT OR Apache-2.0` |
| [grid](https://docs.rs/crate/grid/1.0.1/source/Cargo.toml) | `1.0.1` | `MIT` |
| [guillotiere](https://docs.rs/crate/guillotiere/0.6.2/source/Cargo.toml) | `0.6.2` | `MIT/Apache-2.0` |
| [half](https://docs.rs/crate/half/2.7.1/source/Cargo.toml) | `2.7.1` | `MIT OR Apache-2.0` |
| [harfrust](https://docs.rs/crate/harfrust/0.6.2/source/Cargo.toml) | `0.6.2` | `MIT` |
| [hash32](https://docs.rs/crate/hash32/0.3.1/source/Cargo.toml) | `0.3.1` | `MIT OR Apache-2.0` |
| [hashbrown](https://docs.rs/crate/hashbrown/0.15.5/source/Cargo.toml) | `0.15.5` | `MIT OR Apache-2.0` |
| [hashbrown](https://docs.rs/crate/hashbrown/0.16.1/source/Cargo.toml) | `0.16.1` | `MIT OR Apache-2.0` |
| [hashbrown](https://docs.rs/crate/hashbrown/0.17.1/source/Cargo.toml) | `0.17.1` | `MIT OR Apache-2.0` |
| [heapless](https://docs.rs/crate/heapless/0.9.3/source/Cargo.toml) | `0.9.3` | `MIT OR Apache-2.0` |
| [hermit-abi](https://docs.rs/crate/hermit-abi/0.5.3/source/Cargo.toml) | `0.5.3` | `MIT OR Apache-2.0` |
| [hexasphere](https://docs.rs/crate/hexasphere/18.0.0/source/Cargo.toml) | `18.0.0` | `MIT OR Apache-2.0` |
| [hexf-parse](https://docs.rs/crate/hexf-parse/0.2.1/source/Cargo.toml) | `0.2.1` | `CC0-1.0` |
| [hostname](https://docs.rs/crate/hostname/0.4.1/source/Cargo.toml) | `0.4.1` | `MIT` |
| [icu_collections](https://docs.rs/crate/icu_collections/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_locale_core](https://docs.rs/crate/icu_locale_core/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_locale_fallback](https://docs.rs/crate/icu_locale_fallback/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_locale_fallback_data](https://docs.rs/crate/icu_locale_fallback_data/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_normalizer](https://docs.rs/crate/icu_normalizer/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_normalizer_data](https://docs.rs/crate/icu_normalizer_data/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_properties](https://docs.rs/crate/icu_properties/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_properties_data](https://docs.rs/crate/icu_properties_data/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_provider](https://docs.rs/crate/icu_provider/2.3.1/source/Cargo.toml) | `2.3.1` | `Unicode-3.0` |
| [icu_segmenter](https://docs.rs/crate/icu_segmenter/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [icu_segmenter_data](https://docs.rs/crate/icu_segmenter_data/2.3.0/source/Cargo.toml) | `2.3.0` | `Unicode-3.0` |
| [image](https://docs.rs/crate/image/0.25.10/source/Cargo.toml) | `0.25.10` | `MIT OR Apache-2.0` |
| [indexmap](https://docs.rs/crate/indexmap/2.14.2/source/Cargo.toml) | `2.14.2` | `Apache-2.0 OR MIT` |
| [inventory](https://docs.rs/crate/inventory/0.3.24/source/Cargo.toml) | `0.3.24` | `MIT OR Apache-2.0` |
| [itertools](https://docs.rs/crate/itertools/0.14.0/source/Cargo.toml) | `0.14.0` | `MIT OR Apache-2.0` |
| [jni](https://docs.rs/crate/jni/0.21.1/source/Cargo.toml) | `0.21.1` | `MIT/Apache-2.0` |
| [jni](https://docs.rs/crate/jni/0.22.4/source/Cargo.toml) | `0.22.4` | `MIT OR Apache-2.0` |
| [jni-macros](https://docs.rs/crate/jni-macros/0.22.4/source/Cargo.toml) | `0.22.4` | `MIT OR Apache-2.0` |
| [jni-sys](https://docs.rs/crate/jni-sys/0.3.1/source/Cargo.toml) | `0.3.1` | `MIT OR Apache-2.0` |
| [jni-sys](https://docs.rs/crate/jni-sys/0.4.1/source/Cargo.toml) | `0.4.1` | `MIT OR Apache-2.0` |
| [jni-sys-macros](https://docs.rs/crate/jni-sys-macros/0.4.1/source/Cargo.toml) | `0.4.1` | `MIT OR Apache-2.0` |
| [jobserver](https://docs.rs/crate/jobserver/0.1.35/source/Cargo.toml) | `0.1.35` | `MIT OR Apache-2.0` |
| [js-sys](https://docs.rs/crate/js-sys/0.3.106/source/Cargo.toml) | `0.3.106` | `MIT OR Apache-2.0` |
| [lazy_static](https://docs.rs/crate/lazy_static/1.5.1/source/Cargo.toml) | `1.5.1` | `MIT OR Apache-2.0` |
| [libc](https://docs.rs/crate/libc/0.2.190/source/Cargo.toml) | `0.2.190` | `MIT OR Apache-2.0` |
| [libloading](https://docs.rs/crate/libloading/0.8.9/source/Cargo.toml) | `0.8.9` | `ISC` |
| [libm](https://docs.rs/crate/libm/0.2.16/source/Cargo.toml) | `0.2.16` | `MIT` |
| [libredox](https://docs.rs/crate/libredox/0.1.25/source/Cargo.toml) | `0.1.25` | `MIT` |
| [linebender_resource_handle](https://docs.rs/crate/linebender_resource_handle/0.1.1/source/Cargo.toml) | `0.1.1` | `Apache-2.0 OR MIT` |
| [linux-raw-sys](https://docs.rs/crate/linux-raw-sys/0.12.1/source/Cargo.toml) | `0.12.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` |
| [linux-raw-sys](https://docs.rs/crate/linux-raw-sys/0.4.15/source/Cargo.toml) | `0.4.15` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` |
| [litemap](https://docs.rs/crate/litemap/0.8.3/source/Cargo.toml) | `0.8.3` | `Unicode-3.0` |
| [litrs](https://docs.rs/crate/litrs/1.0.0/source/Cargo.toml) | `1.0.0` | `MIT OR Apache-2.0` |
| [lock_api](https://docs.rs/crate/lock_api/0.4.14/source/Cargo.toml) | `0.4.14` | `MIT OR Apache-2.0` |
| [log](https://docs.rs/crate/log/0.4.34/source/Cargo.toml) | `0.4.34` | `MIT OR Apache-2.0` |
| [mach2](https://docs.rs/crate/mach2/0.5.0/source/Cargo.toml) | `0.5.0` | `BSD-2-Clause OR MIT OR Apache-2.0` |
| [matchers](https://docs.rs/crate/matchers/0.2.0/source/Cargo.toml) | `0.2.0` | `MIT` |
| [memchr](https://docs.rs/crate/memchr/2.8.3/source/Cargo.toml) | `2.8.3` | `Unlicense OR MIT` |
| [memmap2](https://docs.rs/crate/memmap2/0.9.11/source/Cargo.toml) | `0.9.11` | `MIT OR Apache-2.0` |
| [miniz_oxide](https://docs.rs/crate/miniz_oxide/0.8.9/source/Cargo.toml) | `0.8.9` | `MIT OR Zlib OR Apache-2.0` |
| [miniz_oxide](https://docs.rs/crate/miniz_oxide/0.9.1/source/Cargo.toml) | `0.9.1` | `MIT OR Zlib OR Apache-2.0` |
| [moxcms](https://docs.rs/crate/moxcms/0.8.1/source/Cargo.toml) | `0.8.1` | `BSD-3-Clause OR Apache-2.0` |
| [multiversion_no_op](https://docs.rs/crate/multiversion_no_op/1.0.0/source/Cargo.toml) | `1.0.0` | `Apache-2.0 OR MIT` |
| [naga](https://docs.rs/crate/naga/29.0.4/source/Cargo.toml) | `29.0.4` | `MIT OR Apache-2.0` |
| [naga_oil](https://docs.rs/crate/naga_oil/0.22.0/source/Cargo.toml) | `0.22.0` | `MIT OR Apache-2.0` |
| [ndk](https://docs.rs/crate/ndk/0.9.0/source/Cargo.toml) | `0.9.0` | `MIT OR Apache-2.0` |
| [ndk-context](https://docs.rs/crate/ndk-context/0.1.1/source/Cargo.toml) | `0.1.1` | `MIT OR Apache-2.0` |
| [ndk-sys](https://docs.rs/crate/ndk-sys/0.6.0+11769913/source/Cargo.toml) | `0.6.0+11769913` | `MIT OR Apache-2.0` |
| [nix](https://docs.rs/crate/nix/0.31.3/source/Cargo.toml) | `0.31.3` | `MIT` |
| [nonmax](https://docs.rs/crate/nonmax/0.5.5/source/Cargo.toml) | `0.5.5` | `MIT OR Apache-2.0` |
| [ntapi](https://docs.rs/crate/ntapi/0.4.3/source/Cargo.toml) | `0.4.3` | `Apache-2.0 OR MIT` |
| [nu-ansi-term](https://docs.rs/crate/nu-ansi-term/0.50.3/source/Cargo.toml) | `0.50.3` | `MIT` |
| [num-bigint](https://docs.rs/crate/num-bigint/0.4.8/source/Cargo.toml) | `0.4.8` | `MIT OR Apache-2.0` |
| [num-derive](https://docs.rs/crate/num-derive/0.4.2/source/Cargo.toml) | `0.4.2` | `MIT OR Apache-2.0` |
| [num-integer](https://docs.rs/crate/num-integer/0.1.47/source/Cargo.toml) | `0.1.47` | `MIT OR Apache-2.0` |
| [num-rational](https://docs.rs/crate/num-rational/0.4.2/source/Cargo.toml) | `0.4.2` | `MIT OR Apache-2.0` |
| [num-traits](https://docs.rs/crate/num-traits/0.2.19/source/Cargo.toml) | `0.2.19` | `MIT OR Apache-2.0` |
| [num_enum](https://docs.rs/crate/num_enum/0.7.6/source/Cargo.toml) | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` |
| [num_enum_derive](https://docs.rs/crate/num_enum_derive/0.7.6/source/Cargo.toml) | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` |
| [objc-sys](https://docs.rs/crate/objc-sys/0.3.5/source/Cargo.toml) | `0.3.5` | `MIT` |
| [objc2](https://docs.rs/crate/objc2/0.5.3/source/Cargo.toml) | `0.5.3` | `MIT` |
| [objc2](https://docs.rs/crate/objc2/0.6.5/source/Cargo.toml) | `0.6.5` | `MIT` |
| [objc2-app-kit](https://docs.rs/crate/objc2-app-kit/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-audio-toolbox](https://docs.rs/crate/objc2-audio-toolbox/0.3.2/source/Cargo.toml) | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` |
| [objc2-avf-audio](https://docs.rs/crate/objc2-avf-audio/0.3.2/source/Cargo.toml) | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` |
| [objc2-cloud-kit](https://docs.rs/crate/objc2-cloud-kit/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-contacts](https://docs.rs/crate/objc2-contacts/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-core-audio](https://docs.rs/crate/objc2-core-audio/0.3.2/source/Cargo.toml) | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` |
| [objc2-core-audio-types](https://docs.rs/crate/objc2-core-audio-types/0.3.2/source/Cargo.toml) | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` |
| [objc2-core-data](https://docs.rs/crate/objc2-core-data/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-core-foundation](https://docs.rs/crate/objc2-core-foundation/0.3.2/source/Cargo.toml) | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` |
| [objc2-core-image](https://docs.rs/crate/objc2-core-image/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-core-location](https://docs.rs/crate/objc2-core-location/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-encode](https://docs.rs/crate/objc2-encode/4.1.0/source/Cargo.toml) | `4.1.0` | `MIT` |
| [objc2-foundation](https://docs.rs/crate/objc2-foundation/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-foundation](https://docs.rs/crate/objc2-foundation/0.3.2/source/Cargo.toml) | `0.3.2` | `MIT` |
| [objc2-io-kit](https://docs.rs/crate/objc2-io-kit/0.3.2/source/Cargo.toml) | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` |
| [objc2-link-presentation](https://docs.rs/crate/objc2-link-presentation/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-metal](https://docs.rs/crate/objc2-metal/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-metal](https://docs.rs/crate/objc2-metal/0.3.2/source/Cargo.toml) | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` |
| [objc2-quartz-core](https://docs.rs/crate/objc2-quartz-core/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-quartz-core](https://docs.rs/crate/objc2-quartz-core/0.3.2/source/Cargo.toml) | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` |
| [objc2-symbols](https://docs.rs/crate/objc2-symbols/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-ui-kit](https://docs.rs/crate/objc2-ui-kit/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-uniform-type-identifiers](https://docs.rs/crate/objc2-uniform-type-identifiers/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [objc2-user-notifications](https://docs.rs/crate/objc2-user-notifications/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT` |
| [offset-allocator](https://docs.rs/crate/offset-allocator/0.2.0/source/Cargo.toml) | `0.2.0` | `MIT` |
| [once_cell](https://docs.rs/crate/once_cell/1.21.4/source/Cargo.toml) | `1.21.4` | `MIT OR Apache-2.0` |
| [orbclient](https://docs.rs/crate/orbclient/0.3.55/source/Cargo.toml) | `0.3.55` | `MIT` |
| [ordered-float](https://docs.rs/crate/ordered-float/5.5.0/source/Cargo.toml) | `5.5.0` | `MIT` |
| [parking](https://docs.rs/crate/parking/2.2.1/source/Cargo.toml) | `2.2.1` | `Apache-2.0 OR MIT` |
| [parking_lot](https://docs.rs/crate/parking_lot/0.12.5/source/Cargo.toml) | `0.12.5` | `MIT OR Apache-2.0` |
| [parking_lot_core](https://docs.rs/crate/parking_lot_core/0.9.12/source/Cargo.toml) | `0.9.12` | `MIT OR Apache-2.0` |
| [parlance](https://docs.rs/crate/parlance/0.1.0/source/Cargo.toml) | `0.1.0` | `Apache-2.0 OR MIT` |
| [parley](https://docs.rs/crate/parley/0.9.0/source/Cargo.toml) | `0.9.0` | `Apache-2.0 OR MIT` |
| [parley_data](https://docs.rs/crate/parley_data/0.9.0/source/Cargo.toml) | `0.9.0` | `Apache-2.0 OR MIT` |
| [percent-encoding](https://docs.rs/crate/percent-encoding/2.3.2/source/Cargo.toml) | `2.3.2` | `MIT OR Apache-2.0` |
| [pin-project](https://docs.rs/crate/pin-project/1.1.13/source/Cargo.toml) | `1.1.13` | `Apache-2.0 OR MIT` |
| [pin-project-internal](https://docs.rs/crate/pin-project-internal/1.1.13/source/Cargo.toml) | `1.1.13` | `Apache-2.0 OR MIT` |
| [pin-project-lite](https://docs.rs/crate/pin-project-lite/0.2.17/source/Cargo.toml) | `0.2.17` | `Apache-2.0 OR MIT` |
| [piper](https://docs.rs/crate/piper/0.2.5/source/Cargo.toml) | `0.2.5` | `MIT OR Apache-2.0` |
| [pkg-config](https://docs.rs/crate/pkg-config/0.3.34/source/Cargo.toml) | `0.3.34` | `MIT OR Apache-2.0` |
| [plain](https://docs.rs/crate/plain/0.2.3/source/Cargo.toml) | `0.2.3` | `MIT/Apache-2.0` |
| [png](https://docs.rs/crate/png/0.18.1/source/Cargo.toml) | `0.18.1` | `MIT OR Apache-2.0` |
| [polling](https://docs.rs/crate/polling/3.11.0/source/Cargo.toml) | `3.11.0` | `Apache-2.0 OR MIT` |
| [portable-atomic](https://docs.rs/crate/portable-atomic/1.15.0/source/Cargo.toml) | `1.15.0` | `Apache-2.0 OR MIT` |
| [portable-atomic-util](https://docs.rs/crate/portable-atomic-util/0.2.8/source/Cargo.toml) | `0.2.8` | `Apache-2.0 OR MIT` |
| [potential_utf](https://docs.rs/crate/potential_utf/0.1.6/source/Cargo.toml) | `0.1.6` | `Unicode-3.0` |
| [pp-rs](https://docs.rs/crate/pp-rs/0.2.1/source/Cargo.toml) | `0.2.1` | `BSD-3-Clause` |
| [presser](https://docs.rs/crate/presser/0.3.1/source/Cargo.toml) | `0.3.1` | `MIT OR Apache-2.0` |
| [proc-macro-crate](https://docs.rs/crate/proc-macro-crate/3.5.0/source/Cargo.toml) | `3.5.0` | `MIT OR Apache-2.0` |
| [proc-macro2](https://docs.rs/crate/proc-macro2/1.0.107/source/Cargo.toml) | `1.0.107` | `MIT OR Apache-2.0` |
| [profiling](https://docs.rs/crate/profiling/1.0.18/source/Cargo.toml) | `1.0.18` | `MIT OR Apache-2.0` |
| [pxfm](https://docs.rs/crate/pxfm/0.1.30/source/Cargo.toml) | `0.1.30` | `BSD-3-Clause OR Apache-2.0` |
| [quote](https://docs.rs/crate/quote/1.0.47/source/Cargo.toml) | `1.0.47` | `MIT OR Apache-2.0` |
| [r-efi](https://docs.rs/crate/r-efi/6.0.0/source/Cargo.toml) | `6.0.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` |
| [radsort](https://docs.rs/crate/radsort/0.1.1/source/Cargo.toml) | `0.1.1` | `MIT OR Apache-2.0` |
| [rand](https://docs.rs/crate/rand/0.10.3/source/Cargo.toml) | `0.10.3` | `MIT OR Apache-2.0` |
| [rand_core](https://docs.rs/crate/rand_core/0.10.1/source/Cargo.toml) | `0.10.1` | `MIT OR Apache-2.0` |
| [rand_distr](https://docs.rs/crate/rand_distr/0.6.0/source/Cargo.toml) | `0.6.0` | `MIT OR Apache-2.0` |
| [range-alloc](https://docs.rs/crate/range-alloc/0.1.5/source/Cargo.toml) | `0.1.5` | `MIT OR Apache-2.0` |
| [raw-window-handle](https://docs.rs/crate/raw-window-handle/0.6.2/source/Cargo.toml) | `0.6.2` | `MIT OR Apache-2.0 OR Zlib` |
| [raw-window-metal](https://docs.rs/crate/raw-window-metal/1.1.0/source/Cargo.toml) | `1.1.0` | `MIT OR Apache-2.0` |
| [read-fonts](https://docs.rs/crate/read-fonts/0.39.2/source/Cargo.toml) | `0.39.2` | `MIT OR Apache-2.0` |
| [read-fonts](https://docs.rs/crate/read-fonts/0.41.0/source/Cargo.toml) | `0.41.0` | `MIT OR Apache-2.0` |
| [rectangle-pack](https://docs.rs/crate/rectangle-pack/0.4.2/source/Cargo.toml) | `0.4.2` | `MIT/Apache-2.0` |
| [redox_syscall](https://docs.rs/crate/redox_syscall/0.4.1/source/Cargo.toml) | `0.4.1` | `MIT` |
| [redox_syscall](https://docs.rs/crate/redox_syscall/0.5.18/source/Cargo.toml) | `0.5.18` | `MIT` |
| [redox_syscall](https://docs.rs/crate/redox_syscall/0.9.4/source/Cargo.toml) | `0.9.4` | `MIT` |
| [regex](https://docs.rs/crate/regex/1.13.1/source/Cargo.toml) | `1.13.1` | `MIT OR Apache-2.0` |
| [regex-automata](https://docs.rs/crate/regex-automata/0.4.18/source/Cargo.toml) | `0.4.18` | `MIT OR Apache-2.0` |
| [regex-syntax](https://docs.rs/crate/regex-syntax/0.8.11/source/Cargo.toml) | `0.8.11` | `MIT OR Apache-2.0` |
| [renderdoc-sys](https://docs.rs/crate/renderdoc-sys/1.1.0/source/Cargo.toml) | `1.1.0` | `MIT OR Apache-2.0` |
| [rodio](https://docs.rs/crate/rodio/0.22.2/source/Cargo.toml) | `0.22.2` | `MIT OR Apache-2.0` |
| [ron](https://docs.rs/crate/ron/0.12.2/source/Cargo.toml) | `0.12.2` | `MIT OR Apache-2.0` |
| [roxmltree](https://docs.rs/crate/roxmltree/0.20.0/source/Cargo.toml) | `0.20.0` | `MIT OR Apache-2.0` |
| [rustc-hash](https://docs.rs/crate/rustc-hash/1.1.0/source/Cargo.toml) | `1.1.0` | `Apache-2.0/MIT` |
| [rustc_version](https://docs.rs/crate/rustc_version/0.4.1/source/Cargo.toml) | `0.4.1` | `MIT OR Apache-2.0` |
| [rustix](https://docs.rs/crate/rustix/0.38.44/source/Cargo.toml) | `0.38.44` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` |
| [rustix](https://docs.rs/crate/rustix/1.1.5/source/Cargo.toml) | `1.1.5` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` |
| [rustversion](https://docs.rs/crate/rustversion/1.0.23/source/Cargo.toml) | `1.0.23` | `MIT OR Apache-2.0` |
| [same-file](https://docs.rs/crate/same-file/1.0.6/source/Cargo.toml) | `1.0.6` | `Unlicense/MIT` |
| [scopeguard](https://docs.rs/crate/scopeguard/1.2.0/source/Cargo.toml) | `1.2.0` | `MIT OR Apache-2.0` |
| [semver](https://docs.rs/crate/semver/1.0.28/source/Cargo.toml) | `1.0.28` | `MIT OR Apache-2.0` |
| [send_wrapper](https://docs.rs/crate/send_wrapper/0.6.0/source/Cargo.toml) | `0.6.0` | `MIT/Apache-2.0` |
| [serde](https://docs.rs/crate/serde/1.0.229/source/Cargo.toml) | `1.0.229` | `MIT OR Apache-2.0` |
| [serde_core](https://docs.rs/crate/serde_core/1.0.229/source/Cargo.toml) | `1.0.229` | `MIT OR Apache-2.0` |
| [serde_derive](https://docs.rs/crate/serde_derive/1.0.229/source/Cargo.toml) | `1.0.229` | `MIT OR Apache-2.0` |
| [sha2](https://docs.rs/crate/sha2/0.10.9/source/Cargo.toml) | `0.10.9` | `MIT OR Apache-2.0` |
| [sharded-slab](https://docs.rs/crate/sharded-slab/0.1.7/source/Cargo.toml) | `0.1.7` | `MIT` |
| [shlex](https://docs.rs/crate/shlex/2.0.1/source/Cargo.toml) | `2.0.1` | `MIT OR Apache-2.0` |
| [simd-adler32](https://docs.rs/crate/simd-adler32/0.3.10/source/Cargo.toml) | `0.3.10` | `MIT` |
| [simd_cesu8](https://docs.rs/crate/simd_cesu8/1.2.0/source/Cargo.toml) | `1.2.0` | `Apache-2.0 OR MIT` |
| [simdutf8](https://docs.rs/crate/simdutf8/0.1.5/source/Cargo.toml) | `0.1.5` | `MIT OR Apache-2.0` |
| [skrifa](https://docs.rs/crate/skrifa/0.42.1/source/Cargo.toml) | `0.42.1` | `MIT OR Apache-2.0` |
| [skrifa](https://docs.rs/crate/skrifa/0.44.0/source/Cargo.toml) | `0.44.0` | `MIT OR Apache-2.0` |
| [slab](https://docs.rs/crate/slab/0.4.12/source/Cargo.toml) | `0.4.12` | `MIT` |
| [slotmap](https://docs.rs/crate/slotmap/1.1.1/source/Cargo.toml) | `1.1.1` | `Zlib` |
| [smallvec](https://docs.rs/crate/smallvec/1.16.2/source/Cargo.toml) | `1.16.2` | `MIT OR Apache-2.0` |
| [smol_str](https://docs.rs/crate/smol_str/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT OR Apache-2.0` |
| [spin](https://docs.rs/crate/spin/0.10.1/source/Cargo.toml) | `0.10.1` | `MIT` |
| [spirv](https://docs.rs/crate/spirv/0.4.0+sdk-1.4.341.0/source/Cargo.toml) | `0.4.0+sdk-1.4.341.0` | `Apache-2.0` |
| [stable_deref_trait](https://docs.rs/crate/stable_deref_trait/1.2.1/source/Cargo.toml) | `1.2.1` | `MIT OR Apache-2.0` |
| [stackfuture](https://docs.rs/crate/stackfuture/0.3.1/source/Cargo.toml) | `0.3.1` | `MIT` |
| [static_assertions](https://docs.rs/crate/static_assertions/1.1.0/source/Cargo.toml) | `1.1.0` | `MIT OR Apache-2.0` |
| [svg_fmt](https://docs.rs/crate/svg_fmt/0.4.5/source/Cargo.toml) | `0.4.5` | `MIT/Apache-2.0` |
| [swash](https://docs.rs/crate/swash/0.2.10/source/Cargo.toml) | `0.2.10` | `Apache-2.0 OR MIT` |
| [symphonia](https://docs.rs/crate/symphonia/0.5.5/source/Cargo.toml) | `0.5.5` | `MPL-2.0` |
| [symphonia-bundle-mp3](https://docs.rs/crate/symphonia-bundle-mp3/0.5.5/source/Cargo.toml) | `0.5.5` | `MPL-2.0` |
| [symphonia-codec-adpcm](https://docs.rs/crate/symphonia-codec-adpcm/0.5.5/source/Cargo.toml) | `0.5.5` | `MPL-2.0` |
| [symphonia-codec-pcm](https://docs.rs/crate/symphonia-codec-pcm/0.5.5/source/Cargo.toml) | `0.5.5` | `MPL-2.0` |
| [symphonia-core](https://docs.rs/crate/symphonia-core/0.5.5/source/Cargo.toml) | `0.5.5` | `MPL-2.0` |
| [symphonia-format-riff](https://docs.rs/crate/symphonia-format-riff/0.5.5/source/Cargo.toml) | `0.5.5` | `MPL-2.0` |
| [symphonia-metadata](https://docs.rs/crate/symphonia-metadata/0.5.5/source/Cargo.toml) | `0.5.5` | `MPL-2.0` |
| [syn](https://docs.rs/crate/syn/1.0.109/source/Cargo.toml) | `1.0.109` | `MIT OR Apache-2.0` |
| [syn](https://docs.rs/crate/syn/2.0.119/source/Cargo.toml) | `2.0.119` | `MIT OR Apache-2.0` |
| [syn](https://docs.rs/crate/syn/3.0.6/source/Cargo.toml) | `3.0.6` | `MIT OR Apache-2.0` |
| [synstructure](https://docs.rs/crate/synstructure/0.14.0/source/Cargo.toml) | `0.14.0` | `MIT` |
| [sys-locale](https://docs.rs/crate/sys-locale/0.3.2/source/Cargo.toml) | `0.3.2` | `MIT OR Apache-2.0` |
| [sysinfo](https://docs.rs/crate/sysinfo/0.40.1/source/Cargo.toml) | `0.40.1` | `MIT` |
| [taffy](https://docs.rs/crate/taffy/0.10.1/source/Cargo.toml) | `0.10.1` | `MIT` |
| [termcolor](https://docs.rs/crate/termcolor/1.4.1/source/Cargo.toml) | `1.4.1` | `Unlicense OR MIT` |
| [thiserror](https://docs.rs/crate/thiserror/1.0.69/source/Cargo.toml) | `1.0.69` | `MIT OR Apache-2.0` |
| [thiserror](https://docs.rs/crate/thiserror/2.0.21/source/Cargo.toml) | `2.0.21` | `MIT OR Apache-2.0` |
| [thiserror-impl](https://docs.rs/crate/thiserror-impl/1.0.69/source/Cargo.toml) | `1.0.69` | `MIT OR Apache-2.0` |
| [thiserror-impl](https://docs.rs/crate/thiserror-impl/2.0.21/source/Cargo.toml) | `2.0.21` | `MIT OR Apache-2.0` |
| [thread_local](https://docs.rs/crate/thread_local/1.1.10/source/Cargo.toml) | `1.1.10` | `MIT OR Apache-2.0` |
| [tinystr](https://docs.rs/crate/tinystr/0.8.4/source/Cargo.toml) | `0.8.4` | `Unicode-3.0` |
| [tinyvec](https://docs.rs/crate/tinyvec/1.13.3/source/Cargo.toml) | `1.13.3` | `Zlib OR Apache-2.0 OR MIT` |
| [tokio](https://docs.rs/crate/tokio/1.53.2/source/Cargo.toml) | `1.53.2` | `MIT` |
| [toml_datetime](https://docs.rs/crate/toml_datetime/1.1.1+spec-1.1.0/source/Cargo.toml) | `1.1.1+spec-1.1.0` | `MIT OR Apache-2.0` |
| [toml_edit](https://docs.rs/crate/toml_edit/0.25.15+spec-1.1.0/source/Cargo.toml) | `0.25.15+spec-1.1.0` | `MIT OR Apache-2.0` |
| [toml_parser](https://docs.rs/crate/toml_parser/1.1.3+spec-1.1.0/source/Cargo.toml) | `1.1.3+spec-1.1.0` | `MIT OR Apache-2.0` |
| [tracing](https://docs.rs/crate/tracing/0.1.44/source/Cargo.toml) | `0.1.44` | `MIT` |
| [tracing-attributes](https://docs.rs/crate/tracing-attributes/0.1.31/source/Cargo.toml) | `0.1.31` | `MIT` |
| [tracing-core](https://docs.rs/crate/tracing-core/0.1.36/source/Cargo.toml) | `0.1.36` | `MIT` |
| [tracing-log](https://docs.rs/crate/tracing-log/0.2.0/source/Cargo.toml) | `0.2.0` | `MIT` |
| [tracing-oslog](https://docs.rs/crate/tracing-oslog/0.3.0/source/Cargo.toml) | `0.3.0` | `Zlib` |
| [tracing-subscriber](https://docs.rs/crate/tracing-subscriber/0.3.23/source/Cargo.toml) | `0.3.23` | `MIT` |
| [tracing-wasm](https://docs.rs/crate/tracing-wasm/0.2.1/source/Cargo.toml) | `0.2.1` | `MIT OR Apache-2.0` |
| [ttf-parser](https://docs.rs/crate/ttf-parser/0.25.1/source/Cargo.toml) | `0.25.1` | `MIT OR Apache-2.0` |
| [typeid](https://docs.rs/crate/typeid/1.0.3/source/Cargo.toml) | `1.0.3` | `MIT OR Apache-2.0` |
| [typenum](https://docs.rs/crate/typenum/1.20.1/source/Cargo.toml) | `1.20.1` | `MIT OR Apache-2.0` |
| [typewit](https://docs.rs/crate/typewit/1.15.2/source/Cargo.toml) | `1.15.2` | `Zlib` |
| [unicode-ident](https://docs.rs/crate/unicode-ident/1.0.26/source/Cargo.toml) | `1.0.26` | `(MIT OR Apache-2.0) AND Unicode-3.0` |
| [unicode-segmentation](https://docs.rs/crate/unicode-segmentation/1.13.3/source/Cargo.toml) | `1.13.3` | `MIT OR Apache-2.0` |
| [unicode-width](https://docs.rs/crate/unicode-width/0.2.2/source/Cargo.toml) | `0.2.2` | `MIT OR Apache-2.0` |
| [unicode-xid](https://docs.rs/crate/unicode-xid/0.2.6/source/Cargo.toml) | `0.2.6` | `MIT OR Apache-2.0` |
| [utf8_iter](https://docs.rs/crate/utf8_iter/1.0.4/source/Cargo.toml) | `1.0.4` | `Apache-2.0 OR MIT` |
| [uuid](https://docs.rs/crate/uuid/1.27.0/source/Cargo.toml) | `1.27.0` | `Apache-2.0 OR MIT` |
| [valuable](https://docs.rs/crate/valuable/0.1.1/source/Cargo.toml) | `0.1.1` | `MIT` |
| [variadics_please](https://docs.rs/crate/variadics_please/1.1.0/source/Cargo.toml) | `1.1.0` | `MIT OR Apache-2.0` |
| [version_check](https://docs.rs/crate/version_check/0.9.5/source/Cargo.toml) | `0.9.5` | `MIT/Apache-2.0` |
| [walkdir](https://docs.rs/crate/walkdir/2.5.0/source/Cargo.toml) | `2.5.0` | `Unlicense/MIT` |
| [wasm-bindgen](https://docs.rs/crate/wasm-bindgen/0.2.129/source/Cargo.toml) | `0.2.129` | `MIT OR Apache-2.0` |
| [wasm-bindgen-futures](https://docs.rs/crate/wasm-bindgen-futures/0.4.79/source/Cargo.toml) | `0.4.79` | `MIT OR Apache-2.0` |
| [wasm-bindgen-macro](https://docs.rs/crate/wasm-bindgen-macro/0.2.129/source/Cargo.toml) | `0.2.129` | `MIT OR Apache-2.0` |
| [wasm-bindgen-macro-support](https://docs.rs/crate/wasm-bindgen-macro-support/0.2.129/source/Cargo.toml) | `0.2.129` | `MIT OR Apache-2.0` |
| [wasm-bindgen-shared](https://docs.rs/crate/wasm-bindgen-shared/0.2.129/source/Cargo.toml) | `0.2.129` | `MIT OR Apache-2.0` |
| [weak-table](https://docs.rs/crate/weak-table/0.3.2/source/Cargo.toml) | `0.3.2` | `MIT` |
| [web-sys](https://docs.rs/crate/web-sys/0.3.106/source/Cargo.toml) | `0.3.106` | `MIT OR Apache-2.0` |
| [web-task](https://docs.rs/crate/web-task/1.1.3/source/Cargo.toml) | `1.1.3` | `MIT OR Apache-2.0` |
| [web-time](https://docs.rs/crate/web-time/1.1.0/source/Cargo.toml) | `1.1.0` | `MIT OR Apache-2.0` |
| [wgpu](https://docs.rs/crate/wgpu/29.0.4/source/Cargo.toml) | `29.0.4` | `MIT OR Apache-2.0` |
| [wgpu-core](https://docs.rs/crate/wgpu-core/29.0.4/source/Cargo.toml) | `29.0.4` | `MIT OR Apache-2.0` |
| [wgpu-core-deps-apple](https://docs.rs/crate/wgpu-core-deps-apple/29.0.4/source/Cargo.toml) | `29.0.4` | `MIT OR Apache-2.0` |
| [wgpu-core-deps-windows-linux-android](https://docs.rs/crate/wgpu-core-deps-windows-linux-android/29.0.4/source/Cargo.toml) | `29.0.4` | `MIT OR Apache-2.0` |
| [wgpu-hal](https://docs.rs/crate/wgpu-hal/29.0.4/source/Cargo.toml) | `29.0.4` | `MIT OR Apache-2.0` |
| [wgpu-naga-bridge](https://docs.rs/crate/wgpu-naga-bridge/29.0.4/source/Cargo.toml) | `29.0.4` | `MIT OR Apache-2.0` |
| [wgpu-types](https://docs.rs/crate/wgpu-types/29.0.4/source/Cargo.toml) | `29.0.4` | `MIT OR Apache-2.0` |
| [winapi](https://docs.rs/crate/winapi/0.3.9/source/Cargo.toml) | `0.3.9` | `MIT/Apache-2.0` |
| [winapi-i686-pc-windows-gnu](https://docs.rs/crate/winapi-i686-pc-windows-gnu/0.4.0/source/Cargo.toml) | `0.4.0` | `MIT/Apache-2.0` |
| [winapi-util](https://docs.rs/crate/winapi-util/0.1.11/source/Cargo.toml) | `0.1.11` | `Unlicense OR MIT` |
| [winapi-x86_64-pc-windows-gnu](https://docs.rs/crate/winapi-x86_64-pc-windows-gnu/0.4.0/source/Cargo.toml) | `0.4.0` | `MIT/Apache-2.0` |
| [windows](https://docs.rs/crate/windows/0.62.2/source/Cargo.toml) | `0.62.2` | `MIT OR Apache-2.0` |
| [windows-collections](https://docs.rs/crate/windows-collections/0.3.2/source/Cargo.toml) | `0.3.2` | `MIT OR Apache-2.0` |
| [windows-core](https://docs.rs/crate/windows-core/0.62.2/source/Cargo.toml) | `0.62.2` | `MIT OR Apache-2.0` |
| [windows-future](https://docs.rs/crate/windows-future/0.3.2/source/Cargo.toml) | `0.3.2` | `MIT OR Apache-2.0` |
| [windows-implement](https://docs.rs/crate/windows-implement/0.60.2/source/Cargo.toml) | `0.60.2` | `MIT OR Apache-2.0` |
| [windows-interface](https://docs.rs/crate/windows-interface/0.59.3/source/Cargo.toml) | `0.59.3` | `MIT OR Apache-2.0` |
| [windows-link](https://docs.rs/crate/windows-link/0.1.3/source/Cargo.toml) | `0.1.3` | `MIT OR Apache-2.0` |
| [windows-link](https://docs.rs/crate/windows-link/0.2.1/source/Cargo.toml) | `0.2.1` | `MIT OR Apache-2.0` |
| [windows-numerics](https://docs.rs/crate/windows-numerics/0.3.1/source/Cargo.toml) | `0.3.1` | `MIT OR Apache-2.0` |
| [windows-result](https://docs.rs/crate/windows-result/0.4.1/source/Cargo.toml) | `0.4.1` | `MIT OR Apache-2.0` |
| [windows-strings](https://docs.rs/crate/windows-strings/0.5.1/source/Cargo.toml) | `0.5.1` | `MIT OR Apache-2.0` |
| [windows-sys](https://docs.rs/crate/windows-sys/0.45.0/source/Cargo.toml) | `0.45.0` | `MIT OR Apache-2.0` |
| [windows-sys](https://docs.rs/crate/windows-sys/0.52.0/source/Cargo.toml) | `0.52.0` | `MIT OR Apache-2.0` |
| [windows-sys](https://docs.rs/crate/windows-sys/0.59.0/source/Cargo.toml) | `0.59.0` | `MIT OR Apache-2.0` |
| [windows-sys](https://docs.rs/crate/windows-sys/0.61.2/source/Cargo.toml) | `0.61.2` | `MIT OR Apache-2.0` |
| [windows-targets](https://docs.rs/crate/windows-targets/0.42.2/source/Cargo.toml) | `0.42.2` | `MIT OR Apache-2.0` |
| [windows-targets](https://docs.rs/crate/windows-targets/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [windows-threading](https://docs.rs/crate/windows-threading/0.2.1/source/Cargo.toml) | `0.2.1` | `MIT OR Apache-2.0` |
| [windows_aarch64_gnullvm](https://docs.rs/crate/windows_aarch64_gnullvm/0.42.2/source/Cargo.toml) | `0.42.2` | `MIT OR Apache-2.0` |
| [windows_aarch64_gnullvm](https://docs.rs/crate/windows_aarch64_gnullvm/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [windows_aarch64_msvc](https://docs.rs/crate/windows_aarch64_msvc/0.42.2/source/Cargo.toml) | `0.42.2` | `MIT OR Apache-2.0` |
| [windows_aarch64_msvc](https://docs.rs/crate/windows_aarch64_msvc/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [windows_i686_gnu](https://docs.rs/crate/windows_i686_gnu/0.42.2/source/Cargo.toml) | `0.42.2` | `MIT OR Apache-2.0` |
| [windows_i686_gnu](https://docs.rs/crate/windows_i686_gnu/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [windows_i686_gnullvm](https://docs.rs/crate/windows_i686_gnullvm/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [windows_i686_msvc](https://docs.rs/crate/windows_i686_msvc/0.42.2/source/Cargo.toml) | `0.42.2` | `MIT OR Apache-2.0` |
| [windows_i686_msvc](https://docs.rs/crate/windows_i686_msvc/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [windows_x86_64_gnu](https://docs.rs/crate/windows_x86_64_gnu/0.42.2/source/Cargo.toml) | `0.42.2` | `MIT OR Apache-2.0` |
| [windows_x86_64_gnu](https://docs.rs/crate/windows_x86_64_gnu/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [windows_x86_64_gnullvm](https://docs.rs/crate/windows_x86_64_gnullvm/0.42.2/source/Cargo.toml) | `0.42.2` | `MIT OR Apache-2.0` |
| [windows_x86_64_gnullvm](https://docs.rs/crate/windows_x86_64_gnullvm/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [windows_x86_64_msvc](https://docs.rs/crate/windows_x86_64_msvc/0.42.2/source/Cargo.toml) | `0.42.2` | `MIT OR Apache-2.0` |
| [windows_x86_64_msvc](https://docs.rs/crate/windows_x86_64_msvc/0.52.6/source/Cargo.toml) | `0.52.6` | `MIT OR Apache-2.0` |
| [winit](https://docs.rs/crate/winit/0.30.13/source/Cargo.toml) | `0.30.13` | `Apache-2.0` |
| [winnow](https://docs.rs/crate/winnow/1.0.4/source/Cargo.toml) | `1.0.4` | `MIT` |
| [winreg](https://docs.rs/crate/winreg/0.55.0/source/Cargo.toml) | `0.55.0` | `MIT` |
| [writeable](https://docs.rs/crate/writeable/0.6.4/source/Cargo.toml) | `0.6.4` | `Unicode-3.0` |
| [x11-dl](https://docs.rs/crate/x11-dl/2.21.0/source/Cargo.toml) | `2.21.0` | `MIT` |
| [x11rb](https://docs.rs/crate/x11rb/0.13.2/source/Cargo.toml) | `0.13.2` | `MIT OR Apache-2.0` |
| [x11rb-protocol](https://docs.rs/crate/x11rb-protocol/0.13.2/source/Cargo.toml) | `0.13.2` | `MIT OR Apache-2.0` |
| [xkbcommon-dl](https://docs.rs/crate/xkbcommon-dl/0.4.2/source/Cargo.toml) | `0.4.2` | `MIT` |
| [xkeysym](https://docs.rs/crate/xkeysym/0.2.1/source/Cargo.toml) | `0.2.1` | `MIT OR Apache-2.0 OR Zlib` |
| [yazi](https://docs.rs/crate/yazi/0.2.1/source/Cargo.toml) | `0.2.1` | `Apache-2.0 OR MIT` |
| [yoke](https://docs.rs/crate/yoke/0.8.3/source/Cargo.toml) | `0.8.3` | `Unicode-3.0` |
| [yoke-derive](https://docs.rs/crate/yoke-derive/0.8.4/source/Cargo.toml) | `0.8.4` | `Unicode-3.0` |
| [zeno](https://docs.rs/crate/zeno/0.3.3/source/Cargo.toml) | `0.3.3` | `Apache-2.0 OR MIT` |
| [zerocopy](https://docs.rs/crate/zerocopy/0.8.60/source/Cargo.toml) | `0.8.60` | `BSD-2-Clause OR Apache-2.0 OR MIT` |
| [zerocopy-derive](https://docs.rs/crate/zerocopy-derive/0.8.60/source/Cargo.toml) | `0.8.60` | `BSD-2-Clause OR Apache-2.0 OR MIT` |
| [zerofrom](https://docs.rs/crate/zerofrom/0.1.8/source/Cargo.toml) | `0.1.8` | `Unicode-3.0` |
| [zerofrom-derive](https://docs.rs/crate/zerofrom-derive/0.1.8/source/Cargo.toml) | `0.1.8` | `Unicode-3.0` |
| [zerotrie](https://docs.rs/crate/zerotrie/0.2.5/source/Cargo.toml) | `0.2.5` | `Unicode-3.0` |
| [zerovec](https://docs.rs/crate/zerovec/0.11.8/source/Cargo.toml) | `0.11.8` | `Unicode-3.0` |
| [zerovec-derive](https://docs.rs/crate/zerovec-derive/0.11.6/source/Cargo.toml) | `0.11.6` | `Unicode-3.0` |
| [zlib-rs](https://docs.rs/crate/zlib-rs/0.6.8/source/Cargo.toml) | `0.6.8` | `Zlib` |

## Bevy's bundled default font

The client enables Bevy's `default_font` feature. The exact `bevy_text` 0.19.1
package embeds `src/FiraMono-subset.ttf`. Its font-name table states:

> Digitized data copyright © 2012-2015, The Mozilla Foundation and Telefonica S.A.

> Licensed under the Open Font License, version 1.1 or later

The font's embedded license URL is [the SIL Open Font License](http://scripts.sil.org/OFL).
This is the default fallback font supplied by Bevy, separate from any original
game fonts loaded from the user's installation.
