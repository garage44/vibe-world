use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
// use futures_lite::future;
use crate::tile_system::types::*;
use crate::tile_system::loader::{TileLoader, TileLoadedEvent};
use crate::tile_system::cache::TileCache;
use crate::tile_system::quadtree::TileQuadtree;
// use std::sync::{Arc, Mutex};
use std::collections::{VecDeque, HashSet};

/// Maximum batch size for tile downloads
const MAX_BATCH_SIZE: usize = 64;

/// Component to track download tasks for tiles
#[derive(Component)]
pub struct TileDownloadTask {
    /// The ID of the tile being downloaded
    pub id: TileId,
    /// The task that's downloading the tile
    pub task: Task<Result<TileLoadResult, TileError>>,
}

/// Event triggered when a batch of tiles is requested for download
#[derive(Event)]
pub struct TileDownloadBatchEvent {
    /// The tiles to be downloaded
    pub tiles: Vec<TileId>,
}

/// Resource that manages the queue of tiles to be downloaded
#[derive(Resource)]
pub struct TileDownloadQueue {
    /// Tiles that are queued for download
    queued: HashSet<TileId>,
    /// Tiles that are currently being downloaded
    downloading: HashSet<TileId>,
    /// Queue of tiles to download
    queue: VecDeque<TileId>,
}

impl Default for TileDownloadQueue {
    fn default() -> Self {
        Self {
            queued: HashSet::new(),
            downloading: HashSet::new(),
            queue: VecDeque::new(),
        }
    }
}

impl TileDownloadQueue {
    /// Create a new, empty download queue
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a tile for download if not already queued or downloading
    pub fn queue(&mut self, tile_id: TileId) {
        if !self.queued.contains(&tile_id) && !self.downloading.contains(&tile_id) {
            self.queued.insert(tile_id);
            self.queue.push_back(tile_id);
        }
    }

    /// Get the next batch of tiles to download, up to MAX_BATCH_SIZE
    pub fn get_next_batch(&mut self) -> Vec<TileId> {
        let mut batch = Vec::new();
        
        while batch.len() < MAX_BATCH_SIZE && !self.queue.is_empty() {
            if let Some(tile_id) = self.queue.pop_front() {
                self.queued.remove(&tile_id);
                self.downloading.insert(tile_id);
                batch.push(tile_id);
            }
        }
        
        batch
    }

    /// Mark a tile as completed (no longer downloading)
    pub fn mark_completed(&mut self, tile_id: &TileId) {
        self.downloading.remove(tile_id);
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.downloading.is_empty()
    }

    /// Get the number of queued tiles
    pub fn queued_count(&self) -> usize {
        self.queued.len()
    }

    /// Get the number of tiles currently downloading
    pub fn downloading_count(&self) -> usize {
        self.downloading.len()
    }

    /// Clear the queue
    pub fn clear(&mut self) {
        self.queued.clear();
        self.downloading.clear();
        self.queue.clear();
    }
}

/// Plugin for handling tile downloads
#[derive(Default)]
pub struct TileDownloaderPlugin;

impl Plugin for TileDownloaderPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<TileDownloadQueue>()
            .add_event::<TileDownloadBatchEvent>()
            .add_systems(Update, (
                queue_tiles_for_download,
                process_download_results,
            ));
    }
}

/// System to handle queuing tiles for download based on TileLoadRequest events
fn queue_tiles_for_download(
    mut tile_load_requests: EventReader<TileLoadRequest>,
    mut download_queue: ResMut<TileDownloadQueue>,
    tile_cache: Res<TileCache>,
    mut tile_loader: ResMut<TileLoader>,
    mut download_batch_events: EventWriter<TileDownloadBatchEvent>,
) {
    let mut batch = Vec::new();

    for request in tile_load_requests.read() {
        let tile_id = request.tile_id;
        
        // Skip if already in cache
        if tile_cache.contains(&tile_id) {
            continue;
        }
        
        // Skip if already being downloaded
        if tile_loader.is_pending(&tile_id) {
            continue;
        }
        
        // Queue for download
        download_queue.queue(tile_id);
        batch.push(tile_id);
    }
    
    if !batch.is_empty() {
        // Process the next batch from the queue
        let next_batch = download_queue.get_next_batch();
        if !next_batch.is_empty() {
            // Queue tiles in the TileLoader
            tile_loader.queue_tiles(next_batch.clone());
            
            // Emit batch event
            download_batch_events.send(TileDownloadBatchEvent { tiles: next_batch });
        }
    }
}

