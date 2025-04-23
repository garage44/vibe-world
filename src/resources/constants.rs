use bevy::prelude::*;

/// Constants for OSM tile system
pub const DEFAULT_ZOOM_LEVEL: u32 = 13;
pub const MIN_ZOOM_LEVEL: u32 = 1;  // Furthest zoom out (least detail)
pub const MAX_ZOOM_LEVEL: u32 = 19;  // Closest zoom in (most detail)
pub const BACKGROUND_ZOOM_LEVEL: u32 = 2; // Low-resolution background tiles

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

// Determines the appropriate zoom level based on camera height
// Uses OSM zoom level standards from https://wiki.openstreetmap.org/wiki/Zoom_levels
pub fn zoom_level_from_camera_height(height: f32) -> u32 {
    // Height thresholds for zoom levels (in world units)
    // Adjusted to use lower zoom levels at the same heights to reduce tile loading
    match height {
        h if h <= 1.0 => 19,   // Level 19: Local highways, crossings (1:1000 scale)
        h if h <= 3.0 => 18,   // Level 18: Buildings, trees (1:2000 scale)
        h if h <= 6.0 => 17,   // Level 17: Building blocks, parks, addresses (1:4000 scale)
        h if h <= 12.0 => 16,  // Level 16: Streets (1:8000 scale)
        h if h <= 25.0 => 15,  // Level 15: Small roads (1:15000 scale)
        h if h <= 50.0 => 14,  // Level 14: Detailed roads (1:35000 scale)
        h if h <= 100.0 => 13, // Level 13: Villages, suburbs (1:70000 scale)
        h if h <= 200.0 => 12, // Level 12: Towns, city districts (1:150000 scale)
        h if h <= 400.0 => 11, // Level 11: Cities (1:250000 scale)
        h if h <= 800.0 => 10, // Level 10: Metropolitan areas (1:500000 scale)
        h if h <= 1600.0 => 9, // Level 9: Large metro areas (1:1 million scale)
        h if h <= 3200.0 => 8, // Level 8: (1:2 million scale)
        h if h <= 6400.0 => 7, // Level 7: Small countries, US states (1:4 million scale)
        h if h <= 12800.0 => 6, // Level 6: Large European countries (1:10 million scale)
        h if h <= 25000.0 => 5, // Level 5: Large African countries (1:15 million scale)
        h if h <= 50000.0 => 4, // Level 4: (1:35 million scale)
        h if h <= 100000.0 => 3, // Level 3: Largest countries (1:70 million scale)
        h if h <= 200000.0 => 2, // Level 2: Subcontinental areas (1:150 million scale)
        _ => 1,                  // Level 1: Whole world (1:250 million scale)
    }
}

// Color for highlighting persistent islands - might be used in future
#[allow(dead_code)]
pub const ISLAND_HIGHLIGHT_COLOR: Color = Color::srgba(0.0, 1.0, 0.5, 0.5);
// Border color for islands in regular mode - might be used in future
#[allow(dead_code)]
pub const ISLAND_BORDER_COLOR: Color = Color::srgba(0.2, 0.8, 0.3, 0.3); 