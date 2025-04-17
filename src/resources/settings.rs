use bevy::prelude::*;

// Resources to help manage persistent islands
#[derive(Resource)]
pub struct PersistentIslandSettings {
    // Whether to highlight persistent islands with a different color
    pub highlight_islands: bool,
    // Whether island editing mode is active
    pub editing_mode: bool,
}

impl Default for PersistentIslandSettings {
    fn default() -> Self {
        Self {
            highlight_islands: false,
            editing_mode: false,
        }
    }
}

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