use bevy::prelude::*;
use crate::systems::{
    camera::{mouse_look_system, camera_movement},
    window::{grab_mouse, toggle_cursor_grab},
    debug::debug_info,
};

/// Plugin for camera movement and control
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, grab_mouse)
            .add_systems(Update, (
                mouse_look_system,
                camera_movement,
                toggle_cursor_grab,
                debug_info,
            ));
    }
} 