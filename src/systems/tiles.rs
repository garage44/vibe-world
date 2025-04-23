use bevy::prelude::*;
use crate::resources::{OSMData, TokioRuntime, DebugSettings};
use crate::components::{TileCoords};
use crate::osm::{OSMTile, load_tile_image, create_tile_mesh, create_fallback_tile_mesh};
use crate::utils::coordinate_conversion::world_to_tile_coords;
use crate::resources::constants::{max_tile_index, MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL, BACKGROUND_ZOOM_LEVEL};
use crate::debug_log;

// Process tiles based on camera position and view direction
pub fn process_tiles(
    mut osm_data: ResMut<OSMData>,
    tokio_runtime: Res<TokioRuntime>,
    debug_settings: Res<DebugSettings>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
) {
    // Skip if we have no camera yet
    if let Ok((camera_transform, _camera)) = camera_query.get_single() {
        let camera_pos = camera_transform.translation;
        let camera_forward = camera_transform.forward();
        
        // Calculate base zoom level from camera height - this determines the detail level
        let base_zoom = calculate_base_zoom_level(camera_pos.y);
        
        // Update global zoom level for UI and other systems
        osm_data.current_zoom = base_zoom;
        
        // Set a fixed lower zoom level for background (global context)
        let background_zoom = (base_zoom.saturating_sub(4)).max(MIN_ZOOM_LEVEL).min(6);
        osm_data.background_zoom = background_zoom;
        
        // Generate adaptive tiles with varying zoom levels
        // This system uses larger tiles (lower zoom) for areas further from view center
        generate_adaptive_tiles(
            &mut osm_data,
            &tokio_runtime,
            &debug_settings,
            camera_pos,
            camera_forward.into(),
            base_zoom,
        );
    }
}

// Calculate appropriate base zoom level from camera height
pub fn calculate_base_zoom_level(height: f32) -> u32 {
    // Reduce height increments to make zoom level changes more responsive
    match height {
        h if h <= 1.0 => 19,   // Level 19: Local highways, crossings (1:1000 scale)
        h if h <= 2.0 => 18,   // Level 18: Buildings, trees (1:2000 scale)
        h if h <= 4.0 => 17,   // Level 17: Building blocks, parks, addresses
        h if h <= 8.0 => 16,   // Level 16: Streets
        h if h <= 15.0 => 15,  // Level 15: Small roads
        h if h <= 30.0 => 14,  // Level 14: Detailed roads
        h if h <= 60.0 => 13,  // Level 13: Villages, suburbs
        h if h <= 120.0 => 12, // Level 12: Towns, city districts
        h if h <= 250.0 => 11, // Level 11: Cities
        h if h <= 500.0 => 10, // Level 10: Metropolitan areas
        h if h <= 1000.0 => 9, // Level 9: Large metro areas
        h if h <= 2000.0 => 8, // Level 8
        h if h <= 4000.0 => 7, // Level 7: Small countries, US states
        h if h <= 8000.0 => 6, // Level 6: Large European countries
        h if h <= 16000.0 => 5, // Level 5: Large African countries
        h if h <= 32000.0 => 4, // Level 4
        h if h <= 64000.0 => 3, // Level 3: Largest countries
        h if h <= 128000.0 => 2, // Level 2: Subcontinental areas
        _ => 1,                  // Level 1: Whole world
    }
}

