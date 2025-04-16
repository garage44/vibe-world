use bevy::prelude::*;
use bevy::input::mouse::MouseMotion;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::runtime::Runtime;
use crate::osm::{OSMTile, load_tile_image, create_tile_mesh, create_fallback_tile_mesh};

mod osm;

// Constants for OSM tile system
const ZOOM_LEVEL: u32 = 13;
const MAX_TILE_INDEX: u32 = (1 << ZOOM_LEVEL) - 1; // 2^zoom - 1

// Groningen, Netherlands approximate coordinates at zoom level 13
// OSM tile coordinates at zoom level 13: x=4216, y=2668
const GRONINGEN_X: u32 = 4216;
const GRONINGEN_Y: u32 = 2668;

#[derive(Resource)]
struct OSMData {
    tiles: Vec<(u32, u32, Entity)>,
    loaded_tiles: Vec<(u32, u32)>,
    pending_tiles: Arc<Mutex<Vec<(u32, u32, Option<image::DynamicImage>)>>>,
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

    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(OSMData {
            tiles: Vec::new(),
            loaded_tiles: Vec::new(),
            pending_tiles: Arc::new(Mutex::new(Vec::new())),
        })
        .insert_resource(TokioRuntime(runtime))
        .insert_resource(MouseLookState::default())
        .add_systems(Startup, (setup, grab_mouse))
        .add_systems(Update, (camera_movement, mouse_look_system, toggle_cursor_grab, process_tiles, apply_pending_tiles, debug_info))
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
    info!("Zoom level: {}, MAX_TILE_INDEX: {}", ZOOM_LEVEL, MAX_TILE_INDEX);
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
    let movement_speed = 5.0;
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
    if keyboard_input.pressed(KeyCode::ShiftLeft) {
        movement.y -= 1.0;
    }

    // Normalize movement vector if it's not zero
    if movement != Vec3::ZERO {
        movement = movement.normalize();
    }

    // Apply movement to position
    transform.translation += movement * movement_speed * delta;
}

// Debug system to print information about loaded tiles
fn debug_info(
    osm_data: Res<OSMData>,
    time: Res<Time>,
    camera_query: Query<&Transform, With<Camera3d>>,
) {
    if time.elapsed_secs() % 2.0 < 0.01 {  // Print every 2 seconds
        let tile_count = osm_data.tiles.len();
        let pending_count = osm_data.pending_tiles.lock().len();
        let loaded_count = osm_data.loaded_tiles.len();

        if let Ok(camera_transform) = camera_query.get_single() {
            let pos = camera_transform.translation;
            info!("Camera position: {:?}", pos);

            // Show which tile we're currently over
            let (tile_x, tile_y) = world_to_tile_coords(pos.x, pos.z);
            info!("Current tile: {}, {} at zoom {}", tile_x, tile_y, ZOOM_LEVEL);
        }

        // Print some info about loaded tiles if we have any
        if tile_count > 0 {
            let sample_tiles = osm_data.tiles.iter().take(3).collect::<Vec<_>>();
            info!("Sample tiles: {:?}", sample_tiles.iter().map(|(x, y, _)| (*x, *y)).collect::<Vec<_>>());
        }

        info!("Tiles: {} loaded, {} pending, {} created",
            loaded_count, pending_count, tile_count);
    }
}

// Convert camera world coordinates to OSM tile coordinates
fn world_to_tile_coords(x: f32, z: f32) -> (u32, u32) {
    // OSM tile coordinate system has (0,0) at northwest corner
    // X increases eastward, Y increases southward
    // Our world coordinate system has:
    // - X increases eastward (same as OSM X)
    // - Z increases southward (maps directly to OSM Y)

    // Get the tile X coordinate (same axis direction in both systems)
    let tile_x = x.floor() as u32;

    // Get the tile Y coordinate (Z in world space maps directly to Y in OSM)
    let tile_y = z.floor() as u32;

    // Clamp to valid tile range
    let tile_x = tile_x.clamp(0, MAX_TILE_INDEX);
    let tile_y = tile_y.clamp(0, MAX_TILE_INDEX);

    (tile_x, tile_y)
}

// This system checks for needed tiles and spawns async tasks to load them
fn process_tiles(
    mut osm_data: ResMut<OSMData>,
    tokio_runtime: Res<TokioRuntime>,
    camera_query: Query<&Transform, With<Camera3d>>,
) {
    if let Ok(camera_transform) = camera_query.get_single() {
        let camera_x = camera_transform.translation.x;
        let camera_z = camera_transform.translation.z;

        let (center_tile_x, center_tile_y) = world_to_tile_coords(camera_x, camera_z);

        // Calculate visible tile range - increased for better coverage
        let visible_range = 3;

        for x_offset in -visible_range..=visible_range {
            for y_offset in -visible_range..=visible_range {
                let tile_x = (center_tile_x as i32 + x_offset).clamp(0, MAX_TILE_INDEX as i32) as u32;
                let tile_y = (center_tile_y as i32 + y_offset).clamp(0, MAX_TILE_INDEX as i32) as u32;

                // Check if tile is already loaded or pending
                if !osm_data.loaded_tiles.contains(&(tile_x, tile_y)) &&
                   !osm_data.pending_tiles.lock().iter().any(|(x, y, _)| *x == tile_x && *y == tile_y) {

                    // Mark as loaded to prevent duplicate requests
                    osm_data.loaded_tiles.push((tile_x, tile_y));

                    // Clone the pending_tiles for the async task
                    let pending_tiles = osm_data.pending_tiles.clone();
                    let tile = OSMTile::new(tile_x, tile_y, ZOOM_LEVEL);

                    // Log what we're loading
                    info!("Loading tile: {}, {}", tile_x, tile_y);

                    // Spawn async task to load the tile image using the Tokio runtime
                    tokio_runtime.0.spawn(async move {
                        match load_tile_image(&tile).await {
                            Ok(image) => {
                                info!("Successfully loaded tile: {}, {}", tile.x, tile.y);
                                pending_tiles.lock().push((tile.x, tile.y, Some(image)));
                            },
                            Err(e) => {
                                info!("Failed to load tile: {}, {} - using fallback. Error: {}", tile.x, tile.y, e);
                                pending_tiles.lock().push((tile.x, tile.y, None)); // None means use fallback
                            }
                        }
                    });
                }
            }
        }
    }
}

// This system processes any pending tiles and creates entities for them
fn apply_pending_tiles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut osm_data: ResMut<OSMData>,
) {
    // Take pending tiles
    let mut pending = osm_data.pending_tiles.lock();
    let pending_tiles: Vec<_> = pending.drain(..).collect();
    drop(pending);

    // Process each pending tile
    for (x, y, image_opt) in pending_tiles {
        let tile = OSMTile::new(x, y, ZOOM_LEVEL);

        // Create entity with either the loaded image or a fallback
        let entity = match image_opt {
            Some(image) => {
                info!("Creating entity for tile: {}, {}", x, y);
                create_tile_mesh(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &tile,
                    image,
                )
            },
            None => {
                info!("Creating fallback entity for tile: {}, {}", x, y);
                create_fallback_tile_mesh(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &tile,
                )
            }
        };

        // Store the entity
        osm_data.tiles.push((x, y, entity));
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
