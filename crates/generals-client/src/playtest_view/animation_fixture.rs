//! Headless storage adapter for the actual prepared W3D parts and draw systems.
use super::*;

pub fn geometry(models: &[Model], meshes: &mut Assets<Mesh>) -> Geometry {
    // Materials do not participate in ordinary headless mesh assertions.
    geometry_materials(
        models,
        meshes,
        &std::array::from_fn::<_, 17, _>(|_| Handle::default()),
    )
}

pub fn geometry_materials(
    models: &[Model],
    meshes: &mut Assets<Mesh>,
    materials: &[Handle<SourceMaterial>],
) -> Geometry {
    Geometry {
        colors: vec![[0; 3]; 8],
        models: models
            .iter()
            .map(|model| {
                model_render::parts(model)
                    .unwrap()
                    .into_iter()
                    .map(|part| PreparedPart {
                        light_bounds: part.light_bounds,
                        submission: part.submission,
                        order: part.order,
                        binding: part.binding.map(Arc::new),
                        pickable: part.pickable,
                        mesh: meshes.add(part.mesh),
                        materials: materials.to_vec(),
                    })
                    .collect()
            })
            .collect(),
        generation: 0,
        spawned: 0,
    }
}
