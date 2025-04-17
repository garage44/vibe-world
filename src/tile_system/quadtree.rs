use bevy::prelude::*;
use std::collections::{HashMap, VecDeque, BinaryHeap, HashSet};
use std::cmp::Ordering;
use crate::tile_system::types::*;
use crate::tile_system::cache::TileCache;
use crate::tile_system::downloader;
use crate::tile_system::meshing;
use crate::tile_system::CameraTransform;

/// Maximum number of tiles to process per frame
const MAX_PROCESS_PER_FRAME: usize = 10;

/// Represents a node in the quadtree
pub struct QuadTreeNode {
    /// Tile ID for this node
    tile_id: TileId,
    /// Children nodes (if any)
    children: Option<Box<[QuadTreeNode; 4]>>,
    /// Whether this node is visible
    visible: bool,
    /// Whether this node's tile is loaded
    loaded: bool,
}

impl QuadTreeNode {
    /// Create a new quadtree node
    pub fn new(tile_id: TileId) -> Self {
        Self {
            tile_id,
            children: None,
            visible: false,
            loaded: false,
        }
    }
    
    /// Subdivide this node into four children
    pub fn subdivide(&mut self) {
        if self.children.is_some() {
            return;
        }
        
        let zoom = self.tile_id.zoom + 1;
        let x = self.tile_id.x * 2;
        let y = self.tile_id.y * 2;
        
        self.children = Some(Box::new([
            QuadTreeNode::new(TileId::new(zoom, x, y)),
            QuadTreeNode::new(TileId::new(zoom, x + 1, y)),
            QuadTreeNode::new(TileId::new(zoom, x, y + 1)),
            QuadTreeNode::new(TileId::new(zoom, x + 1, y + 1)),
        ]));
    }
    
    /// Merge children (remove subdivisions)
    pub fn merge(&mut self) {
        self.children = None;
    }
    
    /// Check if this node contains the given point
    pub fn contains(&self, lon: f32, lat: f32) -> bool {
        // Convert point to tile coordinates
        let tile_bounds = TileBounds::from_tile_id(self.tile_id);
        tile_bounds.contains(lon, lat)
    }
    
    /// Set the loaded state
    pub fn set_loaded(&mut self, loaded: bool) {
        self.loaded = loaded;
    }
    
    /// Set the visibility state
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
    
    /// Get the tile ID
    pub fn tile_id(&self) -> TileId {
        self.tile_id
    }
    
    /// Get visibility state
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    
    /// Get loaded state
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }
    
    /// Check if this node has children
    pub fn has_children(&self) -> bool {
        self.children.is_some()
    }
    
    /// Get children nodes
    pub fn children(&self) -> Option<&[QuadTreeNode; 4]> {
        self.children.as_ref().map(|c| &**c)
    }
    
    /// Get mutable children nodes
    pub fn children_mut(&mut self) -> Option<&mut [QuadTreeNode; 4]> {
        self.children.as_mut().map(|c| &mut **c)
    }
}

/// A quadtree for managing map tiles
#[derive(Resource)]
pub struct TileQuadtree {
    /// Root node of the quadtree
    root: QuadTreeNode,
    /// Maximum zoom level allowed
    max_zoom: u8,
    /// Current zoom level (can be fractional for smooth transitions)
    current_zoom: f32,
}

impl Default for TileQuadtree {
    fn default() -> Self {
        Self {
            root: QuadTreeNode::new(TileId::new(0, 0, 0)),
            max_zoom: 18,
            current_zoom: 0.0,
        }
    }
}

impl TileQuadtree {
    /// Create a new quadtree with the specified maximum zoom level
    pub fn new(max_zoom: u8) -> Self {
        Self {
            max_zoom,
            ..Default::default()
        }
    }
    
    /// Update the quadtree based on the current view
    pub fn update(&mut self, center_lon: f32, center_lat: f32, zoom: f32, view_width: f32, view_height: f32) {
        self.current_zoom = zoom.clamp(0.0, self.max_zoom as f32);
        
        // Reset visibility
        self.reset_visibility();
        
        // Calculate the visible area in world coordinates
        let tile_size = 256.0; // Standard tile size
        let scale = 2.0_f32.powf(self.current_zoom);
        let width_in_tiles = view_width / (tile_size * scale);
        let height_in_tiles = view_height / (tile_size * scale);
        
        // Update visibility based on view area
        self.update_visibility_impl(center_lon, center_lat, width_in_tiles, height_in_tiles);
    }
    
    /// Reset visibility of all nodes
    fn reset_visibility(&mut self) {
        // Create a local mutable reference to avoid self-borrowing issues
        let root = &mut self.root;
        Self::reset_node_visibility(root);
    }
    
