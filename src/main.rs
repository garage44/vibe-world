use bevy::prelude::*;
use bevy::input::mouse::MouseMotion;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::runtime::Runtime;
use crate::osm::{OSMTile, load_tile_image, create_tile_mesh, create_fallback_tile_mesh, init_tile_cache};

mod osm;

// Constants for OSM tile system
const DEFAULT_ZOOM_LEVEL: u32 = 13;
const MIN_ZOOM_LEVEL: u32 = 10;  // Furthest zoom out (least detail)
const MAX_ZOOM_LEVEL: u32 = 19;  // Closest zoom in (most detail)

// Calculate MAX_TILE_INDEX dynamically based on zoom level
fn max_tile_index(zoom: u32) -> u32 {
    (1 << zoom) - 1 // 2^zoom - 1
}

// Export the constant for osm.rs to use
pub const MAX_TILE_INDEX: u32 = (1 << MAX_ZOOM_LEVEL) - 1;

// Groningen, Netherlands approximate coordinates at zoom level 13
// OSM tile coordinates at zoom level 13: x=4216, y=2668
const GRONINGEN_X: u32 = 4216;
const GRONINGEN_Y: u32 = 2668;

// Component to mark tiles with their coordinates and zoom level for quick lookups
#[derive(Component)]
struct TileCoords {
    x: u32,
    y: u32,
    zoom: u32,
    last_used: f32, // Time when this tile was last in view
}

#[derive(Resource)]
struct OSMData {
    tiles: Vec<(u32, u32, u32, Entity)>, // (x, y, zoom, entity)
    loaded_tiles: Vec<(u32, u32, u32)>,  // (x, y, zoom)
    pending_tiles: Arc<Mutex<Vec<(u32, u32, u32, Option<image::DynamicImage>)>>>, // (x, y, zoom, image)
    current_zoom: u32,
    height_thresholds: Vec<(f32, u32)>, // (min_height, zoom_level)
    total_time: f32, // Track total time for garbage collection
}

#[derive(Resource)]
struct TokioRuntime(Runtime);

// Add a resource to track mouse motion
#[derive(Resource, Default)]
struct MouseLookState {
    mouse_motion: Vec2,
    pitch: f32,
    yaw: f32,
}

fn main() {
    // Create the Tokio runtime
    let runtime = Runtime::new().expect("Failed to create Tokio runtime");

    // Initialize tile cache
    if let Err(e) = init_tile_cache() {
        eprintln!("Warning: Failed to initialize tile cache: {}", e);
    }

    // Calculate zoom level height thresholds
    // Higher altitude = lower zoom level (less detail but wider area)
    let mut height_thresholds = Vec::new();
    for zoom in MIN_ZOOM_LEVEL..=MAX_ZOOM_LEVEL {
        // More gradual height changes between zoom levels
        // Exponential relationship between height and zoom level
        // Each zoom level increase doubles the detail and halves the view area
        // Use more compressed ranges for higher zoom levels to allow closer zooming
        let min_height = match zoom {
            z if z >= 17 => 3.0 + (z - 17) as f32 * 0.7, // Very close zoom levels
            z if z >= 15 => 5.0 + (z - 15) as f32 * 1.0, // Close zoom levels
            _ => 10.0 * 1.5_f32.powi((DEFAULT_ZOOM_LEVEL as i32 - zoom as i32) as i32), // Standard progression
        };
        height_thresholds.push((min_height, zoom));
    }
    // Sort by height descending (higher altitude = lower zoom)
    height_thresholds.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    // Log the height thresholds for debugging
    for (height, zoom) in &height_thresholds {
        println!("Zoom level {}: min height {}", zoom, height);
    }

    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(OSMData {
            tiles: Vec::new(),
            loaded_tiles: Vec::new(),
            pending_tiles: Arc::new(Mutex::new(Vec::new())),
            current_zoom: DEFAULT_ZOOM_LEVEL,
            height_thresholds,
            total_time: 0.0,
        })
        .insert_resource(TokioRuntime(runtime))
        .insert_resource(MouseLookState::default())
        .add_systems(Startup, (setup, grab_mouse))
        .add_systems(Update, (
            camera_movement,
            mouse_look_system,
            toggle_cursor_grab,
            auto_detect_zoom_level,
            process_tiles,
            apply_pending_tiles,
            update_visible_tiles,
            cleanup_old_tiles,
            debug_info,
        ))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Calculate world coordinates for Groningen location
    // With our new coordinate system:
    // - X = OSM tile X (increasing eastward)
    // - Z = OSM tile Y (increasing southward)
    let world_x = GRONINGEN_X as f32;
    let world_z = GRONINGEN_Y as f32;  // Direct mapping now, no need to invert

    // Camera - positioned slightly elevated with a first-person view
    // Position at Groningen coordinates
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(world_x, 5.0, world_z) // Raised camera height for better overview
            .looking_at(Vec3::new(world_x, 0.0, world_z), Vec3::Y),
    ));

    // Main light - directional to simulate sunlight
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(10.0, 10.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Add ambient light for better visibility
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.5,
    });

    // Add a ground plane for reference
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(1000.0, 1000.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.3, 0.3),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(world_x, -0.01, world_z), // Position at camera center
    ));

    // Log current position for debugging (console only)
    info!("Starting at world position: ({}, {})", world_x, world_z);
    info!("Corresponding to OSM tile: ({}, {})", GRONINGEN_X, GRONINGEN_Y);
    info!("Zoom level: {}, MAX_TILE_INDEX: {}", DEFAULT_ZOOM_LEVEL, MAX_TILE_INDEX);
}

