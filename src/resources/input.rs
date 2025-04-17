use bevy::prelude::*;

// Resource to track mouse motion
#[derive(Resource, Default)]
pub struct MouseLookState {
    pub mouse_motion: Vec2,
    pub pitch: f32,
    pub yaw: f32,
} 