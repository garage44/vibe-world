use bevy::prelude::*;
use crate::systems::tiles::{
    process_tiles,
    apply_pending_tiles,
    update_visible_tiles,
    cleanup_old_tiles,
    auto_detect_zoom_level,
};

/// Plugin for managing OSM tiles
pub struct TilesPlugin;

impl Plugin for TilesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
            process_tiles,
            apply_pending_tiles,
            update_visible_tiles,
            cleanup_old_tiles,
            auto_detect_zoom_level,
        ));
    }
} 