// Generate an adaptive grid of tiles with varying zoom levels
fn generate_adaptive_tiles(
    osm_data: &mut OSMData,
    tokio_runtime: &TokioRuntime,
    debug_settings: &DebugSettings,
    camera_pos: Vec3,
    camera_forward: Vec3,
    base_zoom: u32,
) {
    // Project camera forward onto XZ plane
    let view_dir_xz = Vec3::new(camera_forward.x, 0.0, camera_forward.z).normalize();
    
    // Calculate viewing distance based on camera height and viewing angle
    let cam_height = camera_pos.y;
    let _angle_factor = 1.0 + (1.0 - camera_forward.y.abs()) * 2.0;
    
    // For more horizontal views, look farther ahead
    // For more vertical views, look closer to camera position
    let horizontal_factor = (1.0 - camera_forward.y.abs()).powf(0.5); // Square root for smoother transition
    let view_distance = cam_height * 1.5 * (1.0 + horizontal_factor * 3.0);
    
    // Calculate the exact point on the ground where the camera ray intersects
    // This is where we want to center our high-detail tiles
    let ground_y = 0.0; // Ground level
    let t = if camera_forward.y != 0.0 {
        // Parameter for ray-plane intersection: camera_pos + t * camera_forward = point on ground
        (ground_y - camera_pos.y) / camera_forward.y
    } else {
        // If camera is perfectly horizontal, use a default distance
        view_distance
    };
    
    // Only use the intersection point if it's in front of the camera (t > 0)
    // and within a reasonable distance
    let view_target = if t > 0.0 && t < view_distance * 2.0 {
        camera_pos + camera_forward * t
    } else {
        // Fallback: look ahead based on camera height
        camera_pos + view_dir_xz * view_distance
    };
    
    debug_log!(debug_settings, "View target: ({:.1}, {:.1}, {:.1}), height: {:.1}", 
              view_target.x, view_target.y, view_target.z, cam_height);
    
    // All tiles to load with their coordinates and priority
    let mut tiles_to_load = Vec::new();
    
    // Handle background (global context) tiles - use even lower zoom level
    // and much fewer tiles to reduce the total load
    let bg_zoom = (base_zoom.saturating_sub(5)).max(MIN_ZOOM_LEVEL).min(4);
    osm_data.background_zoom = bg_zoom;
    
    // Get tile at camera position for background layer
    let (bg_center_x, bg_center_y) = world_to_tile_coords(camera_pos.x, camera_pos.z, bg_zoom);
    
    // Add minimal set of background tiles (just enough for context)
    let bg_range = 1; // Minimal background
    for x_offset in -bg_range..=bg_range {
        for y_offset in -bg_range..=bg_range {
            let tile_x = (bg_center_x as i32 + x_offset).max(0) as u32;
            let tile_y = (bg_center_y as i32 + y_offset).max(0) as u32;
            
            let priority = 1000 + x_offset.abs() + y_offset.abs(); // Lowest priority
            tiles_to_load.push((tile_x, tile_y, bg_zoom, priority, true)); // true = background
        }
    }
    
    // Create a multi-level adaptive grid around the view target
    // The key is to use larger tiles (lower zoom) for areas further from the view center
    
    // Determine the highest zoom level we'll use (based on camera height)
    let highest_zoom = base_zoom.min(MAX_ZOOM_LEVEL);
    
    // OPTIMIZATION: Create much more aggressive zoom level reduction
    // Based on camera height, dynamically calculate how many zoom levels to use
    // and drastically reduce the number of tiles loaded
    
    // Dynamic zoom reduction based on camera height
    let max_zoom_levels = if cam_height > 500.0 {
        1 // At very high heights, just use one zoom level
    } else if cam_height > 200.0 {
        2 // At high heights, use two zoom levels
    } else {
        3 // At lower heights, use three zoom levels for more detail variation
    };
    
    // Create zoom level array
    let mut zoom_levels = Vec::with_capacity(max_zoom_levels);
    
    // Add highest zoom first - this is the center of view
    zoom_levels.push(highest_zoom);
    
    // Add lower zoom levels as needed
    if max_zoom_levels > 1 {
        zoom_levels.push((highest_zoom.saturating_sub(2)).max(MIN_ZOOM_LEVEL));
    }
    
    if max_zoom_levels > 2 {
        zoom_levels.push((highest_zoom.saturating_sub(4)).max(MIN_ZOOM_LEVEL));
    }
    
    // OPTIMIZATION: Keep track of covered areas to avoid loading redundant tiles
    let mut covered_areas: Vec<(u32, u32, u32)> = Vec::new(); // (tile_x, tile_y, zoom)
    
    // 2. Generate tiles for each zoom level ring
    for (ring_idx, &zoom) in zoom_levels.iter().enumerate() {
        // Skip this ring if it's too similar to background
        if zoom <= bg_zoom + 1 {
            continue;
        }
        
        // OPTIMIZATION: Use smaller radius for each ring
        // Higher zoom levels (more detailed) should cover smaller areas
        let radius = match ring_idx {
            0 => 3, // Increased radius for highest detail ring
            1 => 2, // Increased radius for middle ring 
            _ => 2, // Increased radius for outer ring
        };
        
        // Calculate target center - inner rings are centered precisely at view_target
        // Outer rings can be slightly biased towards the camera position
        let ring_center = if ring_idx == 0 {
            view_target // Center ring is at exact view target
        } else {
            // Blend between view_target and camera_pos for outer rings
            // This creates a better distribution for angled views
            let blend_factor = ring_idx as f32 * 0.25; // 0.25 for ring 1, 0.5 for ring 2...
            Vec3::lerp(
                view_target,
                Vec3::new(camera_pos.x, 0.0, camera_pos.z), // Project camera to ground
                blend_factor
            )
        };
        
        // Get tile coordinates for center of this ring
        let (center_x, center_y) = world_to_tile_coords(ring_center.x, ring_center.z, zoom);
        
        // Max tile index for this zoom level
        let max_index = max_tile_index(zoom);
        
        // Priority base for this ring - inner rings have higher priority
        let priority_base = ring_idx as i32 * 100;
        
        // Add tiles in a square pattern to cover the area
        for x_offset in -radius as i32..=radius as i32 {
            for y_offset in -radius as i32..=radius as i32 {
                // For outer rings, focus on the edges and corners
                let manhattan_dist = x_offset.abs() + y_offset.abs();
                
                // Skip inner tiles in outer rings to avoid redundancy
                if ring_idx > 0 && manhattan_dist < ring_idx as i32 {
                    continue;
                }
                
                // Add extra coverage for diagonal directions
                // This helps fill in gaps in the corners of the view
                let is_diagonal = x_offset.abs() == y_offset.abs() && x_offset != 0;
                
                // Calculate tile coordinates with bounds checking
                let tile_x = (center_x as i32 + x_offset).clamp(0, max_index as i32) as u32;
                let tile_y = (center_y as i32 + y_offset).clamp(0, max_index as i32) as u32;
                
                // OPTIMIZATION: Check if this area is already covered by a higher zoom level
                // Skip this tile if it would be redundant
                let is_covered = covered_areas.iter().any(|&(x, y, z)| 
                    is_same_area(tile_x, tile_y, zoom, x, y, z));
                
                if is_covered {
                    continue;
                }
                
                // Add this tile to covered areas
                covered_areas.push((tile_x, tile_y, zoom));
                
                // Calculate priority - closer to center = higher priority
                // Give diagonals slightly better priority to improve corner coverage
                let priority_adjustment = if is_diagonal { -1 } else { 0 };
                let priority = priority_base + manhattan_dist + priority_adjustment;
                
                // Add to tiles to load (false = not background)
                tiles_to_load.push((tile_x, tile_y, zoom, priority, false));
            }
        }
    }
    
    // No need to sort by priority - deduplication step will handle proper ordering
    
    // Further reduce total number of tiles
    let max_total_tiles = 60; // Increased from 40 to allow better coverage
    if tiles_to_load.len() > max_total_tiles {
        // Keep all background tiles
        let (background_tiles, mut foreground_tiles): (Vec<_>, Vec<_>) = 
            tiles_to_load.into_iter().partition(|&(_, _, _, _, is_bg)| is_bg);
        
        // Sort foreground tiles by priority
        foreground_tiles.sort_by_key(|&(_, _, _, priority, _)| priority);
        
        // Keep only the highest priority foreground tiles
        foreground_tiles.truncate(max_total_tiles - background_tiles.len());
        
        // Recombine
        tiles_to_load = background_tiles;
        tiles_to_load.extend(foreground_tiles);
    }
    
    // Remove duplicate tiles (keeping highest priority/zoom version)
    // This ensures we don't load both a large tile and its higher detail equivalents
    dedup_tiles(&mut tiles_to_load);
    
    // Process foreground and background tiles separately
    let (foreground_tiles, background_tiles): (Vec<_>, Vec<_>) = 
        tiles_to_load.into_iter()
                    .partition(|&(_, _, _, _, is_bg)| !is_bg);
    
    // Load foreground tiles
    if !foreground_tiles.is_empty() {
        debug_log!(debug_settings, "Loading {} foreground tiles", foreground_tiles.len());
        
        // Convert to the format expected by load_tiles
        let fg_tiles: Vec<(u32, u32, u32, i32)> = foreground_tiles
            .into_iter()
            .map(|(x, y, z, p, _)| (x, y, z, p))
            .collect();
            
        load_tiles(
            osm_data,
            tokio_runtime,
            debug_settings,
            &fg_tiles,
            16, // Increased concurrent loads for smoother loading
            false, // Not background
        );
    }
    
    // Load background tiles
    if !background_tiles.is_empty() {
        debug_log!(debug_settings, "Loading {} background tiles", background_tiles.len());
        
        // Convert to the format expected by load_tiles
        let bg_tiles: Vec<(u32, u32, u32, i32)> = background_tiles
            .into_iter()
            .map(|(x, y, z, p, _)| (x, y, z, p))
            .collect();
            
        load_tiles(
            osm_data,
            tokio_runtime,
            debug_settings,
            &bg_tiles,
            4, // Limit concurrent loads
            true, // Background tiles
        );
    }
}

