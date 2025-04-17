use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use std::collections::HashSet;
use std::sync::Arc;
use std::hash::{Hash, Hasher};
use image::GenericImageView;

/// Represents a unique tile ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId {
    /// Zoom level (0-19)
    pub zoom: u8,
    /// X coordinate
    pub x: u32,
    /// Y coordinate
    pub y: u32,
}

/// Error type for tile-related operations
#[derive(Debug, Clone)]
pub enum TileError {
    /// Tile wasn't found (404)
    NotFound,
    /// Failed to download the tile
    DownloadFailed,
    /// Error loading the tile data
    LoadError(String),
    /// Error when caching the tile
    CacheError,
}

impl TileId {
    /// Create a new tile ID
    pub fn new(zoom: u8, x: u32, y: u32) -> Self {
        Self { zoom, x, y }
    }
    
    /// Create a TileId from a world position and zoom level
    pub fn from_world_position(position: Vec3, zoom: u8) -> Self {
        // Convert from world coordinates to lat/lon
        let lon = position.x / 100.0;
        let lat = position.z / 100.0;
        
        // Calculate tile coordinates based on latitude and longitude
        let n = 2.0_f32.powi(zoom as i32);
        let x = ((lon + 180.0) / 360.0 * n).floor() as u32;
        let y = ((1.0 - (lat.to_radians().tan() + 1.0 / lat.to_radians().cos()).ln() / std::f32::consts::PI) / 2.0 * n).floor() as u32;
        
        Self { zoom, x, y }
    }
    
    /// Get the bounds of the tile in world coordinates
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let n = 2.0_f32.powi(self.zoom as i32);
        
        // Calculate longitude bounds
        let lon1 = self.x as f32 / n * 360.0 - 180.0;
        let lon2 = (self.x as f32 + 1.0) / n * 360.0 - 180.0;
        
        // Calculate latitude bounds
        let lat1 = ((std::f32::consts::PI * (1.0 - 2.0 * self.y as f32 / n)).tan().atan()).to_degrees();
        let lat2 = ((std::f32::consts::PI * (1.0 - 2.0 * (self.y as f32 + 1.0) / n)).tan().atan()).to_degrees();
        
        // Convert to world coordinates
        let min = Vec3::new(lon1 * 100.0, 0.0, lat1 * 100.0);
        let max = Vec3::new(lon2 * 100.0, 0.0, lat2 * 100.0);
        
        (min, max)
    }
    
    /// Return the children tiles (one zoom level deeper)
    pub fn children(&self) -> [TileId; 4] {
        let zoom = self.zoom + 1;
        let x = self.x * 2;
        let y = self.y * 2;
        
        [
            TileId::new(zoom, x, y),
            TileId::new(zoom, x + 1, y),
            TileId::new(zoom, x, y + 1),
            TileId::new(zoom, x + 1, y + 1),
        ]
    }
    
    /// Get adjacent tiles (same zoom level)
    pub fn get_adjacent_tiles(&self) -> Vec<TileId> {
        let mut adjacent = Vec::new();
        
        // Add all 8 adjacent tiles
        for dx in -1..=1 {
            for dy in -1..=1 {
                // Skip self
                if dx == 0 && dy == 0 {
                    continue;
                }
                
                // Calculate new coordinates (handle wraparound for x)
                let max_tiles = 1 << self.zoom;
                let new_x = (self.x as i32 + dx).rem_euclid(max_tiles as i32) as u32;
                
                // Skip if y would be out of bounds
                if (self.y as i32 + dy) < 0 || (self.y as i32 + dy) >= max_tiles as i32 {
                    continue;
                }
                
                let new_y = (self.y as i32 + dy) as u32;
                
                adjacent.push(TileId::new(self.zoom, new_x, new_y));
            }
        }
        
        adjacent
    }
}

/// Convert longitude and latitude to world coordinates
pub fn to_world_coords(lon: f32, lat: f32) -> Vec3 {
    // Simple conversion - scale by 100 to make it a reasonable size in world units
    Vec3::new(lon * 100.0, 0.0, lat * 100.0)
}

/// Calculate the base zoom level for the given camera height
pub fn calculate_base_zoom_level(camera_height: f32) -> u8 {
    // Simple height-based zoom calculation
    let zoom = (16.0 - camera_height.log2()).clamp(1.0, 19.0);
    zoom as u8
}

