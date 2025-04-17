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
        // PERFORMANCE: Use a simpler approach for loading with less logging
        match image::open(&cache_path) {
            Ok(img) => {
                // PERFORMANCE: Avoid excessive logging
                debug!("Loaded tile {},{},{} from cache", tile.x, tile.y, tile.z);
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
    
    // PERFORMANCE: Use standard save but with debug logging instead of info
    match image.save(&cache_path) {
        Ok(_) => debug!("Saved tile {},{},{} to cache", tile.x, tile.y, tile.z),
        Err(e) => warn!("Failed to cache tile: {}", e),
    }
}

pub async fn load_tile_image(tile: &OSMTile) -> Result<DynamicImage, anyhow::Error> {
    // First try loading from cache
    if let Some(cached_image) = load_tile_from_cache(tile) {
        return Ok(cached_image);
    }

    // If not in cache, fetch from network
    debug!("Tile not in cache, fetching from network: {},{},{}", tile.x, tile.y, tile.z);

    // PERFORMANCE: Create a shared client with proper settings
    // In a real application, this would be a global client instance
    let client = Client::builder()
        .timeout(Duration::from_secs(5)) // Shorter timeout for better responsiveness
        .user_agent("bevy_osm_viewer/0.1.0 (github.com/user/bevy_osm_viewer)")
        // Enable HTTP/2 for performance
        .http2_prior_knowledge()
        // Set connection pool limits
        .pool_max_idle_per_host(5)
        .build()?;

    // Attempt to fetch the tile with retries
    let mut last_error = None;
    let max_retries = 2; // Original attempt + 1 retry
    
    for attempt in 0..max_retries {
        if attempt > 0 {
            debug!("Retry attempt {} for tile {},{},{}", attempt, tile.x, tile.y, tile.z);
            // Small delay between retries to avoid overwhelming the server
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // Get URL for this attempt (could be different on retries due to subdomain rotation)
        let url = tile.get_url();
        debug!("Requesting OSM tile URL: {}", url);
        
        // Attempt to load the tile
        match client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    // PERFORMANCE: Optimize memory usage by directly working with response bytes
                    match response.bytes().await {
                        Ok(bytes) => {
                            debug!("Received {} bytes for tile {},{}", bytes.len(), tile.x, tile.y);
                            
                            match image::load_from_memory(&bytes) {
                                Ok(image) => {
                                    debug!("Image loaded: {}x{}", image.width(), image.height());
                                    
                                    // Save to cache
                                    save_tile_to_cache(tile, &image);
                                    
                                    return Ok(image);
                                },
                                Err(e) => {
                                    last_error = Some(anyhow::anyhow!("Failed to parse image: {}", e));
                                    // Continue to next retry
                                }
                            }
                        },
                        Err(e) => {
                            last_error = Some(anyhow::anyhow!("Failed to read response bytes: {}", e));
                            // Continue to next retry
                        }
                    }
                } else {
                    last_error = Some(anyhow::anyhow!("HTTP error: {}", response.status()));
                    // Continue to next retry
                }
            },
            Err(e) => {
                last_error = Some(anyhow::anyhow!("Request error: {}", e));
                // Continue to next retry
            }
        }
    }
    
    // If we got here, all attempts failed
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed to load tile after {} attempts", max_retries)))
} 