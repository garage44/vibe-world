use bevy::prelude::*;

// Settings for debug display
#[derive(Resource)]
pub struct DebugSettings {
    pub debug_mode: bool,
}

impl Default for DebugSettings {
    fn default() -> Self {
        Self {
            debug_mode: false,
        }
    }
} 