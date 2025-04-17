use bevy::prelude::*;
use bevy::ecs::system::ParamSet;
use crate::resources::{OSMData, TokioRuntime, PersistentIslandSettings, DebugSettings};
use crate::components::{TileCoords, PersistentIsland};
use crate::osm::{OSMTile, load_tile_image, create_tile_mesh, create_fallback_tile_mesh};
use crate::utils::coordinate_conversion::world_to_tile_coords;
use crate::resources::constants::{PERSISTENT_ISLAND_ZOOM_LEVEL, max_tile_index, MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL};
use crate::debug_log;

// Process tiles with additional handling for persistent islands
pub fn process_tiles(
    mut osm_data: ResMut<OSMData>,
    tokio_runtime: Res<TokioRuntime>,
    debug_settings: Res<DebugSettings>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
) {
    // Skip if we have no camera yet
    if let Ok((camera_transform, _camera)) = camera_query.get_single() {
        let camera_pos = camera_transform.translation;
        let _forward = camera_transform.forward();
        let current_zoom = osm_data.current_zoom;

        // Calculate the visible range (how many tiles from center to load in each direction)
        // Adjust based on zoom level to prevent loading too many tiles at once
        let visible_range = match current_zoom {
            z if z >= 18 => 2,  // Very close zoom
            z if z >= 16 => 3,  // Close zoom
            z if z >= 14 => 4,  // Medium zoom
            _ => 5,             // Far zoom
        };

        // Center of current view
        let _center_x = camera_pos.x.floor() as i32;
        let _center_y = camera_pos.z.floor() as i32;

        // Tile coordinates at current zoom level
        let (tile_center_x, tile_center_y) = world_to_tile_coords(camera_pos.x, camera_pos.z, current_zoom);

        // Get persistent islands near the camera
        let mut persistent_islands_to_check = Vec::new();
        
        // Convert current camera position to coordinates at persistent island zoom level
        let (_pi_center_x, _pi_center_y) = world_to_tile_coords(
            camera_pos.x, 
            camera_pos.z, 
            PERSISTENT_ISLAND_ZOOM_LEVEL
        );
        
        // Calculate the search range for persistent islands at the PERSISTENT_ISLAND_ZOOM_LEVEL
        // This range needs to be scaled based on the difference between current zoom and island zoom
        let zoom_diff = PERSISTENT_ISLAND_ZOOM_LEVEL as i32 - current_zoom as i32;
        let scale_factor = if zoom_diff > 0 {
            // Current zoom is less than island zoom (zoomed out)
            // Each tile at current zoom contains 2^zoom_diff tiles at island zoom
            1
        } else if zoom_diff < 0 {
            // Current zoom is greater than island zoom (zoomed in)
            // Need to check a wider area of island zoom tiles
            2i32.pow((-zoom_diff) as u32) as i32
        } else {
            1 // Same zoom level
        };
        
        // Adjust search range based on the zoom difference
        let _pi_range = 3 * scale_factor;
            
        // Check for persistent islands in the area
        for (tile_x, tile_y) in osm_data.persistent_islands.keys() {
            // Check if this island is in our current view range
            // Convert island coordinates to current zoom level
            let (scaled_x, scaled_y) = if zoom_diff > 0 {
                // Current zoom < island zoom (zoomed out)
                // Multiple islands map to one current tile
                (*tile_x as i32 >> zoom_diff, *tile_y as i32 >> zoom_diff)
            } else if zoom_diff < 0 {
                // Current zoom > island zoom (zoomed in)
                // One island maps to multiple current tiles
                // In this case, calculate the range of tiles that cover this island
                let abs_diff = (-zoom_diff) as u32;
                let start_x = *tile_x << abs_diff;
                let start_y = *tile_y << abs_diff;
                let end_x = start_x + (1 << abs_diff) - 1;
                let end_y = start_y + (1 << abs_diff) - 1;
                
                // Add all tiles in this range
                for x in start_x..=end_x {
                    for y in start_y..=end_y {
                        persistent_islands_to_check.push((x, y));
                    }
                }
                
                // Return the center tile (converted to i32 to match the other branch)
                (start_x as i32, start_y as i32)
            } else {
                // Same zoom level (converted to i32 to match the other branches)
                (*tile_x as i32, *tile_y as i32)
            };
            
            // Calculate distance to the tile at current zoom level
            let distance = (scaled_x - tile_center_x as i32).abs() + (scaled_y - tile_center_y as i32).abs();
            
            // Only process islands within our view range
            if distance <= visible_range as i32 * 3 {
                persistent_islands_to_check.push((scaled_x as u32, scaled_y as u32));
            }
        }
        
        debug_log!(debug_settings, "Found {} islands near camera to check", persistent_islands_to_check.len());

        // First, always load the actual island tiles at zoom level 17 if they're in view range
        for (pi_x, pi_y) in persistent_islands_to_check.clone() {
            // Skip if already loaded or pending
            if osm_data.loaded_tiles.contains(&(pi_x, pi_y, PERSISTENT_ISLAND_ZOOM_LEVEL)) ||
               osm_data.pending_tiles.lock().iter().any(|(x, y, z, _)| 
                   *x == pi_x && *y == pi_y && *z == PERSISTENT_ISLAND_ZOOM_LEVEL
               ) {
                continue;
            }
            
            // Mark as loaded to prevent duplicate requests
            osm_data.loaded_tiles.push((pi_x, pi_y, PERSISTENT_ISLAND_ZOOM_LEVEL));
            
            // Clone the pending_tiles for the async task
            let pending_tiles = osm_data.pending_tiles.clone();
            let tile = OSMTile::new(pi_x, pi_y, PERSISTENT_ISLAND_ZOOM_LEVEL);
            
            // Log what we're loading
            debug_log!(debug_settings, "Loading persistent island tile: {}, {}", pi_x, pi_y);
            
            // Use debug flag for async task
            let debug_mode = debug_settings.debug_mode;
            
            // Spawn async task to load the tile image using the Tokio runtime
            tokio_runtime.0.spawn(async move {
                match load_tile_image(&tile).await {
                    Ok(image) => {
                        if debug_mode {
                            info!("Successfully loaded persistent island: {}, {}", tile.x, tile.y);
                        }
                        pending_tiles.lock().push((tile.x, tile.y, tile.z, Some(image)));
                    },
                    Err(e) => {
                        if debug_mode {
                            info!("Failed to load persistent island: {}, {} - using fallback. Error: {}", 
                                  tile.x, tile.y, e);
                        }
                        pending_tiles.lock().push((tile.x, tile.y, tile.z, None)); // None means use fallback
                    }
                }
            });
        }

        // Now handle regular tiles at the current zoom level
        // Generate a list of tile coordinates to load, sorted by distance from center
        let mut tiles_to_load: Vec<(u32, u32, i32)> = Vec::new();

        // For tiles at current zoom level, we need to know which ones correspond to islands
        let mut current_zoom_island_tiles = Vec::new();
        
        for (island_x, island_y) in &persistent_islands_to_check {
            // Convert island coordinates (zoom 17) to current zoom level
            let (current_x, current_y) = if zoom_diff > 0 {
                // Current zoom < island zoom (zoomed out)
                // Multiple islands map to one current tile
                (*island_x >> zoom_diff as u32, *island_y >> zoom_diff as u32)
            } else if zoom_diff < 0 {
                // Current zoom > island zoom (zoomed in)
                // One island maps to multiple current tiles
                // In this case, calculate the range of tiles that cover this island
                let abs_diff = (-zoom_diff) as u32;
                let start_x = *island_x << abs_diff;
                let start_y = *island_y << abs_diff;
                let end_x = start_x + (1 << abs_diff) - 1;
                let end_y = start_y + (1 << abs_diff) - 1;
                
                // Add all tiles in this range
                for x in start_x..=end_x {
                    for y in start_y..=end_y {
                        current_zoom_island_tiles.push((x, y));
                    }
                }
                
                // Return the center tile
                (start_x, start_y)
            } else {
                // Same zoom level
                (*island_x, *island_y)
            };
            
            if zoom_diff >= 0 {
                // Only add if not already added (in case multiple islands map to same tile)
                if !current_zoom_island_tiles.contains(&(current_x, current_y)) {
                    current_zoom_island_tiles.push((current_x, current_y));
                }
            }
        }
        
        debug_log!(debug_settings, "Islands correspond to {} tiles at current zoom {}", current_zoom_island_tiles.len(), current_zoom);

        // We'll define our camera frustum more efficiently for culling
        // Forward vector - where the camera is looking
        let _forward = camera_transform.forward();
        // Side vector - unused but kept for future use
        let _right = camera_transform.right();

        // Simple frustum culling - check if tile is in camera's field of view
        for x_offset in -visible_range as i32..=visible_range as i32 {
            for y_offset in -visible_range as i32..=visible_range as i32 {
                let max_index = max_tile_index(current_zoom);
                let tile_x = (tile_center_x as i32 + x_offset).clamp(0, max_index as i32) as u32;
                let tile_y = (tile_center_y as i32 + y_offset).clamp(0, max_index as i32) as u32;

                // Check if this tile corresponds to an island
                let is_island_tile = current_zoom_island_tiles.contains(&(tile_x, tile_y));

                // Calculate world position of this tile
                let tile_pos = Vec3::new(tile_x as f32, 0.0, tile_y as f32);

                // Calculate direction from camera to tile
                let to_tile = tile_pos - camera_transform.translation;

                // Normalize the direction
                let dist = to_tile.length();

                // Skip if too far away
                if dist > visible_range as f32 * 2.5 {
                    continue;
                }

                // Simple frustum test - is the tile in front of the camera?
                // This helps avoid loading tiles behind the camera
                let dot = to_tile.normalize().dot(*_forward);

                // Skip tiles outside of viewing angle (behind or far to sides)
                // Increasing this number narrows the viewing angle
                // 0.0 would be 90 degrees to either side
                let frustum_angle = -0.2; // Slightly behind camera to avoid pop-in when turning
                if dot < frustum_angle {
                    continue;
                }

                // Calculate manhattan distance from center for priority
                let distance = x_offset.abs() + y_offset.abs();
                
                // Prioritize island tiles by giving them lower distance value
                let adjusted_distance = if is_island_tile {
                    // Make islands higher priority by artificially reducing their distance
                    distance / 2
                } else {
                    distance
                };

                // Add to load queue if not already loaded/pending
                tiles_to_load.push((tile_x, tile_y, adjusted_distance));
            }
        }

        // Sort tiles by distance (closest first)
        tiles_to_load.sort_by_key(|&(_, _, distance)| distance);

        // Limit the number of concurrent loads to avoid overwhelming the system
        // Adjust based on zoom level - fewer concurrent loads at higher zoom levels
        let max_concurrent_loads = match current_zoom {
            z if z >= 17 => 4,  // Fewer loads at very high zoom
            z if z >= 15 => 6,  // Medium at medium zoom
            _ => 8,             // More at low zoom
        };

        let mut concurrent_loads = 0;

        // Process tiles in order of priority (closest first)
        for (tile_x, tile_y, _) in tiles_to_load {
            // Check if we've reached the maximum concurrent load limit
            if concurrent_loads >= max_concurrent_loads {
                break;
            }
            
            // Check if this tile corresponds to an island
            let is_island_tile = current_zoom_island_tiles.contains(&(tile_x, tile_y));

            // Check if tile is already loaded or pending
            if !osm_data.loaded_tiles.contains(&(tile_x, tile_y, current_zoom)) &&
               !osm_data.pending_tiles.lock().iter().any(|(x, y, z, _)| *x == tile_x && *y == tile_y && *z == current_zoom) {

                // Mark as loaded to prevent duplicate requests
                osm_data.loaded_tiles.push((tile_x, tile_y, current_zoom));
                concurrent_loads += 1;

                // Clone the pending_tiles for the async task
                let pending_tiles = osm_data.pending_tiles.clone();
                let tile = OSMTile::new(tile_x, tile_y, current_zoom);

                // Log what we're loading
                if is_island_tile {
                    debug_log!(debug_settings, "Loading island-corresponding tile: {}, {}, zoom {}", tile_x, tile_y, current_zoom);
                } else {
                    debug_log!(debug_settings, "Loading regular tile: {}, {}, zoom {}", tile_x, tile_y, current_zoom);
                }

                // Keep track whether this is an island tile (for rendering)
                let tile_type = if is_island_tile { "island" } else { "regular" };
                
                // Use debug flag for async task
                let debug_mode = debug_settings.debug_mode;

                // Spawn async task to load the tile image using the Tokio runtime
                tokio_runtime.0.spawn(async move {
                    match load_tile_image(&tile).await {
                        Ok(image) => {
                            if debug_mode {
                                info!("Successfully loaded {} tile: {}, {}, zoom {}", 
                                      tile_type, tile.x, tile.y, tile.z);
                            }
                            // Include the tile type info in the pending_tiles data
                            pending_tiles.lock().push((tile.x, tile.y, tile.z, Some(image)));
                        },
                        Err(e) => {
                            if debug_mode {
                                info!("Failed to load {} tile: {}, {}, zoom {} - using fallback. Error: {}", 
                                      tile_type, tile.x, tile.y, tile.z, e);
                            }
                            pending_tiles.lock().push((tile.x, tile.y, tile.z, None)); // None means use fallback
                        }
                    }
                });
            }
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
    _island_settings: Res<PersistentIslandSettings>,
    debug_settings: Res<DebugSettings>,
    time: Res<Time>,
) {
    // Take pending tiles
    let mut pending = osm_data.pending_tiles.lock();
    let pending_tiles: Vec<_> = pending.drain(..).collect();
    drop(pending);

    // Process each pending tile
    for (x, y, z, image_opt) in pending_tiles {
        let tile = OSMTile::new(x, y, z);
        let current_time = time.elapsed_secs();
        
        // Check if this is a persistent island tile at zoom level 17
        let is_exact_island = z == PERSISTENT_ISLAND_ZOOM_LEVEL && 
                           osm_data.persistent_islands.contains_key(&(x, y));
        
        // Check if this tile corresponds to an island at the current zoom level
        let is_island_corresponding_tile = z != PERSISTENT_ISLAND_ZOOM_LEVEL && {
            // Calculate the zoom difference
            let zoom_diff = PERSISTENT_ISLAND_ZOOM_LEVEL as i32 - z as i32;
            
            if zoom_diff > 0 {
                // Current zoom < island zoom (zoomed out)
                // Check if any island, when scaled down, maps to this tile
                osm_data.persistent_islands.keys().any(|(island_x, island_y)| {
                    (*island_x >> zoom_diff as u32) == x && (*island_y >> zoom_diff as u32) == y
                })
            } else if zoom_diff < 0 {
                // Current zoom > island zoom (zoomed in)
                // Check if this tile is inside any island's area when scaled up
                let abs_diff = (-zoom_diff) as u32;
                osm_data.persistent_islands.keys().any(|(island_x, island_y)| {
                    let start_x = *island_x << abs_diff;
                    let start_y = *island_y << abs_diff;
                    let end_x = start_x + (1 << abs_diff) - 1;
                    let end_y = start_y + (1 << abs_diff) - 1;
                    
                    x >= start_x && x <= end_x && y >= start_y && y <= end_y
                })
            } else {
                // Same zoom level - this is handled by is_exact_island
                false
            }
        };
        
        // Determine if this tile should receive island visual treatment
        let needs_island_visuals = is_exact_island || is_island_corresponding_tile;

        // Create entity with either the loaded image or a fallback
        let entity = match image_opt {
            Some(image) => {
                if z == PERSISTENT_ISLAND_ZOOM_LEVEL {
                    debug_log!(debug_settings, "Creating exact island tile: {}, {}, zoom {}", x, y, z);
                } else if is_island_corresponding_tile {
                    debug_log!(debug_settings, "Creating island corresponding tile: {}, {}, zoom {}", x, y, z);
                } else {
                    debug_log!(debug_settings, "Creating regular tile: {}, {}, zoom {}", x, y, z);
                }
                
                if needs_island_visuals {
                    // Island visualization for both exact islands and corresponding tiles
                    // Instead of creating a completely modified image with border, just apply a subtle darkening
                    let modified_image = image.clone();
                    let rgba_image = modified_image.to_rgba8();
                    
                    // Create a modified version with subtle darkening
                    let mut rgba_modified = rgba_image.clone();
                    let width = rgba_image.width();
                    let height = rgba_image.height();
                    
                    // Apply a subtle darkening effect across the entire image
                    // This is less distracting than the green border
                    let darken_factor = 0.2; // 20% darker
                    
                    for x in 0..width {
                        for y in 0..height {
                            let pixel = rgba_modified.get_pixel_mut(x, y);
                            let p = pixel.0;
                            // Darken by reducing RGB values
                            pixel.0 = [
                                (p[0] as f32 * (1.0 - darken_factor)) as u8,
                                (p[1] as f32 * (1.0 - darken_factor)) as u8,
                                (p[2] as f32 * (1.0 - darken_factor)) as u8,
                                p[3]
                            ];
                        }
                    }
                    
                    // Still apply a subtle border to help identify the island
                    let mut border_width = (width as f32 * 0.03) as u32; // Thinner border
                    border_width = border_width.max(1).min(5); // 1-5 pixels only
                    
                    // Use a more subtle color for the border
                    let border_color = [40, 40, 40, 150]; // Dark gray semi-transparent border
                    
                    // Only draw border around the edges
                    for x in 0..width {
                        for y in 0..height {
                            if x < border_width || x >= width - border_width || 
                               y < border_width || y >= height - border_width {
                                // We're on the border
                                let pixel = rgba_modified.get_pixel_mut(x, y);
                                // Blend the border color with the existing pixel
                                let p = pixel.0;
                                let alpha_factor = border_color[3] as f32 / 255.0;
                                pixel.0 = [
                                    ((1.0 - alpha_factor) * p[0] as f32 + alpha_factor * border_color[0] as f32) as u8,
                                    ((1.0 - alpha_factor) * p[1] as f32 + alpha_factor * border_color[1] as f32) as u8,
                                    ((1.0 - alpha_factor) * p[2] as f32 + alpha_factor * border_color[2] as f32) as u8,
                                    p[3]
                                ];
                            }
                        }
                    }
                    
                    // Convert back to DynamicImage
                    let modified_dynamic = image::DynamicImage::ImageRgba8(rgba_modified);
                    
                    // Create the tile with the modified image
                    create_tile_mesh(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &tile,
                        modified_dynamic,
                    )
                } else {
                    // Standard tile creation for non-islands
                    create_tile_mesh(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &tile,
                        image,
                    )
                }
            },
            None => {
                debug_log!(debug_settings, "Creating fallback entity for tile: {}, {}, zoom {}", x, y, z);
                if needs_island_visuals {
                    // For islands, create a special colored fallback
                    let mut entity_builder = commands.spawn_empty();
                    
                    // Create vertices just like in the fallback_tile_mesh function
                    let mut mesh = Mesh::new(
                        bevy::render::mesh::PrimitiveTopology::TriangleList,
                        bevy::render::render_asset::RenderAssetUsages::default(),
                    );
                    
                    let vertices: [[f32; 8]; 4] = [
                        [0.0, 0.0, 0.0,    0.0, 1.0, 0.0,          0.0, 0.0], // northwest corner
                        [1.0, 0.0, 0.0,    0.0, 1.0, 0.0,          1.0, 0.0], // northeast corner
                        [1.0, 0.0, 1.0,    0.0, 1.0, 0.0,          1.0, 1.0], // southeast corner
                        [0.0, 0.0, 1.0,    0.0, 1.0, 0.0,          0.0, 1.0], // southwest corner
                    ];

                    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| [v[0], v[1], v[2]]).collect();
                    let normals: Vec<[f32; 3]> = vertices.iter().map(|v| [v[3], v[4], v[5]]).collect();
                    let uvs: Vec<[f32; 2]> = vertices.iter().map(|v| [v[6], v[7]]).collect();
                    let indices = vec![0, 1, 2, 0, 2, 3]; // triangulate the quad

                    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
                    mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));
                    
                    let mesh_handle = meshes.add(mesh);
                    
                    // Special material for islands - green instead of red
                    // Use different color intensity for exact islands vs corresponding tiles
                    let green_intensity = if is_exact_island { 0.7 } else { 0.5 };
                    let material = materials.add(StandardMaterial {
                        base_color: Color::srgb(0.1, green_intensity, 0.3), // Green color for island fallbacks
                        emissive: LinearRgba::new(0.1, 0.5, 0.1, 0.5), // Slight green glow
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        double_sided: true,
                        cull_mode: None,
                        ..default()
                    });
                    
                    entity_builder
                        .insert((
                            Mesh3d(mesh_handle),
                            MeshMaterial3d(material),
                            Transform::from_xyz(x as f32, 0.0, y as f32),
                            GlobalTransform::default(),
                            Name::new(format!("Island Fallback Tile {},{}, zoom {}", x, y, z)),
                        ))
                        .id()
                } else {
                    // Standard fallback for non-islands
                    create_fallback_tile_mesh(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &tile,
                    )
                }
            }
        };
        
        // Add PersistentIsland component if this is an exact island tile at zoom level 17
        if is_exact_island {
            if let Some(island_data) = osm_data.persistent_islands.get(&(x, y)) {
                commands.entity(entity).insert(PersistentIsland {
                    name: island_data.name.clone(),
                    // Copy any other fields
                });
            }
        }
        
        // Add a component to mark corresponding island tiles at other zoom levels
        if is_island_corresponding_tile {
            commands.entity(entity).insert(Name::new(format!("Island Tile Proxy {},{}, zoom {}", x, y, z)));
        }

        // Add TileCoords component to ALL tiles
        commands.entity(entity).insert(TileCoords {
            x,
            y,
            zoom: z,
            last_used: current_time,
        });

        // Add to our list of active tiles
        osm_data.tiles.push((x, y, z, entity));
    }
}

