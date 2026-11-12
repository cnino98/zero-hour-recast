//! Original volume-layer placement before submission to the shared triangle pool.

pub(super) fn position(
    point: [f32; 3],
    camera: [f32; 3],
    first_size: f32,
    depth: u8,
    layer: u8,
) -> [f32; 3] {
    // PointGroupClass::RenderVolumeParticle deliberately uses the first size
    // for every particle and resets each layer to the original point array.
    let reciprocal_depth = 0.1 / f32::from(depth);
    let layer_size = f32::from(layer) * first_size;
    let shift = layer_size * reciprocal_depth;
    let mut delta = std::array::from_fn::<_, 3, _>(|axis| camera[axis] - point[axis]);
    let xx = delta[0] * delta[0];
    let yy = delta[1] * delta[1];
    let zz = delta[2] * delta[2];
    let length_squared = xx + yy + zz;
    if length_squared != 0.0 {
        let reciprocal_length = 1.0 / length_squared.sqrt();
        delta = delta.map(|component| component * reciprocal_length);
    }
    std::array::from_fn(|axis| {
        let offset = delta[axis] * shift;
        point[axis] + offset
    })
}

#[cfg(test)]
mod tests;
