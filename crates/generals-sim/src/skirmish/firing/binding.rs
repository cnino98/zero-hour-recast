//! Native Weapon retains its template pointer independently of the firing Object.
#[derive(Clone, Copy)]
pub struct WeaponBinding {
    pub type_index: usize,
    pub weapon_index: usize,
    pub weapon_slot: usize,
}
