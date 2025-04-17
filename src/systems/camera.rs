use bevy::prelude::*;
use bevy::input::mouse::MouseMotion;
use crate::resources::MouseLookState;
use crate::utils::coordinate_conversion::world_to_tile_coords;
use crate::resources::constants::DEFAULT_ZOOM_LEVEL;

/// System to capture mouse movement for camera look
pub fn mouse_look_system(
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut mouse_look_state: ResMut<MouseLookState>,
) {
    for event in mouse_motion_events.read() {
        mouse_look_state.mouse_motion += event.delta;
    }
}

pub fn camera_movement(
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut mouse_look_state: ResMut<MouseLookState>,
    mut query: Query<&mut Transform, With<Camera3d>>,
) {
    // Movement settings
    let base_movement_speed = 5.0;
    let boost_multiplier = 3.0; // Speed multiplier when shift is pressed
    let look_sensitivity = 0.002;
    let delta = time.delta_secs();

    // Apply mouse motion to update camera rotation (looking around)
    if !mouse_look_state.mouse_motion.is_nan() && mouse_look_state.mouse_motion.length_squared() > 0.0 {
        // Update pitch and yaw based on mouse motion
        mouse_look_state.yaw -= mouse_look_state.mouse_motion.x * look_sensitivity;
        mouse_look_state.pitch -= mouse_look_state.mouse_motion.y * look_sensitivity;

        // Clamp pitch to prevent the camera from flipping
        mouse_look_state.pitch = mouse_look_state.pitch.clamp(-1.5, 1.5);

        // Reset motion for next frame
        mouse_look_state.mouse_motion = Vec2::ZERO;
    }

    // Apply rotation to camera transform
    let mut transform = query.single_mut();

    // Create rotation quaternion from pitch and yaw
    let yaw_rotation = Quat::from_rotation_y(mouse_look_state.yaw);
    let pitch_rotation = Quat::from_rotation_x(mouse_look_state.pitch);

    // Combine rotations and set the camera's rotation
    transform.rotation = yaw_rotation * pitch_rotation;

    // Calculate movement direction based on camera orientation
    let forward = *transform.forward();
    let right = *transform.right();
    let mut movement = Vec3::ZERO;

    // Apply movement based on key input (relative to camera direction)
    if keyboard_input.pressed(KeyCode::KeyW) {
        movement += forward;
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        movement -= forward;
    }
    if keyboard_input.pressed(KeyCode::KeyA) {
        movement -= right;
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        movement += right;
    }

    // Apply up/down movement
    if keyboard_input.pressed(KeyCode::Space) {
        movement.y += 1.0;
    }
    if keyboard_input.pressed(KeyCode::ControlLeft) { // Changed from ShiftLeft to ControlLeft for down movement
        movement.y -= 1.0;
    }

    // Normalize movement vector if it's not zero
    if movement != Vec3::ZERO {
        movement = movement.normalize();
    }

    // Check if boost mode (Shift) is active
    let movement_speed = if keyboard_input.pressed(KeyCode::ShiftLeft) {
        base_movement_speed * boost_multiplier
    } else {
        base_movement_speed
    };

    // Apply movement to position
    transform.translation += movement * movement_speed * delta;
} 