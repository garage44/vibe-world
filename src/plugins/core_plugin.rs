use bevy::prelude::*;
use crate::systems::setup::{setup, init_resources};
use crate::resources::{MouseLookState, PersistentIslandSettings, DebugSettings};

/// Core plugin that handles the basic app setup
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        // Initialize resources
        let (osm_data, tokio_runtime) = init_resources();
        
        app
            .insert_resource(osm_data)
            .insert_resource(tokio_runtime)
            .insert_resource(MouseLookState::default())
            .insert_resource(PersistentIslandSettings {
                highlight_islands: false,
                editing_mode: false,
            })
            .insert_resource(DebugSettings::default())
            .add_systems(Startup, setup);
    }
} 