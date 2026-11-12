#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var<uniform> uv_rect: vec4<f32>;
@group(1) @binding(1) var<uniform> state: vec4<f32>;
@group(1) @binding(2) var<uniform> clock_color: vec4<f32>;
@group(1) @binding(3) var icon_texture: texture_2d<f32>;
@group(1) @binding(4) var icon_sampler: sampler;

fn encode(rgb: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(max(rgb, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055,
        rgb * 12.92, rgb <= vec3(0.0031308));
}

fn decode(rgb: vec3<f32>) -> vec3<f32> {
    return select(pow((rgb + 0.055) / 1.055, vec3(2.4)),
        rgb / 12.92, rgb <= vec3(0.04045));
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // W3DDisplay::drawImage: clockwise-packed art maps screen TL/BL/TR to TR/TL/BR.
    let local_uv = select(in.uv, vec2(1.0 - in.uv.y, in.uv.x), state.w > 0.5);
    let uv = mix(uv_rect.xy, uv_rect.zw, local_uv);
    let sampled = textureSample(icon_texture, icon_sampler, uv);
    let rgb = encode(sampled.rgb);
    var color = rgb * state.y;
    var alpha = sampled.a;
    if state.x > 0.0 {
        // Render2DClass::Render, DOT3-capable path: TFACTOR 0x80A5CA8E,
        // stage 0 MULTIPLYADD followed by stage 1 DOTPRODUCT3, opaque output.
        let first = clamp(rgb * (128.0 / 255.0) + vec3(128.0 / 255.0), vec3(0.0), vec3(1.0));
        let gray = clamp(4.0 * dot(first - vec3(0.5), vec3(165.0, 202.0, 142.0) / 255.0 - vec3(0.5)), 0.0, 1.0);
        color = vec3(gray);
        alpha = 1.0;
    }
    // W3DPushButton draws the clock after the image, including disabled images.
    if state.z >= 0.0 && state.z < 100.0 && remaining_clock(in.uv * in.size, in.size, state.z) {
        color = mix(color, clock_color.rgb, clock_color.a);
    }
    return vec4(decode(color), alpha);
}

fn side(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let edge = b - a;
    let point = p - a;
    return edge.x * point.y - edge.y * point.x;
}
fn triangle(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> bool {
    let signs = vec3(side(p, a, b), side(p, b, c), side(p, c, a));
    return all(signs >= vec3(0.0)) || all(signs <= vec3(0.0));
}
// W3DDisplay::drawRemainingRectClock: preserve its integer half sizes and
// original 13/12 interval arithmetic, including the 62/88 offsets.
fn remaining_clock(p: vec2<f32>, size: vec2<f32>, percent: f32) -> bool {
    let mid = floor(size * 0.5);
    if percent == 0.0 { return true; }
    if percent < 25.0 {
        if p.x <= mid.x || p.y >= mid.y { return true; }
        if percent < 13.0 {
            return triangle(p, mid, vec2(size.x, mid.y), vec2(size.x, 0.0))
                || triangle(p, mid, vec2(size.x, 0.0), vec2(size.x - mid.x * (13.0 - percent) / 13.0, 0.0));
        }
        return triangle(p, mid, vec2(size.x, mid.y), vec2(size.x, mid.y * (percent - 13.0) / 12.0));
    }
    if percent < 50.0 {
        if p.x <= mid.x { return true; }
        if percent < 38.0 {
            return triangle(p, mid, vec2(mid.x, size.y), size)
                || triangle(p, mid, size, vec2(size.x, mid.y + mid.y * (percent - 25.0) / 13.0));
        }
        return triangle(p, mid, vec2(mid.x, size.y), vec2(size.x - mid.x * (percent - 38.0) / 12.0, size.y));
    }
    if percent < 75.0 {
        if p.x <= mid.x && p.y <= mid.y { return true; }
        if percent < 63.0 {
            return triangle(p, mid, vec2(0.0, mid.y), vec2(0.0, size.y))
                || triangle(p, mid, vec2(0.0, size.y), vec2(mid.x - mid.x * (percent - 50.0) / 13.0, size.y));
        }
        return triangle(p, mid, vec2(0.0, mid.y), vec2(0.0, size.y - mid.y * (percent - 62.0) / 12.0));
    }
    if percent < 87.0 {
        return triangle(p, mid, vec2(mid.x, 0.0), vec2(0.0))
            || triangle(p, mid, vec2(0.0), vec2(0.0, mid.y - mid.y * (percent - 75.0) / 13.0));
    }
    return triangle(p, mid, vec2(mid.x, 0.0), vec2(mid.x * (percent - 88.0) / 12.0, 0.0));
}
