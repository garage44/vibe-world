use bevy::prelude::*;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::cmp::Ordering;
use crate::tile_system::{TileCache, TileQuadtree};
use crate::tile_system::types::*;

/// Maximum number of concurrent downloads
const MAX_CONCURRENT_DOWNLOADS: usize = 6;

/// A tile request with priority information
#[derive(Clone, Debug)]
pub struct TileRequest {
    pub id: TileId,
    pub priority: f32,
    pub request_time: f32,
}

impl Eq for TileRequest {}

impl PartialEq for TileRequest {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for TileRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority comes first
        self.priority.partial_cmp(&other.priority)
            .unwrap_or(Ordering::Equal)
            .reverse() // Reverse to make BinaryHeap a min-heap
    }
}

impl PartialOrd for TileRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Scheduler for tile loading requests
#[derive(Resource)]
pub struct TileScheduler {
    /// Priority queue for tile requests
    queue: BinaryHeap<TileRequest>,
    
    /// Currently active downloads
    active_downloads: HashMap<TileId, f32>, // TileId -> start_time
    
    /// Maximum time to wait for a download before considering it failed
    download_timeout: f32,
    
    /// Last time the queue was updated
    last_update_time: f32,
}

impl Default for TileScheduler {
    fn default() -> Self {
        Self {
            queue: BinaryHeap::new(),
            active_downloads: HashMap::new(),
            download_timeout: 10.0, // 10 seconds default timeout
            last_update_time: 0.0,
        }
    }
}

impl TileScheduler {
    /// Add a new tile request to the queue
    pub fn queue_request(&mut self, id: TileId, priority: f32, current_time: f32) {
        // Check if already in active downloads
        if self.active_downloads.contains_key(&id) {
            return;
        }
        
        // Check if already in queue
        let already_queued = self.queue.iter().any(|req| req.id == id);
        if already_queued {
            // Update priority of existing request
            let mut new_queue = BinaryHeap::new();
            for req in self.queue.drain() {
                if req.id == id {
                    // Create new request with updated priority
                    new_queue.push(TileRequest {
                        id,
                        priority: priority.max(req.priority), // Use higher priority
                        request_time: req.request_time,
                    });
                } else {
                    new_queue.push(req);
                }
            }
            self.queue = new_queue;
        } else {
            // Add new request
            self.queue.push(TileRequest {
                id,
                priority,
                request_time: current_time,
            });
        }
    }
    
    /// Get the next batch of tiles to download
    pub fn get_next_batch(&mut self, current_time: f32) -> Vec<TileId> {
        // Update last update time
        self.last_update_time = current_time;
        
        // Check for timed out downloads
        let mut timed_out = Vec::new();
        for (id, start_time) in self.active_downloads.iter() {
            if current_time - start_time > self.download_timeout {
                timed_out.push(*id);
            }
        }
        
        // Remove timed out downloads
        for id in timed_out {
            self.active_downloads.remove(&id);
        }
        
        // Calculate how many new downloads we can start
        let available_slots = MAX_CONCURRENT_DOWNLOADS - self.active_downloads.len();
        if available_slots == 0 {
            return Vec::new();
        }
        
        let mut batch = Vec::new();
        for _ in 0..available_slots {
            if let Some(request) = self.queue.pop() {
                // Add to active downloads
                self.active_downloads.insert(request.id, current_time);
                
                // Add to batch
                batch.push(request.id);
            } else {
                // No more requests in queue
                break;
            }
        }
        
        batch
    }
    
    /// Mark a download as complete
    pub fn mark_completed(&mut self, id: TileId) {
        self.active_downloads.remove(&id);
    }
    
    /// Calculate priority for a tile based on distance, zoom level and visibility
    pub fn calculate_priority(
        id: TileId,
        distance: f32,
        is_visible: bool,
        camera_height: f32
    ) -> f32 {
        // Base priority factors
        let zoom_factor = 1.0 + (id.zoom as f32 / 10.0); // Higher zoom level = higher priority
        let distance_factor = 1.0 / (1.0 + distance); // Closer = higher priority
        
        // Visibility bonus
        let visibility_bonus = if is_visible { 10.0 } else { 1.0 };
        
        // Ideal zoom level based on camera height
        let ideal_zoom = calculate_ideal_zoom(camera_height);
        let zoom_match_bonus = 1.0 / (1.0 + (ideal_zoom as f32 - id.zoom as f32).abs());
        
        // Combine factors
        zoom_factor * distance_factor * visibility_bonus * zoom_match_bonus
    }
}

/// Calculate the ideal zoom level based on camera height
pub fn calculate_ideal_zoom(camera_height: f32) -> u8 {
    // Example implementation:
    // At height 0-10: zoom level 18
    // At height 10-20: zoom level 17
    // At height 20-50: zoom level 16
    // At height 50-100: zoom level 15
    // At height 100-200: zoom level 14
    // etc.
    
    if camera_height < 10.0 {
        18
    } else if camera_height < 20.0 {
        17
    } else if camera_height < 50.0 {
        16
    } else if camera_height < 100.0 {
        15
    } else if camera_height < 200.0 {
        14
    } else if camera_height < 500.0 {
        13
    } else if camera_height < 1000.0 {
        12
    } else if camera_height < 2000.0 {
        11
    } else if camera_height < 5000.0 {
        10
    } else if camera_height < 10000.0 {
        9
    } else {
        8
    }
}

/// System to update the tile scheduler
pub fn update_tile_scheduler(
    mut scheduler: ResMut<TileScheduler>,
    mut cache: ResMut<TileCache>,
    mut quadtree: ResMut<TileQuadtree>,
    time: Res<Time>,
) {
    let current_time = time.elapsed_secs();
    
    // Get the next batch of tiles to download
    let batch = scheduler.get_next_batch(current_time);
    
    // Start downloading each tile
    for id in batch {
        crate::tile_system::downloader::start_tile_download(id, &mut cache, &quadtree);
    }
}

pub fn schedule_tile_loads(
    mut commands: Commands,
    mut tile_cache: ResMut<TileCache>,
    mut tile_budget: ResMut<TileMemoryBudget>,
    mut quadtree: ResMut<TileQuadtree>,
    // ... rest of parameters
) {
    // ... existing code ...
}

/// Schedule tiles for downloading based on priority
pub fn schedule_tile_downloads(
    mut cache: ResMut<TileCache>,
    mut quadtree: ResMut<TileQuadtree>,
    time: Res<Time>,
) {
    let current_time = time.elapsed_secs();
    // ...
}

/// Update tile download priorities
pub fn update_tile_priorities(
    mut tile_cache: ResMut<TileCache>,
    mut quadtree: ResMut<TileQuadtree>,
    time: Res<Time>,
) {
    let current_time = time.elapsed_secs();
    // ...
} 