use bevy::prelude::*;
use crate::resources::{OSMData, PersistentIslandSettings, DebugSettings};
use crate::components::{TileCoords, PersistentIsland};
use crate::utils::coordinate_conversion::world_to_tile_coords;
use crate::resources::constants::PERSISTENT_ISLAND_ZOOM_LEVEL;

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
    island_settings: Res<PersistentIslandSettings>,
    debug_settings: Res<DebugSettings>,
    time: Res<Time>,
    camera_query: Query<&Transform, With<Camera3d>>,
    _tile_query: Query<&TileCoords>,
    island_query: Query<&PersistentIsland>,
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
        
        // Current tile at persistent island zoom level
        let (island_tile_x, island_tile_y) = world_to_tile_coords(x, z, PERSISTENT_ISLAND_ZOOM_LEVEL);
        
        // Count persistent islands
        let total_islands = osm_data.persistent_islands.len();
        let active_islands = island_query.iter().count();
        
        // Check if we're currently over a persistent island
        let on_persistent_island = osm_data.persistent_islands.contains_key(&(island_tile_x, island_tile_y));
        
        // Debug info
        info!(
            "Pos: ({:.1}, {:.1}, {:.1}) | Zoom: {} | Tile: {},{} | Islands: {}/{} active | On Island: {} | Island Mode: {}",
            x, y, z,
            osm_data.current_zoom,
            tile_x, tile_y,
            active_islands, total_islands,
            if on_persistent_island { "YES" } else { "no" },
            if island_settings.editing_mode { "ON" } else { "off" }
        );
    }
} 