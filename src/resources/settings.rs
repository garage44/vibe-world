use bevy::prelude::*;

// Resources to help manage persistent islands
#[derive(Resource)]
pub struct PersistentIslandSettings {
    // Whether to highlight persistent islands with a different color
    pub highlight_islands: bool,
    // Whether island editing mode is active
    pub editing_mode: bool,
} 