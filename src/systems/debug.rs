use bevy::prelude::*;
use crate::resources::{OSMData, PersistentIslandSettings};
use crate::components::{TileCoords, PersistentIsland};
use crate::utils::coordinate_conversion::world_to_tile_coords;
use crate::resources::constants::PERSISTENT_ISLAND_ZOOM_LEVEL;

/// Debug system to print information about loaded tiles
pub fn debug_info(
    osm_data: Res<OSMData>,
    island_settings: Res<PersistentIslandSettings>,
    time: Res<Time>,
    camera_query: Query<&Transform, With<Camera3d>>,
    _tile_query: Query<&TileCoords>,
    island_query: Query<&PersistentIsland>,
) {
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