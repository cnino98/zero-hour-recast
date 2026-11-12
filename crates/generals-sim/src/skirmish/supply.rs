//! Authored collector action timings and the supply docking geometry.
use super::SupplyDock;

pub struct CollectorRules {
    pub capacity: u32,
    pub depleted_voice: String,
    pub scan_distance: f32,
    pub dock: Option<SupplyDock>,
    pub warehouse: Option<WarehouseRules>,
    pub gather_seconds: f32,
    pub dropoff_seconds: f32,
}

impl CollectorRules {
    pub const NONE: Self = Self {
        capacity: 0,
        depleted_voice: String::new(),
        scan_distance: 0.0,
        dock: None,
        warehouse: None,
        gather_seconds: 0.0,
        dropoff_seconds: 0.0,
    };
}

/// Native `SupplyWarehouseDockUpdate` module data.
pub struct WarehouseRules {
    pub starting_boxes: u32,
    pub delete_when_empty: bool,
}
