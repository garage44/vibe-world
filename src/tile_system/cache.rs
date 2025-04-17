use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, RenderAssetUsages};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use crate::tile_system::types::*;
use image::{DynamicImage, ImageBuffer, Rgba};

/// The maximum number of images to keep in memory
const MAX_CACHE_SIZE: usize = 1000;

/// A cache for storing loaded tiles and their data
#[derive(Resource, Default)]
pub struct TileCache {
    /// Map of tile ID to cached tile data
    tiles: HashMap<TileId, Vec<u8>>,
    /// Last time the tile was accessed
    last_access: HashMap<TileId, u32>,
    /// Tracking tiles that are in the process of loading
    loading: HashMap<TileId, bool>,
    /// Cache directory
    #[cfg(feature = "disk_cache")]
    cache_dir: PathBuf,
    /// Pending tile load results
    #[cfg(feature = "async_loading")]
    pending_results: Arc<Mutex<Vec<TileLoadResult>>>,
}

/// Represents the data for a loaded tile
pub struct TileData {
    /// The raw image data
    pub image_data: Vec<u8>,
    
    /// The Bevy texture handle (if uploaded to GPU)
    pub texture: Option<Handle<Image>>,
    
    /// Size of the tile data in bytes
    pub size: usize,
}

impl TileCache {
    /// Create a new empty tile cache
    pub fn new() -> Self {
        Self {
            tiles: HashMap::new(),
            last_access: HashMap::new(),
            loading: HashMap::new(),
            #[cfg(feature = "disk_cache")]
            cache_dir: PathBuf::from("./cache"),
            #[cfg(feature = "async_loading")]
            pending_results: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Insert a tile into the cache
    pub fn insert(&mut self, id: TileId, data: Vec<u8>) -> Result<(), crate::tile_system::types::TileError> {
        // Check if we can decode the image to validate it
        if image::load_from_memory(&data).is_err() {
            return Err(crate::tile_system::types::TileError::LoadError("Invalid image data".to_string()));
        }
        
        // Add to cache
        self.tiles.insert(id, data);
        
        // Set access time to current time
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;
            
        self.last_access.insert(id, current_time);
        
        // Remove from loading if present
        self.loading.remove(&id);
        
        Ok(())
    }
    
    /// Get a tile from the cache
    pub fn get(&self, id: &TileId) -> Option<&Vec<u8>> {
        self.tiles.get(id)
    }
    
    /// Check if a tile is in the cache
    pub fn contains(&self, id: &TileId) -> bool {
        self.tiles.contains_key(id)
    }
    
    /// Remove a tile from the cache
    pub fn remove(&mut self, id: &TileId) -> Option<Vec<u8>> {
        self.last_access.remove(id);
        self.tiles.remove(id)
    }
    
    /// Update the last access time for a tile
    pub fn update_access_time(&mut self, id: &TileId, current_time: u32) {
        if self.tiles.contains_key(id) {
            self.last_access.insert(*id, current_time);
        }
    }
    
    /// Get the number of tiles in the cache
    pub fn len(&self) -> usize {
        self.tiles.len()
    }
    
    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.tiles.clear();
        self.last_access.clear();
    }
    
    /// Mark a tile as being downloaded
    pub fn mark_downloading(&mut self, id: TileId) {
        self.loading.insert(id, true);
    }
    
    /// Check if a tile is already being downloaded
    pub fn is_loading(&self, id: &TileId) -> bool {
        self.loading.contains_key(id)
    }
    
    /// Initialize the cache system - simplified version
    pub fn initialize(&mut self) {
        // No complex initialization needed for in-memory cache
        info!("Tile cache initialized");
    }
}

/// System to update the access time of tiles in the cache
pub fn update_tile_cache_time(
    _tile_cache: ResMut<TileCache>, 
    _time: Res<Time>,
) {
    // Nothing to do here for now
    // In a real implementation, we might update access times or clean up old tiles
}

/// Create a placeholder texture for tiles not yet loaded
pub fn create_placeholder_texture() -> Image {
    let width = 256;
    let height = 256;
    
    // Create a checkerboard pattern
    let mut buffer = ImageBuffer::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let color = if (x / 32 + y / 32) % 2 == 0 {
                Rgba([200, 200, 200, 255])
            } else {
                Rgba([150, 150, 150, 255])
            };
            buffer.put_pixel(x, y, color);
        }
    }
    
    let dynamic_image = DynamicImage::ImageRgba8(buffer);
    
    // Create an Image
    let rgba8 = dynamic_image.to_rgba8();
    
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba8.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// System to initialize the tile cache
pub fn initialize_tile_cache(
    mut tile_cache: ResMut<TileCache>,
) {
    // Basic initialization
    tile_cache.initialize();
}

