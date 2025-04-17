use bevy::prelude::*;

mod components;
mod resources;
mod systems;
mod plugins;
mod utils;
mod osm;
mod tile_system;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(plugins::AppPlugins)
        .add_plugins(tile_system::TileSystemPlugin)
        // Uncomment the line below to use the tile system example plugin
        // .add_plugins(tile_system::TileSystemExamplePlugin)
        .run();
}
