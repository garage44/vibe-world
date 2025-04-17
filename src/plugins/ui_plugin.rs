use bevy::prelude::*;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::time::common_conditions::on_timer;
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
            // PERFORMANCE: Run UI updates less frequently to reduce CPU overhead
            .add_systems(Update, (
                // Update zoom level text and FPS counter at 4 Hz (250ms)
                update_zoom_level_text.run_if(on_timer(std::time::Duration::from_millis(250))),
                update_fps_counter.run_if(on_timer(std::time::Duration::from_millis(250))),
                // Update tile count less frequently as it's less critical
                update_tile_count_text.run_if(on_timer(std::time::Duration::from_millis(500))),
            ));
    }
} 