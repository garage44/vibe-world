use bevy::prelude::*;
use std::path::Path;
use std::fs;
use std::io;
use std::time::Duration;
use reqwest::Client;
use image::DynamicImage;
use crate::osm::tile::OSMTile;

// Initialize the tile cache system
pub fn init_tile_cache() -> io::Result<()> {
    let cache_dir = Path::new("tile_cache");
    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir)?;
        info!("Created tile cache directory: {}", cache_dir.display());
    }
    Ok(())
}

// Try to load a tile from the cache
pub fn load_tile_from_cache(tile: &OSMTile) -> Option<DynamicImage> {
    let cache_path = tile.get_cache_path();

    if cache_path.exists() {
        match image::open(&cache_path) {
            Ok(img) => {
                info!("Loaded tile {},{},{} from cache", tile.x, tile.y, tile.z);
                return Some(img);
            },
            Err(e) => {
                warn!("Failed to load cached tile: {}", e);
                // Try to remove corrupt cache file
                let _ = fs::remove_file(&cache_path);
            }
        }
    }

    None
}

// Save a tile to the cache
pub fn save_tile_to_cache(tile: &OSMTile, image: &DynamicImage) {
    let cache_path = tile.get_cache_path();

    match image.save(&cache_path) {
        Ok(_) => info!("Saved tile {},{},{} to cache", tile.x, tile.y, tile.z),
        Err(e) => warn!("Failed to cache tile: {}", e),
    }
}

pub async fn load_tile_image(tile: &OSMTile) -> Result<DynamicImage, anyhow::Error> {
    // First try loading from cache
    if let Some(cached_image) = load_tile_from_cache(tile) {
        return Ok(cached_image);
    }

    // If not in cache, fetch from network
    info!("Tile not in cache, fetching from network: {},{},{}", tile.x, tile.y, tile.z);

    // Create a client with proper user agent and timeout
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("bevy_osm_viewer/0.1.0 (github.com/user/bevy_osm_viewer)")
        .build()?;

    let url = tile.get_url();
    info!("Requesting OSM tile URL: {}", url);

    // Attempt to load the tile with better error handling
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        error!("Failed to load tile {},{} - HTTP status: {}", tile.x, tile.y, response.status());
        return Err(anyhow::anyhow!("HTTP error: {}", response.status()));
    }

    let bytes = response.bytes().await?;
    info!("Received {} bytes for tile {},{}", bytes.len(), tile.x, tile.y);

    let image = image::load_from_memory(&bytes)?;
    info!("Image loaded: {}x{}", image.width(), image.height());

    // Save to cache
    save_tile_to_cache(tile, &image);

    Ok(image)
} 