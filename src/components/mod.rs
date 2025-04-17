mod tile;

pub use tile::*;

use bevy::prelude::*;

/// Marker component for the UI text that displays the current zoom level
#[derive(Component)]
pub struct ZoomLevelText; 