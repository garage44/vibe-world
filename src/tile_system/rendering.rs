use bevy::prelude::*;
use bevy::math::primitives::Aabb;
use bevy::render::view::Frustum;
use std::collections::HashMap;
use crate::tile_system::types::*;
use crate::tile_system::meshing;
use crate::resources::constants::{MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL};
use std::collections::HashSet;
use crate::tile_system::{TileCache, TileQuadtree, TileComponent};

/// The maximum number of tiles to process in a single frame
const MAX_TILES_PER_FRAME: usize = 8;

/// System for processing tile load results and creating tile entities
pub fn process_tile_load_results(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<TileCache>,
    mut budget: ResMut<TileMemoryBudget>,
    mut quadtree: ResMut<TileQuadtree>,
    time: Res<Time>,
) {
    // Get current time
    let current_time = time.elapsed_secs();
    
    // Process pending load results
    let results = cache.process_pending_results();
    
    // Limit how many we process in a single frame
    let to_process = results.len().min(MAX_TILES_PER_FRAME);
    
    for result in results.into_iter().take(to_process) {
        match result {
            TileLoadResult { tile_id, result: Ok(()) } => {
                // Get the image from cache
                if let Some(image) = cache.get_image(&tile_id) {
                    let image_arc = std::sync::Arc::new(image);
                    let texture_size = std::mem::size_of_val(&*image_arc) as u32;
                    
                    // Update memory budget
                    budget.current_tiles += 1;
                    budget.current_texture_memory += texture_size as usize;
                    
                    // Create the tile mesh
                    let entity = meshing::create_tile_mesh(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        tile_id,
                        image_arc,
                        current_time,
                    );
                    
                    // Update the quadtree with tile information
                    quadtree.insert_tile(tile_id, entity, texture_size);
                }
            },
            TileLoadResult { tile_id, result: Err(error) } => {
                // Log the error
                warn!("Failed to load tile {},{},{}: {}", 
                    tile_id.x, tile_id.y, tile_id.zoom, 
                    match &error {
                        TileError::NotFound => "not found",
                        TileError::DownloadFailed => "download failed",
                        TileError::LoadError(msg) => msg,
                        TileError::CacheError => "cache error",
                    });
                
                // Create a fallback mesh
                let entity = meshing::create_fallback_tile_mesh(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    tile_id,
                    current_time,
                );
                
                // Update the quadtree with fallback tile
                quadtree.insert_failed_tile(tile_id, entity);
            }
        }
    }
}

