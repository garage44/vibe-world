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

// Resolution of tiles at zoom level 0 (meters per pixel at equator)
// According to OpenStreetMap wiki: 156543.03 meters/pixel at zoom 0
pub const TILE_RESOLUTION_ZOOM_0: f32 = 156543.03;

// Standard OSM tile size in pixels
pub const TILE_SIZE_PIXELS: u32 = 256;

// Exact resolutions in meters per pixel at the equator for each zoom level
// From https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames#Resolution_and_Scale
pub const RESOLUTIONS_METERS_PER_PIXEL: [f32; 20] = [
    156543.03,  // Zoom 0: Whole world
    78271.52,   // Zoom 1
    39135.76,   // Zoom 2: Subcontinental area
    19567.88,   // Zoom 3: Largest country
    9783.94,    // Zoom 4
    4891.97,    // Zoom 5: Large African country
    2445.98,    // Zoom 6: Large European country
    1222.99,    // Zoom 7: Small country, US state
    611.50,     // Zoom 8
    305.75,     // Zoom 9: Wide area, large metropolitan area
    152.87,     // Zoom 10: Metropolitan area
    76.437,     // Zoom 11: City
    38.219,     // Zoom 12: Town or city district
    19.109,     // Zoom 13: Village or suburb
    9.5546,     // Zoom 14
    4.7773,     // Zoom 15: Small road
    2.3887,     // Zoom 16: Street
    1.1943,     // Zoom 17: Block, park, addresses
    0.5972,     // Zoom 18: Buildings, trees
    0.2986,     // Zoom 19: Local highway and crossing details
];

// Calculate resolution (meters per pixel) at a specific zoom level and latitude
pub fn resolution_at_zoom_and_latitude(zoom: u32, latitude_degrees: f32) -> f32 {
    if zoom as usize >= RESOLUTIONS_METERS_PER_PIXEL.len() {
        // Fallback calculation for zoom levels beyond our table
        let latitude_radians = latitude_degrees.to_radians();
        let latitude_factor = latitude_radians.cos();
        TILE_RESOLUTION_ZOOM_0 * latitude_factor / (1 << zoom) as f32
    } else {
        // Use the exact values from our table, adjusted for latitude
        let latitude_radians = latitude_degrees.to_radians();
        let latitude_factor = latitude_radians.cos();
        RESOLUTIONS_METERS_PER_PIXEL[zoom as usize] * latitude_factor
    }
}

// Calculate map scale at a specific zoom level, latitude, and screen DPI
pub fn map_scale_at_zoom(zoom: u32, latitude_degrees: f32, screen_dpi: f32) -> f32 {
    let resolution = resolution_at_zoom_and_latitude(zoom, latitude_degrees);
    let meters_per_inch = 0.0254; // 1 inch = 0.0254 meters
    resolution / (screen_dpi * meters_per_inch)
}

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

// Calculate approximate real-world scale for a given zoom level at a specific latitude
// Returns a string like "1:10000" representing the map scale
pub fn get_scale_for_zoom(zoom: u32, latitude_degrees: f32, screen_dpi: f32) -> String {
    let scale = map_scale_at_zoom(zoom, latitude_degrees, screen_dpi);
    
    // Round to a more readable number
    let rounded_scale = if scale > 1_000_000.0 {
        (scale / 1_000_000.0).round() * 1_000_000.0
    } else if scale > 100_000.0 {
        (scale / 100_000.0).round() * 100_000.0
    } else if scale > 10_000.0 {
        (scale / 10_000.0).round() * 10_000.0
    } else if scale > 1_000.0 {
        (scale / 1_000.0).round() * 1_000.0
    } else {
        scale.round()
    };
    
    format!("1:{}", rounded_scale as u32)
}

// Color for highlighting persistent islands - might be used in future
#[allow(dead_code)]
pub const ISLAND_HIGHLIGHT_COLOR: Color = Color::srgba(0.0, 1.0, 0.5, 0.5);
// Border color for islands in regular mode - might be used in future
#[allow(dead_code)]
pub const ISLAND_BORDER_COLOR: Color = Color::srgba(0.2, 0.8, 0.3, 0.3); 