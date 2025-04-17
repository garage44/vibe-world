use bevy::prelude::*;
use crate::resources::{OSMData, TokioRuntime, DebugSettings};
use crate::components::{TileCoords};
use crate::osm::{OSMTile, load_tile_image, create_tile_mesh, create_fallback_tile_mesh};
use crate::utils::coordinate_conversion::world_to_tile_coords;
use crate::resources::constants::{max_tile_index, MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL, BACKGROUND_ZOOM_LEVEL, zoom_level_from_camera_height};
use crate::debug_log;

// Process tiles based on camera position and current zoom level
pub fn process_tiles(
    mut osm_data: ResMut<OSMData>,
    tokio_runtime: Res<TokioRuntime>,
    debug_settings: Res<DebugSettings>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
) {
    // Skip if we have no camera yet
    if let Ok((camera_transform, _camera)) = camera_query.get_single() {
        let camera_pos = camera_transform.translation;
        let current_zoom = osm_data.current_zoom;
        let background_zoom = osm_data.background_zoom;

        // Process focus area tiles (higher resolution)
        process_zoom_level_tiles(
            &mut osm_data,
            &tokio_runtime,
            &debug_settings,
            camera_pos,
            current_zoom,
            false, // Not background tiles
        );

        // Process background tiles (lower resolution)
        process_zoom_level_tiles(
            &mut osm_data,
            &tokio_runtime,
            &debug_settings,
            camera_pos,
            background_zoom,
            true, // Background tiles
        );
    }
}

// Helper function to process tiles for a specific zoom level
fn process_zoom_level_tiles(
    osm_data: &mut OSMData,
    tokio_runtime: &TokioRuntime,
    debug_settings: &DebugSettings,
    camera_pos: Vec3,
    zoom: u32,
    is_background: bool,
) {
    // Calculate the visible range for the current zoom level
    // For background tiles, we need a much larger range but fewer tiles due to lower zoom
    let visible_range = if is_background {
        // Background tiles cover a larger area with fewer tiles
        // At zoom level 2, each tile covers a very large area
        match zoom {
            0 => 1,  // At zoom 0, there's only one tile for the whole world
            1 => 2,  // At zoom 1, we need just a few tiles
            2 => 3,  // At zoom 2, slightly more
            3 => 3,  // At zoom 3
            _ => 2,  // Fallback for any other zoom level
        }
    } else {
        // Focus tiles (reuse the existing visible range logic)
        match zoom {
            z if z >= 18 => 6,   // Very close zoom 
            z if z >= 16 => 7,   // Close zoom
            z if z >= 14 => 8,   // Medium-close zoom
            z if z >= 12 => 7,   // Medium zoom
            z if z >= 10 => 6,   // Medium-far zoom
            z if z >= 8 => 5,    // Far zoom
            z if z >= 5 => 4,    // Very far zoom
            z if z >= 3 => 3,    // Extremely far zoom
            z if z >= 1 => 2,    // Global zoom
            _ => 1,              // Minimum zoom
        }
    };

    // Tile coordinates at current zoom level
    let (tile_center_x, tile_center_y) = world_to_tile_coords(camera_pos.x, camera_pos.z, zoom);

    // Generate a list of tile coordinates to load, sorted by distance from center
    let mut tiles_to_load: Vec<(u32, u32, u32, i32)> = Vec::new(); // (x, y, zoom, distance)

    // Calculate the max tile index for this zoom level
    let max_index = max_tile_index(zoom);

    // Create a square grid of tiles around the center for the current zoom level
    for x_offset in -visible_range as i32..=visible_range as i32 {
        for y_offset in -visible_range as i32..=visible_range as i32 {
            // Calculate the tile coordinates with bounds checking
            let tile_x = (tile_center_x as i32 + x_offset).clamp(0, max_index as i32) as u32;
            let tile_y = (tile_center_y as i32 + y_offset).clamp(0, max_index as i32) as u32;
            
            // Calculate manhattan distance for priority (closest first)
            let distance = x_offset.abs() + y_offset.abs();
            
            // Add to load queue with its priority
            tiles_to_load.push((tile_x, tile_y, zoom, distance));
        }
    }

    // Sort tiles by distance (closest first)
    tiles_to_load.sort_by_key(|&(_, _, _, distance)| distance);

    // Set concurrent load limits
    let max_concurrent_loads = if is_background { 8 } else { 16 };
    let mut concurrent_loads = 0;

    // Get appropriate tracking list based on tile type
    let loaded_tiles = if is_background {
        &mut osm_data.loaded_background_tiles
    } else {
        &mut osm_data.loaded_tiles
    };

    // Process tiles in order of priority (closest first)
    for (tile_x, tile_y, tile_zoom, _) in tiles_to_load {
        // Check if we've reached the maximum concurrent load limit
        if concurrent_loads >= max_concurrent_loads {
            break;
        }

        // Check if tile is already loaded or pending
        let already_pending = osm_data.pending_tiles.lock().iter().any(
            |(x, y, z, _, bg)| *x == tile_x && *y == tile_y && *z == tile_zoom && *bg == is_background
        );

        if !loaded_tiles.contains(&(tile_x, tile_y, tile_zoom)) && !already_pending {
            // Mark as loaded to prevent duplicate requests
            loaded_tiles.push((tile_x, tile_y, tile_zoom));
            concurrent_loads += 1;

            // Clone the pending_tiles for the async task
            let pending_tiles = osm_data.pending_tiles.clone();
            let tile = OSMTile::new(tile_x, tile_y, tile_zoom);

            // Log what we're loading
            debug_log!(debug_settings, "Loading {} tile: {}, {}, zoom {}", 
                      if is_background { "background" } else { "focus" }, 
                      tile_x, tile_y, tile_zoom);
            
            // Use debug flag for async task
            let debug_mode = debug_settings.debug_mode;

            // Spawn async task to load the tile image using the Tokio runtime
            tokio_runtime.0.spawn(async move {
                match load_tile_image(&tile).await {
                    Ok(image) => {
                        if debug_mode {
                            info!("Successfully loaded {} tile: {}, {}, zoom {}", 
                                 if is_background { "background" } else { "focus" },
                                 tile.x, tile.y, tile.z);
                        }
                        pending_tiles.lock().push((tile.x, tile.y, tile.z, Some(image), is_background));
                    },
                    Err(e) => {
                        if debug_mode {
                            info!("Failed to load {} tile: {}, {}, zoom {} - using fallback. Error: {}", 
                                 if is_background { "background" } else { "focus" },
                                 tile.x, tile.y, tile.z, e);
                        }
                        pending_tiles.lock().push((tile.x, tile.y, tile.z, None, is_background)); // None means use fallback
                    }
                }
            });
        }
    }
}