    /// Reset visibility of a node and its children
    fn reset_node_visibility(node: &mut QuadTreeNode) {
        node.set_visible(false);
        
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut() {
                Self::reset_node_visibility(child);
            }
        }
    }
    
    /// Update visibility based on the current view
    fn update_visibility_impl(&mut self, center_lon: f32, center_lat: f32, width_in_tiles: f32, height_in_tiles: f32) {
        // Target zoom level (integer)
        let target_zoom = self.current_zoom.floor() as u8;
        
        // Update visibility starting from the root
        let root = &mut self.root;
        Self::update_node_visibility(
            root,
            center_lon,
            center_lat,
            width_in_tiles,
            height_in_tiles,
            target_zoom,
        );
    }
    
    /// Update visibility of a specific node
    fn update_node_visibility(
        node: &mut QuadTreeNode,
        center_lon: f32,
        center_lat: f32,
        width_in_tiles: f32,
        height_in_tiles: f32,
        target_zoom: u8,
    ) {
        // Check if this node is visible in the view
        let tile_bounds = TileBounds::from_tile_id(node.tile_id());
        
        // Simple check: is the tile potentially visible?
        let is_potentially_visible = tile_bounds.intersects_with_view(
            center_lon,
            center_lat,
            width_in_tiles,
            height_in_tiles,
        );
        
        if !is_potentially_visible {
            // If not visible, merge children since we don't need detail here
            node.merge();
            node.set_visible(false);
            return;
        }
        
        // This node is visible
        node.set_visible(true);
        
        // If we're at or past the target zoom level, don't subdivide further
        if node.tile_id().zoom >= target_zoom {
            // We're at the desired detail level
            return;
        }
        
        // If we need more detail, subdivide
        node.subdivide();
        
        // Update children recursively
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut() {
                Self::update_node_visibility(
                    child,
                    center_lon,
                    center_lat,
                    width_in_tiles,
                    height_in_tiles,
                    target_zoom,
                );
            }
        }
    }
    
    /// Get all visible tile IDs that should be rendered
    pub fn get_visible_tiles(&self) -> Vec<TileId> {
        let mut tiles = Vec::new();
        self.collect_visible_tiles(&self.root, &mut tiles);
        tiles
    }
    
    /// Collect visible tiles from a node and its children
    fn collect_visible_tiles(&self, node: &QuadTreeNode, tiles: &mut Vec<TileId>) {
        if !node.is_visible() {
            return;
        }
        
        // If this node has no visible children, add it to the list
        if !node.has_children() || node.tile_id().zoom == self.max_zoom {
            tiles.push(node.tile_id());
            return;
        }
        
        // If it has children, collect from them instead
        if let Some(children) = node.children() {
            let mut any_child_visible = false;
            
            for child in children.iter() {
                if child.is_visible() {
                    any_child_visible = true;
                    self.collect_visible_tiles(child, tiles);
                }
            }
            
            // If no children are visible, use this tile
            if !any_child_visible {
                tiles.push(node.tile_id());
            }
        }
    }
    
    /// Get all tile IDs that should be loaded (visible plus adjacent tiles for smooth scrolling)
    pub fn get_tiles_to_load(&self) -> HashSet<TileId> {
        let visible_tiles = self.get_visible_tiles();
        let mut tiles_to_load = HashSet::new();
        
        // Add all visible tiles
        for tile_id in &visible_tiles {
            tiles_to_load.insert(*tile_id);
            
            // Add adjacent tiles (to preload nearby areas)
            for adjacent in tile_id.get_adjacent_tiles() {
                tiles_to_load.insert(adjacent);
            }
        }
        
        tiles_to_load
    }
    
    /// Update the loaded state of tiles
    pub fn update_loaded_state(&mut self, loaded_tiles: &HashSet<TileId>) {
        // Create a local mutable reference to avoid self-borrowing issues
        let root = &mut self.root;
        Self::update_node_loaded_state(root, loaded_tiles);
    }
    
    /// Update the loaded state of a node and its children
    fn update_node_loaded_state(node: &mut QuadTreeNode, loaded_tiles: &HashSet<TileId>) {
        node.set_loaded(loaded_tiles.contains(&node.tile_id()));
        
        if let Some(children) = node.children_mut() {
            for child in children.iter_mut() {
                Self::update_node_loaded_state(child, loaded_tiles);
            }
        }
    }
    
    /// Set the maximum zoom level
    pub fn set_max_zoom(&mut self, max_zoom: u8) {
        self.max_zoom = max_zoom;
    }
    
    /// Get the current zoom level
    pub fn current_zoom(&self) -> f32 {
        self.current_zoom
    }
    
    /// Check if a specific tile should be loaded based on visibility and zoom level
    pub fn should_load_tile(&self, id: TileId) -> bool {
        // Basic implementation: allow loading any tile up to the maximum zoom level
        // In a more complete implementation, we would check if the tile is within
        // the visible area and at an appropriate zoom level
        id.zoom <= self.max_zoom
    }
    
    /// Insert a successfully loaded tile into the quadtree
    pub fn insert_tile(&mut self, id: TileId, entity: Entity, texture_size: u32) {
        // Find the node corresponding to this tile and mark it as loaded
        let mut node = &mut self.root;
        let path = Self::get_path_to_tile(id);
        
        for (i, &index) in path.iter().enumerate() {
            // If we need to create children to reach this tile, do so
            if node.children.is_none() {
                node.subdivide();
            }
            
            if let Some(children) = node.children_mut() {
                node = &mut children[index];
            } else {
                // We couldn't create children, so we can't reach this tile
                return;
            }
        }
        
        // Mark the node as loaded
        node.set_loaded(true);
    }
    
    /// Insert a failed tile load into the quadtree
    pub fn insert_failed_tile(&mut self, id: TileId, entity: Entity) {
        // Similar to insert_tile, but don't mark as successfully loaded
        let mut node = &mut self.root;
        let path = Self::get_path_to_tile(id);
        
        for (i, &index) in path.iter().enumerate() {
            // If we need to create children to reach this tile, do so
            if node.children.is_none() {
                node.subdivide();
            }
            
            if let Some(children) = node.children_mut() {
                node = &mut children[index];
            } else {
                // We couldn't create children, so we can't reach this tile
                return;
            }
        }
        
        // Don't mark as loaded, as the load failed
        // But we can keep track of the entity for cleanup
    }
    
    /// Calculate the path of indices to reach a specific tile
    fn get_path_to_tile(id: TileId) -> Vec<usize> {
        let mut path = Vec::new();
        let mut current_zoom = 0;
        let mut current_x = 0;
        let mut current_y = 0;
        
        while current_zoom < id.zoom {
            current_zoom += 1;
            let quad_x = (id.x >> (id.zoom - current_zoom)) & 1;
            let quad_y = (id.y >> (id.zoom - current_zoom)) & 1;
            let index = (quad_y << 1) | quad_x;
            path.push(index as usize);
            
            current_x = (current_x << 1) | quad_x;
            current_y = (current_y << 1) | quad_y;
        }
        
        path
    }
}

