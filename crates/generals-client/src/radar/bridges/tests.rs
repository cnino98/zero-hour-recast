use super::*;

#[test]
fn bridge_center_color_fills_pixel_without_neighbor_palette_blending_and_restores_base_mask() {
    let color = [192.0 / 255.0, 100.0 / 255.0, 37.0 / 255.0];
    let pixel = pixel_color(color, [64, 64]);
    let mut paint = Paint::new();
    let index = (63 * 128 + 64) * 4;
    paint.bridges.push(Bridge {
        layer: 2,
        color,
        pixels: vec![(index, pixel)],
        working: true,
    });
    paint.paint();
    let water = [12, 34, 56, 255].repeat(128 * 128);
    assert_eq!(paint.terrain_color(&water, index), pixel);
    assert_eq!(paint.terrain_color(&water, index + 4), [12, 34, 56]);
    assert_eq!(
        &paint.pixels[index..index + 4],
        &[pixel[0], pixel[1], pixel[2], 255]
    );
    // A neighboring center outside the bridge keeps its underlying water/terrain;
    // the chosen bridge is not blurred into that center's samples.
    assert_eq!(&paint.pixels[index + 4..index + 8], &[0; 4]);
    paint.bridges[0].working = false;
    paint.paint();
    assert_eq!(&paint.pixels[index..index + 4], &[0; 4]);
    assert_eq!(paint.terrain_color(&water, index), [12, 34, 56]);
    paint.bridges[0].working = true;
    paint.paint();
    assert_eq!(&paint.pixels[index..index + 3], &pixel);
}

#[ignore = "requires original game installation"]
#[test]
fn installed_original_bridge_palette_is_read_from_roads_including_landmark() {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let colors = crate::world_data::bridge_radar_colors(&assets).unwrap();
    for name in ["Concrete", "WoodenSectional", "MonumentTrainBridge"] {
        assert_eq!(
            colors[name].map(f32::to_bits),
            [192.0_f32 / 255.0; 3].map(f32::to_bits)
        );
    }
}

#[test]
fn bridge_color_uses_native_repeated_addition_for_each_clipped_neighborhood() {
    // An encoded channel chosen to expose the difference from returning the
    // original channel or replacing repeated addition with multiplication.
    let color = [85.0 / 255.0; 3];
    assert_eq!(pixel_color(color, [0, 0]), [85; 3]);
    assert_eq!(pixel_color(color, [0, 64]), [85; 3]);
    assert_eq!(pixel_color(color, [64, 64]), [84; 3]);
}