// This system processes any pending tiles and creates entities for them
pub fn apply_pending_tiles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut osm_data: ResMut<OSMData>,
    debug_settings: Res<DebugSettings>,
    time: Res<Time>,
) {
    // Take pending tiles
    let mut pending = osm_data.pending_tiles.lock();
    let pending_tiles: Vec<_> = pending.drain(..).collect();
    drop(pending);

    // Get current time for tile usage tracking
    let current_time = time.elapsed_secs();

    // Process each pending tile
    for (x, y, z, image_opt, is_background) in pending_tiles {
        let tile = OSMTile::new(x, y, z);
        
        // Create entity with either the loaded image or a fallback
        let entity = match image_opt {
            Some(image) => {
                debug_log!(debug_settings, "Creating {} tile: {}, {}, zoom {}", 
                          if is_background { "background" } else { "focus" }, x, y, z);
                
                // Standard tile creation with current time included
                create_tile_mesh(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &tile,
                    image,
                    current_time,
                    is_background
                )
            },
            None => {
                debug_log!(debug_settings, "Creating fallback entity for {} tile: {}, {}, zoom {}", 
                          if is_background { "background" } else { "focus" }, x, y, z);
                
                // Standard fallback with current time included
                create_fallback_tile_mesh(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &tile,
                    current_time,
                    is_background
                )
            }
        };

        // Add to appropriate list of active tiles
        if is_background {
            osm_data.background_tiles.push((x, y, z, entity));
        } else {
            osm_data.tiles.push((x, y, z, entity));
        }
    }
}

// This system updates which tiles are visible and marks the last time they were seen
pub fn update_visible_tiles(
    mut tile_query: Query<(&mut TileCoords, &Transform)>,
    camera_query: Query<&Transform, With<Camera3d>>,
    time: Res<Time>,
) {
    if let Ok(camera_transform) = camera_query.get_single() {
        let current_time = time.elapsed_secs();
        
        // Update all tiles
        for (mut tile_coords, tile_transform) in tile_query.iter_mut() {
            // Check if this tile is in camera view
            // Simple distance check for now - could be replaced with proper frustum culling later
            let distance = camera_transform.translation.distance(tile_transform.translation);

            // If the tile is close enough to be visible, update its last_used time
            if distance < 30.0 {
                tile_coords.last_used = current_time;
            }
        }
    }
}

