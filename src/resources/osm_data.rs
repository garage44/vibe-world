use bevy::prelude::*;
use std::sync::Arc;
use parking_lot::Mutex;
use std::collections::HashMap;
use crate::components::PersistentIsland;

#[derive(Resource)]
pub struct OSMData {
    pub tiles: Vec<(u32, u32, u32, Entity)>, // (x, y, zoom, entity)
    pub loaded_tiles: Vec<(u32, u32, u32)>,  // (x, y, zoom)
    pub pending_tiles: Arc<Mutex<Vec<(u32, u32, u32, Option<image::DynamicImage>)>>>, // (x, y, zoom, image)
    pub current_zoom: u32,
    pub height_thresholds: Vec<(f32, u32)>, // (min_height, zoom_level)
    pub total_time: f32, // Track total time for garbage collection
    // Map of persistent islands by their coordinates at PERSISTENT_ISLAND_ZOOM_LEVEL
    pub persistent_islands: HashMap<(u32, u32), PersistentIsland>,
} 