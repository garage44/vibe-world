use bevy::prelude::*;

/// Component to mark tiles with their coordinates and zoom level for quick lookups
#[derive(Component)]
pub struct TileCoords {
    pub x: u32,
    pub y: u32,
    pub zoom: u32,
    pub last_used: f32, // Time when this tile was last in view
} 