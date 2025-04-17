use bevy::prelude::*;
use crate::resources::DebugSettings;
use crate::debug_log;

/// System to handle user interaction with the map
pub fn interact_with_map(
    _keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    debug_settings: Res<DebugSettings>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
) {
    // Only perform actions on mouse click
    if mouse_input.just_pressed(MouseButton::Left) {
        if let Ok((camera_transform, _camera)) = camera_query.get_single() {
            // Get camera position and direction
            let ray_origin = camera_transform.translation;
            let ray_direction = camera_transform.forward();
            
            // Simple ray-plane intersection to find which tile we're looking at
            // Assume tiles are on the y=0 plane
            let t = -ray_origin.y / ray_direction.y;
            if t > 0.0 {
                let hit_point = ray_origin + ray_direction * t;
                debug_log!(debug_settings, "Ray hit ground at position: {:?}", hit_point);
                
                // Convert hit point to tile coordinates at the current zoom level
                // (We can add this functionality later if needed)
            } else {
                debug_log!(debug_settings, "Ray didn't hit ground plane - make sure you're looking at the ground");
            }
        } else {
            debug_log!(debug_settings, "Camera not found!");
        }
    }
} 