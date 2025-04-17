use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::render::mesh::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::math::primitives::Quad;
use std::collections::HashSet;

pub mod types;
pub mod loader;
pub mod quadtree;
pub mod example;
pub mod cache;
pub mod downloader;
pub mod scheduler;
pub mod rendering;
pub mod meshing;

// Re-export main types
pub use types::{TileId, TileLoadRequest, TileLoadResult, TileError};
pub use loader::{TileLoader, TileLoaderPlugin, TileLoadedEvent};
pub use quadtree::*;
pub use cache::*;
pub use downloader::{TileDownloadQueue, TileDownloaderPlugin, TileDownloadBatchEvent};
pub use example::TileSystemExamplePlugin;

// Re-export all the modules for easier access
pub use self::cache::*;
pub use self::downloader::*;
pub use self::loader::*;
pub use self::meshing::*;
pub use self::quadtree::*;
pub use self::rendering::*;
pub use self::scheduler::*;
pub use self::types::*;

// Internal modules
mod cache;
mod downloader;
mod loader;
mod meshing;
mod quadtree;
mod rendering;
mod scheduler;
mod types;
mod example;

/// Component that marks a camera used for tile system rendering
#[derive(Component, Reflect)]
pub struct CameraTransform;

/// Main plugin for the tile system
pub struct TileSystemPlugin;

impl Plugin for TileSystemPlugin {
    fn build(&self, app: &mut App) {
        // Register the necessary components
        app.register_type::<CameraTransform>()
            // Add the tile cache
            .init_resource::<TileCache>()
            // Add the tile quadtree
            .init_resource::<TileQuadtree>()
            // Add the tile loader
            .init_resource::<TileLoader>()
            // Add the tile download queue
            .init_resource::<TileDownloadQueue>()
            // Register the TileLoadedEvent
            .add_event::<TileLoadedEvent>()
            // Register the TileLoadResult event
            .add_event::<TileLoadResult>()
            // Register the TileLoadRequest event
            .add_event::<TileLoadRequest>()
            // Register the TileDownloadBatchEvent
            .add_event::<TileDownloadBatchEvent>()
            // Add plugins
            .add_plugins(TileLoaderPlugin)
            .add_plugins(TileDownloaderPlugin)
            // Add systems to the update schedule
            .add_systems(Update, (
                process_tile_downloads,
                update_tile_priorities,
                schedule_tile_downloads,
                render_tiles,
            ));
    }
}

/// Create a new image from raw RGBA8 data
pub fn create_image_from_rgba8(data: &[u8], width: u32, height: u32) -> Image {
    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    
    Image::new(
        size,
        TextureDimension::D2,
        data.to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

/// Create a new mesh for a tile
pub fn create_mesh_for_tile(tile_id: &TileId) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    
    let vertices: [[f32; 8]; 4] = [
        [-0.5, 0.0, -0.5,    0.0, 1.0, 0.0,    0.0, 0.0],
        [0.5, 0.0, -0.5,     0.0, 1.0, 0.0,    1.0, 0.0],
        [0.5, 0.0, 0.5,      0.0, 1.0, 0.0,    1.0, 1.0],
        [-0.5, 0.0, 0.5,     0.0, 1.0, 0.0,    0.0, 1.0],
    ];
    
    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| [v[0], v[1], v[2]]).collect();
    let normals: Vec<[f32; 3]> = vertices.iter().map(|v| [v[3], v[4], v[5]]).collect();
    let uvs: Vec<[f32; 2]> = vertices.iter().map(|v| [v[6], v[7]]).collect();
    let indices = vec![0, 1, 2, 0, 2, 3];
    
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    
    mesh
}

/// A component that marks an entity as a tile
#[derive(Component)]
pub struct TileComponent {
    pub id: TileId,
}

/// Initialize the tile system when it's first created
pub fn initialize_tile_system(
    _commands: Commands,
    mut tile_loader: ResMut<TileLoader>,
    mut tile_quadtree: ResMut<TileQuadtree>,
    mut _tile_cache: ResMut<TileCache>,
) {
    // Set default tile server URL
    tile_loader.set_tile_server("https://tile.openstreetmap.org".to_string());
    
    // Set maximum zoom level
    tile_quadtree.set_max_zoom(18);
    
    // Log initialization
    info!("Tile system initialized");
}

