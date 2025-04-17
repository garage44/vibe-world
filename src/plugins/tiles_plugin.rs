use bevy::prelude::*;
use crate::tile_system::TileSystemPlugin;

/// Plugin for managing OSM tiles
pub struct TilesPlugin;

impl Plugin for TilesPlugin {
    fn build(&self, app: &mut App) {
        // Add the new tile system - it already registers all necessary systems
        app.add_plugins(TileSystemPlugin);
    }
} 