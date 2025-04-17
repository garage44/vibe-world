use bevy::prelude::*;
use crate::resources::DebugSettings;

/// Logs a message only when debug mode is enabled
#[allow(dead_code)]
pub fn debug_log(debug_settings: &DebugSettings, message: impl AsRef<str>) {
    if debug_settings.debug_mode {
        info!("{}", message.as_ref());
    }
}

/// Logs a formatted message only when debug mode is enabled
#[macro_export]
macro_rules! debug_log {
    ($debug_settings:expr, $($arg:tt)*) => {
        if $debug_settings.debug_mode {
            bevy::prelude::info!($($arg)*);
        }
    };
} 