use std::collections::{HashMap, VecDeque};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;
use reqwest::Client;

use crate::tile_system::types::{TileId, TileError};
use crate::tile_system::cache::TileCache;

/// Maximum number of concurrent downloads
const MAX_CONCURRENT_DOWNLOADS: usize = 8;

/// Event emitted when a tile is loaded (successfully or with an error)
#[derive(Event)]
pub struct TileLoadedEvent {
    /// The ID of the tile that was loaded
    pub tile_id: TileId,
    /// The data of the loaded tile, or an error if loading failed
    pub data: Result<Vec<u8>, TileError>,
}

/// Struct to manage the loading of tiles from a tile server
#[derive(Resource)]
pub struct TileLoader {
    /// HTTP client for making requests
    client: Client,
    /// Currently active downloads, mapped from TileId to the async task
    active_downloads: HashMap<TileId, Task<Result<Vec<u8>, TileError>>>,
    /// Queue of tiles to be downloaded
    download_queue: VecDeque<TileId>,
    /// Set of pending tiles (both in active_downloads and download_queue)
    pending_tiles: HashMap<TileId, ()>,
    /// Base URL for the tile server
    tile_server_url: String,
}

impl Default for TileLoader {
    fn default() -> Self {
        Self {
            client: Client::new(),
            active_downloads: HashMap::new(),
            download_queue: VecDeque::new(),
            pending_tiles: HashMap::new(),
            tile_server_url: "https://tile.openstreetmap.org".to_string(),
        }
    }
}

impl TileLoader {
    /// Create a new TileLoader with the default tile server
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Queue a single tile for download
    pub fn queue_tile(&mut self, tile_id: TileId) {
        // Skip if already downloading or queued
        if self.pending_tiles.contains_key(&tile_id) {
            return;
        }
        
        // Add to queue and pending set
        self.download_queue.push_back(tile_id);
        self.pending_tiles.insert(tile_id, ());
    }
    
    /// Queue multiple tiles for download
    pub fn queue_tiles(&mut self, tile_ids: impl IntoIterator<Item = TileId>) {
        for tile_id in tile_ids {
            self.queue_tile(tile_id);
        }
    }
    
    /// Start downloads for queued tiles, up to the maximum concurrent limit
    pub fn start_downloads(&mut self) {
        while self.active_downloads.len() < MAX_CONCURRENT_DOWNLOADS && !self.download_queue.is_empty() {
            if let Some(tile_id) = self.download_queue.pop_front() {
                let client = self.client.clone();
                let url = format!("{}/{}/{}/{}.png", 
                    self.tile_server_url,
                    tile_id.zoom,
                    tile_id.x,
                    tile_id.y
                );
                
                // Create and spawn the download task
                let task = AsyncComputeTaskPool::get().spawn(async move {
                    match client.get(&url).send().await {
                        Ok(response) => {
                            if response.status().is_success() {
                                match response.bytes().await {
                                    Ok(bytes) => Ok(bytes.to_vec()),
                                    Err(_) => Err(TileError::DownloadFailed),
                                }
                            } else if response.status().as_u16() == 404 {
                                Err(TileError::NotFound)
                            } else {
                                Err(TileError::DownloadFailed)
                            }
                        },
                        Err(_) => Err(TileError::DownloadFailed),
                    }
                });
                
                self.active_downloads.insert(tile_id, task);
            }
        }
    }
    
    /// Process completed downloads, returning a list of (TileId, Result<Vec<u8>, TileError>)
    pub fn process_completed_downloads(&mut self) -> Vec<(TileId, Result<Vec<u8>, TileError>)> {
        let mut completed = Vec::new();
        let mut done_ids = Vec::new();
        
        // Check each active download for completion
        for (tile_id, task) in &mut self.active_downloads {
            if let Some(result) = future::block_on(future::poll_once(task)) {
                done_ids.push(*tile_id);
                completed.push((*tile_id, result));
            }
        }
        
        // Remove completed downloads
        for tile_id in done_ids {
            self.active_downloads.remove(&tile_id);
            self.pending_tiles.remove(&tile_id);
        }
        
        completed
    }
    
    /// Set the tile server URL
    pub fn set_tile_server(&mut self, url: String) {
        self.tile_server_url = url;
    }
    
    /// Get the number of active downloads
    pub fn active_download_count(&self) -> usize {
        self.active_downloads.len()
    }
    
    /// Get the number of queued downloads
    pub fn queued_download_count(&self) -> usize {
        self.download_queue.len()
    }
    
    /// Clear the download queue
    pub fn clear_queue(&mut self) {
        for tile_id in self.download_queue.drain(..) {
            self.pending_tiles.remove(&tile_id);
        }
    }
    
    /// Check if a tile is pending (queued or actively downloading)
    pub fn is_pending(&self, tile_id: &TileId) -> bool {
        self.pending_tiles.contains_key(tile_id)
    }
}

/// Process tile downloads system
pub fn process_tile_downloads(
    mut loader: ResMut<TileLoader>,
    mut tile_loaded_events: EventWriter<TileLoadedEvent>,
) {
    // Start new downloads
    loader.start_downloads();
    
    // Process completed downloads
    let completed = loader.process_completed_downloads();
    
    // Send events for completed downloads
    for (tile_id, result) in completed {
        tile_loaded_events.send(TileLoadedEvent {
            tile_id,
            data: result,
        });
    }
}

/// Plugin for managing tile loading
#[derive(Default)]
pub struct TileLoaderPlugin;

impl Plugin for TileLoaderPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<TileLoader>()
            .add_event::<TileLoadedEvent>()
            .add_systems(Update, process_tile_downloads);
    }
} 