/// Calculate image dimensions from bytes
pub fn calculate_image_dimensions(bytes: &[u8]) -> Option<Extent3d> {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let dimensions = img.dimensions();
            Some(Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            })
        }
        Err(_) => None,
    }
}

/// Represents the geographic bounds of a tile
#[derive(Debug, Clone, Copy)]
pub struct TileBounds {
    /// Minimum longitude (west)
    pub min_lon: f32,
    /// Minimum latitude (south)
    pub min_lat: f32,
    /// Maximum longitude (east)
    pub max_lon: f32,
    /// Maximum latitude (north)
    pub max_lat: f32,
}

impl TileBounds {
    /// Create new tile bounds
    pub fn new(min_lon: f32, min_lat: f32, max_lon: f32, max_lat: f32) -> Self {
        Self {
            min_lon,
            min_lat,
            max_lon,
            max_lat,
        }
    }
    
    /// Calculate tile bounds from a TileId
    pub fn from_tile_id(tile_id: TileId) -> Self {
        let n = 1u32 << tile_id.zoom;
        let lon1 = tile_id.x as f32 / n as f32 * 360.0 - 180.0;
        let lon2 = (tile_id.x + 1) as f32 / n as f32 * 360.0 - 180.0;
        let lat1 = (f32::atan(f32::sinh(std::f32::consts::PI * (1.0 - 2.0 * tile_id.y as f32 / n as f32)))) * 180.0 / std::f32::consts::PI;
        let lat2 = (f32::atan(f32::sinh(std::f32::consts::PI * (1.0 - 2.0 * (tile_id.y + 1) as f32 / n as f32)))) * 180.0 / std::f32::consts::PI;
        
        Self {
            min_lon: lon1,
            max_lon: lon2,
            min_lat: lat2,  // Note: lat2 < lat1 in this calculation
            max_lat: lat1,
        }
    }
    
    /// Check if a point is contained within these bounds
    pub fn contains(&self, lon: f32, lat: f32) -> bool {
        lon >= self.min_lon && lon <= self.max_lon && lat >= self.min_lat && lat <= self.max_lat
    }
    
    /// Get the center point
    pub fn center(&self) -> (f32, f32) {
        (
            (self.min_lon + self.max_lon) / 2.0,
            (self.min_lat + self.max_lat) / 2.0,
        )
    }
    
    /// Check if this bounds intersects with the current view
    pub fn intersects_with_view(
        &self,
        center_lon: f32,
        center_lat: f32,
        width_in_tiles: f32,
        height_in_tiles: f32,
    ) -> bool {
        // Convert width/height in tiles to degrees
        let width_in_degrees = width_in_tiles * (self.max_lon - self.min_lon);
        let height_in_degrees = height_in_tiles * (self.max_lat - self.min_lat);
        
        // Calculate view bounds
        let view_min_lon = center_lon - width_in_degrees / 2.0;
        let view_max_lon = center_lon + width_in_degrees / 2.0;
        let view_min_lat = center_lat - height_in_degrees / 2.0;
        let view_max_lat = center_lat + height_in_degrees / 2.0;
        
        // Check for intersection
        !(self.max_lon < view_min_lon || 
          self.min_lon > view_max_lon || 
          self.max_lat < view_min_lat || 
          self.min_lat > view_max_lat)
    }
    
    /// Convert to world coordinates
    pub fn to_world_coords(&self) -> (Vec3, Vec3) {
        // Convert lon/lat to X/Z coordinates (simplified for example)
        // In a real app, you would use a proper projection
        let scale = 100.0; // This scale is arbitrary for demonstration
        let min = Vec3::new(
            self.min_lon * scale, 
            0.0, 
            self.min_lat * scale
        );
        let max = Vec3::new(
            self.max_lon * scale, 
            0.0, 
            self.max_lat * scale
        );
        (min, max)
    }
}

/// Represents the state of a tile
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileState {
    /// Tile not loaded yet
    NotLoaded,
    /// Tile is queued for loading
    Loading,
    /// Tile load failed
    Failed,
    /// Tile is loaded successfully
    Loaded {
        /// The entity representing this tile in the scene
        entity: Entity,
        /// Time when the tile was loaded (stored as integer milliseconds)
        load_time: u32,
    },
}

