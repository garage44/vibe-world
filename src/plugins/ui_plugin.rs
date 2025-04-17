use bevy::prelude::*;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use crate::systems::ui::{setup_ui, update_zoom_level_text, update_tile_count_text, update_fps_counter};

/// Plugin for managing UI elements like text displays
pub struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app
            // Add diagnostics for FPS tracking
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            // Add UI setup and update systems
            .add_systems(Startup, setup_ui)
            .add_systems(Update, (
                update_zoom_level_text,
                update_tile_count_text,
                update_fps_counter,
            ));
    }
} 