/// Information about a tile for cleanup
struct TileCleanupInfo {
    id: TileId,
    last_visible: f32,
    entity: Option<Entity>,
    texture_size: usize,
}

/// Tile loading request with priority ordering
#[derive(Debug, Clone, Copy)]
struct TileLoadRequest {
    id: TileId,
    priority: i32,
    request_time: f32,
}

impl PartialEq for TileLoadRequest {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for TileLoadRequest {}

impl PartialOrd for TileLoadRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TileLoadRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower priority value = higher actual priority
        other.priority.cmp(&self.priority)
    }
}

/// Calculate base zoom level from camera height
fn calculate_base_zoom_level(height: f32) -> u32 {
    match height {
        h if h <= 1.0 => 19,   // Level 19: Local highways, crossings (1:1000 scale)
        h if h <= 2.0 => 18,   // Level 18: Buildings, trees (1:2000 scale)
        h if h <= 4.0 => 17,   // Level 17: Building blocks, parks, addresses
        h if h <= 8.0 => 16,   // Level 16: Streets
        h if h <= 15.0 => 15,  // Level 15: Small roads
        h if h <= 30.0 => 14,  // Level 14: Detailed roads
        h if h <= 60.0 => 13,  // Level 13: Villages, suburbs
        h if h <= 120.0 => 12, // Level 12: Towns, city districts
        h if h <= 250.0 => 11, // Level 11: Cities
        h if h <= 500.0 => 10, // Level 10: Metropolitan areas
        h if h <= 1000.0 => 9, // Level 9: Large metro areas
        h if h <= 2000.0 => 8, // Level 8
        h if h <= 4000.0 => 7, // Level 7: Small countries, US states
        h if h <= 8000.0 => 6, // Level 6: Large European countries
        h if h <= 16000.0 => 5, // Level 5: Large African countries
        h if h <= 32000.0 => 4, // Level 4
        h if h <= 64000.0 => 3, // Level 3: Largest countries
        h if h <= 128000.0 => 2, // Level 2: Subcontinental areas
        _ => 1,                  // Level 1: Whole world
    }
}

/// Internal representation of camera state
#[derive(Clone, Debug)]
struct CameraState {
    position: Vec2,
    viewport_size: Vec2,
    zoom: f32,
}

/// System to update visible tiles based on camera transform
pub fn update_visible_tiles(
    mut quadtree: ResMut<TileQuadtree>,
    camera_query: Query<(&Transform, &Camera), With<crate::tile_system::CameraTransform>>,
    windows: Query<&Window>,
) {
    let Ok((camera_transform, _camera)) = camera_query.get_single() else {
        return;
    };
    
    let Ok(window) = windows.get_single() else {
        return;
    };
    
    let viewport_size = Vec2::new(window.width(), window.height());
    quadtree.update(camera_transform.translation.x, camera_transform.translation.y, 1.0 / camera_transform.scale.x, viewport_size.x, viewport_size.y);
} 