/// Represents a map tile with metadata
#[derive(Component)]
pub struct Tile {
    /// Tile's unique identifier
    pub id: TileId,
    /// Current state of the tile
    pub state: TileState,
    /// When this tile was last visible
    pub last_visible: f32,
    /// Size of the texture in bytes
    pub texture_size: u32,
    /// Children tiles (if any)
    pub children: Option<Box<[Tile; 4]>>,
}

impl Tile {
    /// Create a new tile
    pub fn new(id: TileId) -> Self {
        Self {
            id,
            state: TileState::NotLoaded,
            last_visible: 0.0,
            texture_size: 0,
            children: None,
        }
    }
    
    /// Create a tile with children
    pub fn with_children(id: TileId) -> Self {
        let mut tile = Self::new(id);
        tile.create_children();
        tile
    }
    
    /// Create child tiles
    pub fn create_children(&mut self) {
        if self.children.is_some() {
            return;
        }
        
        let children = self.id.children();
        self.children = Some(Box::new([
            Self::new(children[0]),
            Self::new(children[1]),
            Self::new(children[2]),
            Self::new(children[3]),
        ]));
    }
    
    /// Mark the tile as loaded
    pub fn set_loaded(&mut self, entity: Entity, texture_size: u32, current_time: f32) {
        self.state = TileState::Loaded {
            entity,
            load_time: current_time as u32,
        };
        self.texture_size = texture_size;
    }
    
    /// Mark the tile as loading
    pub fn set_loading(&mut self) {
        if let TileState::Loaded { .. } = self.state {
            // Don't change state if already loaded
            return;
        }
        self.state = TileState::Loading;
    }
    
    /// Mark the tile as failed
    pub fn set_failed(&mut self) {
        if let TileState::Loaded { .. } = self.state {
            // Don't change state if already loaded
            return;
        }
        self.state = TileState::Failed;
    }
    
    /// Check if the tile is visible from a given camera position
    pub fn is_visible(&self, camera_pos: Vec3, camera_dir: Vec3) -> bool {
        // Get tile bounds in world coordinates
        let bounds = TileBounds::from_tile_id(self.id);
        let (min, max) = bounds.to_world_coords();
        
        // Simple frustum culling
        // TODO: Implement proper frustum culling
        let center = (min + max) / 2.0;
        let to_tile = center - camera_pos;
        
        // Check if tile is in front of camera
        let dot = to_tile.dot(camera_dir);
        dot > 0.0
    }
    
    /// Update the last visible time
    pub fn update_visibility(&mut self, current_time: f32) {
        self.last_visible = current_time;
    }
}

/// Represents the result of loading a tile
#[derive(Debug, Clone, Event)]
pub struct TileLoadResult {
    /// The ID of the tile
    pub tile_id: TileId,
    /// The result of loading the tile
    pub result: Result<(), TileError>,
}

/// Request to load a tile
#[derive(Debug, Clone, Event)]
pub struct TileLoadRequest {
    /// The ID of the tile to load
    pub tile_id: TileId,
}

/// Information about a tile for cleanup purposes
#[derive(Debug, Clone, Copy)]
pub struct TileCleanupInfo {
    /// Tile ID
    pub id: TileId,
    /// When this tile was last visible
    pub last_visible: f32,
    /// Entity representing this tile (if any)
    pub entity: Option<Entity>,
    /// Size of the tile texture in bytes
    pub texture_size: u32,
}

/// Camera state for the tile system
#[derive(Debug, Clone, Copy)]
pub struct CameraState {
    /// Longitude of the camera position
    pub lon: f32,
    /// Latitude of the camera position
    pub lat: f32,
    /// Camera zoom level
    pub zoom: f32,
}

/// Memory budget constraints for tile loading
#[derive(Resource)]
pub struct TileMemoryBudget {
    pub max_tiles: usize,
    pub max_texture_memory: usize,
    pub current_tiles: usize,
    pub current_texture_memory: usize,
}

impl Default for TileMemoryBudget {
    fn default() -> Self {
        Self {
            max_tiles: 500,
            max_texture_memory: 512 * 1024 * 1024, // 512 MB
            current_tiles: 0,
            current_texture_memory: 0,
        }
    }
}

/// The state of a tile loading request
pub enum TileLoadState {
    Pending {
        id: TileId,
        priority: i32,
        request_time: f32,
    },
    InProgress {
        id: TileId,
        priority: i32,
        request_time: f32,
    },
    Completed {
        id: TileId,
        result: TileLoadResult,
    },
} 