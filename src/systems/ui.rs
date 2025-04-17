use bevy::prelude::*;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use crate::components::{ZoomLevelText, TileCountText, FpsCounterText, TileCoords};
use crate::resources::constants::{resolution_at_zoom_and_latitude, get_scale_for_zoom};
use crate::resources::OSMData;
use crate::systems::tiles;

/// Sets up the UI elements for the game
pub fn setup_ui(mut commands: Commands, _asset_server: Res<AssetServer>) {
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
    diagnostics: Res<DiagnosticsStore>,
    mut text_query: Query<&mut Text, With<FpsCounterText>>,
    time: Res<Time>,
) {
    // PERFORMANCE: Only update the FPS display a few times per second
    // Use the time as a simple frame counter by checking the fractional part
    if (time.elapsed_secs() * 2.0).fract() > 0.1 {
        return; // Only update a few times per second
    }
    
    if let Ok(mut text) = text_query.get_single_mut() {
        // Use the FPS diagnostic from FrameTimeDiagnosticsPlugin
        if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(value) = fps.smoothed() {
                // Update with smoothed FPS value
                text.0 = format!("FPS: {:.1}", value);
            }
        }
    }
}

/// Updates the UI text to show the number of active tiles
pub fn update_tile_count_text_old(
    osm_data: Res<OSMData>,
    mut query: Query<&mut Text, With<TileCountText>>
) {
    if let Ok(mut text) = query.get_single_mut() {
        // Count foreground and background tiles separately
        let fg_count = osm_data.tiles.len();
        let bg_count = osm_data.background_tiles.len();
        let total_count = fg_count + bg_count;
        
        // Count tiles by zoom level
        let mut zoom_counts = std::collections::HashMap::new();
        
        // Count foreground tiles by zoom
        for &(_, _, zoom, _) in &osm_data.tiles {
            *zoom_counts.entry(zoom).or_insert(0) += 1;
        }
        
        // Count background tiles by zoom
        for &(_, _, zoom, _) in &osm_data.background_tiles {
            *zoom_counts.entry(zoom).or_insert(0) += 1;
        }
        
        // Create breakdown of tiles by zoom level
        let mut zoom_breakdown = String::new();
        let mut zoom_keys: Vec<_> = zoom_counts.keys().collect();
        zoom_keys.sort();
        
        for &zoom in zoom_keys {
            zoom_breakdown.push_str(&format!("\nz{}: {}", zoom, zoom_counts[&zoom]));
        }
        
        // Update the text
        text.0 = format!(
            "Tiles: {} ({}fg + {}bg){}",
            total_count,
            fg_count,
            bg_count,
            zoom_breakdown
        );
    }
}

/// Updates the UI text to show the current FPS
pub fn update_fps_text(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsCounterText>>
) {
    if let Ok(mut text) = query.get_single_mut() {
        if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(value) = fps.smoothed() {
                // Update the text with the current FPS
                text.0 = format!("FPS: {:.1}", value);
            }
        }
    }
} 