mod tiles_plugin;
mod camera_plugin;
mod island_plugin;
mod core_plugin;

use bevy::prelude::*;
use bevy::app::PluginGroupBuilder;
use tiles_plugin::TilesPlugin;
use camera_plugin::CameraPlugin;
use island_plugin::IslandPlugin;
use core_plugin::CorePlugin;

/// Consolidated plugin struct that groups all application plugins
pub struct AppPlugins;

impl PluginGroup for AppPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(CorePlugin)
            .add(CameraPlugin)
            .add(TilesPlugin)
            .add(IslandPlugin)
    }
} 