// Helper function to remove duplicate tiles, preferring higher zoom (detail) levels
fn dedup_tiles(tiles: &mut Vec<(u32, u32, u32, i32, bool)>) {
    // Sort by coordinates and background flag
    tiles.sort_by(|a, b| {
        // Compare background flag first (group backgrounds together)
        a.4.cmp(&b.4)
        // Then by coordinates
        .then(a.0.cmp(&b.0))
        .then(a.1.cmp(&b.1))
        // Then by zoom level in DESCENDING order (higher zoom = more detail)
        .then(b.2.cmp(&a.2))
    });
    
    // Dedup by coordinates - this keeps the first occurrence which will be 
    // the highest zoom level (most detailed) version
    let mut i = 0;
    while i < tiles.len() {
        let mut j = i + 1;
        while j < tiles.len() {
            // Check if tiles refer to the same area
            if is_same_area(tiles[i].0, tiles[i].1, tiles[i].2, 
                           tiles[j].0, tiles[j].1, tiles[j].2) &&
               tiles[i].4 == tiles[j].4 { // And same background status
                // Remove the duplicate (lower zoom version)
                tiles.remove(j);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
    
    // Resort by priority
    tiles.sort_by_key(|&(_, _, _, priority, _)| priority);
}

// Helper function to check if two tiles refer to the same geographic area
// A higher zoom tile (z2) is contained within a lower zoom tile (z1) if its coordinates
// are derived from the lower zoom tile's coordinates
fn is_same_area(x1: u32, y1: u32, z1: u32, x2: u32, y2: u32, z2: u32) -> bool {
    // First check if tiles are exactly the same
    if x1 == x2 && y1 == y2 && z1 == z2 {
        return true;
    }
    
    // If zoom levels are the same but coordinates differ, they're different areas
    if z1 == z2 {
        return false;
    }
    
    // Handle case where one tile is at a higher zoom level than the other
    if z1 < z2 {
        // z1 is the lower zoom (larger tile)
        // Check if the higher zoom tile (x2,y2,z2) is contained within (x1,y1,z1)
        let zoom_diff = z2 - z1;
        let factor = 1 << zoom_diff; // 2^zoom_diff
        
        // Calculate the expected range of higher zoom tiles that would fit in the lower zoom tile
        let min_x2 = x1 * factor;
        let min_y2 = y1 * factor;
        let max_x2 = min_x2 + factor - 1;
        let max_y2 = min_y2 + factor - 1;
        
        // Check if the higher zoom tile is within this range
        return x2 >= min_x2 && x2 <= max_x2 && y2 >= min_y2 && y2 <= max_y2;
    } else {
        // z2 is the lower zoom (larger tile) - reverse the check
        let zoom_diff = z1 - z2;
        let factor = 1 << zoom_diff; // 2^zoom_diff
        
        // Calculate the expected range of higher zoom tiles that would fit in the lower zoom tile
        let min_x1 = x2 * factor;
        let min_y1 = y2 * factor;
        let max_x1 = min_x1 + factor - 1;
        let max_y1 = min_y1 + factor - 1;
        
        // Check if the higher zoom tile is within this range
        return x1 >= min_x1 && x1 <= max_x1 && y1 >= min_y1 && y1 <= max_y1;
    }
}

// Function to handle the actual tile loading logic (shared between adaptive and background systems)
fn load_tiles(
    osm_data: &mut OSMData,
    tokio_runtime: &TokioRuntime,
    debug_settings: &DebugSettings,
    tiles_to_load: &[(u32, u32, u32, i32)], // (x, y, zoom, priority)
    max_concurrent_loads: usize,
    is_background: bool,
) {
    let mut concurrent_loads = 0;

    // Get appropriate tracking list based on tile type
    let loaded_tiles = if is_background {
        &mut osm_data.loaded_background_tiles
    } else {
        &mut osm_data.loaded_tiles
    };

    // Process tiles in order of priority
    for &(tile_x, tile_y, tile_zoom, _) in tiles_to_load {
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
    mut tile_query: Query<(&mut TileCoords, &Transform, Entity)>,
    camera_query: Query<&Transform, With<Camera3d>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    if let Ok(camera_transform) = camera_query.get_single() {
        let current_time = time.elapsed_secs();
        
        // Get camera position and forward direction
        let camera_pos = camera_transform.translation;
        let camera_forward = camera_transform.forward();
        
        // Create list of entities to despawn
        let mut to_despawn = Vec::new();
        
        // Update all tiles
        for (mut tile_coords, tile_transform, entity) in tile_query.iter_mut() {
            let tile_pos = tile_transform.translation;
            
            // Calculate the vector from camera to tile center
            let to_tile = tile_pos - camera_pos;
            let distance = to_tile.length();
            
            // For tiles to be visible, they should be:
            // 1. Within a reasonable distance (based on zoom level)
            // 2. Roughly within the camera's field of view
            
            // Calculate max visible distance based on zoom and allow larger view area
            let zoom_factor = 1.0 + 0.7 * (MAX_ZOOM_LEVEL - tile_coords.zoom) as f32;
            let max_distance = 75.0 * zoom_factor; // Increased from 50.0 to 75.0 for wider view
            
            // Use a wider angle check (more permissive) to avoid gaps at edges
            let forward_dot = camera_forward.dot(to_tile.normalize());
            
            // Is the tile visible? More permissive check
            // Forward dot > -0.3 means roughly within ~110 degree field of view (instead of 90)
            let is_visible = distance < max_distance && forward_dot > -0.3;
            
            if is_visible {
                // Update last used time if visible
                tile_coords.last_used = current_time;
            } else {
                // Tile is not visible
                let time_since_used = current_time - tile_coords.last_used;
                
                // After 1.5 seconds of being outside view, remove non-background tiles
                // Slightly increased from 1.0 to 1.5 to prevent rapid flickering at edges
                if time_since_used > 1.5 && tile_coords.zoom > 6 {
                    to_despawn.push(entity);
                }
            }
        }
        
        // Despawn entities outside view
        for entity in to_despawn {
            commands.entity(entity).despawn_recursive();
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

    // Run cleanup more frequently - every 1 second
    if osm_data.total_time % 1.0 > 0.05 {
        return;
    }

    // How long a tile can be unused before being unloaded (in seconds)
    // More aggressive cleanup for detailed tiles
    const FOCUS_TILE_TIMEOUT: f32 = 3.0;      // Much shorter timeout for focus tiles
    const BACKGROUND_TILE_TIMEOUT: f32 = 30.0; // Background tiles can stay longer
    
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
        let timeout = if is_background { 
            BACKGROUND_TILE_TIMEOUT 
        } else { 
            // Scale timeout by zoom level - higher zoom (more detailed) = shorter timeout
            let zoom_factor = (MAX_ZOOM_LEVEL - tile_coords.zoom) as f32 / MAX_ZOOM_LEVEL as f32;
            FOCUS_TILE_TIMEOUT * (1.0 + zoom_factor * 5.0) // 3-15 seconds depending on zoom
        };

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

// The auto_detect_zoom_level system is no longer needed as our adaptive system handles zoom levels
// Keep this system empty as a placeholder in case other systems depend on it being registered
pub fn auto_detect_zoom_level(_: ResMut<OSMData>, _: Query<&Transform, With<Camera3d>>, _: Commands, _: Res<DebugSettings>) {
    // Intentionally empty - zoom level detection is now handled in process_tiles
} 