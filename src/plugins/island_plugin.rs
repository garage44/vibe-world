use bevy::prelude::*;
use crate::systems::interaction::interact_with_islands;

/// Plugin for managing persistent islands
pub struct IslandPlugin;

impl Plugin for IslandPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, interact_with_islands);
    }
} 