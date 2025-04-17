use bevy::prelude::*;
use crate::components::ZoomLevelText;
use crate::resources::constants::{resolution_at_zoom_and_latitude, get_scale_for_zoom};
use crate::resources::OSMData;

/// Sets up the UI elements
pub fn setup_ui(mut commands: Commands, _asset_server: Res<AssetServer>) {
    // Create a UI camera with higher order to avoid ambiguity with Camera3d
    commands.spawn((
        Camera2d::default(),
        Camera {
            order: 1, // Higher order means it renders on top of the 3D camera
            ..default()
        },
    ));

    // Spawn text to display current zoom level
    commands.spawn((
        // Use the Text component directly
        Text::new("Zoom Level: 0"),
        // Position absolutely
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            bottom: Val::Px(10.0),
            ..default()
        },
        // Add background color for readability
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        // Mark with our custom component
        ZoomLevelText,
    ));
}

/// Updates the UI text to show the current zoom level
pub fn update_zoom_level_text(
    osm_data: Res<OSMData>,
    camera_query: Query<&Transform, With<Camera3d>>,
    mut query: Query<&mut Text, With<ZoomLevelText>>
) {
    if let Ok(mut text) = query.get_single_mut() {
        if let Ok(camera_transform) = camera_query.get_single() {
            let current_zoom = osm_data.current_zoom;
            let camera_height = camera_transform.translation.y;
            
            // Calculate the approximate real-world scale (assuming 96 DPI screen)
            let scale = get_scale_for_zoom(current_zoom, 52.0, 96.0); // 52.0 is roughly latitude of Groningen
            
            // Calculate the resolution in meters per pixel at current zoom
            let resolution = resolution_at_zoom_and_latitude(current_zoom, 52.0);
            
            // Update the text with the current zoom level, camera height, and real-world scale
            *text = Text::new(format!(
                "Zoom Level: {} (Height: {:.1})\nScale: {} (1 pixel ≈ {:.2} m)\nMin: {}, Max: {}",
                current_zoom,
                camera_height,
                scale,
                resolution,
                crate::resources::constants::MIN_ZOOM_LEVEL,
                crate::resources::constants::MAX_ZOOM_LEVEL
            ));
        }
    }
} 