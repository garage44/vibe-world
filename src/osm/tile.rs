use bevy::prelude::*;
use std::path::{Path, PathBuf};
use std::fs;

// Constants for the OSM tile system
#[allow(dead_code)]
const TILE_SIZE: usize = 256; // Standard OSM tile size in pixels
const CACHE_DIR: &str = "tile_cache"; // Directory for caching tiles

pub struct OSMTile {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

impl OSMTile {
    pub fn new(x: u32, y: u32, z: u32) -> Self {
        Self { x, y, z }
    }

    pub fn get_url(&self) -> String {
        // Use the standard OSM tile server
        // The URL format is zoom/x/y where:
        // - x increases from west to east (0 to 2^zoom-1)
        // - y increases from north to south (0 to 2^zoom-1)
        format!(
            "https://a.tile.openstreetmap.org/{}/{}/{}.png",
            self.z, self.x, self.y
        )
    }

    // Get cache file path for this tile
    pub fn get_cache_path(&self) -> PathBuf {
        let cache_path = Path::new(CACHE_DIR)
            .join(self.z.to_string())
            .join(self.x.to_string());

        fs::create_dir_all(&cache_path).unwrap_or_else(|e| {
            warn!("Failed to create cache directory: {}", e);
        });

        cache_path.join(format!("{}.png", self.y))
    }
}

impl Clone for OSMTile {
    fn clone(&self) -> Self {
        Self {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
} 