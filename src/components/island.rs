use bevy::prelude::*;

/// Component to mark a tile as a persistent island (OpenSimulator region)
#[derive(Component)]
pub struct PersistentIsland {
    pub name: String,
    // Add any island-specific data here
    // For example, custom terrain modifications, objects, etc.
} 