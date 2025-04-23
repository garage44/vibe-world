use bevy::prelude::*;

/// Marker component for the UI text that displays the current zoom level
#[derive(Component)]
pub struct ZoomLevelText;

#[derive(Component)]
pub struct TileCountText;

#[derive(Component)]
pub struct FpsCounterText;

#[derive(Component)]
pub struct TileCoords {
    pub x: u32,
    pub y: u32,
    pub zoom: u32,
    pub last_used: f32,
}

#[derive(Component)]
pub struct BackgroundTile; 