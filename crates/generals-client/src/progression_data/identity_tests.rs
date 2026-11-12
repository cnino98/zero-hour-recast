use super::{multiplayer_colors, player_template};
use generals_assets::{
    content,
    virtual_assets::{Asset, VirtualAssets},
};
use generals_sim::skirmish::Faction;

fn assets() -> VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

#[ignore = "requires original game installation"]
#[test]
fn player_identity_original_templates_keep_exact_side_playability_and_preferred_color() {
    let file = content::scan(assets().read("data/ini/playertemplate.ini").unwrap()).unwrap();
    for (name, faction, side, color, playable) in [
        ("FactionAmerica", Faction::Usa, "America", 0xff00_00ff, true),
        ("FactionChina", Faction::China, "China", 0xffff_0000, true),
        ("FactionGLA", Faction::Gla, "GLA", 0xff00_ff00, true),
        (
            "FactionCivilian",
            Faction::Civilian,
            "Civilian",
            0xffff_ffff,
            false,
        ),
    ] {
        let declaration = file
            .declarations
            .iter()
            .find(|item| item.kind == "PlayerTemplate" && item.name == name)
            .unwrap();
        let parsed = player_template(&declaration.lines, faction).unwrap();
        assert_eq!(parsed.side, side);
        assert_eq!(parsed.preferred_color, color);
        assert_eq!(parsed.playable_side, playable);
    }
}

#[ignore = "requires original game installation"]
#[test]
fn player_identity_original_multiplayer_palette_keeps_authored_order_and_packed_rgb() {
    let file = content::scan(assets().read("data/ini/multiplayer.ini").unwrap()).unwrap();
    assert_eq!(
        multiplayer_colors(&file).unwrap(),
        [
            0xffdd_e20d,
            0xffff_0000,
            0xff43_68fe,
            0xff3e_d12e,
            0xffff_a019,
            0xff32_d7e6,
            0xff96_00c8,
            0xffff_96ff,
        ]
    );
}

#[test]
fn player_identity_side_is_not_base_side_and_native_template_defaults_are_explicit() {
    let file = content::scan(Asset {
        path: "playertemplate.ini".into(), origin: "source identity fields".into(),
        bytes: b"PlayerTemplate FactionAmerica\nSide=AmericaLaserGeneral\nBaseSide=USA\nPlayableSide=Yes\nPreferredColor=R:67 G:104 B:254\nEnd\nPlayerTemplate Empty\nEnd\n".to_vec(),
    }).unwrap();
    let parsed = player_template(&file.declarations[0].lines, Faction::Usa).unwrap();
    assert_eq!(parsed.side, "AmericaLaserGeneral");
    assert_eq!(parsed.preferred_color, 0xff43_68fe);
    assert!(parsed.playable_side);
    let parsed = player_template(&file.declarations[1].lines, Faction::Civilian).unwrap();
    assert!(parsed.side.is_empty());
    assert!(!parsed.playable_side);
    assert_eq!(parsed.preferred_color, 0xff00_0000);
}