// System to capture mouse movement for camera look
fn mouse_look_system(
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut mouse_look_state: ResMut<MouseLookState>,
) {
    for event in mouse_motion_events.read() {
        mouse_look_state.mouse_motion += event.delta;
    }
}

fn camera_movement(
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut mouse_look_state: ResMut<MouseLookState>,
    mut query: Query<&mut Transform, With<Camera3d>>,
) {
    // Movement settings
    let base_movement_speed = 5.0;
    let boost_multiplier = 3.0; // Speed multiplier when shift is pressed
    let look_sensitivity = 0.002;
    let delta = time.delta_secs();

    // Apply mouse motion to update camera rotation (looking around)
    if !mouse_look_state.mouse_motion.is_nan() && mouse_look_state.mouse_motion.length_squared() > 0.0 {
        // Update pitch and yaw based on mouse motion
        mouse_look_state.yaw -= mouse_look_state.mouse_motion.x * look_sensitivity;
        mouse_look_state.pitch -= mouse_look_state.mouse_motion.y * look_sensitivity;

        // Clamp pitch to prevent the camera from flipping
        mouse_look_state.pitch = mouse_look_state.pitch.clamp(-1.5, 1.5);

        // Reset motion for next frame
        mouse_look_state.mouse_motion = Vec2::ZERO;
    }

    // Apply rotation to camera transform
    let mut transform = query.single_mut();

    // Create rotation quaternion from pitch and yaw
    let yaw_rotation = Quat::from_rotation_y(mouse_look_state.yaw);
    let pitch_rotation = Quat::from_rotation_x(mouse_look_state.pitch);

    // Combine rotations and set the camera's rotation
    transform.rotation = yaw_rotation * pitch_rotation;

    // Calculate movement direction based on camera orientation
    let forward = *transform.forward();
    let right = *transform.right();
    let mut movement = Vec3::ZERO;

    // Apply movement based on key input (relative to camera direction)
    if keyboard_input.pressed(KeyCode::KeyW) {
        movement += forward;
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        movement -= forward;
    }
    if keyboard_input.pressed(KeyCode::KeyA) {
        movement -= right;
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        movement += right;
    }

    // Apply up/down movement
    if keyboard_input.pressed(KeyCode::Space) {
        movement.y += 1.0;
    }
    if keyboard_input.pressed(KeyCode::ControlLeft) { // Changed from ShiftLeft to ControlLeft for down movement
        movement.y -= 1.0;
    }

    // Normalize movement vector if it's not zero
    if movement != Vec3::ZERO {
        movement = movement.normalize();
    }

    // Check if boost mode (Shift) is active
    let movement_speed = if keyboard_input.pressed(KeyCode::ShiftLeft) {
        base_movement_speed * boost_multiplier
    } else {
        base_movement_speed
    };

    // Apply movement to position
    transform.translation += movement * movement_speed * delta;
}

