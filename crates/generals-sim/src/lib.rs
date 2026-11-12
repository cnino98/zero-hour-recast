//! Game simulation translated from the pinned Generals source.

pub mod collision_geometry;
mod grid;
pub mod native_real;
pub mod skirmish;
pub mod terrain;

pub use grid::{Cell, Grid, GridError};