impl TileCache {
    /// Get the pending results - stub for compatibility
    pub fn pending_results(&self) -> Option<Vec<TileLoadResult>> {
        match self.pending_results.lock() {
            Ok(pending) => {
                if pending.is_empty() {
                    None
                } else {
                    // Clone the contents to avoid holding the lock
                    Some(pending.clone())
                }
            },
            Err(_) => None, // Lock failed
        }
    }
    
    /// Mark a tile as being loaded
    pub fn mark_loading(&mut self, id: TileId) {
        self.loading.insert(id, true);
    }
    
    /// Get a tile image from the cache
    pub fn get_image(&mut self, id: &TileId) -> Option<Image> {
        if let Some(image_data) = self.get(id) {
            if let Ok(dyn_img) = image::load_from_memory(image_data) {
                return Some(Image::from_dynamic(
                    dyn_img, 
                    true, // sRGB
                    RenderAssetUsages::RENDER_WORLD,
                ));
            }
        }
        
        None
    }
    
    /// Cache path - stub for compatibility 
    pub fn cache_path(&self, _id: &TileId) -> PathBuf {
        PathBuf::from("./temp")
    }
    
    /// Check if a tile is cached on disk
    pub fn is_cached_on_disk(&self, id: &TileId) -> bool {
        self.cache_path(id).exists()
    }
    
    /// Attempt to load a tile from the disk cache
    pub fn load_from_disk(&self, id: &TileId) -> Option<Image> {
        #[cfg(feature = "disk_cache")]
        {
            let cache_path = self.cache_path(id);
            
            if cache_path.exists() {
                match image::open(&cache_path) {
                    Ok(img) => {
                        let dynamic_img = image::DynamicImage::ImageRgba8(img.into_rgba8());
                        let image = Image::from_dynamic(
                            dynamic_img, 
                            true, // sRGB
                            RenderAssetUsages::RENDER_WORLD,
                        );
                        return Some(image);
                    },
                    Err(e) => {
                        warn!("Failed to load cached tile: {}", e);
                        // Try to remove corrupt cache file
                        let _ = fs::remove_file(&cache_path);
                    }
                }
            }
        }
        
        None
    }
    
    /// Save a tile to the disk cache
    pub fn save_to_disk(&self, id: &TileId, image: &Image) {
        let cache_path = self.cache_path(id);
        
        // Convert Image to DynamicImage
        // Note: This is a simplification, actual implementation would depend on Image internal format
        // TODO: Implement proper Image to DynamicImage conversion
        
        // Save to disk
        // image.save(&cache_path).unwrap_or_else(|e| {
        //     warn!("Failed to cache tile: {}", e);
        // });
        
        // For now, just log that we would save here
        debug!("Would save tile {},{},{} to cache", id.x, id.y, id.zoom);
    }
    
    /// Process any pending load results
    /// 
    /// # Returns
    /// 
    /// A tuple (successes, failures) with the number of successfully processed tiles
    /// and the number of failures.
    pub fn process_pending_results(
        &self,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        transforms: &mut Query<&mut Transform>,
        visible: &mut Query<&mut Visibility>,
        entities: &mut Query<&TileEntity>,
        renderer: &TileRenderer,
    ) -> (usize, usize) {
        let results = match self.pending_results() {
            Some(results) => results,
            None => return (0, 0), // No results to process
        };
        
        let mut success_count = 0;
        let mut failure_count = 0;

        // Clear the pending results after we've retrieved them
        if let Ok(mut pending) = self.pending_results.lock() {
            pending.clear();
        }

        for result in results {
            match result {
                // ... existing code ...
            }
        }

        (success_count, failure_count)
    }
    
    /// Estimate the size of an image for the memory budget
    pub fn estimate_image_size(&self, image: &Image) -> usize {
        // A simple but reasonable approximation
        let dimensions = image.texture_descriptor.size;
        let bytes_per_pixel = match image.texture_descriptor.format {
            // Most common formats
            bevy::render::render_resource::TextureFormat::Rgba8Unorm => 4,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb => 4,
            bevy::render::render_resource::TextureFormat::Bgra8Unorm => 4,
            bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb => 4,
            // Reasonable default for other formats
            _ => 4,
        };
        
        dimensions.width as usize * dimensions.height as usize * bytes_per_pixel
    }
} 