use bevy::prelude::*;
use crate::resources::constants::{DEFAULT_ZOOM_LEVEL, MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL, GRONINGEN_X, GRONINGEN_Y, MAX_TILE_INDEX};
use crate::osm::init_tile_cache;
use crate::resources::{OSMData, TokioRuntime};
use std::sync::Arc;
use parking_lot::Mutex;
use std::collections::HashMap;
use tokio::runtime::Runtime;

/// Initialize resources for the application
pub fn init_resources() -> (OSMData, TokioRuntime) {
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

    let osm_data = OSMData {
        tiles: Vec::new(),
        loaded_tiles: Vec::new(),
        pending_tiles: Arc::new(Mutex::new(Vec::new())),
        current_zoom: DEFAULT_ZOOM_LEVEL,
        height_thresholds,
        total_time: 0.0,
        persistent_islands: HashMap::new(),
    };

    (osm_data, TokioRuntime(runtime))
}

/// Setup the scene with initial camera, lighting, and ground plane
pub fn setup(
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