use bevy::prelude::*;
use crate::resources::constants::{DEFAULT_ZOOM_LEVEL, MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL, BACKGROUND_ZOOM_LEVEL, GRONINGEN_X, GRONINGEN_Y, MAX_TILE_INDEX, zoom_level_from_camera_height};
use crate::osm::init_tile_cache;
use crate::resources::{OSMData, TokioRuntime, DebugSettings};
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::runtime::Runtime;
use crate::debug_log;

/// Initialize resources for the application
pub fn init_resources() -> (OSMData, TokioRuntime) {
    // Create the Tokio runtime
    let runtime = Runtime::new().expect("Failed to create Tokio runtime");

    // Initialize tile cache
    if let Err(e) = init_tile_cache() {
        eprintln!("Warning: Failed to initialize tile cache: {}", e);
    }

    // Calculate zoom level height thresholds using the standardized function
    let mut height_thresholds = Vec::new();
    
    // Start from a low height (near the ground) and go up
    // This provides a more balanced distribution following OpenStreetMap conventions
    let heights = [
        1.5, 3.0, 5.0, 8.0, 12.0, 20.0, 35.0, 60.0, 
        100.0, 150.0, 200.0, 250.0, 300.0, 400.0, 500.0, 
        700.0, 1000.0, 1500.0, 2000.0
    ];
    
    for height in heights.iter() {
        let zoom = zoom_level_from_camera_height(*height);
        if zoom >= MIN_ZOOM_LEVEL && zoom <= MAX_ZOOM_LEVEL {
            height_thresholds.push((*height, zoom));
        }
    }
    
    // Sort by height ascending (lower height = higher zoom)
    height_thresholds.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Log the height thresholds for debugging
    for (height, zoom) in &height_thresholds {
        println!("Height {:.1}: zoom level {}", height, zoom);
    }

    let osm_data = OSMData {
        tiles: Vec::new(),
        background_tiles: Vec::new(),
        loaded_tiles: Vec::new(),
        loaded_background_tiles: Vec::new(),
        pending_tiles: Arc::new(Mutex::new(Vec::new())),
        current_zoom: DEFAULT_ZOOM_LEVEL,
        background_zoom: BACKGROUND_ZOOM_LEVEL,
        height_thresholds,
        total_time: 0.0,
    };

    (osm_data, TokioRuntime(runtime))
}

/// Setup the scene with initial camera, lighting, and ground plane
pub fn setup(
    mut commands: Commands,
    _meshes: ResMut<Assets<Mesh>>,
    _materials: ResMut<Assets<StandardMaterial>>,
    debug_settings: Res<DebugSettings>,
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
        PerspectiveProjection {
            fov: std::f32::consts::PI / 2.0, // 90 degrees FOV
            aspect_ratio: 1.0, // Will be updated by Bevy
            near: 0.1,
            far: 10000.0,
        },
        Transform::from_xyz(world_x, 200.0, world_z) // Higher camera for better overview
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

    // Ground plane removed - not needed with tile-based map

    // Log current position for debugging (console only)
    debug_log!(debug_settings, "Starting at world position: ({}, {})", world_x, world_z);
    debug_log!(debug_settings, "Corresponding to OSM tile: ({}, {})", GRONINGEN_X, GRONINGEN_Y);
    debug_log!(debug_settings, "Zoom level: {}, MAX_TILE_INDEX: {}", DEFAULT_ZOOM_LEVEL, MAX_TILE_INDEX);
} 