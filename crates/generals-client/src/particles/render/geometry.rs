use bevy::prelude::{Assets, Handle, Mesh};

pub(super) fn update(meshes: &mut Assets<Mesh>, handle: &Handle<Mesh>, next: Mesh) {
    if meshes
        .get(handle)
        .is_some_and(|current| matches(current, &next))
    {
        return;
    }
    if let Some(mut current) = meshes.get_mut(handle) {
        *current = next;
    }
}

// Compare the generated particle mesh directly, without packing or cloning its
// vertex buffers. Byte comparison retains signed zero and every source float bit.
pub(super) fn matches(current: &Mesh, next: &Mesh) -> bool {
    current.primitive_topology() == next.primitive_topology()
        && current.asset_usage == next.asset_usage
        && current.enable_raytracing == next.enable_raytracing
        && current.indices() == next.indices()
        && current.attributes().count() == next.attributes().count()
        && current.attributes().zip(next.attributes()).all(
            |((left, values), (right, following))| {
                left == right
                    && std::mem::discriminant(values) == std::mem::discriminant(following)
                    && values.get_bytes() == following.get_bytes()
            },
        )
}
