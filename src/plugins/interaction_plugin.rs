use bevy::prelude::*;
use crate::systems::interaction::interact_with_map;

/// Plugin for map interaction
pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, interact_with_map);
    }
} 