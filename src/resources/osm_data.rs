use bevy::prelude::*;
use std::sync::Arc;
use parking_lot::Mutex;

#[derive(Resource)]
pub struct OSMData {
    pub tiles: Vec<(u32, u32, u32, Entity)>, // (x, y, zoom, entity)
    pub background_tiles: Vec<(u32, u32, u32, Entity)>, // (x, y, zoom, entity) for low-res background
    pub loaded_tiles: Vec<(u32, u32, u32)>,  // (x, y, zoom)
    pub loaded_background_tiles: Vec<(u32, u32, u32)>,  // (x, y, zoom) for background
    pub pending_tiles: Arc<Mutex<Vec<(u32, u32, u32, Option<image::DynamicImage>, bool)>>>, // (x, y, zoom, image, is_background)
    pub current_zoom: u32,
    pub background_zoom: u32, // Zoom level for background tiles
    pub height_thresholds: Vec<(f32, u32)>, // (min_height, zoom_level)
    pub total_time: f32, // Track total time for garbage collection
} 