/// System to process completed downloads and update the tile cache
fn process_download_results(
    mut tile_loaded_events: EventReader<TileLoadedEvent>,
    mut download_queue: ResMut<TileDownloadQueue>,
    mut tile_cache: ResMut<TileCache>,
    mut tile_load_results: EventWriter<TileLoadResult>,
) {
    for event in tile_loaded_events.read() {
        let tile_id = event.tile_id;
        
        // Mark as no longer downloading
        download_queue.mark_completed(&tile_id);
        
        match &event.data {
            Ok(data) => {
                // Add successful download to cache
                match tile_cache.insert(tile_id, data.clone()) {
                    Ok(()) => {
                        // Send success event
                        tile_load_results.send(TileLoadResult {
                            tile_id,
                            result: Ok(()),
                        });
                    },
                    Err(_) => {
                        // Failed to add to cache
                        tile_load_results.send(TileLoadResult {
                            tile_id,
                            result: Err(TileError::CacheError),
                        });
                    }
                }
            },
            Err(error) => {
                // Send error event
                tile_load_results.send(TileLoadResult {
                    tile_id,
                    result: Err(error.clone()),
                });
            }
        }
    }
}

/// Queue a tile for download
pub fn queue_tile_for_download(
    id: TileId, 
    cache: &TileCache,
    quadtree: &TileQuadtree,
    download_queue: &mut TileDownloadQueue
) {
    // If tile is already in cache, don't download again
    if cache.contains(&id) {
        return;
    }
    
    // Check if this tile level should even be loaded based on quadtree
    if !quadtree.should_load_tile(id) {
        return;
    }
    
    // Add to the download queue
    download_queue.queue(id);
}

/// Start downloading a tile
pub fn start_tile_download(
    id: TileId,
    cache: &mut TileCache,
    quadtree: &TileQuadtree,
) {
    // If tile is already in cache, don't download again
    if cache.contains(&id) {
        return;
    }
    
    // Check if this tile level should even be loaded based on quadtree
    if !quadtree.should_load_tile(id) {
        return;
    }
    
    // Create a new download task
    let task_pool = AsyncComputeTaskPool::get();
    let id_copy = id.clone();
    
    let task = task_pool.spawn(async move {
        // Simulate network delay - replace with actual download in real implementation
        async_std::task::sleep(std::time::Duration::from_millis(200)).await;
        
        // Build URL for the tile
        let url = build_tile_url(id_copy);
        
        // Simulate download failures for some tiles
        if id_copy.x % 13 == 0 && id_copy.y % 7 == 0 {
            return Err(TileError::DownloadFailed);
        }
        
        // In a real implementation, we would:
        // 1. Make an HTTP request to download the tile image
        // 2. Process the image data
        // 3. Return the processed data
        
        // For this example, we'll just create dummy data
        let image_data = create_dummy_tile_data(id_copy);
        
        Ok(TileLoadResult {
            tile_id: id_copy,
            result: Ok(()),
        })
    });
    
    // Mark as being downloaded in the cache
    cache.mark_downloading(id);
    
    // Return the task - it will be processed by the update_tile_downloads system
    // return task;
}

/// Build a URL for a tile
fn build_tile_url(id: TileId) -> String {
    // Format: https://tile.openstreetmap.org/{z}/{x}/{y}.png
    format!("https://tile.openstreetmap.org/{}/{}/{}.png", id.zoom, id.x, id.y)
}

/// Create dummy tile data for testing - but don't attempt to create a TileLoadResult
fn create_dummy_tile_data(id: TileId) -> Vec<u8> {
    // In a real implementation, this would be the actual image data
    // For testing, we'll just create a small vector with some identifying data
    let mut data = Vec::new();
    data.push(id.zoom);
    data.push((id.x % 256) as u8);
    data.push((id.y % 256) as u8);
    data.extend_from_slice(b"TILE_DATA");
    data
} 