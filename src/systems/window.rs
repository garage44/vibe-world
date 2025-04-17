use bevy::prelude::*;

/// Grab the mouse cursor when the app starts
pub fn grab_mouse(mut windows: Query<&mut Window>) {
    if let Ok(mut window) = windows.get_single_mut() {
        window.cursor_options.visible = false;
        window.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
    }
}

/// Toggle cursor grab with Escape key
pub fn toggle_cursor_grab(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        if let Ok(mut window) = windows.get_single_mut() {
            match window.cursor_options.grab_mode {
                bevy::window::CursorGrabMode::None => {
                    window.cursor_options.visible = false;
                    window.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
                    info!("Mouse locked for camera movement");
                }
                _ => {
                    window.cursor_options.visible = true;
                    window.cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
                    info!("Mouse unlocked for UI interaction");
                }
            }
        }
    }
} 