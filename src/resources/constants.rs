use bevy::prelude::*;

/// Constants for OSM tile system
pub const DEFAULT_ZOOM_LEVEL: u32 = 13;
pub const MIN_ZOOM_LEVEL: u32 = 10;  // Furthest zoom out (least detail)
pub const MAX_ZOOM_LEVEL: u32 = 19;  // Closest zoom in (most detail)
pub const PERSISTENT_ISLAND_ZOOM_LEVEL: u32 = 17;

// Calculate MAX_TILE_INDEX dynamically based on zoom level
pub fn max_tile_index(zoom: u32) -> u32 {
    (1 << zoom) - 1 // 2^zoom - 1
}

// Export the constant for osm.rs to use
pub const MAX_TILE_INDEX: u32 = (1 << MAX_ZOOM_LEVEL) - 1;

// Groningen, Netherlands approximate coordinates at zoom level 13
// OSM tile coordinates at zoom level 13: x=4216, y=2668
pub const GRONINGEN_X: u32 = 4216;
pub const GRONINGEN_Y: u32 = 2668;

// Color for highlighting persistent islands
pub const ISLAND_HIGHLIGHT_COLOR: Color = Color::srgba(0.0, 1.0, 0.5, 0.5);
// Border color for islands in regular mode - more subtle
pub const ISLAND_BORDER_COLOR: Color = Color::srgba(0.2, 0.8, 0.3, 0.3); 