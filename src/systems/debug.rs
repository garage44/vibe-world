use bevy::prelude::*;
use crate::resources::{OSMData, DebugSettings};
use crate::components::{TileCoords};
use crate::utils::coordinate_conversion::world_to_tile_coords;

/// System to toggle debug mode with the 1 key
pub fn toggle_debug_mode(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut debug_settings: ResMut<DebugSettings>,
) {
    if keyboard_input.just_pressed(KeyCode::Digit1) {
        debug_settings.debug_mode = !debug_settings.debug_mode;
        info!("Debug mode: {}", if debug_settings.debug_mode { "ON" } else { "OFF" });
    }
}

/// Debug system to print information about loaded tiles
pub fn debug_info(
    osm_data: Res<OSMData>,
    debug_settings: Res<DebugSettings>,
    time: Res<Time>,
    camera_query: Query<&Transform, With<Camera3d>>,
    tile_query: Query<&TileCoords>,
) {
    // Skip if debug mode is disabled
    if !debug_settings.debug_mode {
        return;
    }

    // Only run every few seconds
    if time.elapsed_secs() as usize % 5 != 0 {
        return;
    }

    if let Ok(camera_transform) = camera_query.get_single() {
        let x = camera_transform.translation.x;
        let y = camera_transform.translation.y;
        let z = camera_transform.translation.z;
        
        // Current tile at current zoom level
        let (tile_x, tile_y) = world_to_tile_coords(x, z, osm_data.current_zoom);
        
        // Count active tiles
        let active_tiles = tile_query.iter().count();
        
        // Debug info
        info!(
            "Pos: ({:.1}, {:.1}, {:.1}) | Zoom: {} | Tile: {},{} | Active tiles: {}",
            x, y, z,
            osm_data.current_zoom,
            tile_x, tile_y,
            active_tiles
        );
    }
} 