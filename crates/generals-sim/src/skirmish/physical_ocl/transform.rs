//! Original `Matrix3D` post-rotation; rebuilding Euler angles loses the source pose.
#[must_use]
pub fn yaw(angle: f32) -> [[f32; 3]; 3] {
    let (sine, cosine) = angle.sin_cos();
    [[cosine, -sine, 0.0], [sine, cosine, 0.0], [0.0, 0.0, 1.0]]
}

#[must_use]
pub fn vector(basis: [[f32; 3]; 3], value: [f32; 3]) -> [f32; 3] {
    basis.map(|row| {
        let x = row[0] * value[0];
        let y = row[1] * value[1];
        let z = row[2] * value[2];
        (x + y) + z
    })
}

pub fn rotate(basis: &mut [[f32; 3]; 3], angles: [f32; 3]) {
    let (sine, cosine) = angles[0].sin_cos();
    for row in &mut *basis {
        let y = row[1] * cosine;
        let z = row[2] * sine;
        let yy = -sine * row[1];
        let zz = row[2] * cosine;
        row[1] = y + z;
        row[2] = yy + zz;
    }
    let (sine, cosine) = angles[1].sin_cos();
    for row in &mut *basis {
        let x = row[0] * cosine;
        let z = row[2] * sine;
        let xx = sine * row[0];
        let zz = row[2] * cosine;
        row[0] = x - z;
        row[2] = xx + zz;
    }
    let (sine, cosine) = angles[2].sin_cos();
    for row in basis {
        let x = row[0] * cosine;
        let y = row[1] * sine;
        let xx = -sine * row[0];
        let yy = row[1] * cosine;
        row[0] = x + y;
        row[1] = xx + yy;
    }
}

/// Retained native matrix components beyond the cached Object heading/position.
pub(in crate::skirmish) struct Matrix {
    pub basis: [[f32; 3]; 3],
    // Thing::setOrientation on terrain can leave matrix Z distinct from cached Z.
    pub translation: Option<[f32; 3]>,
}

impl Matrix {
    pub(in crate::skirmish) const fn from_basis(basis: [[f32; 3]; 3]) -> Self {
        Self {
            basis,
            translation: None,
        }
    }
}

/// `TerrainLogic::makeAlignToNormalMatrix`, using `Coord3D` component divisions.
pub(in crate::skirmish) fn aligned(angle: f32, normal: [f32; 3]) -> [[f32; 3]; 3] {
    let (sine, cosine) = angle.sin_cos();
    let mut x = [cosine, sine, 0.0];
    if normal[2] != 0.0 {
        let xx = x[0] * normal[0];
        let yy = x[1] * normal[1];
        x[2] = -(xx + yy) / normal[2];
        x = normalize(x);
    }
    let yz = normal[1] * x[2];
    let zy = normal[2] * x[1];
    let zx = normal[2] * x[0];
    let xz = normal[0] * x[2];
    let xy = normal[0] * x[1];
    let yx = normal[1] * x[0];
    let y = normalize([yz - zy, zx - xz, xy - yx]);
    std::array::from_fn(|row| [x[row], y[row], normal[row]])
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let squared = value.map(|component| component * component);
    let length = ((squared[0] + squared[1]) + squared[2]).sqrt();
    if length == 0.0 {
        value
    } else {
        value.map(|component| component / length)
    }
}

/// Terrain borrowed for the duration of a native transform or locomotor operation.
pub(in crate::skirmish) struct TerrainContext<'a> {
    pub terrain: Option<&'a crate::skirmish::projectiles::Terrain>,
    pub layers: &'a crate::terrain::TerrainMap,
    pub height: f32,
}

impl TerrainContext<'_> {
    pub(in crate::skirmish) fn ground_height(&self, point: [f32; 2]) -> f32 {
        self.terrain.map_or(0.0, |terrain| terrain.sample(point))
    }

    pub(in crate::skirmish) fn aligned(
        &self,
        position: [f32; 3],
        angle: f32,
    ) -> ([f32; 3], [[f32; 3]; 3]) {
        let point = [position[0], position[1]];
        let ground = self.ground_height(point);
        let normal = self
            .terrain
            .map_or([0.0, 0.0, 1.0], |terrain| terrain.normal(point));
        let layer = self.layers.get_layer_for_destination(position, ground);
        let (mut height, normal) = self
            .layers
            .layer_height_and_normal(layer, point, ground, normal);
        if layer != 1 {
            height += 2.5;
        }
        ([position[0], position[1], height], aligned(angle, normal))
    }

    pub(in crate::skirmish) fn rotated_basis(
        &self,
        actor: &mut crate::skirmish::Entity,
        basis: [[f32; 3]; 3],
    ) -> f32 {
        let height = actor
            .transform
            .as_ref()
            .and_then(|matrix| matrix.translation)
            .map_or(self.height, |position| position[2]);
        let position = [actor.position[0], actor.position[1], height];
        actor.set_object_basis(basis);
        let ground = self
            .layers
            .bridge_height(actor.layer, actor.position)
            .unwrap_or_else(|| self.ground_height(actor.position));
        actor.write_object_position(position, ground);
        height
    }

    pub(in crate::skirmish) fn orient(
        &self,
        actor: &mut crate::skirmish::Entity,
        unit: &crate::skirmish::UnitType,
        angle: f32,
    ) -> f32 {
        let position = actor
            .transform
            .as_ref()
            .and_then(|matrix| matrix.translation)
            .map_or(
                [actor.position[0], actor.position[1], self.height],
                |position| [actor.position[0], actor.position[1], position[2]],
            );
        let stick = unit
            .kind_of
            .iter()
            .any(|kind| kind == "STICK_TO_TERRAIN_SLOPE");
        let (translation, basis) = if stick {
            self.aligned(position, angle)
        } else {
            (position, yaw(angle))
        };
        actor.set_object_basis(basis);
        let point = [position[0], position[1]];
        let ground = self
            .layers
            .bridge_height(actor.layer, point)
            .unwrap_or_else(|| self.ground_height(point));
        actor.write_object_position(position, ground);
        actor
            .transform
            .as_mut()
            .expect("orientation matrix")
            .translation = stick.then_some(translation);
        actor.facing = crate::skirmish::locomotor::normalize_angle(angle);
        position[2]
    }
}

/// Native world-axis pre-rotation leaves matrix translation unchanged.
pub fn pre_rotate(basis: &mut [[f32; 3]; 3], axis: usize, angle: f32) {
    let (sine, cosine) = angle.sin_cos();
    let (first, second, sign) = match axis {
        0 => (1, 2, -1.0),
        1 => (0, 2, 1.0),
        2 => (0, 1, -1.0),
        _ => unreachable!("rotation axis"),
    };
    let (before, after) = basis.split_at_mut(second);
    for (first_value, second_value) in before[first].iter_mut().zip(&mut after[0]) {
        let first_cosine = cosine * *first_value;
        let second_sine = sine * *second_value;
        let first_sine = sine * *first_value;
        let second_cosine = cosine * *second_value;
        if sign < 0.0 {
            *first_value = first_cosine - second_sine;
            *second_value = first_sine + second_cosine;
        } else {
            *first_value = first_cosine + second_sine;
            *second_value = -first_sine + second_cosine;
        }
    }
}