/// System for view frustum culling of tile entities
pub fn frustum_culling(
    mut visibility_query: Query<(&GlobalTransform, &mut Visibility, &TileComponent)>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    // Get camera frustum
    if let Ok((camera, camera_transform)) = camera_query.get_single() {
        // Get camera projection via the projection view component
        if let Ok(projection_view) = camera.get_projection() {
            let view_matrix = camera_transform.compute_matrix();
            let projection_matrix = projection_view.get_projection_matrix();
            
            // Create frustum from view and projection matrices
            let frustum = Frustum::from_view_projection(
                &view_matrix,
                &projection_matrix,
                &camera_transform.translation(),
                &camera_transform.back(),
            );
            
            // Update all tile visibilities
            for (transform, mut visibility, _tile) in visibility_query.iter_mut() {
                // Create AABB for the tile
                let aabb = create_aabb_from_transform(transform);
                
                // Test against frustum
                let is_visible = frustum.intersects_aabb(aabb);
                
                // Update visibility
                *visibility = if is_visible {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

/// Create an AABB from a transform
fn create_aabb_from_transform(transform: &GlobalTransform) -> Aabb {
    let position = transform.translation();
    let scale = transform.scale();
    
    // Create AABB centered at the transform position with half-extents from scale
    Aabb::new(
        position,
        scale * 0.5
    )
}

/// Compute the projected center of a tile in view space
/// Returns the view position and distance to camera
pub fn project_tile_to_view(
    tile_transform: &GlobalTransform,
    camera_transform: &GlobalTransform,
    projection: &Projection,
) -> (Vec2, f32) {
    // Get the tile position in world space
    let world_pos = tile_transform.translation();
    
    // Get the camera position
    let camera_pos = camera_transform.translation();
    
    // Compute distance from camera to tile
    let distance = (world_pos - camera_pos).length();
    
    // Transform tile position to view space
    let camera_view = camera_transform.compute_matrix().inverse();
    let view_pos = camera_view.transform_point3(world_pos);
    
    // If behind camera, return off-screen position
    if view_pos.z <= 0.0 {
        return (Vec2::new(-2.0, -2.0), distance);
    }
    
    // Get projection matrix based on projection type
    let proj_matrix = match projection {
        Projection::Perspective(persp) => {
            Mat4::perspective_rh(
                persp.fov,
                persp.aspect_ratio,
                persp.near,
                persp.far,
            )
        },
        Projection::Orthographic(ortho) => {
            let scale = ortho.scale;
            let area = ortho.area;
            let half_width = area.width() * scale * 0.5;
            let half_height = area.height() * scale * 0.5;
            
            Mat4::orthographic_rh(
                -half_width,  // left
                half_width,   // right
                -half_height, // bottom
                half_height,  // top
                ortho.near,
                ortho.far,
            )
        },
    };
    
    let clip_pos = proj_matrix.project_point3(view_pos);
    
    // Convert clip space coordinates to screen space (NDC)
    let ndc = Vec2::new(clip_pos.x, clip_pos.y);
    
    (ndc, distance)
}

/// System to render tiles
pub fn render_tiles(
    mut commands: Commands,
    mut cache: ResMut<TileCache>,
    time: Res<Time>,
    mut quadtree: ResMut<TileQuadtree>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let current_time = time.elapsed_secs();
    
    // Process results from the loader
    let mut results_to_process: Vec<TileLoadResult> = Vec::new();
    
    // In the actual implementation, this would pull from a queue or event reader
    for result in results_to_process.drain(..) {
        match result.result {
            Ok(()) => {
                // Success - the tile was loaded successfully
                // Create mesh and material
            },
            Err(error) => {
                // Error loading the tile
                warn!("Failed to load tile: {:?}", error);
            }
        }
    }
}

/// System to update tile visibility based on camera
pub fn update_tile_visibility(
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut visibility_query: Query<(&GlobalTransform, &mut Visibility, &TileComponent)>,
    time: Res<Time>,
) {
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };
    
    // Get camera position and direction
    let camera_pos = camera_transform.translation();
    let camera_forward = camera_transform.forward();
    
    for (transform, mut visibility, tile_comp) in visibility_query.iter_mut() {
        let tile_pos = transform.translation();
        let to_tile = tile_pos - camera_pos;
        
        // Simple frustum culling - just check if in front of camera
        let dot = to_tile.dot(Vec3::from(*camera_forward));
        *visibility = if dot > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Calculate the projection matrix for a camera
fn calculate_projection_matrix(projection: &Projection) -> Mat4 {
    match projection {
        Projection::Perspective(persp) => {
            // Calculate perspective projection matrix
            Mat4::perspective_rh(
                persp.fov,
                persp.aspect_ratio,
                persp.near,
                persp.far,
            )
        },
        Projection::Orthographic(ortho) => {
            // Calculate orthographic projection matrix based on scale and area
            let scale = ortho.scale;
            let area = ortho.area;
            // Calculate dimensions
            let half_width = area.width() * scale * 0.5;
            let half_height = area.height() * scale * 0.5;
            
            Mat4::orthographic_rh(
                -half_width,  // left
                half_width,   // right
                -half_height, // bottom
                half_height,  // top
                ortho.near,
                ortho.far,
            )
        },
    }
} 