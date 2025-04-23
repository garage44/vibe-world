use bevy::prelude::*;
use crate::components::{ZoomLevelText, TileCountText, FpsCounterText, TileCoords};
use crate::systems::tiles;

/// Sets up the UI elements for the game
pub fn setup_ui(mut commands: Commands) {
    // UI camera with higher order value to ensure it renders on top
    commands.spawn((
        Camera2d,
        // Use a higher order value for the UI camera to render on top of the 3D camera
        Camera {
            order: 1, // Higher than the default 0 for the 3D camera
            ..default()
        },
    ));
    
    // Spawn zoom level text (top left)
    commands.spawn((
        Text::new("Zoom: 0"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        // Set a background color to make text more visible
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        ZoomLevelText,
    ));
    
    // Spawn tile count text (below zoom level)
    commands.spawn((
        Text::new("Tiles: 0"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(40.0),
            left: Val::Px(10.0),
            ..default()
        },
        // Set a background color to make text more visible
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        TileCountText,
    ));
    
    // Spawn FPS counter text (below tile count)
    commands.spawn((
        Text::new("FPS: 0"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(70.0),
            left: Val::Px(10.0),
            ..default()
        },
        // Set a background color to make text more visible
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        FpsCounterText,
    ));
}

/// Updates the zoom level text based on the camera's current position
pub fn update_zoom_level_text(
    mut text_query: Query<&mut Text, With<ZoomLevelText>>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
) {
    let (transform, _) = if let Ok(cam) = camera_query.get_single() {
        cam
    } else {
        return;
    };

    // Function is in the same module, we can access it directly
    let zoom_level = tiles::calculate_base_zoom_level(transform.translation.y);

    if let Ok(mut text) = text_query.get_single_mut() {
        text.0 = format!("Zoom: {}", zoom_level);
    }
}

/// Updates the tile count text with the number of tiles currently in the scene
pub fn update_tile_count_text(
    mut text_query: Query<&mut Text, With<TileCountText>>,
    tile_query: Query<&TileCoords>,
) {
    let tile_count = tile_query.iter().count();
    
    if let Ok(mut text) = text_query.get_single_mut() {
        text.0 = format!("Tiles: {}", tile_count);
    }
}

/// Updates the FPS counter text
pub fn update_fps_counter(
    mut text_query: Query<&mut Text, With<FpsCounterText>>,
    time: Res<Time>,
) {
    let fps = 1.0 / time.delta_secs();
    
    if let Ok(mut text) = text_query.get_single_mut() {
        text.0 = format!("FPS: {:.1}", fps);
    }
}
