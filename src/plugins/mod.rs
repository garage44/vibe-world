pub mod core_plugin;
pub mod tiles_plugin;
pub mod camera_plugin;
pub mod interaction_plugin;
pub mod ui_plugin;

use bevy::prelude::*;
use bevy::app::PluginGroupBuilder;

pub use core_plugin::CorePlugin;
pub use tiles_plugin::TilesPlugin;
pub use camera_plugin::CameraPlugin;
pub use interaction_plugin::InteractionPlugin;
pub use ui_plugin::UIPlugin;

/// Consolidated plugin struct that groups all application plugins
pub struct AppPlugins;

impl PluginGroup for AppPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(CorePlugin)
            .add(CameraPlugin)
            .add(TilesPlugin)
            .add(InteractionPlugin)
            .add(UIPlugin)
    }
} 