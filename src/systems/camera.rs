use bevy::prelude::*;
use bevy::input::mouse::MouseMotion;
use crate::resources::MouseLookState;

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

    // Calculate altitude-based speed multiplier
    // As camera height increases, speed increases proportionally
    let height = transform.translation.y.max(1.0); // Ensure minimum height of 1.0
    let altitude_factor = {
        if height <= 5.0 {
            1.0 // Base speed at low heights
        } else if height <= 20.0 {
            // Linear scaling for medium heights: 1.0 - 4.0x
            1.0 + (height - 5.0) / 5.0
        } else if height <= 50.0 {
            // Medium-high altitudes: 4.0 - 8.0x
            4.0 + (height - 20.0) / 10.0
        } else if height <= 100.0 {
            // High altitudes: 8.0 - 15.0x
            8.0 + (height - 50.0) / 10.0
        } else {
            // Very high altitudes: 15.0x and above
            15.0 + (height - 100.0) / 20.0
        }
    };

    // Check if boost mode (Shift) is active
    let boost = if keyboard_input.pressed(KeyCode::ShiftLeft) {
        boost_multiplier
    } else {
        1.0
    };

    // Calculate final movement speed using both altitude and boost factors
    let movement_speed = base_movement_speed * altitude_factor * boost;

    // Apply movement to position
    transform.translation += movement * movement_speed * delta;
} 