// This system updates which tiles are visible and marks the last time they were seen
pub fn update_visible_tiles(
    mut param_set: ParamSet<(
        Query<(&mut TileCoords, &Transform)>,
        Query<(Entity, &TileCoords, &Transform), With<PersistentIsland>>
    )>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
    time: Res<Time>,
) {
    if let Ok((camera_transform, _camera)) = camera_query.get_single() {
        // First, collect all persistent island entities that need updating
        let mut islands_to_update = Vec::new();
        
        // Get info from the persistent islands query
        {
            let island_query = param_set.p1();
            for (entity, tile_coords, tile_transform) in island_query.iter() {
                // For persistent islands, we use a larger visibility radius
                let distance = camera_transform.translation.distance(tile_transform.translation);
                
                // Always keep persistent islands "fresh" when they're in view
                if distance < 50.0 {  // Larger distance for persistent islands
                    islands_to_update.push((entity, tile_coords.x, tile_coords.y, tile_coords.zoom));
                }
            }
        }
        
        // Now update the TileCoords from the main query for both islands and regular tiles
        {
            let mut main_query = param_set.p0();
            
            // First update persistent islands
            let current_time = time.elapsed_secs();
            for (_island_entity, x, y, zoom) in islands_to_update {
                // Find the entity in the main query
                for (mut coords, _) in main_query.iter_mut() {
                    if coords.x == x && coords.y == y && coords.zoom == zoom {
                        coords.last_used = current_time;
                        break;
                    }
                }
            }
            
            // Now update regular tiles
            for (mut tile_coords, tile_transform) in main_query.iter_mut() {
                // Skip islands as they were already handled
                if tile_coords.zoom == PERSISTENT_ISLAND_ZOOM_LEVEL {
                    // We already updated islands, so skip them
                    continue;
                }
                
                // Check if this tile is in camera view
                // Simple distance check for now - could be replaced with proper frustum culling later
                let distance = camera_transform.translation.distance(tile_transform.translation);

                // If the tile is close enough to be visible, update its last_used time
                if distance < 30.0 {
                    tile_coords.last_used = time.elapsed_secs();
                }
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
    mut param_set: ParamSet<(
        Query<(Entity, &TileCoords)>,
        Query<(Entity, &TileCoords), With<PersistentIsland>>
    )>,
) {
    // Update total time
    osm_data.total_time += time.delta_secs();

    // Only run cleanup every 5 seconds to avoid constant checking
    if osm_data.total_time % 5.0 > 0.05 {
        return;
    }

    // How long a tile can be unused before being unloaded (in seconds)
    const TILE_TIMEOUT: f32 = 30.0;
    // Longer timeout for persistent islands
    const PERSISTENT_ISLAND_TIMEOUT: f32 = 120.0;
    
    let current_time = time.elapsed_secs();

    let mut tiles_to_remove = Vec::new();
    let mut indices_to_remove = Vec::new();
    
    // First, collect all persistent island entities and their coordinates
    let mut persistent_islands = Vec::new();
    {
        let island_query = param_set.p1();
        for (entity, tile_coords) in island_query.iter() {
            persistent_islands.push((entity, tile_coords.x, tile_coords.y, tile_coords.zoom));
        }
    }

    // Now check for tiles to remove based on last_used time
    {
        let tile_query = param_set.p0();
        for (entity, tile_coords) in tile_query.iter() {
            // Check if this is a persistent island tile
            let is_persistent_island = tile_coords.zoom == PERSISTENT_ISLAND_ZOOM_LEVEL &&
                                      persistent_islands.iter().any(|(_, x, y, z)| 
                                          *x == tile_coords.x && 
                                          *y == tile_coords.y &&
                                          *z == tile_coords.zoom
                                      );
            
            let timeout = if is_persistent_island {
                PERSISTENT_ISLAND_TIMEOUT // Longer timeout for persistent islands
            } else {
                TILE_TIMEOUT              // Standard timeout for regular tiles
            };
            
            if current_time - tile_coords.last_used > timeout {
                // Skip removing persistent islands completely if we want them to be truly persistent
                if !is_persistent_island {
                    tiles_to_remove.push(entity);

                    // Find the index in our OSMData.tiles array
                    if let Some(idx) = osm_data.tiles.iter().position(|&(x, y, z, e)|
                        x == tile_coords.x && y == tile_coords.y && z == tile_coords.zoom && e == entity) {
                        indices_to_remove.push(idx);
                    }
                }
            }
        }
    }

    // Sort indices in reverse order so we can remove without changing other indices
    indices_to_remove.sort_by(|a, b| b.cmp(a));

    // Remove tiles from far to near to avoid index shifting
    for idx in indices_to_remove {
        if idx < osm_data.tiles.len() {
            osm_data.tiles.remove(idx);
        }
    }

    // Despawn entities
    for &entity in &tiles_to_remove {
        commands.entity(entity).despawn_recursive();
    }

    // Log cleanup results if any tiles were removed
    if !tiles_to_remove.is_empty() {
        debug_log!(debug_settings, "Cleaned up {} unused tiles", tiles_to_remove.len());
    }
}

// This system automatically detects and sets the zoom level based on camera height
pub fn auto_detect_zoom_level(
    mut osm_data: ResMut<OSMData>,
    camera_query: Query<&Transform, With<Camera3d>>,
    mut commands: Commands,
    mut _meshes: ResMut<Assets<Mesh>>,
    mut _materials: ResMut<Assets<StandardMaterial>>,
    tokio_runtime: Res<TokioRuntime>,
    debug_settings: Res<DebugSettings>,
    _time: Res<Time>,
) {
    if let Ok(camera_transform) = camera_query.get_single() {
        let camera_height = camera_transform.translation.y;
        let camera_x = camera_transform.translation.x;
        let camera_z = camera_transform.translation.z;

        // Add some hysteresis to prevent oscillation between zoom levels
        // Only change zoom if we're significantly into the new zoom level's range
        let mut new_zoom = osm_data.current_zoom;
        let mut min_height_for_zoom = 0.0;

        // Find the appropriate zoom level based on camera height
        for &(min_height, zoom) in &osm_data.height_thresholds {
            if camera_height >= min_height + 1.0 { // Add 1.0 as hysteresis buffer
                new_zoom = zoom;
                min_height_for_zoom = min_height;
                break;
            }
        }

        // Don't switch back to higher zoom until we're significantly below the threshold
        if new_zoom > osm_data.current_zoom && camera_height < min_height_for_zoom + 3.0 {
            new_zoom = osm_data.current_zoom;
        }

        // Preload tiles for the next potential zoom level
        // This helps make transitions smoother by starting to load next zoom level
        // tiles before we actually change levels
        let next_potential_zoom = if camera_height > min_height_for_zoom + min_height_for_zoom * 0.7 {
            // Going up, so maybe need to load lower zoom level (less detail)
            if osm_data.current_zoom > MIN_ZOOM_LEVEL { osm_data.current_zoom - 1 } else { osm_data.current_zoom }
        } else if camera_height < min_height_for_zoom + min_height_for_zoom * 0.3 {
            // Going down, so maybe need to load higher zoom level (more detail)
            if osm_data.current_zoom < MAX_ZOOM_LEVEL { osm_data.current_zoom + 1 } else { osm_data.current_zoom }
        } else {
            osm_data.current_zoom // Stay at current zoom
        };

        // Only preload if it's a different zoom than current but not the one we're actively changing to
        if next_potential_zoom != osm_data.current_zoom && next_potential_zoom != new_zoom {
            // Preload just the center tile at the potential next zoom level
            let (center_x, center_y) = world_to_tile_coords(camera_x, camera_z, next_potential_zoom);

            // Check if tile is already loaded or pending
            if !osm_data.loaded_tiles.contains(&(center_x, center_y, next_potential_zoom)) &&
               !osm_data.pending_tiles.lock().iter().any(|(x, y, z, _)| *x == center_x && *y == center_y && *z == next_potential_zoom) {

                // Mark as loaded to prevent duplicate requests
                osm_data.loaded_tiles.push((center_x, center_y, next_potential_zoom));

                // Set up async task to preload this tile
                let pending_tiles = osm_data.pending_tiles.clone();
                let tile = OSMTile::new(center_x, center_y, next_potential_zoom);

                debug_log!(debug_settings, "Preloading tile for potential zoom change: {}, {}, zoom {}", center_x, center_y, next_potential_zoom);

                // Use debug flag for async task
                let debug_mode = debug_settings.debug_mode;

                tokio_runtime.0.spawn(async move {
                    match load_tile_image(&tile).await {
                        Ok(image) => {
                            if debug_mode {
                                info!("Successfully preloaded tile: {}, {}, zoom {}", tile.x, tile.y, tile.z);
                            }
                            pending_tiles.lock().push((tile.x, tile.y, tile.z, Some(image)));
                        },
                        Err(e) => {
                            if debug_mode {
                                info!("Failed to preload tile: {}, {}, zoom {} - Error: {}", tile.x, tile.y, tile.z, e);
                            }
                            pending_tiles.lock().push((tile.x, tile.y, tile.z, None));
                        }
                    }
                });
            }
        }

        // Only change zoom levels if it's been stable for a while
        // This prevents rapid oscillation between zoom levels
        if new_zoom != osm_data.current_zoom {
            let old_zoom = osm_data.current_zoom;
            osm_data.current_zoom = new_zoom;

            debug_log!(debug_settings, "Zoom level changed from {} to {} (camera height: {})",
                  old_zoom, new_zoom, camera_height);

            // Clean up any tiles that are too far from current view
            // This is a more gradual approach than removing all tiles
            let mut tiles_to_remove = Vec::new();
            let (center_x, center_y) = world_to_tile_coords(camera_x, camera_z, new_zoom);

            // Calculate maximum visible distance at this zoom level
            let visible_range = 5; // Increased for smoother transitions

            // Find tiles to remove (those at wrong zoom level)
            // Keep old tiles until new ones load to prevent flashing
            for (i, &(tile_x, tile_y, tile_zoom, entity)) in osm_data.tiles.iter().enumerate() {
                if tile_zoom != new_zoom {
                    // Only remove tiles that are very far from current view
                    // to prevent gaps during loading
                    let (scaled_x, scaled_y) = if tile_zoom > new_zoom {
                        // Converting from higher zoom to lower zoom (e.g., 14 -> 13)
                        // Divide by 2 for each level difference
                        let div = 2_i32.pow(tile_zoom - new_zoom);
                        (tile_x as i32 / div, tile_y as i32 / div)
                    } else {
                        // Converting from lower zoom to higher zoom (e.g., 12 -> 13)
                        // Multiply by 2 for each level difference
                        let mul = 2_i32.pow(new_zoom - tile_zoom);
                        (tile_x as i32 * mul, tile_y as i32 * mul)
                    };

                    if (scaled_x - center_x as i32).abs() > visible_range * 3 ||
                       (scaled_y - center_y as i32).abs() > visible_range * 3 {
                        tiles_to_remove.push((i, entity));
                    }
                }
            }

            // Remove tiles from furthest to closest to avoid index shifting issues
            tiles_to_remove.sort_by(|a, b| b.0.cmp(&a.0));
            for (idx, entity) in tiles_to_remove {
                commands.entity(entity).despawn_recursive();
                osm_data.tiles.remove(idx);
            }

            // Don't clear all loaded tiles - just the ones that are too far from view
            // This helps prevent regenerating tiles that we might need again soon
            let center_coords = (center_x, center_y);
            osm_data.loaded_tiles.retain(|(x, y, z)| {
                if *z != new_zoom {
                    let (scaled_x, scaled_y) = if *z > new_zoom {
                        // Converting from higher zoom to lower zoom
                        let div = 2_i32.pow(*z - new_zoom);
                        (*x as i32 / div, *y as i32 / div)
                    } else {
                        // Converting from lower zoom to higher zoom
                        let mul = 2_i32.pow(new_zoom - *z);
                        (*x as i32 * mul, *y as i32 * mul)
                    };

                    // Keep if close to center
                    let x_diff = (scaled_x - center_coords.0 as i32).abs();
                    let y_diff = (scaled_y - center_coords.1 as i32).abs();

                    x_diff <= (visible_range * 3) &&
                    y_diff <= (visible_range * 3)
                } else {
                    // Keep all tiles at the current zoom level
                    true
                }
            });
        }
    }
} 