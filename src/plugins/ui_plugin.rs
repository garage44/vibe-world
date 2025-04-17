use bevy::prelude::*;
use crate::systems::ui::{setup_ui, update_zoom_level_text};

/// Plugin for managing UI elements like text displays
pub struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app
            // Add UI setup and update systems
            .add_systems(Startup, setup_ui)
            .add_systems(Update, update_zoom_level_text);
    }
} 