/// System to queue tile loading based on visibility
pub fn queue_tiles_for_loading(
    mut tile_loader: ResMut<TileLoader>,
    tile_quadtree: Res<TileQuadtree>,
    _time: Res<Time>,
) {
    // Get all tiles to load
    let tiles_to_load: HashSet<TileId> = tile_quadtree.get_tiles_to_load();
    
    // Queue all tiles for loading
    if !tiles_to_load.is_empty() {
        debug!("Queueing {} tiles for download", tiles_to_load.len());
        tile_loader.queue_tiles(tiles_to_load);
    }
}

/// System to spawn entities for loaded tiles
pub fn spawn_loaded_tiles(
    mut commands: Commands,
    mut tile_loader: ResMut<TileLoader>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut tile_cache: ResMut<TileCache>,
    time: Res<Time>,
) {
    // Process completed downloads
    let completed = tile_loader.process_completed_downloads();
    
    if !completed.is_empty() {
        debug!("Processing {} completed tile downloads", completed.len());
    }
    
    for (tile_id, image_data_result) in completed {
        // Skip errors
        let image_data = match image_data_result {
            Ok(data) => data,
            Err(err) => {
                warn!("Failed to download tile {}/{}/{}: {:?}", tile_id.zoom, tile_id.x, tile_id.y, err);
                continue;
            }
        };
        
        // Store in cache
        if let Ok(()) = tile_cache.insert(tile_id, image_data.clone()) {
            // Create the image from downloaded data
            let img_result = image::load_from_memory(&image_data);
            if let Ok(img) = img_result {
                let img = img.to_rgba8();
                let width = img.width();
                let height = img.height();
                
                let extent = Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                };
                
                let image = Image::new(
                    extent,
                    TextureDimension::D2,
                    img.into_raw(),
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                );
                
                // Add to assets
                let texture_handle = images.add(image);
                
                // Create a material with the texture
                let material_handle = materials.add(StandardMaterial {
                    base_color_texture: Some(texture_handle),
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                });
                
                // Create the tile mesh (a simple plane)
                let mesh = Mesh::from(Quad::new(Vec2::new(1.0, 1.0)));
                let mesh_handle = meshes.add(mesh);
                
                // Get bounds for this tile
                let bounds = crate::tile_system::types::TileBounds::from_tile_id(tile_id);
                let (min, max) = bounds.to_world_coords();
                
                // Calculate position and scale
                let width = max.x - min.x;
                let depth = max.z - min.z;
                let position = Vec3::new(
                    (min.x + max.x) / 2.0,
                    0.0,
                    (min.z + max.z) / 2.0,
                );
                
                // Spawn the tile entity with Transform component rather than TransformBundle
                commands.spawn((
                    PbrBundle {
                        mesh: mesh_handle,
                        material: material_handle,
                        transform: Transform::from_translation(position)
                            .with_scale(Vec3::new(width, 1.0, depth)),
                        ..default()
                    },
                    TileComponent { id: tile_id },
                ));
                
                debug!("Spawned tile entity for tile {}/{}/{}", tile_id.zoom, tile_id.x, tile_id.y);
            } else {
                warn!("Failed to decode image for tile {}/{}/{}", tile_id.zoom, tile_id.x, tile_id.y);
            }
        }
    }
}

/// System to update visible tiles based on camera transform
pub fn update_visible_tiles(
    mut quadtree: ResMut<TileQuadtree>,
    camera_query: Query<(&Transform, &Camera), With<CameraTransform>>,
    windows: Query<&Window>,
) {
    let Ok((camera_transform, _camera)) = camera_query.get_single() else {
        return;
    };
    
    let Ok(window) = windows.get_single() else {
        return;
    };
    
    let viewport_size = Vec2::new(window.width(), window.height());
    
    // Extract camera position
    let transform = camera_transform;
    let camera_height = transform.translation.y;
    
    // Estimate zoom level based on camera height
    let zoom = types::calculate_base_zoom_level(camera_height) as f32;
    
    // Convert camera position to lon/lat
    // This is a simplification - in a real app, you'd use a proper projection
    let lon = transform.translation.x / 100.0; // Scale for demo
    let lat = transform.translation.z / 100.0; // Scale for demo
    
    // Update the quadtree
    quadtree.update(lon, lat, zoom, viewport_size.x, viewport_size.y);
} 