// Debug system to print information about loaded tiles
fn debug_info(
    osm_data: Res<OSMData>,
    time: Res<Time>,
    camera_query: Query<&Transform, With<Camera3d>>,
    tile_query: Query<&TileCoords>,
) {
    if time.elapsed_secs() % 2.0 < 0.01 {  // Print every 2 seconds
        let tile_count = osm_data.tiles.len();
        let pending_count = osm_data.pending_tiles.lock().len();
        let loaded_count = osm_data.loaded_tiles.len();
        let current_zoom = osm_data.current_zoom;

        // Count total loaded tiles
        let active_tiles = tile_query.iter().count();

        // Count tiles by zoom level
        let mut zoom_counts = std::collections::HashMap::new();
        for tile in tile_query.iter() {
            *zoom_counts.entry(tile.zoom).or_insert(0) += 1;
        }

        // Create a sorted vec of zoom level counts
        let mut zoom_stats: Vec<_> = zoom_counts.iter().collect();
        zoom_stats.sort_by_key(|&(z, _)| *z);

        if let Ok(camera_transform) = camera_query.get_single() {
            let pos = camera_transform.translation;
            info!("Camera position: {:?}", pos);

            // Show which tile we're currently over
            let (tile_x, tile_y) = world_to_tile_coords(pos.x, pos.z, current_zoom);
            info!("Current tile: {}, {} at zoom {}", tile_x, tile_y, current_zoom);
        }

        // Print some info about loaded tiles if we have any
        if tile_count > 0 {
            let sample_tiles = osm_data.tiles.iter().take(3).collect::<Vec<_>>();
            info!("Sample tiles: {:?}", sample_tiles.iter().map(|(x, y, z, _)| (*x, *y, *z)).collect::<Vec<_>>());
        }

        info!("Tiles: {} active, {} loaded, {} pending, {} tracked, zoom level: {}",
            active_tiles, loaded_count, pending_count, tile_count, current_zoom);

        // Print counts by zoom level
        if !zoom_stats.is_empty() {
            let zoom_info: Vec<String> = zoom_stats
                .iter()
                .map(|&(z, count)| format!("z{}:{}", z, count))
                .collect();
            info!("Tiles by zoom level: {}", zoom_info.join(", "));
        }
    }
}