// This system periodically cleans up tiles that haven't been visible for a while
pub fn cleanup_old_tiles(
    mut commands: Commands,
    mut osm_data: ResMut<OSMData>,
    debug_settings: Res<DebugSettings>,
    time: Res<Time>,
    tile_query: Query<(Entity, &TileCoords)>,
) {
    // Update total time
    osm_data.total_time += time.delta_secs();

    // Only run cleanup every 5 seconds to avoid constant checking
    if osm_data.total_time % 5.0 > 0.05 {
        return;
    }

    // How long a tile can be unused before being unloaded (in seconds)
    // Background tiles can stay longer since they cover more area
    const FOCUS_TILE_TIMEOUT: f32 = 30.0;
    const BACKGROUND_TILE_TIMEOUT: f32 = 60.0;
    
    let current_time = time.elapsed_secs();

    let mut focus_tiles_to_remove = Vec::new();
    let mut background_tiles_to_remove = Vec::new();
    let mut focus_indices_to_remove = Vec::new();
    let mut background_indices_to_remove = Vec::new();

    // Check all tiles in the system
    for (entity, tile_coords) in tile_query.iter() {
        let time_since_used = current_time - tile_coords.last_used;
        let is_background = tile_coords.zoom <= BACKGROUND_ZOOM_LEVEL;
        
        // Apply different timeouts based on tile type
        let timeout = if is_background { BACKGROUND_TILE_TIMEOUT } else { FOCUS_TILE_TIMEOUT };

        // Check if the timeout has been exceeded
        if time_since_used > timeout {
            if is_background {
                // Check if it's a background tile
                if let Some(idx) = osm_data.background_tiles.iter().position(|&(x, y, z, e)|
                    x == tile_coords.x && y == tile_coords.y && z == tile_coords.zoom && e == entity) {
                    background_tiles_to_remove.push(entity);
                    background_indices_to_remove.push(idx);
                }
            } else {
                // Check if it's a focus tile
                if let Some(idx) = osm_data.tiles.iter().position(|&(x, y, z, e)|
                    x == tile_coords.x && y == tile_coords.y && z == tile_coords.zoom && e == entity) {
                    focus_tiles_to_remove.push(entity);
                    focus_indices_to_remove.push(idx);
                }
            }
        }
    }

    // Sort indices in reverse order so we can remove without changing other indices
    focus_indices_to_remove.sort_by(|a, b| b.cmp(a));
    background_indices_to_remove.sort_by(|a, b| b.cmp(a));

    // Remove focus tiles from our tracking list
    for &idx in &focus_indices_to_remove {
        if idx < osm_data.tiles.len() {
            osm_data.tiles.remove(idx);
        }
    }

    // Remove background tiles from our tracking list
    for &idx in &background_indices_to_remove {
        if idx < osm_data.background_tiles.len() {
            osm_data.background_tiles.remove(idx);
        }
    }

    // Count the number of tiles to be removed
    let focus_removed = focus_tiles_to_remove.len();
    let background_removed = background_tiles_to_remove.len();

    // Now despawn entities after we've updated our tracking data
    for entity in focus_tiles_to_remove.into_iter().chain(background_tiles_to_remove) {
        commands.entity(entity).despawn_recursive();
    }

    // Also clean up the loaded_tiles lists periodically to prevent them from growing too large
    // Keep entries for currently loaded tiles
    let active_focus_coords: Vec<(u32, u32, u32)> = osm_data.tiles
        .iter()
        .map(|&(x, y, z, _)| (x, y, z))
        .collect();
    
    let active_background_coords: Vec<(u32, u32, u32)> = osm_data.background_tiles
        .iter()
        .map(|&(x, y, z, _)| (x, y, z))
        .collect();
    
    // Remove entries from loaded_tiles that are no longer needed
    osm_data.loaded_tiles.retain(|coords| active_focus_coords.contains(coords));
    osm_data.loaded_background_tiles.retain(|coords| active_background_coords.contains(coords));

    // Log cleanup results if any tiles were removed
    if focus_removed > 0 || background_removed > 0 {
        debug_log!(debug_settings, "Cleaned up {} unused focus tiles and {} background tiles", 
                  focus_removed, background_removed);
    }
}

// This system automatically detects and sets the zoom level based on camera height
pub fn auto_detect_zoom_level(
    mut osm_data: ResMut<OSMData>,
    camera_query: Query<&Transform, With<Camera3d>>,
    mut commands: Commands,
    debug_settings: Res<DebugSettings>,
) {
    if let Ok(camera_transform) = camera_query.get_single() {
        let camera_height = camera_transform.translation.y;

        // Use the function from constants.rs that implements proper zoom level distribution
        // based on OpenStreetMap's slippy map tilenames specifications
        let new_zoom = zoom_level_from_camera_height(camera_height)
            .clamp(MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL);

        // Only respond to zoom changes
        if new_zoom != osm_data.current_zoom {
            debug_log!(debug_settings, "Zoom level changing from {} to {} (camera height: {:.2})",
                  osm_data.current_zoom, new_zoom, camera_height);

            // Update current zoom level
            let old_zoom = osm_data.current_zoom;
            osm_data.current_zoom = new_zoom;

            debug_log!(debug_settings, "Zoom level changed from {} to {} (camera height: {:.2})",
                  old_zoom, new_zoom, camera_height);

            // Clear all focus tiles - simpler approach than trying to keep some tiles
            let mut entities_to_remove = Vec::new();
            
            // Collect all focus tile entities to remove (background tiles are kept)
            for &(_, _, _, entity) in &osm_data.tiles {
                entities_to_remove.push(entity);
            }
            
            // Clear the focus tile tracking lists
            osm_data.tiles.clear();
            osm_data.loaded_tiles.clear();
            
            // Despawn all focus tile entities at once
            for entity in entities_to_remove {
                if commands.get_entity(entity).is_some() {
                    commands.entity(entity).despawn_recursive();
                }
            }
            
            debug_log!(debug_settings, "Cleared all focus tiles for zoom level change");
        }
    }
} 