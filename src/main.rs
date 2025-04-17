use bevy::prelude::*;

mod components;
mod resources;
mod systems;
mod plugins;
mod utils;
mod osm;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(plugins::AppPlugins)
        .run();
}