// Convert camera world coordinates to OSM tile coordinates
fn world_to_tile_coords(x: f32, z: f32, zoom: u32) -> (u32, u32) {
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

// This system checks for needed tiles and spawns async tasks to load them
fn process_tiles(
    mut osm_data: ResMut<OSMData>,
    tokio_runtime: Res<TokioRuntime>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
) {
    if let Ok((camera_transform, _camera)) = camera_query.get_single() {
        let camera_x = camera_transform.translation.x;
        let camera_z = camera_transform.translation.z;
        let current_zoom = osm_data.current_zoom;

        let (center_tile_x, center_tile_y) = world_to_tile_coords(camera_x, camera_z, current_zoom);

        // Calculate visible tile range - adjusted based on zoom level
        // At higher zoom levels, we need fewer surrounding tiles
        let visible_range = match current_zoom {
            z if z >= 17 => 3,  // Fewer tiles at high zoom (more detailed)
            z if z >= 15 => 3,  // Medium range at medium zoom
            _ => 4,             // More tiles at low zoom (less detailed)
        };

        // Generate a list of tile coordinates to load, sorted by distance from center
        let mut tiles_to_load = Vec::new();

        // We'll define our camera frustum more efficiently for culling
        // Forward vector - where the camera is looking
        let forward = camera_transform.forward();
        // Side vector - unused but kept for future use
        let _right = camera_transform.right();

        // Simple frustum culling - check if tile is in camera's field of view
        for x_offset in -visible_range..=visible_range {
            for y_offset in -visible_range..=visible_range {
                let max_index = max_tile_index(current_zoom);
                let tile_x = (center_tile_x as i32 + x_offset).clamp(0, max_index as i32) as u32;
                let tile_y = (center_tile_y as i32 + y_offset).clamp(0, max_index as i32) as u32;

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
                let dot = to_tile.normalize().dot(*forward);

                // Skip tiles outside of viewing angle (behind or far to sides)
                // Increasing this number narrows the viewing angle
                // 0.0 would be 90 degrees to either side
                let frustum_angle = -0.2; // Slightly behind camera to avoid pop-in when turning
                if dot < frustum_angle {
                    continue;
                }

                // Calculate manhattan distance from center for priority
                let distance = x_offset.abs() + y_offset.abs();

                // Add to load queue if not already loaded/pending
                tiles_to_load.push((tile_x, tile_y, distance));
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
                info!("Loading tile: {}, {}, zoom {}", tile_x, tile_y, current_zoom);

                // Spawn async task to load the tile image using the Tokio runtime
                tokio_runtime.0.spawn(async move {
                    match load_tile_image(&tile).await {
                        Ok(image) => {
                            info!("Successfully loaded tile: {}, {}, zoom {}", tile.x, tile.y, tile.z);
                            pending_tiles.lock().push((tile.x, tile.y, tile.z, Some(image)));
                        },
                        Err(e) => {
                            info!("Failed to load tile: {}, {}, zoom {} - using fallback. Error: {}", tile.x, tile.y, tile.z, e);
                            pending_tiles.lock().push((tile.x, tile.y, tile.z, None)); // None means use fallback
                        }
                    }
                });
            }
        }
    }
}

// This system updates which tiles are visible and marks the last time they were seen
fn update_visible_tiles(
    mut q_tiles: Query<(&mut TileCoords, &Transform)>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
    time: Res<Time>,
) {
    if let Ok((camera_transform, _camera)) = camera_query.get_single() {
        for (mut tile_coords, tile_transform) in q_tiles.iter_mut() {
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

// This system periodically cleans up tiles that haven't been visible for a while
fn cleanup_old_tiles(
    mut commands: Commands,
    mut osm_data: ResMut<OSMData>,
    time: Res<Time>,
    q_tiles: Query<(Entity, &TileCoords)>,
) {
    // Update total time
    osm_data.total_time += time.delta_secs();

    // Only run cleanup every 5 seconds to avoid constant checking
    if osm_data.total_time % 5.0 > 0.05 {
        return;
    }

    // How long a tile can be unused before being unloaded (in seconds)
    const TILE_TIMEOUT: f32 = 30.0;
    let current_time = time.elapsed_secs();

    let mut tiles_to_remove = Vec::new();
    let mut indices_to_remove = Vec::new();

    // Check for tiles to remove based on last_used time
    for (entity, tile_coords) in q_tiles.iter() {
        if current_time - tile_coords.last_used > TILE_TIMEOUT {
            tiles_to_remove.push(entity);

            // Find the index in our OSMData.tiles array
            if let Some(idx) = osm_data.tiles.iter().position(|&(x, y, z, e)|
                x == tile_coords.x && y == tile_coords.y && z == tile_coords.zoom && e == entity) {
                indices_to_remove.push(idx);
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
        info!("Cleaned up {} unused tiles", tiles_to_remove.len());
    }
}

// This system processes any pending tiles and creates entities for them
fn apply_pending_tiles(
    mut commands: Commands,
    mut _meshes: ResMut<Assets<Mesh>>,
    mut _materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut osm_data: ResMut<OSMData>,
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

        // Create entity with either the loaded image or a fallback
        let entity = match image_opt {
            Some(image) => {
                info!("Creating entity for tile: {}, {}, zoom {}", x, y, z);
                create_tile_mesh(
                    &mut commands,
                    &mut _meshes,
                    &mut _materials,
                    &mut images,
                    &tile,
                    image,
                )
            },
            None => {
                info!("Creating fallback entity for tile: {}, {}, zoom {}", x, y, z);
                create_fallback_tile_mesh(
                    &mut commands,
                    &mut _meshes,
                    &mut _materials,
                    &tile,
                )
            }
        };

        // Add TileCoords component to the entity for fast lookup and management
        commands.entity(entity).insert(TileCoords {
            x,
            y,
            zoom: z,
            last_used: current_time,
        });

        // Store the entity
        osm_data.tiles.push((x, y, z, entity));
    }
}

// Grab the mouse cursor when the app starts
fn grab_mouse(mut windows: Query<&mut Window>) {
    if let Ok(mut window) = windows.get_single_mut() {
        window.cursor_options.visible = false;
        window.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
    }
}

// Toggle cursor grab with Escape key
fn toggle_cursor_grab(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        if let Ok(mut window) = windows.get_single_mut() {
            match window.cursor_options.grab_mode {
                bevy::window::CursorGrabMode::None => {
                    window.cursor_options.visible = false;
                    window.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
                }
                _ => {
                    window.cursor_options.visible = true;
                    window.cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
                }
            }
        }
    }
}

// This system automatically detects and sets the zoom level based on camera height
fn auto_detect_zoom_level(
    mut osm_data: ResMut<OSMData>,
    camera_query: Query<&Transform, With<Camera3d>>,
    mut commands: Commands,
    mut _meshes: ResMut<Assets<Mesh>>,
    mut _materials: ResMut<Assets<StandardMaterial>>,
    tokio_runtime: Res<TokioRuntime>,
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

                info!("Preloading tile for potential zoom change: {}, {}, zoom {}", center_x, center_y, next_potential_zoom);

                tokio_runtime.0.spawn(async move {
                    match load_tile_image(&tile).await {
                        Ok(image) => {
                            info!("Successfully preloaded tile: {}, {}, zoom {}", tile.x, tile.y, tile.z);
                            pending_tiles.lock().push((tile.x, tile.y, tile.z, Some(image)));
                        },
                        Err(e) => {
                            info!("Failed to preload tile: {}, {}, zoom {} - Error: {}", tile.x, tile.y, tile.z, e);
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

            info!("Zoom level changed from {} to {} (camera height: {})",
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
