use crate::resources::constants::{DEFAULT_ZOOM_LEVEL, max_tile_index};

/// Convert camera world coordinates to OSM tile coordinates
pub fn world_to_tile_coords(x: f32, z: f32, zoom: u32) -> (u32, u32) {
    // OSM tile coordinate system has (0,0) at northwest corner
    // X increases eastward, Y increases southward
    // Our world coordinate system has:
    // - X increases eastward (same as OSM X)
    // - Z increases southward (maps directly to OSM Y)

    // OSM zoom level scaling - at each level, number of tiles doubles in each dimension
    // At zoom level 0, the world is 1 tile
    // At zoom level 1, the world is 2x2 tiles
    // At zoom level 2, the world is 4x4 tiles
    // And so on - each zoom level multiplies tile count by 2^(zoom difference)

    // Our world coordinates are based on tile indexes at DEFAULT_ZOOM_LEVEL
    // Scale them to match the requested zoom level

    // For example, if DEFAULT_ZOOM_LEVEL is 13 and zoom is 14:
    // - Each DEFAULT_ZOOM_LEVEL tile becomes 2x2 tiles at zoom 14
    // - So we multiply coordinates by 2

    // If DEFAULT_ZOOM_LEVEL is 13 and zoom is 12:
    // - Each 2x2 tile block at DEFAULT_ZOOM_LEVEL becomes 1 tile at zoom 12
    // - So we divide coordinates by 2

    let zoom_difference = zoom as i32 - DEFAULT_ZOOM_LEVEL as i32;
    let scale_factor = 2_f32.powi(zoom_difference);

    // Scale world coordinates to the target zoom level
    let scaled_x = x * scale_factor;
    let scaled_z = z * scale_factor;

    // Get the tile X,Y coordinates at this zoom level
    let tile_x = scaled_x.floor() as u32;
    let tile_y = scaled_z.floor() as u32;

    // Clamp to valid tile range for this zoom level
    let max_index = max_tile_index(zoom);
    let tile_x = tile_x.clamp(0, max_index);
    let tile_y = tile_y.clamp(0, max_index);

    (tile_x, tile_y)
} 